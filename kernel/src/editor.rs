// editor.rs - PulseEditor: ANSI Syntax-Highlighted Full-Screen In-Kernel Text Editor
//
// Worst-case execution time: Documented per function.

use crate::fs::{fs_read, fs_write};
use crate::lang::run_pulse_script;
use crate::serial::{SERIAL};
use crate::serial_print;
use crate::serial_println;

pub const MAX_EDITOR_BUF: usize = 4096;

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

        if let Some(data) = fs_read(filename) {
            let len = core::cmp::min(data.len(), MAX_EDITOR_BUF);
            self.buffer[..len].copy_from_slice(&data[..len]);
            self.buf_len = len;
            self.set_status("File loaded.");
        } else {
            self.set_status("New file.");
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
                self.set_status("File saved successfully.");
            }
            Err(_e) => {
                self.set_status("Save failed!");
            }
        }
        self.needs_redraw = true;
    }

    // Function: insert_char
    // Description: Insert character at cursor position.
    // Worst-case execution time: ~800 ns
    pub fn insert_char(&mut self, c: u8) {
        if self.buf_len < MAX_EDITOR_BUF - 1 {
            // Shift right
            for i in (self.cursor..self.buf_len).rev() {
                self.buffer[i + 1] = self.buffer[i];
            }
            self.buffer[self.cursor] = c;
            self.cursor += 1;
            self.buf_len += 1;
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

    // Function: redraw
    // Description: Render full editor screen with ANSI color syntax highlighting.
    // Worst-case execution time: ~60_000 ns
    pub fn redraw(&mut self) {
        // Clear screen and move cursor to top-left
        serial_print!("\x1b[2J\x1b[H");

        // Top Status Header (Inverted Bar)
        serial_print!("\x1b[7m LatencyOS PulseEditor | File: {} | Size: {}B | WCET Guard: ON \x1b[0m\r\n", self.filename_str(), self.buf_len);

        // Render code lines with syntax coloring
        let mut line_num = 1;
        serial_print!("\x1b[90m{:2} | \x1b[0m", line_num);

        let mut in_comment = false;
        let mut in_string = false;

        let mut i = 0;
        while i < self.buf_len {
            let b = self.buffer[i];

            if b == b'\n' {
                line_num += 1;
                in_comment = false;
                in_string = false;
                serial_print!("\x1b[0m\r\n\x1b[90m{:2} | \x1b[0m", line_num);
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
                serial_print!("\x1b[36m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
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
                serial_print!("\x1b[35m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
                i += len;
                continue;
            }

            // Number / Time Literal highlighting (Yellow)
            if b.is_ascii_digit() {
                let len = self.get_time_literal_len(i);
                serial_print!("\x1b[33m{}\x1b[0m", core::str::from_utf8(&self.buffer[i..i + len]).unwrap_or(""));
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
            // Check boundary
            let after = pos + kw.len();
            if after >= self.buf_len || !self.buffer[after].is_ascii_alphanumeric() && self.buffer[after] != b'.' && self.buffer[after] != b'_' {
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
        while pos + len < self.buf_len && (self.buffer[pos + len].is_ascii_alphanumeric()) {
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
// Description: Launch full-screen interactive PulseEditor on Core 0.
// Worst-case execution time: Variable (user interactive loop)
pub fn start_editor(filename: &str, tsc_freq_hz: u64) {
    unsafe {
        EDITOR.load_file(filename);
        EDITOR.is_running = true;
        EDITOR.redraw();

        while EDITOR.is_running {
            if let Some(b) = SERIAL.read_byte_nonblocking() {
                match b {
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

                    // Backspace / Delete
                    0x08 | 0x7F => {
                        EDITOR.delete_char_backspace();
                        EDITOR.redraw();
                    }

                    // Enter
                    b'\r' | b'\n' => {
                        EDITOR.insert_char(b'\n');
                        EDITOR.redraw();
                    }

                    // ANSI Escape sequences (e.g. Arrow keys)
                    0x1B => {
                        if let Some(b2) = SERIAL.read_byte_nonblocking() {
                            if b2 == b'[' {
                                if let Some(b3) = SERIAL.read_byte_nonblocking() {
                                    match b3 {
                                        b'A' => { // Up
                                            EDITOR.cursor = EDITOR.cursor.saturating_sub(20);
                                            EDITOR.redraw();
                                        }
                                        b'B' => { // Down
                                            EDITOR.cursor = core::cmp::min(EDITOR.cursor + 20, EDITOR.buf_len);
                                            EDITOR.redraw();
                                        }
                                        b'C' => { // Right
                                            EDITOR.cursor = core::cmp::min(EDITOR.cursor + 1, EDITOR.buf_len);
                                            EDITOR.redraw();
                                        }
                                        b'D' => { // Left
                                            EDITOR.cursor = EDITOR.cursor.saturating_sub(1);
                                            EDITOR.redraw();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }

                    // Printable ASCII
                    0x20..=0x7E => {
                        EDITOR.insert_char(b);
                        EDITOR.redraw();
                    }

                    _ => {}
                }
            }
            core::hint::spin_loop();
        }
    }
}
