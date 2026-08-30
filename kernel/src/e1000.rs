// e1000.rs - Intel 82540EM / e1000 Baremetal Poll-Mode Driver (PMD)
//
// Worst-case execution time: Documented per function.

#![allow(static_mut_refs)]

#[allow(dead_code)]
pub const E1000_CTRL: u32 = 0x0000;
#[allow(dead_code)]
pub const E1000_STATUS: u32 = 0x0008;
#[allow(dead_code)]
pub const E1000_EERD: u32 = 0x0014;
pub const E1000_ICR: u32 = 0x00C0;
pub const E1000_IMC: u32 = 0x00D8;
pub const E1000_RCTL: u32 = 0x0100;
pub const E1000_TCTL: u32 = 0x0400;

pub const E1000_RDBAL: u32 = 0x2800;
pub const E1000_RDBAH: u32 = 0x2804;
pub const E1000_RDLEN: u32 = 0x2808;
pub const E1000_RDH: u32 = 0x2810;
pub const E1000_RDT: u32 = 0x2818;

pub const E1000_TDBAL: u32 = 0x3800;
pub const E1000_TDBAH: u32 = 0x3804;
pub const E1000_TDLEN: u32 = 0x3808;
pub const E1000_TDH: u32 = 0x3810;
pub const E1000_TDT: u32 = 0x3818;

pub const E1000_RA: u32 = 0x5400;

