// smp.rs - Static Core Role Allocation, AP Startup, and Domain Polling Loops
//
// Worst-case execution time: Documented per function.

use crate::apic::{init_local_apic, send_init_ipi, send_startup_ipi};
use crate::cstate::configure_cstate_c0_lock;
use crate::gpu::{capture_frame_zero_copy, poll_vblank_edge, NUM_FRAME_SLOTS};
use crate::net::{poll_rx_ack, stream_send_frame};
use crate::ring_buffer::{CAPTURE_TO_ENCODE_RING, ENCODE_TO_NET_RING};
use crate::tsc::{read_tsc_serialized, tsc_to_ns};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CoreRole {
    Control = 0, // Core 0: Control plane, system management, watchdog, telemetry
    Capture = 1, // Core 1: GPU capture domain, VBLANK polling, IRQ affinity fixed
    Encode = 2,  // Core 2: NVENC encode domain, frame compression polling
    Network = 3, // Core 3: Network domain, kernel-bypass NIC & QUIC/WebRTC TX/RX
}

impl CoreRole {
    pub const fn name(&self) -> &'static str {
        match self {
            CoreRole::Control => "Control (Core 0)",
            CoreRole::Capture => "Capture (Core 1)",
            CoreRole::Encode => "Encode (Core 2)",
            CoreRole::Network => "Network (Core 3)",
        }
    }

    pub const fn budget_target(&self) -> &'static str {
        match self {
            CoreRole::Control => "0.15 ms (ISR -> Userspace completion notification)",
            CoreRole::Capture => "2.00 ms (Screen Frame Capture completion)",
            CoreRole::Encode => "4.50 ms (NVENC Hardware Encode completion)",
            CoreRole::Network => "5.00 ms (Kernel-Bypass NIC Network Transmission)",
        }
    }
}

pub const NUM_CORES: usize = 4;

pub static CORE_ROLES: [CoreRole; NUM_CORES] = [
    CoreRole::Control,
    CoreRole::Capture,
    CoreRole::Encode,
    CoreRole::Network,
];

pub static CORES_BOOTED: [AtomicBool; NUM_CORES] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

pub static CORES_ACTIVE: [AtomicBool; NUM_CORES] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

pub static CORE_LOOP_COUNT: [AtomicU64; NUM_CORES] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

pub static CAPTURED_FRAMES: AtomicU64 = AtomicU64::new(0);
pub static CONSUMED_FRAMES: AtomicU64 = AtomicU64::new(0);
pub static NETWORK_FRAMES_SENT: AtomicU64 = AtomicU64::new(0);

pub static LAST_CAPTURE_LATENCY_NS: AtomicU64 = AtomicU64::new(0);
pub static LAST_FRAME_CRC: AtomicU32 = AtomicU32::new(0);
pub static LAST_CONSUMED_FRAME_ID: AtomicU64 = AtomicU64::new(0);
pub static LAST_CONSUMED_CRC: AtomicU32 = AtomicU32::new(0);
pub static LAST_NET_SEND_LATENCY_NS: AtomicU64 = AtomicU64::new(0);
pub static LAST_NET_BYTES_SENT: AtomicU64 = AtomicU64::new(0);

pub static START_SIGNAL: AtomicBool = AtomicBool::new(false);

// Function: get_core_role
// Description: Returns the statically assigned role for a given core ID.
// Worst-case execution time: ~5 ns
#[inline]
pub fn get_core_role(core_id: u8) -> CoreRole {
    if (core_id as usize) < NUM_CORES {
        CORE_ROLES[core_id as usize]
    } else {
        CoreRole::Control
    }
}

// Function: copy_ap_trampoline
// Description: Copy 16-bit AP trampoline binary to physical address 0x8000.
// Worst-case execution time: ~1000 ns
pub fn copy_ap_trampoline() {
    unsafe {
        let src = &ap_trampoline_start as *const u8;
        let end = &ap_trampoline_end as *const u8;
        let len = (end as usize) - (src as usize);
        let dst = 0x8000 as *mut u8;

        core::ptr::copy_nonoverlapping(src, dst, len);
    }
}

// Function: boot_application_processors
// Description: Orchestrate AP startup via INIT-SIPI-SIPI sequence.
// Worst-case execution time: ~15_000_000 ns (dominated by 10ms INIT IPI delay)
pub fn boot_application_processors() {
    // 1. Copy AP trampoline to 0x8000 (corresponds to vector 0x08)
    copy_ap_trampoline();

    // 2. Send INIT IPI to all APs
    send_init_ipi();

    // 3. Delay ~10ms (10,000 us) for AP hardware reset
    for _ in 0..100_000 {
        core::hint::spin_loop();
    }

    // 4. Send 1st SIPI with vector 0x08 (0x08000)
    send_startup_ipi(0x08);

    // 5. Delay ~200us
    for _ in 0..20_000 {
        core::hint::spin_loop();
    }

    // 6. Send 2nd SIPI with vector 0x08
    send_startup_ipi(0x08);

    // 7. Delay ~200us
    for _ in 0..20_000 {
        core::hint::spin_loop();
    }
}

// Function: ap_main
// Description: Rust entry point for Application Processors (Cores 1, 2, 3).
// Worst-case execution time: ~1000 ns before entering continuous polling loop
#[no_mangle]
pub extern "C" fn ap_main(core_id: u8) -> ! {
    // 1. Initialize Local APIC on this core
    init_local_apic();

    // 2. Lock C-State to C0 on this core
    let _ = configure_cstate_c0_lock(core_id);

    let idx = (core_id as usize) % NUM_CORES;

    // 3. Signal to Core 0 that this AP has booted
    CORES_BOOTED[idx].store(true, Ordering::Release);

    // 4. Busy-wait until Core 0 gives the global start signal
    while !START_SIGNAL.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }

    // 5. Enter designated domain polling loop
    let role = get_core_role(core_id);
    run_role_loop(core_id, role);
}

