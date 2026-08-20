// editor.rs - PulseEditor: ANSI Syntax-Highlighted Full-Screen In-Kernel Text Editor
//
// Worst-case execution time: Documented per function.

use crate::fs::{fs_read, fs_write};
use crate::lang::run_pulse_script;
use crate::serial::SERIAL;
use crate::serial_print;
use crate::serial_println;

pub const MAX_EDITOR_BUF: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorEscState {
    Normal,
    Esc,
    Csi,
    CsiParam(u8),
}

pub struct PulseEditor {
    pub buffer: [u8; MAX_EDITOR_BUF],
    pub buf_len: usize,
    pub cursor: usize,
    pub filename: [u8; 32],
    pub filename_len: usize,
    pub status_msg: [u8; 64],
    pub status_msg_len: usize,
    pub needs_redraw: bool,
    pub is_running: bool,
    esc_state: EditorEscState,
}

impl PulseEditor {
    pub const fn new() -> Self {
        Self {
            buffer: [0; MAX_EDITOR_BUF],
            buf_len: 0,
            cursor: 0,
            filename: [0; 32],
            filename_len: 0,
            status_msg: [0; 64],
            status_msg_len: 0,
            needs_redraw: true,
            is_running: false,
            esc_state: EditorEscState::Normal,
        }
    }

    pub fn set_filename(&mut self, name: &str) {
        self.filename_len = core::cmp::min(name.len(), 32);
        self.filename[..self.filename_len].copy_from_slice(&name.as_bytes()[..self.filename_len]);
    }

    pub fn filename_str(&self) -> &str {
        if self.filename_len == 0 {
            return "untitled.flow";
        }
        core::str::from_utf8(&self.filename[..self.filename_len]).unwrap_or("untitled.flow")
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status_msg_len = core::cmp::min(msg.len(), 64);
        self.status_msg[..self.status_msg_len].copy_from_slice(&msg.as_bytes()[..self.status_msg_len]);
    }

    pub fn status_str(&self) -> &str {
        if self.status_msg_len == 0 {
            return "";
        }
        core::str::from_utf8(&self.status_msg[..self.status_msg_len]).unwrap_or("")
    }

    // Function: load_file
    // Description: Load file contents from LatencyFS into editor buffer.
    // Worst-case execution time: ~1500 ns
    pub fn load_file(&mut self, filename: &str) {
        self.set_filename(filename);
        self.cursor = 0;
        self.buf_len = 0;
        self.esc_state = EditorEscState::Normal;

        if let Some(data) = fs_read(filename) {
            let len = core::cmp::min(data.len(), MAX_EDITOR_BUF);
            self.buffer[..len].copy_from_slice(&data[..len]);
            self.buf_len = len;
            self.set_status("File loaded from LatencyFS.");
        } else {
            self.set_status("New file created.");
        }
        self.needs_redraw = true;
    }

    // Function: save_file
    // Description: Save editor buffer to LatencyFS.
    // Worst-case execution time: ~2500 ns
    pub fn save_file(&mut self) {
        let name = self.filename_str();
        match fs_write(name, &self.buffer[..self.buf_len]) {
            Ok(()) => {
                self.set_status("Saved to LatencyFS.");
            }
            Err(_e) => {
                self.set_status("Save failed: Disk full or invalid name!");
            }
        }
        self.needs_redraw = true;
    }

    // Function: insert_char
    // Description: Insert character at cursor position.
    // Worst-case execution time: ~800 ns
    pub fn insert_char(&mut self, c: u8) {
        if self.buf_len < MAX_EDITOR_BUF - 1 {
            for i in (self.cursor..self.buf_len).rev() {
                self.buffer[i + 1] = self.buffer[i];
            }
            self.buffer[self.cursor] = c;
            self.cursor += 1;
            self.buf_len += 1;
            self.needs_redraw = true;
        }
    }

    // Function: insert_str
    // Description: Insert string at cursor position (e.g. 4 spaces for Tab).
    // Worst-case execution time: ~2000 ns
    pub fn insert_str(&mut self, s: &[u8]) {
        for &b in s {
            self.insert_char(b);
        }
    }

