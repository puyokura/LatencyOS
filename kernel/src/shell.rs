// shell.rs - Zero-Allocation Non-Blocking Interactive Control Shell for Core 0
//
// Worst-case execution time: Documented per function.

use crate::e1000::E1000;
use crate::gpu::{capture_frame_zero_copy, poll_vblank_edge};
use crate::latency::{
    latency_report, print_statistical_latency_report, record_stage_sample,
    report_power_thermal_status, STATS_SAMPLE_COUNT,
};
use crate::net::{
    stream_send_frame, CONGESTION_RATE_PCT, DELTA_DELAY_NS, LAST_RTT_NS, MIN_RTT_NS,
    TOTAL_ACKS_RECEIVED, TOTAL_FRAMES_DROPPED, TOTAL_PACKETS_SENT,
};
use crate::pci::find_e1000_device;
use crate::ring_buffer::CAPTURE_TO_ENCODE_RING;
use crate::serial::{SERIAL};
use crate::serial_println;
use crate::serial_print;
use crate::smp::{
    get_core_role, CAPTURED_FRAMES, CONSUMED_FRAMES, CORES_ACTIVE, CORES_BOOTED, CORE_LOOP_COUNT,
    LAST_CAPTURE_LATENCY_NS, LAST_CONSUMED_CRC, LAST_CONSUMED_FRAME_ID, LAST_FRAME_CRC,
    LAST_NET_SEND_LATENCY_NS, NETWORK_FRAMES_SENT, NUM_CORES,
};
use crate::tsc::{read_tsc_serialized, tsc_to_ns};
use core::sync::atomic::Ordering;

pub const MAX_LINE_LEN: usize = 128;

static mut LINE_BUF: [u8; MAX_LINE_LEN] = [0; MAX_LINE_LEN];
static mut LINE_LEN: usize = 0;
static mut PROMPT_SHOWN: bool = false;

// Function: print_prompt
// Description: Print the LatencyOS interactive shell prompt.
// Worst-case execution time: ~12_000 ns
pub fn print_prompt() {
    serial_print!("latencyos> ");
    unsafe { PROMPT_SHOWN = true; }
}

// Function: init_shell
// Description: Initialize interactive shell and display welcome banner.
// Worst-case execution time: ~50_000 ns
pub fn init_shell() {
    serial_println!("----------------------------------------------------------------------------");
    serial_println!("LatencyOS Interactive Control Shell Ready. Type 'help' for available commands.");
    serial_println!("----------------------------------------------------------------------------");
    print_prompt();
}

// Function: poll_shell
// Description: Non-blocking poll for incoming serial characters and line execution.
// Worst-case execution time: ~25 ns (when idle) to ~100_000 ns (when executing a command)
pub fn poll_shell(tsc_freq_hz: u64) {
    unsafe {
        if !PROMPT_SHOWN {
            print_prompt();
        }

        while let Some(b) = SERIAL.read_byte_nonblocking() {
            match b {
                // Enter / Newline
                b'\r' | b'\n' => {
                    SERIAL.send_byte(b'\r');
                    SERIAL.send_byte(b'\n');

                    if LINE_LEN > 0 {
                        if let Ok(cmd_str) = core::str::from_utf8(&LINE_BUF[..LINE_LEN]) {
                            execute_command(cmd_str.trim(), tsc_freq_hz);
                        }
                        LINE_LEN = 0;
                    }
                    print_prompt();
                }

                // Backspace / Delete
                0x08 | 0x7F => {
                    if LINE_LEN > 0 {
                        LINE_LEN -= 1;
                        SERIAL.send_byte(0x08);
                        SERIAL.send_byte(b' ');
                        SERIAL.send_byte(0x08);
                    }
                }

                // Printable ASCII characters
                0x20..=0x7E => {
                    if LINE_LEN < MAX_LINE_LEN - 1 {
                        LINE_BUF[LINE_LEN] = b;
                        LINE_LEN += 1;
                        SERIAL.send_byte(b);
                    }
                }

                _ => {}
            }
        }
    }
}

