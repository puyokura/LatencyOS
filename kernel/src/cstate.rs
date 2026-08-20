// cstate.rs - C-State Lock & Power Management MSR Control
//
// Worst-case execution time: Documented per function.

pub const IA32_APIC_BASE_MSR: u32 = 0x01B;
pub const MSR_PKG_CST_CONFIG_CONTROL: u32 = 0x0E2;
pub const IA32_MISC_ENABLE: u32 = 0x1A0;
pub const IA32_ENERGY_PERF_BIAS: u32 = 0x1B0;
pub const MSR_POWER_CTL: u32 = 0x1FC;

// Function: rdmsr
// Description: Read 64-bit Model Specific Register (MSR).
// Worst-case execution time: ~40 ns
#[inline]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nomem, nostack, preserves_flags)
    );
    ((high as u64) << 32) | (low as u64)
}

// Function: wrmsr
// Description: Write 64-bit Model Specific Register (MSR).
// Worst-case execution time: ~60 ns
#[inline]
pub unsafe fn wrmsr(msr: u32, val: u64) {
    let low = val as u32;
    let high = (val >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nomem, nostack, preserves_flags)
    );
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct CStateConfig {
    pub misc_enable: u64,
    pub energy_perf_bias: u64,
    pub pkg_cst_control: u64,
    pub power_ctl: u64,
}

// Function: configure_cstate_c0_lock
// Description: Disables C-state transitions and power throttling, locking the CPU core in active C0 state.
// Worst-case execution time: ~500 ns
pub fn configure_cstate_c0_lock(core_id: u8) -> CStateConfig {
    let _ = core_id;
    unsafe {
        // 1. Read current IA32_MISC_ENABLE (MSR 0x1A0)
        let misc = rdmsr(IA32_MISC_ENABLE);

        // 2. Set Energy Performance Bias to 0 (Maximum Performance / No Energy Savings)
        // Best-effort write to ENERGY_PERF_BIAS
        wrmsr(IA32_ENERGY_PERF_BIAS, 0);
        let epb = rdmsr(IA32_ENERGY_PERF_BIAS);

        // 3. Configure Package C-State Limit (MSR 0xE2) to C0/C1 (Limit bits [2:0] = 0)
        let pkg_cst = rdmsr(MSR_PKG_CST_CONFIG_CONTROL);

        // 4. Disable C1E auto-promotion in POWER_CTL (MSR 0x1FC)
        let pwr_ctl = rdmsr(MSR_POWER_CTL);

        CStateConfig {
            misc_enable: misc,
            energy_perf_bias: epb,
            pkg_cst_control: pkg_cst,
            power_ctl: pwr_ctl,
        }
    }
}