    // Function: delete_char_backspace
    // Description: Delete character before cursor position.
    // Worst-case execution time: ~800 ns
    pub fn delete_char_backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            for i in self.cursor..self.buf_len - 1 {
                self.buffer[i] = self.buffer[i + 1];
            }
            self.buf_len -= 1;
            self.needs_redraw = true;
        }
    }

    // Function: delete_char_under_cursor
    // Description: Delete character under cursor position (Delete key).
    // Worst-case execution time: ~800 ns
    pub fn delete_char_under_cursor(&mut self) {
        if self.cursor < self.buf_len {
            for i in self.cursor..self.buf_len - 1 {
                self.buffer[i] = self.buffer[i + 1];
            }
            self.buf_len -= 1;
            self.needs_redraw = true;
        }
    }

    // Function: move_cursor_up
    // Description: Move cursor to the same column in the previous line.
    // Worst-case execution time: ~500 ns
    pub fn move_cursor_up(&mut self) {
        let (cur_line_start, col) = self.get_current_line_start_and_col();
        if cur_line_start > 0 {
            // Find start of previous line
            let prev_line_end = cur_line_start - 1;
            let mut prev_line_start = 0;
            for i in (0..prev_line_end).rev() {
                if self.buffer[i] == b'\n' {
                    prev_line_start = i + 1;
                    break;
                }
            }
            let prev_line_len = prev_line_end - prev_line_start;
            self.cursor = prev_line_start + core::cmp::min(col, prev_line_len);
            self.needs_redraw = true;
        }
    }

    // Function: move_cursor_down
    // Description: Move cursor to the same column in the next line.
    // Worst-case execution time: ~500 ns
    pub fn move_cursor_down(&mut self) {
        let (_cur_line_start, col) = self.get_current_line_start_and_col();
        // Find start of next line
        let mut next_line_start = None;
        for i in self.cursor..self.buf_len {
            if self.buffer[i] == b'\n' {
                next_line_start = Some(i + 1);
                break;
            }
        }
        if let Some(start) = next_line_start {
            if start <= self.buf_len {
                // Find end of next line
                let mut end = self.buf_len;
                for i in start..self.buf_len {
                    if self.buffer[i] == b'\n' {
                        end = i;
                        break;
                    }
                }
                let next_line_len = end - start;
                self.cursor = start + core::cmp::min(col, next_line_len);
                self.needs_redraw = true;
            }
        }
    }

    fn get_current_line_start_and_col(&self) -> (usize, usize) {
        let mut line_start = 0;
        for i in (0..self.cursor).rev() {
            if self.buffer[i] == b'\n' {
                line_start = i + 1;
                break;
            }
        }
        (line_start, self.cursor - line_start)
    }

    pub fn get_row_col(&self) -> (usize, usize) {
        let mut row = 1;
        let mut col = 1;
        for i in 0..self.cursor {
            if self.buffer[i] == b'\n' {
                row += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (row, col)
    }

    // Function: redraw
    // Description: Render full editor screen with ANSI color syntax highlighting and crisp alignment.
    // Worst-case execution time: ~60_000 ns
    pub fn redraw(&mut self) {
        // Clear screen and move cursor to top-left
        serial_print!("\x1b[2J\x1b[H");

        let (row, col) = self.get_row_col();

        // Top Status Header (Inverted Bar)
        serial_print!(
            "\x1b[7m LatencyOS PulseEditor | File: {:16} | Size: {:4}B | Line: {:2} Col: {:2} \x1b[0m\r\n",
            self.filename_str(),
            self.buf_len,
            row,
            col
        );

        // Render code lines with syntax coloring and line numbers
        let mut line_num = 1;
        serial_print!("\x1b[90m{:3} |\x1b[0m ", line_num);

        let mut in_comment = false;
        let mut in_string = false;

        let mut i = 0;
        while i < self.buf_len {
            let b = self.buffer[i];

            if b == b'\n' {
                line_num += 1;
                in_comment = false;
                in_string = false;
                serial_print!("\x1b[0m\r\n\x1b[90m{:3} |\x1b[0m ", line_num);
                i += 1;
                continue;
            }

            if b == b'\t' {
                // Render tabs as 4 spaces
                serial_print!("    ");
                i += 1;
                continue;
            }

            // Comment start
            if !in_string && b == b'/' && i + 1 < self.buf_len && self.buffer[i + 1] == b'/' {
                in_comment = true;
                serial_print!("\x1b[90m");
            }

            // String start/end
            if !in_comment && b == b'"' {
                if in_string {
                    serial_print!("\"\x1b[0m");
                    in_string = false;
                    i += 1;
                    continue;
                } else {
                    in_string = true;
                    serial_print!("\x1b[32m\"");
                    i += 1;
                    continue;
                }
            }

            if in_comment || in_string {
                SERIAL.send_byte(b);
                i += 1;
                continue;
            }

            // Keyword & Syntax highlighting
            if self.match_keyword_at(i, b"pipeline")
                || self.match_keyword_at(i, b"within")
                || self.match_keyword_at(i, b"budget")
                || self.match_keyword_at(i, b"let")
                || self.match_keyword_at(i, b"if")
                || self.match_keyword_at(i, b"else")
                || self.match_keyword_at(i, b"while")
                || self.match_keyword_at(i, b"on")
                || self.match_keyword_at(i, b"emit")
                || self.match_keyword_at(i, b"drop")
                || self.match_keyword_at(i, b"or")
            {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;36m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                i += len;
                continue;
            }

            // Native function highlighting
            if self.match_keyword_at(i, b"gpu.capture")
                || self.match_keyword_at(i, b"net.send")
                || self.match_keyword_at(i, b"net.rtt")
                || self.match_keyword_at(i, b"net.set_rate")
                || self.match_keyword_at(i, b"sys.tsc")
                || self.match_keyword_at(i, b"print")
                || self.match_keyword_at(i, b"println")
            {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;35m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                i += len;
                continue;
            }

            // Number / Time Literal highlighting (Yellow)
            if b.is_ascii_digit() {
                let len = self.get_time_literal_len(i);
                serial_print!("\x1b[1;33m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                i += len;
                continue;
            }

            // Pipe operator (|>)
            if b == b'|' && i + 1 < self.buf_len && self.buffer[i + 1] == b'>' {
                serial_print!("\x1b[1;33m|>\x1b[0m");
                i += 2;
                continue;
            }

            SERIAL.send_byte(b);
            i += 1;
        }

        serial_print!("\x1b[0m\r\n\r\n");

        // Status & Shortcut Footer
        let status = self.status_str();
        if !status.is_empty() {
            serial_print!("\x1b[1;32m[MSG] {}\x1b[0m\r\n", status);
        }
        serial_print!("\x1b[7m [^R Run/Compile]  [^S Save]  [^Q Quit]  [^C Clear] \x1b[0m\r\n");

        self.needs_redraw = false;
    }

    fn match_keyword_at(&self, pos: usize, kw: &[u8]) -> bool {
        if pos + kw.len() <= self.buf_len && &self.buffer[pos..pos + kw.len()] == kw {
            let after = pos + kw.len();
            if after >= self.buf_len || (!self.buffer[after].is_ascii_alphanumeric() && self.buffer[after] != b'.' && self.buffer[after] != b'_') {
                return true;
            }
        }
        false
    }

    fn get_word_len(&self, pos: usize) -> usize {
        let mut len = 0;
        while pos + len < self.buf_len && (self.buffer[pos + len].is_ascii_alphanumeric() || self.buffer[pos + len] == b'.' || self.buffer[pos + len] == b'_') {
            len += 1;
        }
        len
    }

    fn get_time_literal_len(&self, pos: usize) -> usize {
        let mut len = 0;
        while pos + len < self.buf_len && self.buffer[pos + len].is_ascii_alphanumeric() {
            len += 1;
        }
        len
    }

    // Function: run_code
    // Description: Compile and execute the current script in the editor buffer.
    // Worst-case execution time: ~100_000 ns
    pub fn run_code(&mut self, tsc_freq_hz: u64) {
        serial_println!("\r\n==================== [PulseLang Execution Output] ====================");
        match run_pulse_script(&self.buffer[..self.buf_len], tsc_freq_hz) {
            Ok(()) => {
                serial_println!("==================== [Execution Success: 0 Errors] ====================");
                self.set_status("Code executed successfully.");
            }
            Err(e) => {
                serial_println!("[ERROR] PulseLang Compile/Runtime Error: {}", e);
                serial_println!("==================== [Execution Failed] ====================");
                self.set_status("Compile/Runtime error!");
            }
        }
        serial_println!("Press any key to return to editor...");
        while SERIAL.read_byte_nonblocking().is_none() {
            core::hint::spin_loop();
        }
        self.needs_redraw = true;
    }
}

pub static mut EDITOR: PulseEditor = PulseEditor::new();

// Function: start_editor
// Description: Launch full-screen interactive PulseEditor on Core 0 with stateful escape sequence parsing.
// Worst-case execution time: Variable (user interactive loop)
pub fn start_editor(filename: &str, tsc_freq_hz: u64) {
    unsafe {
        EDITOR.load_file(filename);
        EDITOR.is_running = true;
        EDITOR.redraw();

        while EDITOR.is_running {
            if let Some(b) = SERIAL.read_byte_nonblocking() {
                match EDITOR.esc_state {
                    EditorEscState::Normal => {
                        match b {
                            // Escape character
                            0x1B => {
                                EDITOR.esc_state = EditorEscState::Esc;
                            }

                            // Ctrl+Q: Quit editor
                            0x11 => {
                                EDITOR.is_running = false;
                                serial_print!("\x1b[2J\x1b[H");
                                break;
                            }

                            // Ctrl+S: Save file
                            0x13 => {
                                EDITOR.save_file();
                                EDITOR.redraw();
                            }

                            // Ctrl+R: Run/Compile code
                            0x12 => {
                                EDITOR.run_code(tsc_freq_hz);
                                EDITOR.redraw();
                            }

                            // Ctrl+C: Clear buffer
                            0x03 => {
                                EDITOR.buf_len = 0;
                                EDITOR.cursor = 0;
                                EDITOR.set_status("Buffer cleared.");
                                EDITOR.redraw();
                            }

                            // Tab: Insert 4 spaces
                            b'\t' => {
                                EDITOR.insert_str(b"    ");
                                EDITOR.redraw();
                            }

                            // Backspace
                            0x08 | 0x7F => {
                                EDITOR.delete_char_backspace();
                                EDITOR.redraw();
                            }

                            // Enter
                            b'\r' | b'\n' => {
                                EDITOR.insert_char(b'\n');
                                EDITOR.redraw();
                            }

                            // Printable ASCII
                            0x20..=0x7E => {
                                EDITOR.insert_char(b);
                                EDITOR.redraw();
                            }

                            _ => {}
                        }
                    }

                    EditorEscState::Esc => {
                        if b == b'[' {
                            EDITOR.esc_state = EditorEscState::Csi;
                        } else {
                            EDITOR.esc_state = EditorEscState::Normal;
                        }
                    }

                    EditorEscState::Csi => {
                        match b {
                            // Up Arrow
                            b'A' => {
                                EDITOR.move_cursor_up();
                                EDITOR.redraw();
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Down Arrow
                            b'B' => {
                                EDITOR.move_cursor_down();
                                EDITOR.redraw();
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Right Arrow
                            b'C' => {
                                if EDITOR.cursor < EDITOR.buf_len {
                                    EDITOR.cursor += 1;
                                    EDITOR.redraw();
                                }
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Left Arrow
                            b'D' => {
                                if EDITOR.cursor > 0 {
                                    EDITOR.cursor -= 1;
                                    EDITOR.redraw();
                                }
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Home
                            b'H' | b'1' => {
                                let (start, _) = EDITOR.get_current_line_start_and_col();
                                EDITOR.cursor = start;
                                EDITOR.redraw();
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // End
                            b'F' | b'4' => {
                                let (start, _) = EDITOR.get_current_line_start_and_col();
                                let mut end = EDITOR.buf_len;
                                for i in start..EDITOR.buf_len {
                                    if EDITOR.buffer[i] == b'\n' {
                                        end = i;
                                        break;
                                    }
                                }
                                EDITOR.cursor = end;
                                EDITOR.redraw();
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Delete key sequence \x1b[3~
                            b'3' => {
                                EDITOR.esc_state = EditorEscState::CsiParam(3);
                            }

                            _ => {
                                EDITOR.esc_state = EditorEscState::Normal;
                            }
                        }
                    }

                    EditorEscState::CsiParam(param) => {
                        if param == 3 && b == b'~' {
                            EDITOR.delete_char_under_cursor();
                            EDITOR.redraw();
                        }
                        EDITOR.esc_state = EditorEscState::Normal;
                    }
                }
            }
            core::hint::spin_loop();
        }
    }
}
