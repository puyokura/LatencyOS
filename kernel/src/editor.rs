// editor.rs - PulseEditor: ANSI Syntax-Highlighted Full-Screen In-Kernel Text Editor
//
// Worst-case execution time: Documented per function.

use crate::fs::{fs_read, fs_write};
use crate::serial::SERIAL;
use crate::serial_print;
use crate::serial_println;

pub const MAX_EDITOR_BUF: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorEscState {
    Normal,
    Esc,
    Ss3,
    Csi {
        params: [u16; 4],
        param_count: usize,
    },
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
            return "untitled.pl";
        }
        core::str::from_utf8(&self.filename[..self.filename_len]).unwrap_or("untitled.pl")
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
            self.set_status("Loaded from LatencyFS.");
        } else {
            self.set_status("New buffer.");
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
                self.set_status("Save failed!");
            }
        }
        self.needs_redraw = true;
    }

    // Function: insert_char
    // Description: Insert character at cursor position (O(1) at end of buffer).
    // Worst-case execution time: ~800 ns
    pub fn insert_char(&mut self, c: u8) {
        if self.buf_len < MAX_EDITOR_BUF - 1 {
            if self.cursor == self.buf_len {
                self.buffer[self.cursor] = c;
                self.cursor += 1;
                self.buf_len += 1;
            } else {
                for i in (self.cursor..self.buf_len).rev() {
                    self.buffer[i + 1] = self.buffer[i];
                }
                self.buffer[self.cursor] = c;
                self.cursor += 1;
                self.buf_len += 1;
            }
            self.needs_redraw = true;
        }
    }

    // Function: insert_str
    // Description: Insert a byte slice at cursor position with single-shift block copy.
    // Worst-case execution time: ~1200 ns
    pub fn insert_str(&mut self, s: &[u8]) {
        let len = s.len();
        if self.buf_len + len <= MAX_EDITOR_BUF {
            if self.cursor == self.buf_len {
                self.buffer[self.cursor..self.cursor + len].copy_from_slice(s);
                self.cursor += len;
                self.buf_len += len;
            } else {
                for i in (self.cursor..self.buf_len).rev() {
                    self.buffer[i + len] = self.buffer[i];
                }
                self.buffer[self.cursor..self.cursor + len].copy_from_slice(s);
                self.cursor += len;
                self.buf_len += len;
            }
            self.needs_redraw = true;
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

    // Function: move_word_left
    // Description: Move cursor to previous word boundary (Ctrl+Left).
    // Worst-case execution time: ~400 ns
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut pos = self.cursor;
        while pos > 0 && (self.buffer[pos - 1] == b' ' || self.buffer[pos - 1] == b'\t' || self.buffer[pos - 1] == b'\n') {
            pos -= 1;
        }
        while pos > 0 && self.buffer[pos - 1] != b' ' && self.buffer[pos - 1] != b'\t' && self.buffer[pos - 1] != b'\n' {
            pos -= 1;
        }
        self.cursor = pos;
        self.needs_redraw = true;
    }

    // Function: move_word_right
    // Description: Move cursor to next word boundary (Ctrl+Right).
    // Worst-case execution time: ~400 ns
    pub fn move_word_right(&mut self) {
        if self.cursor >= self.buf_len {
            return;
        }
        let mut pos = self.cursor;
        while pos < self.buf_len && self.buffer[pos] != b' ' && self.buffer[pos] != b'\t' && self.buffer[pos] != b'\n' {
            pos += 1;
        }
        while pos < self.buf_len && (self.buffer[pos] == b' ' || self.buffer[pos] == b'\t' || self.buffer[pos] == b'\n') {
            pos += 1;
        }
        self.cursor = pos;
        self.needs_redraw = true;
    }

    pub fn get_current_line_start_and_col(&self) -> (usize, usize) {
        let mut line_start = 0;
        for i in (0..self.cursor).rev() {
            if self.buffer[i] == b'\n' {
                line_start = i + 1;
                break;
            }
        }
        (line_start, self.cursor - line_start)
    }

    // Function: move_cursor_up
    // Description: Move cursor to the same column in the previous line.
    // Worst-case execution time: ~500 ns
    pub fn move_cursor_up(&mut self) {
        let (cur_line_start, col) = self.get_current_line_start_and_col();
        if cur_line_start > 0 {
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
        let mut next_line_start = None;
        for i in self.cursor..self.buf_len {
            if self.buffer[i] == b'\n' {
                next_line_start = Some(i + 1);
                break;
            }
        }
        if let Some(start) = next_line_start {
            if start <= self.buf_len {
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

    // Function: kill_line_forward
    // Description: Delete text from cursor to end of current line.
    // Worst-case execution time: ~1200 ns
    #[allow(dead_code)]
    pub fn kill_line_forward(&mut self) {
        if self.cursor >= self.buf_len {
            return;
        }
        let mut end = self.cursor;
        while end < self.buf_len && self.buffer[end] != b'\n' {
            end += 1;
        }
        if end == self.cursor && end < self.buf_len && self.buffer[end] == b'\n' {
            end += 1;
        }
        let count = end - self.cursor;
        if count > 0 {
            for i in self.cursor..self.buf_len - count {
                self.buffer[i] = self.buffer[i + count];
            }
            self.buf_len -= count;
            self.needs_redraw = true;
        }
    }

    // Function: kill_line_backward
    // Description: Delete text from cursor back to start of line.
    // Worst-case execution time: ~1200 ns
    #[allow(dead_code)]
    pub fn kill_line_backward(&mut self) {
        let (start, _) = self.get_current_line_start_and_col();
        let count = self.cursor - start;
        if count > 0 {
            for i in start..self.buf_len - count {
                self.buffer[i] = self.buffer[i + count];
            }
            self.buf_len -= count;
            self.cursor = start;
            self.needs_redraw = true;
        }
    }

    pub fn get_row_col(&self) -> (usize, usize) {
        let mut row = 1;
        let mut col = 1;
        for i in 0..self.cursor {
            if self.buffer[i] == b'\n' {
                row += 1;
                col = 1;
            } else if self.buffer[i] == b'\t' {
                col += 4;
            } else {
                col += 1;
            }
        }
        (row, col)
    }

    // Function: update_cursor_only
    // Description: Smoothly reposition cursor and update status header without redrawing lines (Zero Flicker).
    // Worst-case execution time: ~1200 ns
    pub fn update_cursor_only(&self) {
        let (row, col) = self.get_row_col();
        // Update top line
        serial_print!(
            "\x1b[H\x1b[7m LatencyOS PulseEditor | File: {:16} | Size: {:4}B | Line: {:2} Col: {:2} \x1b[0m\x1b[K",
            self.filename_str(),
            self.buf_len,
            row,
            col
        );
        // Move cursor to editing position
        serial_print!("\x1b[{};{}H\x1b[?25h", row + 1, col + 6);
    }

    // Function: redraw_current_line
    // Description: Redraw only the current line and update header (Micro-Update for instant typing/pasting with zero character drops).
    // Worst-case execution time: ~1500 ns
    pub fn redraw_current_line(&mut self) {
        let (row, col) = self.get_row_col();
        let (line_start, _) = self.get_current_line_start_and_col();

        // Update top line
        serial_print!(
            "\x1b[H\x1b[7m LatencyOS PulseEditor | File: {:16} | Size: {:4}B | Line: {:2} Col: {:2} \x1b[0m\x1b[K",
            self.filename_str(),
            self.buf_len,
            row,
            col
        );

        // Position at start of current row (row + 1 because top header is row 1)
        serial_print!("\x1b[{};1H\x1b[90m{:3} |\x1b[0m ", row + 1, row);

        let mut line_end = self.buf_len;
        for idx in line_start..self.buf_len {
            if self.buffer[idx] == b'\n' {
                line_end = idx;
                break;
            }
        }

        let mut in_comment = false;
        let mut in_string = false;
        let mut i = line_start;
        while i < line_end {
            let b = self.buffer[i];

            if b == b'\t' {
                serial_print!("    ");
                i += 1;
                continue;
            }

            if !in_string && b == b'/' && i + 1 < line_end && self.buffer[i + 1] == b'/' {
                in_comment = true;
                serial_print!("\x1b[90m");
            }

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

            if b == b'@' {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;36m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..core::cmp::min(i + len, line_end)]).unwrap_or(""));
                i += len;
                continue;
            }

            if b == b'$' {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;32m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..core::cmp::min(i + len, line_end)]).unwrap_or(""));
                i += len;
                continue;
            }

            if b == b'#' {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;35m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..core::cmp::min(i + len, line_end)]).unwrap_or(""));
                i += len;
                continue;
            }

            if (b == b':' || b == b'+' || b == b'-') && i + 1 < line_end && self.buffer[i + 1] == b'=' {
                serial_print!("\x1b[1;33m{}{}\x1b[0m", b as char, '=' as char);
                i += 2;
                continue;
            }

            if b == b'|' && i + 1 < line_end && self.buffer[i + 1] == b'>' {
                serial_print!("\x1b[1;33m|>\x1b[0m");
                i += 2;
                continue;
            }

            if b.is_ascii_digit() {
                let len = self.get_time_literal_len(i);
                let actual_len = core::cmp::min(len, line_end - i);
                let is_time = actual_len > 2
                    && (self.buffer[i + actual_len - 2..i + actual_len] == *b"us"
                        || self.buffer[i + actual_len - 2..i + actual_len] == *b"ns"
                        || self.buffer[i + actual_len - 2..i + actual_len] == *b"ms");
                if is_time {
                    serial_print!("\x1b[1;35m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + actual_len]).unwrap_or(""));
                } else {
                    serial_print!("\x1b[33m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + actual_len]).unwrap_or(""));
                }
                i += actual_len;
                continue;
            }

            if b == b'{' || b == b'}' || b == b'(' || b == b')' || b == b'[' || b == b']' {
                serial_print!("\x1b[1;37m{}\x1b[0m", b as char);
                i += 1;
                continue;
            }

            SERIAL.send_byte(b);
            i += 1;
        }

        // Clear rest of current line and reposition cursor
        serial_print!("\x1b[0m\x1b[K\x1b[{};{}H\x1b[?25h", row + 1, col + 6);
        self.needs_redraw = false;
    }

    // Function: redraw
    // Description: Render full editor screen smoothly using line-by-line clear (No \x1b[2J full-screen flash).
    // Worst-case execution time: ~40_000 ns
    pub fn redraw(&mut self) {
        serial_print!("\x1b[H");

        let (row, col) = self.get_row_col();

        // Top Status Header (Inverted Bar)
        serial_print!(
            "\x1b[7m LatencyOS PulseEditor | File: {:16} | Size: {:4}B | Line: {:2} Col: {:2} \x1b[0m\x1b[K\r\n",
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
                serial_print!("\x1b[0m\x1b[K\r\n\x1b[90m{:3} |\x1b[0m ", line_num);
                i += 1;
                continue;
            }

            if b == b'\t' {
                serial_print!("    ");
                i += 1;
                continue;
            }

            if !in_string && b == b'/' && i + 1 < self.buf_len && self.buffer[i + 1] == b'/' {
                in_comment = true;
                serial_print!("\x1b[90m");
            }

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

            // PulseLang v2: Directives & Intrinsics (@contract, @tsc, etc.)
            if b == b'@' {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;36m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                i += len;
                continue;
            }

            // PulseLang v2: Variables ($rtt, $sum)
            if b == b'$' {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;32m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                i += len;
                continue;
            }

            // PulseLang v2: Hardware Handles (#f, #frame)
            if b == b'#' {
                let len = self.get_word_len(i);
                serial_print!("\x1b[1;35m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                i += len;
                continue;
            }

            // Walrus := or += or -=
            if (b == b':' || b == b'+' || b == b'-') && i + 1 < self.buf_len && self.buffer[i + 1] == b'=' {
                serial_print!("\x1b[1;33m{}{}\x1b[0m", b as char, '=' as char);
                i += 2;
                continue;
            }

            // Pipe |>
            if b == b'|' && i + 1 < self.buf_len && self.buffer[i + 1] == b'>' {
                serial_print!("\x1b[1;33m|>\x1b[0m");
                i += 2;
                continue;
            }

            // Time Literals (e.g. 500us, 8000us, 5ms, 100ns)
            if b.is_ascii_digit() {
                let len = self.get_time_literal_len(i);
                let is_time = len > 2
                    && (self.buffer[i + len - 2..i + len] == *b"us"
                        || self.buffer[i + len - 2..i + len] == *b"ns"
                        || self.buffer[i + len - 2..i + len] == *b"ms");
                if is_time {
                    serial_print!("\x1b[1;35m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                } else {
                    serial_print!("\x1b[33m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                }
                i += len;
                continue;
            }

            // Syntax highlighting for braces, brackets, and operators
            if b == b'{' || b == b'}' || b == b'(' || b == b')' || b == b'[' || b == b']' {
                serial_print!("\x1b[1;37m{}\x1b[0m", b as char);
                i += 1;
                continue;
            }

            SERIAL.send_byte(b);
            i += 1;
        }

        // Empty bottom line and status bar
        serial_print!("\x1b[0m\x1b[K\r\n\r\n");
        let status = self.status_str();
        if !status.is_empty() {
            serial_print!("\x1b[33m[MSG] {}\x1b[0m\x1b[K\r\n", status);
        } else {
            serial_print!("\x1b[K\r\n");
        }

        // Footer Navigation Bar
        serial_print!("\x1b[7m [^S / F2 Save]  [^R / F5 Run]  [^Q / F10 Quit]  [^X Save&Quit]  [Esc C Clear] \x1b[0m\x1b[K\x1b[J");

        // Reposition terminal cursor at the exact editing position and make it visible
        serial_print!("\x1b[{};{}H\x1b[?25h", row + 1, col + 6);

        self.needs_redraw = false;
    }

    fn get_word_len(&self, pos: usize) -> usize {
        let mut len = 0;
        while pos + len < self.buf_len && (self.buffer[pos + len].is_ascii_alphanumeric() || self.buffer[pos + len] == b'.' || self.buffer[pos + len] == b'_') {
            len += 1;
        }
        if len == 0 && pos < self.buf_len {
            len = 1;
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
        serial_print!("\x1b[2J\x1b[H");
        serial_println!("==================== [PulseLang Execution Output] ====================");
        match crate::lang::run_pulse_auto(&self.buffer[..self.buf_len], tsc_freq_hz) {
            Ok(()) => {
                serial_println!("==================== [Execution Success: 0 Errors] ====================");
                self.set_status("Code executed successfully.");
            }
            Err(err) => {
                crate::lang::print_compile_diagnostic(&self.buffer[..self.buf_len], self.filename_str(), &err);
                self.set_status("Compile/Runtime error! (Diagnostic displayed)");
            }
        }
        serial_println!("=======================================================================");
        serial_println!("[Press any key to return to PulseEditor]");
        while SERIAL.read_byte_nonblocking().is_some() {}
        while SERIAL.read_byte_nonblocking().is_none() {
            core::hint::spin_loop();
        }
        self.needs_redraw = true;
        self.redraw();
    }
}

pub static mut EDITOR: PulseEditor = PulseEditor::new();

// Function: start_editor
// Description: Launch full-screen interactive PulseEditor on Core 0 with fast batch paste, rich Ctrl-key bindings, and stateful escape sequence parsing.
// Worst-case execution time: Variable (user interactive loop)
pub fn start_editor(filename: &str, tsc_freq_hz: u64) {
    unsafe {
        EDITOR.load_file(filename);
        EDITOR.is_running = true;
        EDITOR.set_status("Ready. (^S: Save, ^R: Run, ^Q: Quit, ^X: Save&Quit, Esc C: Clear)");
        EDITOR.redraw();

        let mut last_was_cr = false;
        let mut line_changed = false;
        let mut structure_changed = false;
        let mut cursor_only = false;

        while EDITOR.is_running {
            while let Some(b) = SERIAL.read_byte_nonblocking() {

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
                                structure_changed = true;
                            }

                            // Ctrl+R: Run/Compile code
                            0x12 => {
                                EDITOR.run_code(tsc_freq_hz);
                                structure_changed = true;
                            }

                            // Ctrl+X: Save & Exit
                            0x18 => {
                                EDITOR.save_file();
                                EDITOR.is_running = false;
                                serial_print!("\x1b[2J\x1b[H");
                                break;
                            }

                            // Ctrl+L: Full Redraw
                            0x0C => {
                                structure_changed = true;
                            }

                            // Tab: Insert 4 spaces
                            b'\t' => {
                                EDITOR.insert_str(b"    ");
                                line_changed = true;
                                last_was_cr = false;
                            }

                            // Backspace
                            0x08 | 0x7F => {
                                EDITOR.delete_char_backspace();
                                structure_changed = true;
                                last_was_cr = false;
                            }

                            // Enter / Newline
                            b'\r' => {
                                EDITOR.insert_char(b'\n');
                                structure_changed = true;
                                last_was_cr = true;
                            }
                            b'\n' => {
                                if !last_was_cr {
                                    EDITOR.insert_char(b'\n');
                                    structure_changed = true;
                                }
                                last_was_cr = false;
                            }

                            // Printable ASCII
                            0x20..=0x7E => {
                                EDITOR.insert_char(b);
                                line_changed = true;
                                last_was_cr = false;
                            }

                            _ => {
                                last_was_cr = false;
                            }
                        }
                    }

                    EditorEscState::Esc => {
                        match b {
                            b'[' => {
                                EDITOR.esc_state = EditorEscState::Csi {
                                    params: [0; 4],
                                    param_count: 0,
                                };
                            }
                            b'O' => {
                                EDITOR.esc_state = EditorEscState::Ss3;
                            }
                            // Alt+S / Esc S: Save
                            b's' | b'S' => {
                                EDITOR.save_file();
                                structure_changed = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }
                            // Alt+R / Esc R: Run
                            b'r' | b'R' => {
                                EDITOR.run_code(tsc_freq_hz);
                                structure_changed = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }
                            // Alt+Q / Esc Q: Quit
                            b'q' | b'Q' => {
                                EDITOR.is_running = false;
                                serial_print!("\x1b[2J\x1b[H");
                                break;
                            }
                            // Alt+C / Esc C: Clear
                            b'c' | b'C' => {
                                EDITOR.buf_len = 0;
                                EDITOR.cursor = 0;
                                EDITOR.set_status("Buffer cleared.");
                                structure_changed = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }
                            _ => {
                                if (0x20..=0x7E).contains(&b) {
                                    EDITOR.insert_char(b);
                                    line_changed = true;
                                }
                                EDITOR.esc_state = EditorEscState::Normal;
                            }
                        }
                    }

                    EditorEscState::Ss3 => {
                        match b {
                            b'Q' => { // F2: Save
                                EDITOR.save_file();
                                structure_changed = true;
                            }
                            b'R' => { // F3
                                structure_changed = true;
                            }
                            _ => {}
                        }
                        EDITOR.esc_state = EditorEscState::Normal;
                    }

                    EditorEscState::Csi { ref mut params, ref mut param_count } => {
                        match b {
                            b'0'..=b'9' => {
                                if *param_count < 4 {
                                    params[*param_count] = params[*param_count].saturating_mul(10).saturating_add((b - b'0') as u16);
                                }
                            }

                            b';' => {
                                if *param_count < 3 {
                                    *param_count += 1;
                                }
                            }

                            // Up Arrow
                            b'A' => {
                                let is_ctrl = (params[0] == 1 && params[1] == 5) || params[0] == 5;
                                if is_ctrl {
                                    for _ in 0..5 {
                                        EDITOR.move_cursor_up();
                                    }
                                } else {
                                    EDITOR.move_cursor_up();
                                }
                                cursor_only = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Down Arrow
                            b'B' => {
                                let is_ctrl = (params[0] == 1 && params[1] == 5) || params[0] == 5;
                                if is_ctrl {
                                    for _ in 0..5 {
                                        EDITOR.move_cursor_down();
                                    }
                                } else {
                                    EDITOR.move_cursor_down();
                                }
                                cursor_only = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Right Arrow / Ctrl+Right
                            b'C' => {
                                let is_ctrl = (params[0] == 1 && params[1] == 5) || params[0] == 5;
                                if is_ctrl {
                                    EDITOR.move_word_right();
                                } else if EDITOR.cursor < EDITOR.buf_len {
                                    EDITOR.cursor += 1;
                                }
                                cursor_only = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Left Arrow / Ctrl+Left
                            b'D' => {
                                let is_ctrl = (params[0] == 1 && params[1] == 5) || params[0] == 5;
                                if is_ctrl {
                                    EDITOR.move_word_left();
                                } else if EDITOR.cursor > 0 {
                                    EDITOR.cursor -= 1;
                                }
                                cursor_only = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Home
                            b'H' => {
                                let (start, _) = EDITOR.get_current_line_start_and_col();
                                EDITOR.cursor = start;
                                cursor_only = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // End
                            b'F' => {
                                let (start, _) = EDITOR.get_current_line_start_and_col();
                                let mut end = EDITOR.buf_len;
                                for i in start..EDITOR.buf_len {
                                    if EDITOR.buffer[i] == b'\n' {
                                        end = i;
                                        break;
                                    }
                                }
                                EDITOR.cursor = end;
                                cursor_only = true;
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // Tilde sequences: 3~ (Delete), 1~/7~ (Home), 4~/8~ (End), 12~ (F2), 15~ (F5), 21~ (F10), 200~/201~ (Paste wrapper)
                            b'~' => {
                                if params[0] == 12 { // F2: Save
                                    EDITOR.save_file();
                                    structure_changed = true;
                                } else if params[0] == 15 { // F5: Run
                                    EDITOR.run_code(tsc_freq_hz);
                                    structure_changed = true;
                                } else if params[0] == 21 { // F10: Quit
                                    EDITOR.is_running = false;
                                    serial_print!("\x1b[2J\x1b[H");
                                    break;
                                } else if params[0] == 3 {
                                    EDITOR.delete_char_under_cursor();
                                    structure_changed = true;
                                } else if params[0] == 1 || params[0] == 7 {
                                    let (start, _) = EDITOR.get_current_line_start_and_col();
                                    EDITOR.cursor = start;
                                    cursor_only = true;
                                } else if params[0] == 4 || params[0] == 8 {
                                    let (start, _) = EDITOR.get_current_line_start_and_col();
                                    let mut end = EDITOR.buf_len;
                                    for i in start..EDITOR.buf_len {
                                        if EDITOR.buffer[i] == b'\n' {
                                            end = i;
                                            break;
                                        }
                                    }
                                    EDITOR.cursor = end;
                                    cursor_only = true;
                                } else if params[0] == 200 || params[0] == 201 {
                                    structure_changed = true;
                                }
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            // ANSI SGR colors, terminal modes, erase sequences (e.g. \x1b[0m, \x1b[?25h, \x1b[K)
                            b'm' | b'h' | b'l' | b'J' | b'K' | b'r' => {
                                EDITOR.esc_state = EditorEscState::Normal;
                            }

                            _ => {
                                EDITOR.esc_state = EditorEscState::Normal;
                            }
                        }
                    }
                }
            }

            // Redraw immediately when input has occurred and hardware FIFO is drained
            if (structure_changed || line_changed || cursor_only) && EDITOR.is_running {
                if structure_changed {
                    EDITOR.redraw();
                    structure_changed = false;
                    line_changed = false;
                    cursor_only = false;
                } else if line_changed {
                    EDITOR.redraw_current_line();
                    line_changed = false;
                    cursor_only = false;
                } else if cursor_only {
                    EDITOR.update_cursor_only();
                    cursor_only = false;
                }
            }

            core::hint::spin_loop();
        }
    }
}
