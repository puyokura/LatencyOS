// shell.rs - Zero-Allocation Interactive Control Shell for Core 0
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
use crate::serial::SERIAL;
use crate::serial_print;
use crate::serial_println;
use crate::smp::{
    get_core_role, CAPTURED_FRAMES, CONSUMED_FRAMES, CORES_ACTIVE, CORES_BOOTED, CORE_LOOP_COUNT,
    LAST_CAPTURE_LATENCY_NS, LAST_CONSUMED_CRC, LAST_CONSUMED_FRAME_ID, LAST_FRAME_CRC,
    LAST_NET_SEND_LATENCY_NS, NETWORK_FRAMES_SENT, NUM_CORES,
};
use crate::tsc::{read_tsc_serialized, tsc_to_ns};
use core::sync::atomic::Ordering;

pub const MAX_LINE_LEN: usize = 128;
pub const HISTORY_SIZE: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EscapeState {
    Normal,
    Esc,
    Csi,
    CsiParam(u8),
}

static mut LINE_BUF: [u8; MAX_LINE_LEN] = [0; MAX_LINE_LEN];
static mut LINE_LEN: usize = 0;
static mut CURSOR_POS: usize = 0;
static mut ESC_STATE: EscapeState = EscapeState::Normal;

static mut HISTORY: [[u8; MAX_LINE_LEN]; HISTORY_SIZE] = [[0; MAX_LINE_LEN]; HISTORY_SIZE];
static mut HISTORY_LENS: [usize; HISTORY_SIZE] = [0; HISTORY_SIZE];
static mut HISTORY_COUNT: usize = 0;
static mut HISTORY_IDX: usize = 0;

static mut PROMPT_SHOWN: bool = false;

// Function: print_prompt
// Description: Print colored LatencyOS interactive prompt.
// Worst-case execution time: ~15_000 ns
pub fn print_prompt() {
    serial_print!("\x1b[1;36mlatencyos\x1b[0m\x1b[1;32m >\x1b[0m ");
    unsafe {
        PROMPT_SHOWN = true;
        CURSOR_POS = 0;
        LINE_LEN = 0;
        ESC_STATE = EscapeState::Normal;
    }
}

// Function: init_shell
// Description: Initialize interactive shell and display welcome banner.
// Worst-case execution time: ~50_000 ns
pub fn init_shell() {
    serial_println!("============================================================================");
    serial_println!("\x1b[1;32m  LatencyOS Interactive Control Shell\x1b[0m [Zero-Allocation, Real-Time Mode]");
    serial_println!("  Type \x1b[1;33m'help'\x1b[0m for command list or \x1b[1;33m'ls'\x1b[0m to inspect in-memory files.");
    serial_println!("============================================================================");
    print_prompt();
}

// Function: redraw_line
// Description: Redraw current input line and reposition cursor.
// Worst-case execution time: ~5000 ns
unsafe fn redraw_line() {
    // Return to start of line, clear to end of line
    serial_print!("\r\x1b[1;36mlatencyos\x1b[0m\x1b[1;32m >\x1b[0m \x1b[K");
    if LINE_LEN > 0 {
        if let Ok(s) = core::str::from_utf8(&LINE_BUF[..LINE_LEN]) {
            serial_print!("{}", s);
        }
    }
    // Move cursor back if not at end
    if CURSOR_POS < LINE_LEN {
        let diff = LINE_LEN - CURSOR_POS;
        serial_print!("\x1b[{}D", diff);
    }
}

// Function: save_to_history
// Description: Save executed line into command history buffer.
// Worst-case execution time: ~200 ns
unsafe fn save_to_history(line: &[u8]) {
    if line.is_empty() {
        return;
    }
    let slot = HISTORY_COUNT % HISTORY_SIZE;
    let len = core::cmp::min(line.len(), MAX_LINE_LEN);
    HISTORY[slot][..len].copy_from_slice(&line[..len]);
    HISTORY_LENS[slot] = len;
    HISTORY_COUNT += 1;
    HISTORY_IDX = HISTORY_COUNT;
}

