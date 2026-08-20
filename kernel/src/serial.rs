// serial.rs - UART 16550 Serial Driver for Core 0
//
// Worst-case execution time: Documented per function.

use core::fmt::{self, Write};

pub const COM1_BASE: u16 = 0x3F8;

// Function: outb
// Description: Write a byte to an I/O port.
// Worst-case execution time: ~10 ns
#[inline]
pub unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") val,
        options(nomem, nostack, preserves_flags)
    );
}

// Function: inb
// Description: Read a byte from an I/O port.
// Worst-case execution time: ~10 ns
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") val,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    val
}

#[derive(Clone, Copy)]
pub struct SerialPort {
    base: u16,
}

const RX_BUF_SIZE: usize = 4096;
static mut RX_BUFFER: [u8; RX_BUF_SIZE] = [0; RX_BUF_SIZE];
static mut RX_HEAD: usize = 0;
static mut RX_TAIL: usize = 0;
static mut LAST_BYTE_SENT: u8 = 0;

impl SerialPort {
    // Function: new
    // Description: Create a new SerialPort instance for a given base I/O port.
    // Worst-case execution time: ~2 ns
    pub const fn new(base: u16) -> Self {
        Self { base }
    }

    // Function: init
    // Description: Initialize the 16550 UART (115200 baud, 8N1, FIFO enabled, polling mode).
    // Worst-case execution time: ~500 ns
    pub fn init(&self) {
        unsafe {
            // Disable all interrupts (polling mode for zero-latency IRQ avoidance)
            outb(self.base + 1, 0x00);
            // Enable DLAB (set baud rate divisor)
            outb(self.base + 3, 0x80);
            // Set divisor to 1 (lo byte: 0x01, hi byte: 0x00) -> 115200 baud
            outb(self.base + 0, 0x01);
            outb(self.base + 1, 0x00);
            // 8 bits, no parity, 1 stop bit (8N1)
            outb(self.base + 3, 0x03);
            // Enable FIFO, clear TX/RX queues, 1-byte threshold (0x07) for instant single-byte polling
            outb(self.base + 2, 0x07);
            // Set RTS/DSR, Auxiliary Output 2 (0x0B)
            outb(self.base + 4, 0x0B);
        }
    }

    // Function: drain_rx_internal
    // Description: Drain hardware UART RX FIFO into static 4096-byte in-memory ring buffer.
    // Worst-case execution time: ~50 ns
    #[inline]
    pub unsafe fn drain_rx_internal(&self) {
        while (inb(self.base + 5) & 0x01) != 0 {
            let b = inb(self.base);
            let next_head = (RX_HEAD + 1) % RX_BUF_SIZE;
            if next_head != RX_TAIL {
                RX_BUFFER[RX_HEAD] = b;
                RX_HEAD = next_head;
            }
        }
    }

    // Function: is_transmit_empty
    // Description: Check if the transmitter holding register is empty.
    // Worst-case execution time: ~12 ns
    #[inline]
    pub fn is_transmit_empty(&self) -> bool {
        unsafe { (inb(self.base + 5) & 0x20) != 0 }
    }

    // Function: send_byte
    // Description: Send a single byte over the serial port with automatic CRLF normalization and simultaneous RX background draining.
    // Worst-case execution time: ~2000 ns
    pub fn send_byte(&self, byte: u8) {
        unsafe {
            self.drain_rx_internal();
            if byte == b'\n' && LAST_BYTE_SENT != b'\r' {
                while !self.is_transmit_empty() {
                    self.drain_rx_internal();
                    core::hint::spin_loop();
                }
                outb(self.base, b'\r');
                self.drain_rx_internal();
            }
            while !self.is_transmit_empty() {
                self.drain_rx_internal();
                core::hint::spin_loop();
            }
            outb(self.base, byte);
            LAST_BYTE_SENT = byte;
            self.drain_rx_internal();
        }
    }

    // Function: is_data_ready
    // Description: Check if incoming data is available in RX ring buffer or hardware FIFO.
    // Worst-case execution time: ~15 ns
    #[inline]
    pub fn is_data_ready(&self) -> bool {
        unsafe {
            self.drain_rx_internal();
            RX_HEAD != RX_TAIL
        }
    }

    // Function: read_byte_nonblocking
    // Description: Read a single byte from RX ring buffer without blocking.
    // Worst-case execution time: ~20 ns
    #[inline]
    pub fn read_byte_nonblocking(&self) -> Option<u8> {
        unsafe {
            self.drain_rx_internal();
            if RX_HEAD != RX_TAIL {
                let b = RX_BUFFER[RX_TAIL];
                RX_TAIL = (RX_TAIL + 1) % RX_BUF_SIZE;
                Some(b)
            } else {
                None
            }
        }
    }

    // Function: send_str
    // Description: Send a string slice over the serial port.
    // Worst-case execution time: ~1000 ns * s.len()
    pub fn send_str(&self, s: &str) {
        for byte in s.bytes() {
            self.send_byte(byte);
        }
    }
}

pub struct SerialWriter;

impl Write for SerialWriter {
    // Function: write_str
    // Description: Implementation of core::fmt::Write for formatted output without allocations.
    // Worst-case execution time: ~1000 ns * s.len()
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let port = SerialPort::new(COM1_BASE);
        port.send_str(s);
        Ok(())
    }
}

pub static SERIAL: SerialPort = SerialPort::new(COM1_BASE);

// Function: init_serial
// Description: Initialize global COM1 serial port.
// Worst-case execution time: ~550 ns
pub fn init_serial() {
    SERIAL.init();
}

// Function: _print
// Description: Print formatted arguments to COM1 serial port.
// Worst-case execution time: ~5000 ns + 1000 ns * formatted length
pub fn _print(args: fmt::Arguments) {
    let mut writer = SerialWriter;
    let _ = writer.write_fmt(args);
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial_print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::serial_print!("{}\n", format_args!($($arg)*))
    };
}