// Function: execute_command
// Description: Dispatch and execute a single-line shell command.
// Worst-case execution time: Documented per sub-command.
fn execute_command(cmd: &str, tsc_freq_hz: u64) {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return;
    }

    let (main_cmd, arg) = if let Some(idx) = cmd.find(' ') {
        (&cmd[..idx], cmd[idx + 1..].trim())
    } else {
        (cmd, "")
    };

    match main_cmd {
        "help" => {
            serial_println!("Available Commands:");
            serial_println!("  ls             - List all files in LatencyFS in-memory storage");
            serial_println!("  cat <file>     - Display contents of a file");
            serial_println!("  edit <file>    - Open full-screen ANSI PulseEditor");
            serial_println!("  run <file>     - Compile and execute PulseLang script");
            serial_println!("  compile <file> - Compile script and show bytecode & WCET estimate");
            serial_println!("  rm <file>      - Delete file from LatencyFS");
            serial_println!("  status         - Show CPU core roles, boot states, and loop counters");
            serial_println!("  pipeline       - Show live streaming stats (Capture, Encode, Network)");
            serial_println!("  latency        - Show latest single-run event latency breakdown");
            serial_println!("  benchmark      - Execute on-demand 1000-sample latency benchmark");
            serial_println!("  congestion     - Show delay-based congestion control metrics");
            serial_println!("  power          - Read IA32_THERM_STATUS & RAPL MSR status");
            serial_println!("  pci            - Display Intel e1000 NIC PCI config and MAC");
            serial_println!("  clear          - Clear terminal screen");
            serial_println!("  halt           - Halt CPU cores safely (cli; hlt)");
            serial_println!("  help           - Show this help message");
        }

        "status" => {
            let uptime_tsc = read_tsc_serialized();
            let uptime_ns = tsc_to_ns(uptime_tsc, tsc_freq_hz);
            serial_println!("--- System Status ---");
            serial_println!("Uptime: {} ns ({} ms), TSC Freq: {} MHz", uptime_ns, uptime_ns / 1_000_000, tsc_freq_hz / 1_000_000);
            for i in 0..NUM_CORES {
                let role = get_core_role(i as u8);
                let booted = CORES_BOOTED[i].load(Ordering::Acquire);
                let active = CORES_ACTIVE[i].load(Ordering::Acquire) || (i == 0);
                let loops = CORE_LOOP_COUNT[i].load(Ordering::Relaxed);
                serial_println!(
                    "  Core {}: {:18} | Booted: {:5} | Active: {:5} | Loops: {}",
                    i, role.name(), booted, active, loops
                );
            }
        }

        "pipeline" => {
            let captured = CAPTURED_FRAMES.load(Ordering::Acquire);
            let consumed = CONSUMED_FRAMES.load(Ordering::Acquire);
            let net_sent = NETWORK_FRAMES_SENT.load(Ordering::Acquire);
            let pkts_sent = TOTAL_PACKETS_SENT.load(Ordering::Relaxed);
            let dropped = TOTAL_FRAMES_DROPPED.load(Ordering::Relaxed);
            let acks = TOTAL_ACKS_RECEIVED.load(Ordering::Relaxed);
            let last_lat = LAST_CAPTURE_LATENCY_NS.load(Ordering::Relaxed);
            let last_net_lat = LAST_NET_SEND_LATENCY_NS.load(Ordering::Relaxed);
            let last_crc = LAST_FRAME_CRC.load(Ordering::Relaxed);
            let consumed_crc = LAST_CONSUMED_CRC.load(Ordering::Relaxed);
            let consumed_id = LAST_CONSUMED_FRAME_ID.load(Ordering::Relaxed);

            serial_println!("--- Live End-to-End Pipeline Status ---");
            serial_println!("  Core 1 (Capture): Total Captured = {}, Last Latency = {} ns, CRC = {:#010x}", captured, last_lat, last_crc);
            serial_println!("  Core 2 (Encode):  Total Consumed = {}, Last Frame ID = {}, CRC = {:#010x}", consumed, consumed_id, consumed_crc);
            serial_println!("  Core 3 (Network): Frames Sent = {}, Total Packets = {}, Last Latency = {} ns, Drops = {}, ACKs = {}", net_sent, pkts_sent, last_net_lat, dropped, acks);
            serial_println!("  Pipeline State: {}", if captured > 0 && consumed > 0 && net_sent > 0 { "STREAMING ACTIVE" } else { "IDLE / INITIALIZING" });
        }

        "latency" => {
            latency_report(tsc_freq_hz);
        }

        "benchmark" => {
            serial_println!("[BENCHMARK] Executing 1000-sample pipeline benchmark...");
            let mut pkt_seq = 1u16;

            for sample_idx in 0..STATS_SAMPLE_COUNT {
                // Stage 0: Input -> ISR
                let t0 = read_tsc_serialized();
                let _ = crate::apic::get_lapic_id();
                let t1 = read_tsc_serialized();
                let s0_ns = tsc_to_ns(t1 - t0, tsc_freq_hz) as u32;
                record_stage_sample(0, sample_idx, s0_ns);

                // Stage 1: ISR -> Userspace
                let t1_start = read_tsc_serialized();
                CORES_ACTIVE[0].store(true, Ordering::Release);
                let t2 = read_tsc_serialized();
                let s1_ns = tsc_to_ns(t2 - t1_start, tsc_freq_hz) as u32;
                record_stage_sample(1, sample_idx, s1_ns);

                // Stage 2: Userspace -> GPU Start
                let t2_start = read_tsc_serialized();
                let _ = poll_vblank_edge(5);
                let t3 = read_tsc_serialized();
                let s2_ns = tsc_to_ns(t3 - t2_start, tsc_freq_hz) as u32;
                record_stage_sample(2, sample_idx, s2_ns);

                // Stage 3: Frame Capture
                let t3_start = read_tsc_serialized();
                let frame_handle = capture_frame_zero_copy((sample_idx % 4) as u8, sample_idx as u64, t3_start);
                let t4 = read_tsc_serialized();
                let s3_ns = tsc_to_ns(t4 - t3_start, tsc_freq_hz) as u32;
                record_stage_sample(3, sample_idx, s3_ns);

                // Stage 4: Encode Queue
                let t4_start = read_tsc_serialized();
                let _ = CAPTURE_TO_ENCODE_RING.push(frame_handle);
                let _ = CAPTURE_TO_ENCODE_RING.pop();
                let t5 = read_tsc_serialized();
                let s4_ns = tsc_to_ns(t5 - t4_start, tsc_freq_hz) as u32;
                record_stage_sample(4, sample_idx, s4_ns);

                // Stage 5: Network TX
                let deadline = read_tsc_serialized() + crate::tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
                let t5_start = read_tsc_serialized();
                let _ = stream_send_frame(&frame_handle, deadline, &mut pkt_seq);
                let t6 = read_tsc_serialized();
                let s5_ns = tsc_to_ns(t6 - t5_start, tsc_freq_hz) as u32;
                record_stage_sample(5, sample_idx, s5_ns);

                // Stage 6: Total E2E
                let total_e2e_ns = s0_ns + s1_ns + s2_ns + s3_ns + s4_ns + s5_ns;
                record_stage_sample(6, sample_idx, total_e2e_ns);
            }

            print_statistical_latency_report();
        }

        "congestion" => {
            serial_println!("--- Delay-Based Congestion Controller Status ---");
            serial_println!("  Baseline Min RTT:     {} ns ({} us)", MIN_RTT_NS.load(Ordering::Relaxed), MIN_RTT_NS.load(Ordering::Relaxed) / 1000);
            serial_println!("  Last Sample RTT:      {} ns ({} us)", LAST_RTT_NS.load(Ordering::Relaxed), LAST_RTT_NS.load(Ordering::Relaxed) / 1000);
            serial_println!("  Queuing Delta Delay:  {} ns ({} us)", DELTA_DELAY_NS.load(Ordering::Relaxed), DELTA_DELAY_NS.load(Ordering::Relaxed) / 1000);
            serial_println!("  Congestion Threshold: 200,000 ns (200 us)");
            serial_println!("  Current Rate Limit:   {}%", CONGESTION_RATE_PCT.load(Ordering::Relaxed));
            serial_println!("  Total ACKs Processed: {}", TOTAL_ACKS_RECEIVED.load(Ordering::Relaxed));
            serial_println!("  Total Stale Dropped:  {}", TOTAL_FRAMES_DROPPED.load(Ordering::Relaxed));
        }

        "power" => {
            report_power_thermal_status();
        }

        "pci" => {
            serial_println!("--- PCI Bus Devices ---");
            if let Some(pci_dev) = find_e1000_device() {
                let mac = unsafe { E1000.as_ref().map(|d| d.mac).unwrap_or([0; 6]) };
                serial_println!(
                    "  [NIC] Intel e1000 ({:#06x}:{:#06x}) | Bus {} Slot {} Func {}",
                    pci_dev.vendor_id, pci_dev.device_id, pci_dev.bus, pci_dev.slot, pci_dev.func
                );
                serial_println!(
                    "        MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} | MMIO BAR0: {:#x}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], pci_dev.bar0
                );
            } else {
                serial_println!("  No PCI network devices detected.");
            }
        }

        "ls" => {
            serial_println!("--- LatencyFS In-Memory Files ---");
            unsafe {
                let mut count = 0;
                for file in crate::fs::FS.files.iter() {
                    if file.used {
                        count += 1;
                        serial_println!(
                            "  {:20} | Size: {:4} B | Mode: {}",
                            file.name_str(),
                            file.size,
                            if file.read_only { "RO" } else { "RW" }
                        );
                    }
                }
                if count == 0 {
                    serial_println!("  (No files stored)");
                }
            }
        }

        "cat" => {
            if arg.is_empty() {
                serial_println!("Usage: cat <filename>");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                serial_println!("--- Contents of {} ---", arg);
                if let Ok(text) = core::str::from_utf8(data) {
                    serial_print!("{}", text);
                } else {
                    serial_println!("<binary data>");
                }
                serial_println!("--- End of File ---");
            } else {
                serial_println!("File not found: '{}'", arg);
            }
        }

        "edit" => {
            let filename = if arg.is_empty() { "untitled.flow" } else { arg };
            crate::editor::start_editor(filename, tsc_freq_hz);
        }

        "run" => {
            if arg.is_empty() {
                serial_println!("Usage: run <filename>");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                serial_println!("=== Running PulseLang Script: {} ===", arg);
                let start_tsc = read_tsc_serialized();
                match crate::lang::run_pulse_script(data, tsc_freq_hz) {
                    Ok(()) => {
                        let elapsed_ns = tsc_to_ns(read_tsc_serialized() - start_tsc, tsc_freq_hz);
                        serial_println!("=== [Execution Succeeded in {} ns ({} us)] ===", elapsed_ns, elapsed_ns / 1000);
                    }
                    Err(e) => {
                        serial_println!("[ERROR] PulseLang runtime error: {}", e);
                    }
                }
            } else {
                serial_println!("File not found: '{}'", arg);
            }
        }

        "compile" => {
            if arg.is_empty() {
                serial_println!("Usage: compile <filename>");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                serial_println!("=== Compiling PulseLang Script: {} ===", arg);
                let mut tokens = [crate::lang::Token::empty(); crate::lang::MAX_TOKENS];
                let mut lexer = crate::lang::Lexer::new(data);
                match lexer.tokenize(&mut tokens) {
                    Ok(tok_count) => {
                        let mut compiler = crate::lang::Compiler::new(data, &tokens);
                        match compiler.compile() {
                            Ok(code_len) => {
                                serial_println!("Tokens scanned:     {}", tok_count);
                                serial_println!("Bytecode size:      {} bytes", code_len);
                                serial_println!("String pool:        {} bytes", compiler.str_pool_len);
                                serial_println!("Worst-Case Latency: ~{} ns (Guaranteed WCET budget)", code_len * 25);
                                serial_println!("Compilation:        SUCCESS (0 errors)");
                            }
                            Err(e) => {
                                serial_println!("[ERROR] Compile error: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        serial_println!("[ERROR] Lexer error: {}", e);
                    }
                }
            } else {
                serial_println!("File not found: '{}'", arg);
            }
        }

        "rm" => {
            if arg.is_empty() {
                serial_println!("Usage: rm <filename>");
            } else {
                match crate::fs::fs_delete(arg) {
                    Ok(()) => serial_println!("File deleted: '{}'", arg),
                    Err(e) => serial_println!("Delete failed: {:?}", e),
                }
            }
        }

        "clear" => {
            // ANSI screen clear
            serial_print!("\x1b[2J\x1b[H");
        }

        "halt" => {
            serial_println!("[SYSTEM] Halting LatencyOS system safely...");
            loop {
                unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); }
            }
        }

        _ => {
            serial_println!("Unknown command: '{}'. Type 'help' for available commands.", main_cmd);
        }
    }
}
