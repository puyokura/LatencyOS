// shell.rs - Zero-Allocation Linux-Style Minimal Shell for Core 0
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
pub const HISTORY_SIZE: usize = 8;

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
// Description: Print Linux-style minimal shell prompt.
// Worst-case execution time: ~5000 ns
pub fn print_prompt() {
    serial_print!("latencyos$ ");
    unsafe {
        PROMPT_SHOWN = true;
        CURSOR_POS = 0;
        LINE_LEN = 0;
        ESC_STATE = EscapeState::Normal;
    }
}

// Function: init_shell
// Description: Initialize interactive shell with minimal Unix banner.
// Worst-case execution time: ~10_000 ns
pub fn init_shell() {
    serial_println!("LatencyOS 0.0.4 (x86_64)");
    print_prompt();
}

// Function: redraw_line
// Description: Redraw current input line and reposition cursor.
// Worst-case execution time: ~3000 ns
unsafe fn redraw_line() {
    serial_print!("\rlatencyos$ \x1b[K");
    if LINE_LEN > 0 {
        if let Ok(s) = core::str::from_utf8(&LINE_BUF[..LINE_LEN]) {
            serial_print!("{}", s);
        }
    }
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
// Description: Non-blocking poll for incoming serial characters and line editing.
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
                        0x1B => {
                            ESC_STATE = EscapeState::Esc;
                        }

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

                        // Ctrl+C (Interrupt/Clear line)
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

                        // Tab: simple autocomplete / spacing
                        b'\t' => {
                            // If user types 'ca' and presses tab, or just space
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

                        // Delete: \x1b[3~
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
            serial_println!("LatencyOS built-in commands:");
            serial_println!("  ls [-l]          list directory contents");
            serial_println!("  cat <file>       concatenate files and print on standard output");
            serial_println!("  edit <file>      open text editor");
            serial_println!("  run <file>       execute PulseLang script");
            serial_println!("  compile <file>   compile script and show diagnostics");
            serial_println!("  rm <file>        remove file");
            serial_println!("  status           display core and system status");
            serial_println!("  pipeline         display streaming pipeline metrics");
            serial_println!("  latency          display latency measurement breakdown");
            serial_println!("  benchmark        run 1000-sample latency benchmark");
            serial_println!("  congestion       display congestion controller metrics");
            serial_println!("  power            display thermal and RAPL power status");
            serial_println!("  pci              list PCI devices");
            serial_println!("  clear            clear the terminal screen");
            serial_println!("  halt             halt the system");
            serial_println!("  help             display this help");
        }

        "ls" => {
            let is_long = arg == "-l" || arg == "-la" || arg == "-al";
            unsafe {
                if is_long {
                    for file in crate::fs::FS.files.iter() {
                        if file.used {
                            let mode = if file.read_only { "-r--r--r--" } else { "-rw-r--r--" };
                            serial_println!("{} 1 root root {:5} {}", mode, file.size, file.name_str());
                        }
                    }
                } else {
                    let mut first = true;
                    for file in crate::fs::FS.files.iter() {
                        if file.used {
                            if !first {
                                serial_print!("  ");
                            }
                            serial_print!("{}", file.name_str());
                            first = false;
                        }
                    }
                    if !first {
                        serial_println!();
                    }
                }
            }
        }

        "cat" => {
            if arg.is_empty() {
                serial_println!("cat: missing operand");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                // Linux cat: output raw contents directly
                for &b in data {
                    if b == b'\t' {
                        serial_print!("    ");
                    } else {
                        SERIAL.send_byte(b);
                    }
                }
                // Ensure newline at EOF if needed
                if !data.is_empty() && data[data.len() - 1] != b'\n' {
                    serial_println!();
                }
            } else {
                serial_println!("cat: {}: No such file or directory", arg);
            }
        }

        "edit" => {
            let filename = if arg.is_empty() { "untitled.flow" } else { arg };
            crate::editor::start_editor(filename, tsc_freq_hz);
        }

        "run" => {
            if arg.is_empty() {
                serial_println!("run: missing operand");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                match crate::lang::run_pulse_script(data, tsc_freq_hz) {
                    Ok(()) => {}
                    Err(e) => {
                        serial_println!("pulse: {}: runtime error: {}", arg, e);
                    }
                }
            } else {
                serial_println!("run: cannot access '{}': No such file or directory", arg);
            }
        }

        "compile" => {
            if arg.is_empty() {
                serial_println!("compile: missing operand");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                let mut tokens = [crate::lang::Token::empty(); crate::lang::MAX_TOKENS];
                let mut lexer = crate::lang::Lexer::new(data);
                match lexer.tokenize(&mut tokens) {
                    Ok(tok_count) => {
                        let mut compiler = crate::lang::Compiler::new(data, &tokens);
                        match compiler.compile() {
                            Ok(code_len) => {
                                serial_println!(
                                    "{}: {} tokens, {} bytes bytecode, wcet ~{} ns",
                                    arg,
                                    tok_count,
                                    code_len,
                                    code_len * 25
                                );
                            }
                            Err(e) => {
                                serial_println!("compile: {}: error: {}", arg, e);
                            }
                        }
                    }
                    Err(e) => {
                        serial_println!("compile: {}: lexer error: {}", arg, e);
                    }
                }
            } else {
                serial_println!("compile: cannot access '{}': No such file or directory", arg);
            }
        }

        "rm" => {
            if arg.is_empty() {
                serial_println!("rm: missing operand");
            } else {
                match crate::fs::fs_delete(arg) {
                    Ok(()) => {
                        // Linux rm: silent on success
                    }
                    Err(crate::fs::FsError::FileNotFound) => {
                        serial_println!("rm: cannot remove '{}': No such file or directory", arg);
                    }
                    Err(crate::fs::FsError::ReadOnly) => {
                        serial_println!("rm: cannot remove '{}': Read-only file system", arg);
                    }
                    Err(_) => {
                        serial_println!("rm: cannot remove '{}': Operation failed", arg);
                    }
                }
            }
        }

        "status" => {
            let uptime_tsc = read_tsc_serialized();
            let uptime_ns = tsc_to_ns(uptime_tsc, tsc_freq_hz);
            serial_println!("uptime: {} ms  tsc_freq: {} MHz", uptime_ns / 1_000_000, tsc_freq_hz / 1_000_000);
            for i in 0..NUM_CORES {
                let role = get_core_role(i as u8);
                let booted = CORES_BOOTED[i].load(Ordering::Acquire);
                let active = CORES_ACTIVE[i].load(Ordering::Acquire) || (i == 0);
                let loops = CORE_LOOP_COUNT[i].load(Ordering::Relaxed);
                serial_println!(
                    "core{}: {:7} (booted: {}, active: {}, loops: {})",
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

            serial_println!("capture:  {} frames, latency {} ns, crc {:#010x}", captured, last_lat, last_crc);
            serial_println!("encode:   {} frames, frame_id {}, crc {:#010x}", consumed, consumed_id, consumed_crc);
            serial_println!("network:  {} frames, {} packets, latency {} ns, drops: {}, acks: {}", net_sent, pkts_sent, last_net_lat, dropped, acks);
        }

        "latency" => {
            latency_report(tsc_freq_hz);
        }

        "benchmark" => {
            serial_println!("benchmark: running 1000-sample pipeline benchmark...");
            let mut pkt_seq = 1u16;

            for sample_idx in 0..STATS_SAMPLE_COUNT {
                let t0 = read_tsc_serialized();
                let _ = crate::apic::get_lapic_id();
                let t1 = read_tsc_serialized();
                let s0_ns = tsc_to_ns(t1 - t0, tsc_freq_hz) as u32;
                record_stage_sample(0, sample_idx, s0_ns);

                let t1_start = read_tsc_serialized();
                CORES_ACTIVE[0].store(true, Ordering::Release);
                let t2 = read_tsc_serialized();
                let s1_ns = tsc_to_ns(t2 - t1_start, tsc_freq_hz) as u32;
                record_stage_sample(1, sample_idx, s1_ns);

                let t2_start = read_tsc_serialized();
                let _ = poll_vblank_edge(5);
                let t3 = read_tsc_serialized();
                let s2_ns = tsc_to_ns(t3 - t2_start, tsc_freq_hz) as u32;
                record_stage_sample(2, sample_idx, s2_ns);

                let t3_start = read_tsc_serialized();
                let frame_handle = capture_frame_zero_copy((sample_idx % 4) as u8, sample_idx as u64, t3_start);
                let t4 = read_tsc_serialized();
                let s3_ns = tsc_to_ns(t4 - t3_start, tsc_freq_hz) as u32;
                record_stage_sample(3, sample_idx, s3_ns);

                let t4_start = read_tsc_serialized();
                let _ = CAPTURE_TO_ENCODE_RING.push(frame_handle);
                let _ = CAPTURE_TO_ENCODE_RING.pop();
                let t5 = read_tsc_serialized();
                let s4_ns = tsc_to_ns(t5 - t4_start, tsc_freq_hz) as u32;
                record_stage_sample(4, sample_idx, s4_ns);

                let deadline = read_tsc_serialized() + crate::tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
                let t5_start = read_tsc_serialized();
                let _ = stream_send_frame(&frame_handle, deadline, &mut pkt_seq);
                let t6 = read_tsc_serialized();
                let s5_ns = tsc_to_ns(t6 - t5_start, tsc_freq_hz) as u32;
                record_stage_sample(5, sample_idx, s5_ns);

                let total_e2e_ns = s0_ns + s1_ns + s2_ns + s3_ns + s4_ns + s5_ns;
                record_stage_sample(6, sample_idx, total_e2e_ns);
            }

            print_statistical_latency_report();
        }

        "congestion" => {
            serial_println!(
                "min_rtt: {} us  last_rtt: {} us  delta_delay: {} us  rate: {}%  acks: {}  drops: {}",
                MIN_RTT_NS.load(Ordering::Relaxed) / 1000,
                LAST_RTT_NS.load(Ordering::Relaxed) / 1000,
                DELTA_DELAY_NS.load(Ordering::Relaxed) / 1000,
                CONGESTION_RATE_PCT.load(Ordering::Relaxed),
                TOTAL_ACKS_RECEIVED.load(Ordering::Relaxed),
                TOTAL_FRAMES_DROPPED.load(Ordering::Relaxed)
            );
        }

        "power" => {
            report_power_thermal_status();
        }

        "pci" => {
            if let Some(pci_dev) = find_e1000_device() {
                let mac = unsafe { E1000.as_ref().map(|d| d.mac).unwrap_or([0; 6]) };
                serial_println!(
                    "00:03.0 Ethernet controller: Intel 82540EM ({:#06x}:{:#06x})",
                    pci_dev.vendor_id, pci_dev.device_id
                );
                serial_println!(
                    "        MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}  BAR0: {:#x}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], pci_dev.bar0
                );
            } else {
                serial_println!("pci: no devices found");
            }
        }

        "clear" => {
            serial_print!("\x1b[2J\x1b[H");
        }

        "halt" => {
            serial_println!("System halted.");
            loop {
                unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); }
            }
        }

        _ => {
            serial_println!("latencyos: {}: command not found", main_cmd);
        }
    }
}