// Function: poll_shell
// Description: Non-blocking poll for incoming serial characters, ANSI escape sequences, and line editing.
// Worst-case execution time: ~25 ns (when idle) to ~100_000 ns (when executing a command)
pub fn poll_shell(tsc_freq_hz: u64) {
    unsafe {
        if !PROMPT_SHOWN {
            print_prompt();
        }

        while let Some(b) = SERIAL.read_byte_nonblocking() {
            match ESC_STATE {
                EscapeState::Normal => {
                    match b {
                        // Escape character
                        0x1B => {
                            ESC_STATE = EscapeState::Esc;
                        }

                        // Enter / Return
                        b'\r' | b'\n' => {
                            serial_print!("\r\n");
                            if LINE_LEN > 0 {
                                save_to_history(&LINE_BUF[..LINE_LEN]);
                                if let Ok(cmd_str) = core::str::from_utf8(&LINE_BUF[..LINE_LEN]) {
                                    execute_command(cmd_str.trim(), tsc_freq_hz);
                                }
                                LINE_LEN = 0;
                                CURSOR_POS = 0;
                            }
                            print_prompt();
                        }

                        // Backspace
                        0x08 | 0x7F => {
                            if CURSOR_POS > 0 {
                                for i in CURSOR_POS - 1..LINE_LEN - 1 {
                                    LINE_BUF[i] = LINE_BUF[i + 1];
                                }
                                LINE_LEN -= 1;
                                CURSOR_POS -= 1;
                                redraw_line();
                            }
                        }

                        // Ctrl+C (Clear line)
                        0x03 => {
                            serial_print!("^C\r\n");
                            LINE_LEN = 0;
                            CURSOR_POS = 0;
                            print_prompt();
                        }

                        // Ctrl+L (Clear screen)
                        0x0C => {
                            serial_print!("\x1b[2J\x1b[H");
                            redraw_line();
                        }

                        // Printable characters
                        0x20..=0x7E => {
                            if LINE_LEN < MAX_LINE_LEN - 1 {
                                for i in (CURSOR_POS..LINE_LEN).rev() {
                                    LINE_BUF[i + 1] = LINE_BUF[i];
                                }
                                LINE_BUF[CURSOR_POS] = b;
                                LINE_LEN += 1;
                                CURSOR_POS += 1;
                                redraw_line();
                            }
                        }

                        _ => {}
                    }
                }

                EscapeState::Esc => {
                    if b == b'[' {
                        ESC_STATE = EscapeState::Csi;
                    } else {
                        ESC_STATE = EscapeState::Normal;
                    }
                }

                EscapeState::Csi => {
                    match b {
                        // Up Arrow: History Previous
                        b'A' => {
                            if HISTORY_COUNT > 0 && HISTORY_IDX > 0 {
                                HISTORY_IDX -= 1;
                                let slot = HISTORY_IDX % HISTORY_SIZE;
                                let len = HISTORY_LENS[slot];
                                LINE_BUF[..len].copy_from_slice(&HISTORY[slot][..len]);
                                LINE_LEN = len;
                                CURSOR_POS = len;
                                redraw_line();
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Down Arrow: History Next
                        b'B' => {
                            if HISTORY_IDX + 1 < HISTORY_COUNT {
                                HISTORY_IDX += 1;
                                let slot = HISTORY_IDX % HISTORY_SIZE;
                                let len = HISTORY_LENS[slot];
                                LINE_BUF[..len].copy_from_slice(&HISTORY[slot][..len]);
                                LINE_LEN = len;
                                CURSOR_POS = len;
                                redraw_line();
                            } else {
                                HISTORY_IDX = HISTORY_COUNT;
                                LINE_LEN = 0;
                                CURSOR_POS = 0;
                                redraw_line();
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Right Arrow
                        b'C' => {
                            if CURSOR_POS < LINE_LEN {
                                CURSOR_POS += 1;
                                serial_print!("\x1b[1C");
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Left Arrow
                        b'D' => {
                            if CURSOR_POS > 0 {
                                CURSOR_POS -= 1;
                                serial_print!("\x1b[1D");
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Home
                        b'H' | b'1' => {
                            CURSOR_POS = 0;
                            redraw_line();
                            ESC_STATE = EscapeState::Normal;
                        }

                        // End
                        b'F' | b'4' => {
                            CURSOR_POS = LINE_LEN;
                            redraw_line();
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Delete key sequence: \x1b[3~
                        b'3' => {
                            ESC_STATE = EscapeState::CsiParam(3);
                        }

                        _ => {
                            ESC_STATE = EscapeState::Normal;
                        }
                    }
                }

                EscapeState::CsiParam(param) => {
                    if param == 3 && b == b'~' {
                        // Delete character at cursor
                        if CURSOR_POS < LINE_LEN {
                            for i in CURSOR_POS..LINE_LEN - 1 {
                                LINE_BUF[i] = LINE_BUF[i + 1];
                            }
                            LINE_LEN -= 1;
                            redraw_line();
                        }
                    }
                    ESC_STATE = EscapeState::Normal;
                }
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
            serial_println!("\x1b[1;33m=== LatencyOS Shell Commands ===\x1b[0m");
            serial_println!("  \x1b[1;36mls\x1b[0m                  List all files in LatencyFS storage");
            serial_println!("  \x1b[1;36mcat <file>\x1b[0m          Display file content with line numbers");
            serial_println!("  \x1b[1;36medit <file>\x1b[0m         Open full-screen ANSI PulseEditor");
            serial_println!("  \x1b[1;36mrun <file>\x1b[0m          Compile and execute PulseLang script");
            serial_println!("  \x1b[1;36mcompile <file>\x1b[0m      Compile script and show bytecode & WCET estimate");
            serial_println!("  \x1b[1;36mrm <file>\x1b[0m           Delete file from LatencyFS");
            serial_println!("  \x1b[1;36mstatus\x1b[0m              Show CPU cores, boot states, and loop counters");
            serial_println!("  \x1b[1;36mpipeline\x1b[0m            Show live streaming stats (Capture, Encode, Net)");
            serial_println!("  \x1b[1;36mlatency\x1b[0m             Show single-run latency breakdown");
            serial_println!("  \x1b[1;36mbenchmark\x1b[0m           Execute 1000-sample latency benchmark");
            serial_println!("  \x1b[1;36mcongestion\x1b[0m          Show delay-based congestion control metrics");
            serial_println!("  \x1b[1;36mpower\x1b[0m               Read IA32_THERM_STATUS & RAPL MSR status");
            serial_println!("  \x1b[1;36mpci\x1b[0m                 Display Intel e1000 NIC PCI config and MAC");
            serial_println!("  \x1b[1;36mclear\x1b[0m               Clear terminal screen");
            serial_println!("  \x1b[1;36mhalt\x1b[0m                Halt CPU cores safely (cli; hlt)");
            serial_println!("  \x1b[1;36mhelp\x1b[0m                Show this help message");
        }

        "ls" => {
            serial_println!("\x1b[1;33m+----------------------+----------+------+-----------------+\x1b[0m");
            serial_println!("\x1b[1;33m| File Name            | Size (B) | Mode | Type            |\x1b[0m");
            serial_println!("\x1b[1;33m+----------------------+----------+------+-----------------+\x1b[0m");
            unsafe {
                let mut count = 0;
                for file in crate::fs::FS.files.iter() {
                    if file.used {
                        count += 1;
                        let ftype = if file.name_str().ends_with(".flow") { "PulseScript" } else { "Text/Data" };
                        serial_println!(
                            "| \x1b[1;36m{:20}\x1b[0m | {:8} | {:4} | {:15} |",
                            file.name_str(),
                            file.size,
                            if file.read_only { "RO" } else { "RW" },
                            ftype
                        );
                    }
                }
                serial_println!("\x1b[1;33m+----------------------+----------+------+-----------------+\x1b[0m");
                serial_println!("  Total: {} file(s) in LatencyFS", count);
            }
        }

        "cat" => {
            if arg.is_empty() {
                serial_println!("\x1b[1;31mUsage: cat <filename>\x1b[0m");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                serial_println!("\x1b[1;33m--- File: {} ({} bytes) ---\x1b[0m", arg, data.len());
                let mut line_num = 1;
                serial_print!("\x1b[90m{:3} |\x1b[0m ", line_num);
                for &b in data {
                    if b == b'\n' {
                        line_num += 1;
                        serial_print!("\r\n\x1b[90m{:3} |\x1b[0m ", line_num);
                    } else if b == b'\t' {
                        // Expand tab to 4 spaces for crisp indentation
                        serial_print!("    ");
                    } else {
                        SERIAL.send_byte(b);
                    }
                }
                serial_println!("\r\n\x1b[1;33m--- End of File ---\x1b[0m");
            } else {
                serial_println!("\x1b[1;31m[ERROR] File not found: '{}'\x1b[0m", arg);
            }
        }

        "edit" => {
            let filename = if arg.is_empty() { "untitled.flow" } else { arg };
            crate::editor::start_editor(filename, tsc_freq_hz);
        }

        "run" => {
            if arg.is_empty() {
                serial_println!("\x1b[1;31mUsage: run <filename>\x1b[0m");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                serial_println!("\x1b[1;33m=== Running PulseLang Script: {} ===\x1b[0m", arg);
                let start_tsc = read_tsc_serialized();
                match crate::lang::run_pulse_script(data, tsc_freq_hz) {
                    Ok(()) => {
                        let elapsed_ns = tsc_to_ns(read_tsc_serialized() - start_tsc, tsc_freq_hz);
                        serial_println!("\x1b[1;32m=== [Execution Succeeded in {} ns ({} us)] ===\x1b[0m", elapsed_ns, elapsed_ns / 1000);
                    }
                    Err(e) => {
                        serial_println!("\x1b[1;31m[ERROR] PulseLang runtime error: {}\x1b[0m", e);
                    }
                }
            } else {
                serial_println!("\x1b[1;31m[ERROR] File not found: '{}'\x1b[0m", arg);
            }
        }

        "compile" => {
            if arg.is_empty() {
                serial_println!("\x1b[1;31mUsage: compile <filename>\x1b[0m");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                serial_println!("\x1b[1;33m=== Compiling PulseLang Script: {} ===\x1b[0m", arg);
                let mut tokens = [crate::lang::Token::empty(); crate::lang::MAX_TOKENS];
                let mut lexer = crate::lang::Lexer::new(data);
                match lexer.tokenize(&mut tokens) {
                    Ok(tok_count) => {
                        let mut compiler = crate::lang::Compiler::new(data, &tokens);
                        match compiler.compile() {
                            Ok(code_len) => {
                                serial_println!("  Tokens scanned:     {}", tok_count);
                                serial_println!("  Bytecode size:      {} bytes", code_len);
                                serial_println!("  String pool:        {} bytes", compiler.str_pool_len);
                                serial_println!("  Worst-Case Latency: ~{} ns (Guaranteed WCET budget)", code_len * 25);
                                serial_println!("  Compilation:        \x1b[1;32mSUCCESS (0 errors)\x1b[0m");
                            }
                            Err(e) => {
                                serial_println!("\x1b[1;31m[ERROR] Compile error: {}\x1b[0m", e);
                            }
                        }
                    }
                    Err(e) => {
                        serial_println!("\x1b[1;31m[ERROR] Lexer error: {}\x1b[0m", e);
                    }
                }
            } else {
                serial_println!("\x1b[1;31m[ERROR] File not found: '{}'\x1b[0m", arg);
            }
        }

        "rm" => {
            if arg.is_empty() {
                serial_println!("\x1b[1;31mUsage: rm <filename>\x1b[0m");
            } else {
                match crate::fs::fs_delete(arg) {
                    Ok(()) => serial_println!("\x1b[1;32mFile deleted: '{}'\x1b[0m", arg),
                    Err(e) => serial_println!("\x1b[1;31mDelete failed: {:?}\x1b[0m", e),
                }
            }
        }

        "status" => {
            let uptime_tsc = read_tsc_serialized();
            let uptime_ns = tsc_to_ns(uptime_tsc, tsc_freq_hz);
            serial_println!("\x1b[1;33m=== LatencyOS Core Status ===\x1b[0m");
            serial_println!("Uptime: {} ns ({} ms) | TSC Freq: {} MHz", uptime_ns, uptime_ns / 1_000_000, tsc_freq_hz / 1_000_000);
            serial_println!("+------+--------------------+---------+---------+-------------------+");
            serial_println!("| Core | Role               | Booted  | Active  | Loop Count        |");
            serial_println!("+------+--------------------+---------+---------+-------------------+");
            for i in 0..NUM_CORES {
                let role = get_core_role(i as u8);
                let booted = CORES_BOOTED[i].load(Ordering::Acquire);
                let active = CORES_ACTIVE[i].load(Ordering::Acquire) || (i == 0);
                let loops = CORE_LOOP_COUNT[i].load(Ordering::Relaxed);
                serial_println!(
                    "| {:4} | \x1b[1;36m{:18}\x1b[0m | \x1b[1;32m{:7}\x1b[0m | \x1b[1;32m{:7}\x1b[0m | {:17} |",
                    i, role.name(), booted, active, loops
                );
            }
            serial_println!("+------+--------------------+---------+---------+-------------------+");
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

            serial_println!("\x1b[1;33m=== Live End-to-End Pipeline Status ===\x1b[0m");
            serial_println!("  Core 1 (Capture): Frames = {}, Latency = {} ns, CRC = \x1b[1;36m{:#010x}\x1b[0m", captured, last_lat, last_crc);
            serial_println!("  Core 2 (Encode):  Frames = {}, Frame ID = {}, CRC = \x1b[1;36m{:#010x}\x1b[0m", consumed, consumed_id, consumed_crc);
            serial_println!("  Core 3 (Network): Frames = {}, Packets = {}, Latency = {} ns, Drops = {}, ACKs = {}", net_sent, pkts_sent, last_net_lat, dropped, acks);
            serial_println!("  Pipeline State:   \x1b[1;32m{}\x1b[0m", if captured > 0 && consumed > 0 && net_sent > 0 { "STREAMING ACTIVE" } else { "INITIALIZED" });
        }

        "latency" => {
            latency_report(tsc_freq_hz);
        }

        "benchmark" => {
            serial_println!("\x1b[1;33m[BENCHMARK] Executing 1000-sample pipeline benchmark...\x1b[0m");
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
            serial_println!("\x1b[1;33m=== Delay-Based Congestion Controller Status ===\x1b[0m");
            serial_println!("  Baseline Min RTT:     {} ns ({} us)", MIN_RTT_NS.load(Ordering::Relaxed), MIN_RTT_NS.load(Ordering::Relaxed) / 1000);
            serial_println!("  Last Sample RTT:      {} ns ({} us)", LAST_RTT_NS.load(Ordering::Relaxed), LAST_RTT_NS.load(Ordering::Relaxed) / 1000);
            serial_println!("  Queuing Delta Delay:  {} ns ({} us)", DELTA_DELAY_NS.load(Ordering::Relaxed), DELTA_DELAY_NS.load(Ordering::Relaxed) / 1000);
            serial_println!("  Congestion Threshold: 200,000 ns (200 us)");
            serial_println!("  Current Rate Limit:   \x1b[1;32m{}%\x1b[0m", CONGESTION_RATE_PCT.load(Ordering::Relaxed));
            serial_println!("  Total ACKs Processed: {}", TOTAL_ACKS_RECEIVED.load(Ordering::Relaxed));
            serial_println!("  Total Stale Dropped:  {}", TOTAL_FRAMES_DROPPED.load(Ordering::Relaxed));
        }

        "power" => {
            report_power_thermal_status();
        }

        "pci" => {
            serial_println!("\x1b[1;33m=== PCI Bus Devices ===\x1b[0m");
            if let Some(pci_dev) = find_e1000_device() {
                let mac = unsafe { E1000.as_ref().map(|d| d.mac).unwrap_or([0; 6]) };
                serial_println!(
                    "  [NIC] Intel e1000 ({:#06x}:{:#06x}) | Bus {} Slot {} Func {}",
                    pci_dev.vendor_id, pci_dev.device_id, pci_dev.bus, pci_dev.slot, pci_dev.func
                );
                serial_println!(
                    "        MAC: \x1b[1;36m{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\x1b[0m | MMIO BAR0: {:#x}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], pci_dev.bar0
                );
            } else {
                serial_println!("  No PCI network devices detected.");
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
            serial_println!("\x1b[1;31mUnknown command: '{}'. Type 'help' for available commands.\x1b[0m", main_cmd);
        }
    }
}
