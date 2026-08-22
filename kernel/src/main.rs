// main.rs - LatencyOS Core 0 Kernel Entry Point & Multi-Core Orchestration
//
// Worst-case execution time: Documented per function.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

mod apic;
mod cstate;
mod crypto;
mod e1000;
mod editor;
mod fs;
mod gpu;
mod lang;
mod latency;
mod net;
mod pci;
mod ring_buffer;
mod serial;
mod shell;
mod smp;
mod tsc;
pub mod vfs;
pub mod export_disk;

use core::panic::PanicInfo;
use core::sync::atomic::Ordering;
use gpu::{capture_frame_zero_copy, poll_vblank_edge, FRAME_HEIGHT, FRAME_WIDTH};
use net::{
    on_ack_received, stream_send_frame, SendError, CONGESTION_RATE_PCT, DELTA_DELAY_NS,
    MIN_RTT_NS,
};
use ring_buffer::CAPTURE_TO_ENCODE_RING;
use smp::{
    boot_application_processors, run_role_loop, CoreRole, CORES_BOOTED, CORE_ROLES, NUM_CORES,
};

// Function: panic
// Description: Custom panic handler for baremetal kernel. Outputs panic details to COM1 and halts.
// Worst-case execution time: ~50_000 ns
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[PANIC] LatencyOS Kernel Panic: {}", info);
    loop {
        unsafe {
            core::arch::asm!("cli; hlt", options(nomem, nostack));
        }
    }
}

