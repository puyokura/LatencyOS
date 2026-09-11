// console.rs - Dual Console Output (VGA Text Buffer 0xB8000 + UART COM1)
//
// Allows output to be visible on both the VM/physical screen (VGA) and the serial terminal (COM1).

use crate::serial::{outb, inb};

const VGA_BUFFER_ADDR: usize = 0xB8000;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;
const DEFAULT_COLOR: u8 = 0x07; // Light gray on black

pub struct VgaWriter {
    row: usize,
    col: usize,
    color: u8,
}

pub static mut VGA_WRITER: VgaWriter = VgaWriter {
    row: 0,
    col: 0,
    color: DEFAULT_COLOR,
};

impl VgaWriter {
    pub fn init(&mut self) {
        self.clear_screen();
    }

    pub fn clear_screen(&mut self) {
        let vga = VGA_BUFFER_ADDR as *mut u16;
        let blank = (self.color as u16) << 8 | (b' ' as u16);
        for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
            unsafe {
                vga.add(i).write_volatile(blank);
            }
        }
        self.row = 0;
        self.col = 0;
        self.update_cursor();
    }

    pub fn write_byte(&mut self, b: u8) {
        match b {
            b'\n' => {
                self.new_line();
            }
            b'\r' => {
                self.col = 0;
            }
            b'\x08' => {
                // Backspace
                if self.col > 0 {
                    self.col -= 1;
                    let vga = VGA_BUFFER_ADDR as *mut u16;
                    let offset = self.row * VGA_WIDTH + self.col;
                    let blank = (self.color as u16) << 8 | (b' ' as u16);
                    unsafe {
                        vga.add(offset).write_volatile(blank);
                    }
                }
            }
            byte => {
                if self.col >= VGA_WIDTH {
                    self.new_line();
                }
                let offset = self.row * VGA_WIDTH + self.col;
                let vga = VGA_BUFFER_ADDR as *mut u16;
                let val = (self.color as u16) << 8 | (byte as u16);
                unsafe {
                    vga.add(offset).write_volatile(val);
                }
                self.col += 1;
            }
        }
        self.update_cursor();
    }

    fn new_line(&mut self) {
        self.col = 0;
        if self.row + 1 < VGA_HEIGHT {
            self.row += 1;
        } else {
            // Scroll up 1 line
            let vga = VGA_BUFFER_ADDR as *mut u16;
            for r in 1..VGA_HEIGHT {
                for c in 0..VGA_WIDTH {
                    let from = r * VGA_WIDTH + c;
                    let to = (r - 1) * VGA_WIDTH + c;
                    unsafe {
                        let val = vga.add(from).read_volatile();
                        vga.add(to).write_volatile(val);
                    }
                }
            }
            let blank = (self.color as u16) << 8 | (b' ' as u16);
            let last_row_start = (VGA_HEIGHT - 1) * VGA_WIDTH;
            for c in 0..VGA_WIDTH {
                unsafe {
                    vga.add(last_row_start + c).write_volatile(blank);
                }
            }
        }
    }

    fn update_cursor(&self) {
        let pos = (self.row * VGA_WIDTH + self.col) as u16;
        unsafe {
            outb(0x3D4, 0x0F);
            outb(0x3D5, (pos & 0xFF) as u8);
            outb(0x3D4, 0x0E);
            outb(0x3D5, ((pos >> 8) & 0xFF) as u8);
        }
    }
}

// Check PS/2 Keyboard input
pub fn read_ps2_keyboard_nonblocking() -> Option<u8> {
    unsafe {
        let status = inb(0x64);
        if (status & 0x01) != 0 {
            let scancode = inb(0x60);
            scancode_to_ascii(scancode)
        } else {
            None
        }
    }
}

fn scancode_to_ascii(scancode: u8) -> Option<u8> {
    // Only handle make codes (key down), ignore break codes (key up, bit 7 set)
    if (scancode & 0x80) != 0 {
        return None;
    }
    match scancode {
        0x1C => Some(b'\n'), // Enter
        0x0E => Some(0x08),  // Backspace
        0x01 => Some(0x1B),  // Escape
        0x02..=0x0A => Some(b'1' + (scancode - 0x02)), // 1-9
        0x0B => Some(b'0'),
        0x0C => Some(b'-'),
        0x0D => Some(b'='),
        0x0F => Some(b'\t'), // Tab
        0x10 => Some(b'q'),
        0x11 => Some(b'w'),
        0x12 => Some(b'e'),
        0x13 => Some(b'r'),
        0x14 => Some(b't'),
        0x15 => Some(b'y'),
        0x16 => Some(b'u'),
        0x17 => Some(b'i'),
        0x18 => Some(b'o'),
        0x19 => Some(b'p'),
        0x1E => Some(b'a'),
        0x1F => Some(b's'),
        0x20 => Some(b'd'),
        0x21 => Some(b'f'),
        0x22 => Some(b'g'),
        0x23 => Some(b'h'),
        0x24 => Some(b'j'),
        0x25 => Some(b'k'),
        0x26 => Some(b'l'),
        0x27 => Some(b';'),
        0x28 => Some(b'\''),
        0x2C => Some(b'z'),
        0x2D => Some(b'x'),
        0x2E => Some(b'c'),
        0x2F => Some(b'v'),
        0x30 => Some(b'b'),
        0x31 => Some(b'n'),
        0x32 => Some(b'm'),
        0x33 => Some(b','),
        0x34 => Some(b'.'),
        0x35 => Some(b'/'),
        0x39 => Some(b' '), // Space
        _ => None,
    }
}
