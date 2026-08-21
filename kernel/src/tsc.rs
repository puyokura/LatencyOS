// tsc.rs - Time Stamp Counter (TSC) Driver & High-Precision Calibration
//
// Worst-case execution time: Documented per function.

use crate::serial::{inb, outb};
use core::sync::atomic::AtomicU64;

pub static GLOBAL_TSC_FREQ_HZ: AtomicU64 = AtomicU64::new(2_500_000_000);

// Function: read_tsc
// Description: Read the 64-bit Time Stamp Counter via RDTSC instruction.
// Worst-case execution time: ~15 ns
#[inline]
#[allow(dead_code)]
pub fn read_tsc() -> u64 {
    unsafe {
        let low: u32;
        let high: u32;
        core::arch::asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
        ((high as u64) << 32) | (low as u64)
    }
}

// Function: read_tsc_serialized
// Description: Read the 64-bit Time Stamp Counter with pipeline serialization via LFENCE + RDTSC.
// Worst-case execution time: ~25 ns
#[inline]
pub fn read_tsc_serialized() -> u64 {
    unsafe {
        let low: u32;
        let high: u32;
        core::arch::asm!(
            "lfence",
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
        ((high as u64) << 32) | (low as u64)
    }
}

// Function: calibrate_tsc_freq
// Description: Calibrate TSC frequency (Hz) using 8254 PIT (Programmable Interval Timer) Channel 2 over 10ms.
// Worst-case execution time: ~10_050_000 ns (executed once during boot)
pub fn calibrate_tsc_freq() -> u64 {
    const PIT_FREQ_HZ: u64 = 1_193_182;
    const CALIBRATION_MS: u64 = 10;
    const PIT_TICKS: u16 = (PIT_FREQ_HZ * CALIBRATION_MS / 1000) as u16; // 11931 ticks for 10ms

    unsafe {
        // 1. Configure PIT Channel 2 in Mode 0 (Interrupt on terminal count / one-shot)
        // 0xB0 = Channel 2 (10), Access: lobyte/hibyte (11), Mode 0 (000), Binary (0)
        outb(0x43, 0xB0);

        // 2. Load count into Channel 2 data port (0x42)
        outb(0x42, (PIT_TICKS & 0xFF) as u8);
        outb(0x42, ((PIT_TICKS >> 8) & 0xFF) as u8);

        // 3. Reset PIT Channel 2 Gate (Port 0x61 bit 0 = 0)
        let port_61 = inb(0x61);
        outb(0x61, port_61 & 0xFE);

        // 4. Start PIT Channel 2 count by enabling Gate (Port 0x61 bit 0 = 1)
        outb(0x61, (port_61 & 0xFE) | 0x01);

        let start_tsc = read_tsc_serialized();

        // 5. Poll Port 0x61 bit 5 (PIT Channel 2 Output status) until it goes high (count expired)
        // Safeguard with max loop count to prevent deadlock if hardware/emulator doesn't support PIT Channel 2
        let mut loop_count: u32 = 0;
        const MAX_LOOPS: u32 = 50_000_000;

        while (inb(0x61) & 0x20) == 0 {
            loop_count = loop_count.wrapping_add(1);
            if loop_count >= MAX_LOOPS {
                break;
            }
            core::hint::spin_loop();
        }

        let end_tsc = read_tsc_serialized();

        // 6. Disable PIT Channel 2 Gate
        outb(0x61, port_61 & 0xFE);

        if loop_count < MAX_LOOPS && end_tsc > start_tsc {
            let delta_cycles = end_tsc - start_tsc;
            // Frequency = (delta_cycles * 1000) / CALIBRATION_MS
            delta_cycles * (1000 / CALIBRATION_MS)
        } else {
            // Fallback nominal 2.5 GHz if PIT calibration timed out
            2_500_000_000
        }
    }
}

// Function: tsc_to_ns
// Description: Convert TSC cycle count to nanoseconds given a TSC frequency in Hz.
// Worst-case execution time: ~30 ns
#[inline]
pub fn tsc_to_ns(tsc_cycles: u64, freq_hz: u64) -> u64 {
    if freq_hz == 0 {
        return 0;
    }
    let cycles_u128 = tsc_cycles as u128;
    let freq_u128 = freq_hz as u128;
    ((cycles_u128 * 1_000_000_000) / freq_u128) as u64
}

// Function: ns_to_tsc
// Description: Convert nanoseconds to TSC cycle count given a TSC frequency in Hz.
// Worst-case execution time: ~30 ns
#[inline]
pub fn ns_to_tsc(ns: u64, freq_hz: u64) -> u64 {
    let ns_u128 = ns as u128;
    let freq_u128 = freq_hz as u128;
    ((ns_u128 * freq_u128) / 1_000_000_000) as u64
}