// Function: rust_main
// Description: Rust entry point for Core 0 (BSP). Initializes system with minimal boot overhead and enters interactive control shell.
// Worst-case execution time: ~15_000_000 ns (minimal boot initialization)
#[no_mangle]
pub extern "C" fn rust_main(_multiboot_info_addr: usize) -> ! {
    // 1. Initialize UART 16550 serial port (COM1, 115200 baud)
    serial::init_serial();

    // 2. Read initial boot TSC and calibrate frequency (~10ms)
    let boot_tsc = tsc::read_tsc_serialized();
    let tsc_freq_hz = tsc::calibrate_tsc_freq();
    tsc::GLOBAL_TSC_FREQ_HZ.store(tsc_freq_hz, Ordering::Release);
    let boot_ns = tsc::tsc_to_ns(boot_tsc, tsc_freq_hz);

    serial_println!("============================================================================");
    serial_println!("LatencyOS Core0 booted. Timestamp: {} ns (TSC: {})", boot_ns, boot_tsc);
    serial_println!("[INFO] TSC Frequency: {} MHz", tsc_freq_hz / 1_000_000);

    // 3. Hardware CPU Feature Verification (AES-NI & PCLMULQDQ)
    match crypto::check_crypto_cpu_support() {
        Ok(()) => {
            serial_println!("[CPU] Hardware AES-NI and PCLMULQDQ instruction sets: VERIFIED");
        }
        Err(err) => {
            serial_println!("[FATAL] {}", err);
            loop {
                unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); }
            }
        }
    }

    // 4. Initialize Local APIC on Core 0
    apic::init_local_apic();
    let bsp_lapic_id = apic::get_lapic_id();

    // 5. Configure C-State C0 lock on Core 0 and verify via MSR readback
    let cstate_info = cstate::configure_cstate_c0_lock(bsp_lapic_id);
    serial_println!(
        "[C-STATE] Core {}: MSR 0x1A0 (MISC_ENABLE) = {:#x}, MSR 0x1B0 (ENERGY_PERF_BIAS) = {:#x} [C0 LOCKED]",
        bsp_lapic_id,
        cstate_info.misc_enable,
        cstate_info.energy_perf_bias
    );

    CORES_BOOTED[0].store(true, Ordering::Release);

    // 6. PCI Bus Scan & Intel e1000 Poll-Mode Driver (PMD) Initialization
    serial_println!("[PCI] Scanning PCI bus for high-speed network devices...");
    if let Some(pci_dev) = pci::find_e1000_device() {
        serial_println!(
            "[PCI] Found Intel e1000 NIC ({:#06x}:{:#06x}) at Bus {} Slot {} Func {}",
            pci_dev.vendor_id,
            pci_dev.device_id,
            pci_dev.bus,
            pci_dev.slot,
            pci_dev.func
        );
        match e1000::init_e1000(pci_dev.bar0) {
            Ok(()) => {
                let mac = unsafe { e1000::E1000.as_ref().map(|d| d.mac).unwrap_or([0; 6]) };
                serial_println!(
                    "[NET] e1000 Poll-Mode Driver (PMD) initialized. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, MMIO BAR0: {:#x}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], pci_dev.bar0
                );
            }
            Err(e) => {
                serial_println!("[NET] e1000 init error: {}", e);
            }
        }
    } else {
        serial_println!("[NET] WARNING: No e1000 NIC discovered on PCI bus. Initializing fallback mode.");
    }

    // 7. Start Application Processors (Cores 1-3) via APIC INIT-SIPI-SIPI
    serial_println!("[SMP] Starting Application Processors (Cores 1-3)...");
    boot_application_processors();

    // Wait for all 4 cores to boot (with timeout safeguard)
    let mut all_booted = false;
    for _ in 0..100_000 {
        let mut count = 0;
        for i in 0..NUM_CORES {
            if CORES_BOOTED[i].load(Ordering::Acquire) {
                count += 1;
            }
        }
        if count == NUM_CORES {
            all_booted = true;
            break;
        }
        core::hint::spin_loop();
    }

    if all_booted {
        serial_println!("[SMP] All 4 CPU cores successfully booted via APIC.");
    } else {
        serial_println!("[SMP] WARNING: Some AP cores did not signal boot completion in time.");
    }

    // 8. Display Static Core Allocation Table
    serial_println!("----------------------------------------------------------------------------");
    serial_println!("Static Core Role Allocation:");
    for i in 0..NUM_CORES {
        let role = CORE_ROLES[i];
        let booted = if CORES_BOOTED[i].load(Ordering::Acquire) { "BOOTED" } else { "PENDING" };
        serial_println!(
            "  Core {}: {:18} | Status: {:7} | Target Budget: {}",
            i,
            role.name(),
            booted,
            role.budget_target()
        );
    }
    serial_println!("----------------------------------------------------------------------------");

    // 9. Quick 1-Frame Sanity Check (GPU Capture Domain)
    let vblank_start = poll_vblank_edge(500);
    let test_handle = capture_frame_zero_copy(0, 1, vblank_start);
    let vblank_to_capture_ns = tsc::tsc_to_ns(test_handle.capture_done_tsc.saturating_sub(vblank_start), tsc_freq_hz);

    serial_println!(
        "[GPU] Frame Initialized: {}x{} @ 32bpp, CRC32: {:#010x}, Capture Latency: {} ns",
        FRAME_WIDTH,
        FRAME_HEIGHT,
        test_handle.crc32,
        vblank_to_capture_ns
    );

    let _ = CAPTURE_TO_ENCODE_RING.push(test_handle);

    // 10. Quick 1-Frame Sanity Check (Network Domain)
    let mut pkt_seq = 1u16;
    let future_deadline = tsc::read_tsc_serialized() + tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
    let send_start_tsc = tsc::read_tsc_serialized();
    let send_result = stream_send_frame(&test_handle, future_deadline, &mut pkt_seq);
    let send_end_tsc = tsc::read_tsc_serialized();
    let net_send_lat_ns = tsc::tsc_to_ns(send_end_tsc - send_start_tsc, tsc_freq_hz);

    if let Ok(bytes) = send_result {
        serial_println!(
            "[NET] SRTP Stream Check: {} bytes sent (AES-128-GCM), Latency: {} ns",
            bytes,
            net_send_lat_ns
        );
    }

    // Quick Deadline Drop Check
    let expired_deadline = tsc::read_tsc_serialized().saturating_sub(1_000_000);
    let drop_result = stream_send_frame(&test_handle, expired_deadline, &mut pkt_seq);
    if drop_result == Err(SendError::DeadlineExceeded) {
        serial_println!("[NET] Deadline Drop Check: PASSED");
    }

    // Quick Congestion Controller Init
    let t_sim_tx = tsc::read_tsc_serialized().saturating_sub(200_000);
    on_ack_received(t_sim_tx, tsc_freq_hz);
    serial_println!(
        "[CONGESTION] Controller Initialized: Min RTT = {} ns, Delta Delay = {} ns, Rate = {}%",
        MIN_RTT_NS.load(Ordering::Relaxed),
        DELTA_DELAY_NS.load(Ordering::Relaxed),
        CONGESTION_RATE_PCT.load(Ordering::Relaxed)
    );

    // 12. Initialize LatencyFS in-memory filesystem
    fs::fs_init();

    // 13. Initialize and start Core 0 interactive control shell
    shell::init_shell();

    // Signal all AP cores to begin executing their dedicated real-time loops
    smp::START_SIGNAL.store(true, Ordering::Release);

    // Core 0 enters its designated Control domain loop (which runs shell::poll_shell)
    run_role_loop(0, CoreRole::Control);
}