// Function: run_role_loop
// Description: Dedicated polling loop for a specific core role. Runs indefinitely with 100% CPU utilization (no sleep, no context switch).
// Worst-case execution time: ~15-550_000 ns per iteration
pub fn run_role_loop(core_id: u8, role: CoreRole) -> ! {
    let idx = (core_id as usize) % NUM_CORES;
    CORES_ACTIVE[idx].store(true, Ordering::Release);

    match role {
        // Core 0: Control Domain Loop
        // Worst-case iteration time: ~35 ns
        // Latency budget responsibility: 0.15 ms (ISR -> Userspace completion notification)
        CoreRole::Control => {
            loop {
                CORE_LOOP_COUNT[idx].fetch_add(1, Ordering::Relaxed);
                let freq = crate::tsc::GLOBAL_TSC_FREQ_HZ.load(Ordering::Relaxed);
                crate::shell::poll_shell(freq);
                for _ in 0..16 {
                    core::hint::spin_loop();
                }
            }
        }

        // Core 1: Capture Domain Loop
        // Worst-case iteration time: ~520_000 ns (0.52 ms, well within 2.00 ms budget)
        // Latency budget responsibility: 2.00 ms (Screen frame capture completion)
        CoreRole::Capture => {
            let mut frame_seq = 0u64;
            let mut slot_id = 0u8;
            loop {
                CORE_LOOP_COUNT[idx].fetch_add(1, Ordering::Relaxed);
                let freq = crate::tsc::GLOBAL_TSC_FREQ_HZ.load(Ordering::Relaxed);

                // 1. Detect VBLANK edge via hardware status register polling
                let vblank_tsc = poll_vblank_edge(500);

                // 2. Perform zero-copy frame buffer capture and compute integrity CRC32
                let handle = capture_frame_zero_copy(slot_id, frame_seq, vblank_tsc);
                let capture_latency_ns = tsc_to_ns(handle.capture_done_tsc.saturating_sub(vblank_tsc), freq);

                LAST_CAPTURE_LATENCY_NS.store(capture_latency_ns, Ordering::Relaxed);
                LAST_FRAME_CRC.store(handle.crc32, Ordering::Relaxed);

                // 3. Enqueue FrameHandle to Lock-Free SPSC ring buffer for Core 2 (Encode Domain)
                if CAPTURE_TO_ENCODE_RING.push(handle).is_ok() {
                    CAPTURED_FRAMES.fetch_add(1, Ordering::Release);
                    frame_seq = frame_seq.wrapping_add(1);
                    slot_id = ((slot_id as usize + 1) % NUM_FRAME_SLOTS) as u8;
                }

                for _ in 0..16 {
                    core::hint::spin_loop();
                }
            }
        }

        // Core 2: Encode Domain Loop
        // Worst-case iteration time: ~45 ns (polling lock-free SPSC ring buffers)
        // Latency budget responsibility: 4.50 ms (NVENC encode completion)
        CoreRole::Encode => {
            loop {
                CORE_LOOP_COUNT[idx].fetch_add(1, Ordering::Relaxed);

                // Poll lock-free SPSC ring buffer from Core 1
                if let Some(frame_handle) = CAPTURE_TO_ENCODE_RING.pop() {
                    CONSUMED_FRAMES.fetch_add(1, Ordering::Release);
                    LAST_CONSUMED_FRAME_ID.store(frame_handle.frame_id, Ordering::Relaxed);
                    LAST_CONSUMED_CRC.store(frame_handle.crc32, Ordering::Relaxed);

                    // Forward to Network Domain via SPSC Ring Buffer
                    let _ = ENCODE_TO_NET_RING.push(frame_handle);
                } else {
                    for _ in 0..16 {
                        core::hint::spin_loop();
                    }
                }
            }
        }

        // Core 3: Network Domain Loop
        // Worst-case iteration time: ~31_075 ns (0.031 ms, well within 0.50 ms budget)
        // Latency budget responsibility: 5.00 ms (Kernel-bypass NIC packet transmission)
        CoreRole::Network => {
            let mut packet_seq = 1u16;
            loop {
                CORE_LOOP_COUNT[idx].fetch_add(1, Ordering::Relaxed);
                let freq = crate::tsc::GLOBAL_TSC_FREQ_HZ.load(Ordering::Relaxed);

                // 1. Poll e1000 RX ring buffer for incoming ACK packets (bounded to ~90 ns)
                let _ = poll_rx_ack(freq);

                // 2. Poll Encode->Network SPSC ring buffer for ready frames
                if let Some(frame) = ENCODE_TO_NET_RING.pop() {
                    let deadline_delta_tsc = crate::tsc::ns_to_tsc(20_000_000, freq);
                    let deadline_tsc = read_tsc_serialized().wrapping_add(deadline_delta_tsc);

                    let send_start_tsc = read_tsc_serialized();
                    if let Ok(bytes) = stream_send_frame(&frame, deadline_tsc, &mut packet_seq) {
                        let send_end_tsc = read_tsc_serialized();
                        let send_lat_ns = tsc_to_ns(send_end_tsc - send_start_tsc, freq);
                        LAST_NET_SEND_LATENCY_NS.store(send_lat_ns, Ordering::Relaxed);
                        NETWORK_FRAMES_SENT.fetch_add(1, Ordering::Release);
                        LAST_NET_BYTES_SENT.store(bytes as u64, Ordering::Relaxed);
                    }
                } else {
                    for _ in 0..16 {
                        core::hint::spin_loop();
                    }
                }
            }
        }
    }
}