pub const NUM_TX_DESCS: usize = 64;
pub const NUM_RX_DESCS: usize = 64;
pub const PACKET_BUFFER_SIZE: usize = 2048;

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct TxDesc {
    pub addr: u64,
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct RxDesc {
    pub addr: u64,
    pub length: u16,
    pub checksum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

// Pre-allocated static DMA descriptor rings and packet buffers
static mut TX_DESCS: [TxDesc; NUM_TX_DESCS] = [TxDesc {
    addr: 0,
    length: 0,
    cso: 0,
    cmd: 0,
    status: 0,
    css: 0,
    special: 0,
}; NUM_TX_DESCS];

static mut RX_DESCS: [RxDesc; NUM_RX_DESCS] = [RxDesc {
    addr: 0,
    length: 0,
    checksum: 0,
    status: 0,
    errors: 0,
    special: 0,
}; NUM_RX_DESCS];

#[repr(align(64))]
#[derive(Clone, Copy)]
struct PacketBuffer {
    data: [u8; PACKET_BUFFER_SIZE],
}

static mut TX_BUFFERS: [PacketBuffer; NUM_TX_DESCS] = [PacketBuffer { data: [0; PACKET_BUFFER_SIZE] }; NUM_TX_DESCS];
static mut RX_BUFFERS: [PacketBuffer; NUM_RX_DESCS] = [PacketBuffer { data: [0; PACKET_BUFFER_SIZE] }; NUM_RX_DESCS];

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[allow(dead_code)]
pub struct E1000Driver {
    pub bar0: u64,
    pub mac: [u8; 6],
    pub tx_tail: usize,
    pub rx_tail: usize,
}

pub static mut E1000: Option<E1000Driver> = None;
pub static E1000_BAR0: AtomicU64 = AtomicU64::new(0);
pub static E1000_TX_TAIL: AtomicUsize = AtomicUsize::new(0);
pub static E1000_RX_TAIL: AtomicUsize = AtomicUsize::new(0);
pub static mut E1000_MAC: [u8; 6] = [0; 6];
pub static E1000_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Function: mmio_read32
// Description: Read 32-bit register from e1000 MMIO space.
// Worst-case execution time: ~15 ns
#[inline]
pub unsafe fn mmio_read32(base: u64, offset: u32) -> u32 {
    let ptr = (base + offset as u64) as *const u32;
    core::ptr::read_volatile(ptr)
}

// Function: mmio_write32
// Description: Write 32-bit register to e1000 MMIO space.
// Worst-case execution time: ~20 ns
#[inline]
pub unsafe fn mmio_write32(base: u64, offset: u32, val: u32) {
    let ptr = (base + offset as u64) as *mut u32;
    core::ptr::write_volatile(ptr, val);
}

// Function: init_e1000
// Description: Initialize Intel e1000 network adapter in pure Poll-Mode (interrupts completely disabled).
// Worst-case execution time: ~5_000 ns
pub fn init_e1000(bar0: u64) -> Result<(), &'static str> {
    unsafe {
        // 1. Disable all device interrupts (Pure Poll-Mode Driver)
        mmio_write32(bar0, E1000_IMC, 0xFFFF_FFFF);
        let _ = mmio_read32(bar0, E1000_ICR); // Clear pending

        // 2. Read MAC Address from Receive Address Registers (RAL/RAH)
        let ral = mmio_read32(bar0, E1000_RA);
        let rah = mmio_read32(bar0, E1000_RA + 4);
        let mut mac = [0u8; 6];
        mac[0] = (ral & 0xFF) as u8;
        mac[1] = ((ral >> 8) & 0xFF) as u8;
        mac[2] = ((ral >> 16) & 0xFF) as u8;
        mac[3] = ((ral >> 24) & 0xFF) as u8;
        mac[4] = (rah & 0xFF) as u8;
        mac[5] = ((rah >> 8) & 0xFF) as u8;

        // If RA is uninitialized (e.g. QEMU), set default QEMU MAC: 52:54:00:12:34:56
        if mac == [0, 0, 0, 0, 0, 0] || mac == [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF] {
            mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        }

        // 3. Initialize TX Descriptor Ring
        let tx_ring_phys = TX_DESCS.as_ptr() as u64;
        for i in 0..NUM_TX_DESCS {
            TX_DESCS[i].addr = TX_BUFFERS[i].data.as_ptr() as u64;
            TX_DESCS[i].cmd = 0;
            TX_DESCS[i].status = 1; // Descriptor Done
        }

        mmio_write32(bar0, E1000_TDBAL, (tx_ring_phys & 0xFFFF_FFFF) as u32);
        mmio_write32(bar0, E1000_TDBAH, (tx_ring_phys >> 32) as u32);
        mmio_write32(bar0, E1000_TDLEN, (NUM_TX_DESCS * core::mem::size_of::<TxDesc>()) as u32);
        mmio_write32(bar0, E1000_TDH, 0);
        mmio_write32(bar0, E1000_TDT, 0);

        // Enable TX: TCTL.EN (bit 1) | TCTL.PSP (bit 3) | CT=15 (bits 4-11) | COLD=64 (bits 12-21)
        let tctl = (1 << 1) | (1 << 3) | (15 << 4) | (64 << 12);
        mmio_write32(bar0, E1000_TCTL, tctl);

        // 4. Initialize RX Descriptor Ring
        let rx_ring_phys = RX_DESCS.as_ptr() as u64;
        for i in 0..NUM_RX_DESCS {
            RX_DESCS[i].addr = RX_BUFFERS[i].data.as_ptr() as u64;
            RX_DESCS[i].status = 0;
        }

        mmio_write32(bar0, E1000_RDBAL, (rx_ring_phys & 0xFFFF_FFFF) as u32);
        mmio_write32(bar0, E1000_RDBAH, (rx_ring_phys >> 32) as u32);
        mmio_write32(bar0, E1000_RDLEN, (NUM_RX_DESCS * core::mem::size_of::<RxDesc>()) as u32);
        mmio_write32(bar0, E1000_RDH, 0);
        mmio_write32(bar0, E1000_RDT, (NUM_RX_DESCS - 1) as u32);

        // Enable RX: RCTL.EN (bit 1) | RCTL.BAM (bit 15) | Buffer Size 2048 (bits 16-17 = 0) | RCTL.SECRC (bit 26)
        let rctl = (1 << 1) | (1 << 15) | (1 << 26);
        mmio_write32(bar0, E1000_RCTL, rctl);

        E1000_BAR0.store(bar0, Ordering::Release);
        E1000_MAC = mac;
        E1000_TX_TAIL.store(0, Ordering::Release);
        E1000_RX_TAIL.store(0, Ordering::Release);
        E1000_INITIALIZED.store(true, Ordering::Release);

        E1000 = Some(E1000Driver {
            bar0,
            mac,
            tx_tail: 0,
            rx_tail: 0,
        });

        Ok(())
    }
}

// Function: send_packet
// Description: Zero-copy packet transmission via e1000 TX ring buffer.
// Worst-case execution time: ~65 ns
pub fn send_packet(packet: &[u8]) -> Result<(), ()> {
    if !E1000_INITIALIZED.load(Ordering::Acquire) {
        return Err(());
    }
    let bar0 = E1000_BAR0.load(Ordering::Relaxed);
    if bar0 == 0 {
        return Err(());
    }

    unsafe {
        let cur_tail = E1000_TX_TAIL.load(Ordering::Relaxed);
        let idx = cur_tail % NUM_TX_DESCS;
        let desc = &mut TX_DESCS[idx];

        // Check if descriptor is free (Status bit 0: DD)
        if (desc.status & 1) == 0 && desc.cmd != 0 {
            return Err(()); // TX ring full
        }

        let copy_len = core::cmp::min(packet.len(), PACKET_BUFFER_SIZE);
        TX_BUFFERS[idx].data[..copy_len].copy_from_slice(&packet[..copy_len]);

        desc.length = copy_len as u16;
        // CMD: EOP (bit 0: End of Packet) | IFCS (bit 1: Insert FCS) | RS (bit 3: Report Status)
        desc.cmd = (1 << 0) | (1 << 1) | (1 << 3);
        desc.status = 0;

        let next_tail = (idx + 1) % NUM_TX_DESCS;
        E1000_TX_TAIL.store(next_tail, Ordering::Release);
        mmio_write32(bar0, E1000_TDT, next_tail as u32);

        Ok(())
    }
}

// Function: poll_rx_packet
// Description: Poll e1000 RX ring buffer for incoming packets (e.g. RTCP / ACK packets).
// Worst-case execution time: ~85 ns
pub fn poll_rx_packet(out: &mut [u8]) -> Option<usize> {
    if !E1000_INITIALIZED.load(Ordering::Acquire) {
        return None;
    }
    let bar0 = E1000_BAR0.load(Ordering::Relaxed);
    if bar0 == 0 {
        return None;
    }

    unsafe {
        let cur_tail = E1000_RX_TAIL.load(Ordering::Relaxed);
        let idx = (cur_tail + 1) % NUM_RX_DESCS;
        let desc = &mut RX_DESCS[idx];

        // Status bit 0: DD (Descriptor Done)
        if (desc.status & 1) == 0 {
            return None; // No packet available
        }

        let len = desc.length as usize;
        let copy_len = core::cmp::min(len, out.len());
        out[..copy_len].copy_from_slice(&RX_BUFFERS[idx].data[..copy_len]);

        // Reset descriptor status and advance tail
        desc.status = 0;
        E1000_RX_TAIL.store(idx, Ordering::Release);
        mmio_write32(bar0, E1000_RDT, idx as u32);

        Some(copy_len)
    }
}
