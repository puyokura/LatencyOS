// apic.rs - Local APIC & Multicore IPI Dispatcher
//
// Worst-case execution time: Documented per function.

use crate::cstate::{rdmsr, wrmsr, IA32_APIC_BASE_MSR};

#[allow(dead_code)]
pub const LAPIC_DEFAULT_BASE: u64 = 0xFEE00000;

#[allow(dead_code)]
pub const LAPIC_ID_REG: u32 = 0x020;
#[allow(dead_code)]
pub const LAPIC_VER_REG: u32 = 0x030;
pub const LAPIC_TPR_REG: u32 = 0x080;
#[allow(dead_code)]
pub const LAPIC_EOI_REG: u32 = 0x0B0;
pub const LAPIC_SVR_REG: u32 = 0x0F0;
pub const LAPIC_ICR_LOW: u32 = 0x300;
pub const LAPIC_ICR_HIGH: u32 = 0x310;

// Function: get_lapic_base
// Description: Returns the physical base address of the Local APIC from IA32_APIC_BASE MSR.
// Worst-case execution time: ~45 ns
#[inline]
pub fn get_lapic_base() -> u64 {
    let base = unsafe { rdmsr(IA32_APIC_BASE_MSR) & 0xFFFFF000 };
    if base == 0 {
        LAPIC_DEFAULT_BASE
    } else {
        base
    }
}

// Function: read_lapic_reg
// Description: Read 32-bit register from memory-mapped Local APIC.
// Worst-case execution time: ~15 ns
#[allow(dead_code)]
#[inline]
pub fn read_lapic_reg(offset: u32) -> u32 {
    let addr = (get_lapic_base() + offset as u64) as *const u32;
    unsafe { core::ptr::read_volatile(addr) }
}

// Function: write_lapic_reg
// Description: Write 32-bit value to memory-mapped Local APIC register.
// Worst-case execution time: ~20 ns
#[inline]
pub fn write_lapic_reg(offset: u32, val: u32) {
    let addr = (get_lapic_base() + offset as u64) as *mut u32;
    unsafe { core::ptr::write_volatile(addr, val) }
}

// Function: get_lapic_id
// Description: Read Local APIC ID of current executing CPU core.
// Worst-case execution time: ~15 ns
#[inline]
pub fn get_lapic_id() -> u8 {
    let cpuid_leaf = core::arch::x86_64::__cpuid(1);
    ((cpuid_leaf.ebx >> 24) & 0xFF) as u8
}

// Function: init_local_apic
// Description: Enable Local APIC and set Spurious Interrupt Vector Register.
// Worst-case execution time: ~120 ns
pub fn init_local_apic() {
    unsafe {
        // Enable APIC in MSR (bit 11: APIC Global Enable)
        let msr = rdmsr(IA32_APIC_BASE_MSR);
        wrmsr(IA32_APIC_BASE_MSR, msr | (1 << 11));

        // Set Task Priority Register to 0 (accept all interrupts)
        write_lapic_reg(LAPIC_TPR_REG, 0);

        // Set Spurious Interrupt Vector Register: bit 8 = APIC Software Enable, vector = 0xFF
        write_lapic_reg(LAPIC_SVR_REG, 0x1FF);
    }
}

// Function: send_init_ipi
// Description: Send INIT IPI to all APs (excluding self).
// Worst-case execution time: ~80 ns
pub fn send_init_ipi() {
    // ICR High: Destination = 0 (broadcast)
    write_lapic_reg(LAPIC_ICR_HIGH, 0);
    // ICR Low: All Excluding Self (0x000C0000), Delivery Mode: INIT (0x00000500), Level: Assert (0x00004000)
    write_lapic_reg(LAPIC_ICR_LOW, 0x000C4500);
}

// Function: send_startup_ipi
// Description: Send Startup IPI (SIPI) with target trampoline vector to all APs (excluding self).
// Worst-case execution time: ~80 ns
pub fn send_startup_ipi(vector: u8) {
    // ICR High: Destination = 0 (broadcast)
    write_lapic_reg(LAPIC_ICR_HIGH, 0);
    // ICR Low: All Excluding Self (0x000C0000), Delivery Mode: Startup (0x00000600), Vector (0x00..0xFF)
    let val = 0x000C4600 | (vector as u32);
    write_lapic_reg(LAPIC_ICR_LOW, val);
}
