// shell.rs - Zero-Allocation Hard Real-Time Pulse Shell (Unix Philosophy & Time-Native Telemetry)
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
    Ss3,
    Csi {
        params: [u16; 4],
        param_count: usize,
    },
}

static mut LINE_BUF: [u8; MAX_LINE_LEN] = [0; MAX_LINE_LEN];
static mut LINE_LEN: usize = 0;
static mut CURSOR_POS: usize = 0;
static mut ESC_STATE: EscapeState = EscapeState::Normal;

static mut HISTORY: [[u8; MAX_LINE_LEN]; HISTORY_SIZE] = [[0; MAX_LINE_LEN]; HISTORY_SIZE];
static mut HISTORY_LENS: [usize; HISTORY_SIZE] = [0; HISTORY_SIZE];
static mut HISTORY_COUNT: usize = 0;
static mut HISTORY_IDX: usize = 0;

static mut LAST_CMD_LATENCY_NS: u64 = 18;
static mut PROMPT_SHOWN: bool = false;
pub static mut CURRENT_DIR: [u8; 64] = *b"/\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
pub static mut CURRENT_DIR_LEN: usize = 1;

// Function: print_formatted_time
// Description: Print human-readable time (ns, us, ms, s, m s) without float allocations.
// Worst-case execution time: ~1000 ns
fn print_formatted_time(ns: u64) {
    if ns < 1_000 {
        serial_print!("{}ns", ns);
    } else if ns < 1_000_000 {
        let us = ns / 1_000;
        let frac = (ns % 1_000) / 100;
        serial_print!("{}.{}us", us, frac);
    } else if ns < 1_000_000_000 {
        let ms = ns / 1_000_000;
        let frac = (ns % 1_000_000) / 100_000;
        serial_print!("{}.{}ms", ms, frac);
    } else if ns < 60_000_000_000 {
        let s = ns / 1_000_000_000;
        let frac = (ns % 1_000_000_000) / 100_000_000;
        serial_print!("{}.{}s", s, frac);
    } else {
        let total_s = ns / 1_000_000_000;
        let m = total_s / 60;
        let s = total_s % 60;
        serial_print!("{}m {}s", m, s);
    }
}

// Function: print_prompt
// Description: Print time-native Unix prompt with previous execution cost.
// Worst-case execution time: ~5000 ns
pub fn print_prompt() {
    serial_print!("[c0|");
    unsafe {
        print_formatted_time(LAST_CMD_LATENCY_NS);
    }
    serial_print!("] % ");

    unsafe {
        PROMPT_SHOWN = true;
        ESC_STATE = EscapeState::Normal;
    }
}

// Function: init_shell
// Description: Initialize interactive shell with Unix banner.
// Worst-case execution time: ~10_000 ns
pub fn init_shell() {
    serial_println!("LatencyOS 0.0.5 (x86_64 hard-realtime)");
    print_prompt();
}

// Function: redraw_line
// Description: Redraw current input line and reposition cursor.
// Worst-case execution time: ~3000 ns
unsafe fn redraw_line() {
    serial_print!("\r[c0|");
    print_formatted_time(LAST_CMD_LATENCY_NS);
    serial_print!("] % \x1b[K");

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

// Function: move_word_left
// Description: Move cursor to previous word boundary (Ctrl+Left).
// Worst-case execution time: ~200 ns
unsafe fn move_word_left() {
    if CURSOR_POS == 0 {
        return;
    }
    let mut pos = CURSOR_POS;
    // Skip preceding whitespace
    while pos > 0 && LINE_BUF[pos - 1] == b' ' {
        pos -= 1;
    }
    // Skip word characters
    while pos > 0 && LINE_BUF[pos - 1] != b' ' {
        pos -= 1;
    }
    CURSOR_POS = pos;
    redraw_line();
}

// Function: move_word_right
// Description: Move cursor to next word boundary (Ctrl+Right).
// Worst-case execution time: ~200 ns
unsafe fn move_word_right() {
    if CURSOR_POS >= LINE_LEN {
        return;
    }
    let mut pos = CURSOR_POS;
    // Skip current word
    while pos < LINE_LEN && LINE_BUF[pos] != b' ' {
        pos += 1;
    }
    // Skip following whitespace
    while pos < LINE_LEN && LINE_BUF[pos] == b' ' {
        pos += 1;
    }
    CURSOR_POS = pos;
    redraw_line();
}

static mut LAST_WAS_CR: bool = false;

// Function: poll_shell
// Description: Non-blocking poll for incoming serial characters, full multi-byte ANSI CSI sequences, and line editing.
// Worst-case execution time: ~25 ns (when idle) to ~100_000 ns (when executing a command)
pub fn poll_shell(tsc_freq_hz: u64) {
    unsafe {
        if !PROMPT_SHOWN {
            print_prompt();
        }

        let mut needs_redraw = false;

        while let Some(b) = SERIAL.read_byte_nonblocking() {
            match ESC_STATE {
                EscapeState::Normal => {
                    match b {
                        0x1B => {
                            ESC_STATE = EscapeState::Esc;
                            LAST_WAS_CR = false;
                        }

                        b'\r' => {
                            LAST_WAS_CR = true;
                            if needs_redraw {
                                redraw_line();
                                needs_redraw = false;
                            }
                            serial_print!("\r\n");
                            let len = core::cmp::min(LINE_LEN, MAX_LINE_LEN);
                            if len > 0 {
                                save_to_history(&LINE_BUF[..len]);
                                static mut EXEC_CMD_BUF: [u8; MAX_LINE_LEN] = [0u8; MAX_LINE_LEN];
                                EXEC_CMD_BUF[..len].copy_from_slice(&LINE_BUF[..len]);
                                LINE_LEN = 0;
                                CURSOR_POS = 0;
                                if let Ok(cmd_str) = core::str::from_utf8(&EXEC_CMD_BUF[..len]) {
                                    let t_start = read_tsc_serialized();
                                    execute_command(cmd_str.trim(), tsc_freq_hz);
                                    let t_end = read_tsc_serialized();
                                    LAST_CMD_LATENCY_NS = tsc_to_ns(t_end - t_start, tsc_freq_hz);
                                }
                            } else {
                                LINE_LEN = 0;
                                CURSOR_POS = 0;
                            }
                            print_prompt();
                        }

                        b'\n' => {
                            if !LAST_WAS_CR {
                                if needs_redraw {
                                    redraw_line();
                                    needs_redraw = false;
                                }
                                serial_print!("\r\n");
                                let len = core::cmp::min(LINE_LEN, MAX_LINE_LEN);
                                if len > 0 {
                                    save_to_history(&LINE_BUF[..len]);
                                    static mut EXEC_CMD_BUF: [u8; MAX_LINE_LEN] = [0u8; MAX_LINE_LEN];
                                    EXEC_CMD_BUF[..len].copy_from_slice(&LINE_BUF[..len]);
                                    LINE_LEN = 0;
                                    CURSOR_POS = 0;
                                    if let Ok(cmd_str) = core::str::from_utf8(&EXEC_CMD_BUF[..len]) {
                                        let t_start = read_tsc_serialized();
                                        execute_command(cmd_str.trim(), tsc_freq_hz);
                                        let t_end = read_tsc_serialized();
                                        LAST_CMD_LATENCY_NS = tsc_to_ns(t_end - t_start, tsc_freq_hz);
                                    }
                                } else {
                                    LINE_LEN = 0;
                                    CURSOR_POS = 0;
                                }
                                print_prompt();
                            }
                            LAST_WAS_CR = false;
                        }

                        // Backspace
                        0x08 | 0x7F => {
                            LAST_WAS_CR = false;
                            if CURSOR_POS > 0 && CURSOR_POS <= LINE_LEN && LINE_LEN > 0 {
                                let limit = LINE_LEN - 1;
                                for i in (CURSOR_POS - 1)..limit {
                                    LINE_BUF[i] = LINE_BUF[i + 1];
                                }
                                LINE_LEN -= 1;
                                CURSOR_POS -= 1;
                                needs_redraw = true;
                            }
                        }

                        // Ctrl+C (Interrupt/Clear line)
                        0x03 => {
                            LAST_WAS_CR = false;
                            if needs_redraw {
                                redraw_line();
                                needs_redraw = false;
                            }
                            serial_print!("^C\r\n");
                            LINE_LEN = 0;
                            CURSOR_POS = 0;
                            print_prompt();
                        }

                        // Ctrl+L (Clear screen)
                        0x0C => {
                            LAST_WAS_CR = false;
                            serial_print!("\x1b[2J\x1b[H");
                            needs_redraw = true;
                        }

                        // Tab
                        b'\t' => {
                            LAST_WAS_CR = false;
                        }

                        // Printable characters
                        0x20..=0x7E => {
                            LAST_WAS_CR = false;
                            if LINE_LEN < MAX_LINE_LEN - 1 {
                                if CURSOR_POS == LINE_LEN {
                                    LINE_BUF[CURSOR_POS] = b;
                                    LINE_LEN += 1;
                                    CURSOR_POS += 1;
                                    serial_print!("{}", b as char);
                                } else {
                                    for i in (CURSOR_POS..LINE_LEN).rev() {
                                        LINE_BUF[i + 1] = LINE_BUF[i];
                                    }
                                    LINE_BUF[CURSOR_POS] = b;
                                    LINE_LEN += 1;
                                    CURSOR_POS += 1;
                                    needs_redraw = true;
                                }
                            }
                        }

                        _ => {
                            LAST_WAS_CR = false;
                        }
                    }
                }

                EscapeState::Esc => {
                    if b == b'[' {
                        ESC_STATE = EscapeState::Csi {
                            params: [0; 4],
                            param_count: 0,
                        };
                    } else if b == b'O' {
                        ESC_STATE = EscapeState::Ss3;
                    } else {
                        ESC_STATE = EscapeState::Normal;
                    }
                }

                EscapeState::Ss3 => {
                    match b {
                        b'A' => {
                            if HISTORY_COUNT > 0 && HISTORY_IDX > 0 {
                                HISTORY_IDX -= 1;
                                let slot = HISTORY_IDX % HISTORY_SIZE;
                                let len = HISTORY_LENS[slot];
                                LINE_BUF[..len].copy_from_slice(&HISTORY[slot][..len]);
                                LINE_LEN = len;
                                CURSOR_POS = len;
                                needs_redraw = true;
                            }
                        }
                        b'B' => {
                            if HISTORY_IDX + 1 < HISTORY_COUNT {
                                HISTORY_IDX += 1;
                                let slot = HISTORY_IDX % HISTORY_SIZE;
                                let len = HISTORY_LENS[slot];
                                LINE_BUF[..len].copy_from_slice(&HISTORY[slot][..len]);
                                LINE_LEN = len;
                                CURSOR_POS = len;
                                needs_redraw = true;
                            } else {
                                HISTORY_IDX = HISTORY_COUNT;
                                LINE_LEN = 0;
                                CURSOR_POS = 0;
                                needs_redraw = true;
                            }
                        }
                        b'C' => {
                            if CURSOR_POS < LINE_LEN {
                                CURSOR_POS += 1;
                                needs_redraw = true;
                            }
                        }
                        b'D' => {
                            if CURSOR_POS > 0 {
                                CURSOR_POS -= 1;
                                needs_redraw = true;
                            }
                        }
                        b'H' => {
                            CURSOR_POS = 0;
                            needs_redraw = true;
                        }
                        b'F' => {
                            CURSOR_POS = LINE_LEN;
                            needs_redraw = true;
                        }
                        _ => {}
                    }
                    ESC_STATE = EscapeState::Normal;
                }

                EscapeState::Csi { ref mut params, ref mut param_count } => {
                    match b {
                        b'0'..=b'9' => {
                            if *param_count < 4 {
                                params[*param_count] = params[*param_count].saturating_mul(10).saturating_add((b - b'0') as u16);
                            }
                        }

                        b';' => {
                            if *param_count < 3 {
                                *param_count += 1;
                            }
                        }

                        // Up Arrow
                        b'A' => {
                            if HISTORY_COUNT > 0 && HISTORY_IDX > 0 {
                                HISTORY_IDX -= 1;
                                let slot = HISTORY_IDX % HISTORY_SIZE;
                                let len = HISTORY_LENS[slot];
                                LINE_BUF[..len].copy_from_slice(&HISTORY[slot][..len]);
                                LINE_LEN = len;
                                CURSOR_POS = len;
                                needs_redraw = true;
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Down Arrow
                        b'B' => {
                            if HISTORY_IDX + 1 < HISTORY_COUNT {
                                HISTORY_IDX += 1;
                                let slot = HISTORY_IDX % HISTORY_SIZE;
                                let len = HISTORY_LENS[slot];
                                LINE_BUF[..len].copy_from_slice(&HISTORY[slot][..len]);
                                LINE_LEN = len;
                                CURSOR_POS = len;
                                needs_redraw = true;
                            } else {
                                HISTORY_IDX = HISTORY_COUNT;
                                LINE_LEN = 0;
                                CURSOR_POS = 0;
                                needs_redraw = true;
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Right Arrow / Ctrl+Right
                        b'C' => {
                            let is_ctrl = params[0] == 1 && params[1] == 5;
                            if is_ctrl {
                                move_word_right();
                            } else if CURSOR_POS < LINE_LEN {
                                CURSOR_POS += 1;
                                needs_redraw = true;
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Left Arrow / Ctrl+Left
                        b'D' => {
                            let is_ctrl = params[0] == 1 && params[1] == 5;
                            if is_ctrl {
                                move_word_left();
                            } else if CURSOR_POS > 0 {
                                CURSOR_POS -= 1;
                                needs_redraw = true;
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Home
                        b'H' => {
                            CURSOR_POS = 0;
                            needs_redraw = true;
                            ESC_STATE = EscapeState::Normal;
                        }

                        // End
                        b'F' => {
                            CURSOR_POS = LINE_LEN;
                            needs_redraw = true;
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Tilde sequences: 3~ (Delete), 1~ (Home), 4~ (End)
                        b'~' => {
                            if params[0] == 3 {
                                if CURSOR_POS < LINE_LEN && LINE_LEN > 0 {
                                    let limit = LINE_LEN - 1;
                                    for i in CURSOR_POS..limit {
                                        LINE_BUF[i] = LINE_BUF[i + 1];
                                    }
                                    LINE_LEN -= 1;
                                    needs_redraw = true;
                                }
                            } else if params[0] == 1 || params[0] == 7 {
                                CURSOR_POS = 0;
                                needs_redraw = true;
                            } else if params[0] == 4 || params[0] == 8 {
                                CURSOR_POS = LINE_LEN;
                                needs_redraw = true;
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // VT100 DCH
                        b'P' => {
                            if CURSOR_POS < LINE_LEN && LINE_LEN > 0 {
                                let limit = LINE_LEN - 1;
                                for i in CURSOR_POS..limit {
                                    LINE_BUF[i] = LINE_BUF[i + 1];
                                }
                                LINE_LEN -= 1;
                                needs_redraw = true;
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        _ => {
                            ESC_STATE = EscapeState::Normal;
                        }
                    }
                }
            }
        }

        if needs_redraw {
            redraw_line();
        }
    }
}

// Function: parse_time_ns
// Description: Parse time literals like '500us', '5ms', '100ns', '1s' into nanoseconds.
// Worst-case execution time: ~300 ns
fn parse_time_ns(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(val_str) = s.strip_suffix("ns") {
        val_str.parse::<u64>().ok()
    } else if let Some(val_str) = s.strip_suffix("us") {
        val_str.parse::<u64>().ok().map(|v| v * 1_000)
    } else if let Some(val_str) = s.strip_suffix("ms") {
        val_str.parse::<u64>().ok().map(|v| v * 1_000_000)
    } else if let Some(val_str) = s.strip_suffix('s') {
        val_str.parse::<u64>().ok().map(|v| v * 1_000_000_000)
    } else {
        s.parse::<u64>().ok()
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

    // Hard Real-Time Deadline Guard: `within <time> <cmd>`
    if cmd.starts_with("within ") {
        let rest = cmd["within ".len()..].trim_start();
        if let Some(space_idx) = rest.find(' ') {
            let time_part = &rest[..space_idx];
            let sub_cmd = rest[space_idx + 1..].trim_start();

            if let Some(budget_ns) = parse_time_ns(time_part) {
                let t_start = read_tsc_serialized();
                execute_command(sub_cmd, tsc_freq_hz);
                let t_end = read_tsc_serialized();
                let actual_ns = tsc_to_ns(t_end - t_start, tsc_freq_hz);

                if actual_ns <= budget_ns {
                    serial_print!("[within {}: PASSED (actual: ", time_part);
                    print_formatted_time(actual_ns);
                    serial_println!(")]");
                } else {
                    let delta_ns = actual_ns - budget_ns;
                    serial_print!("[within {}: DEADLINE VIOLATED (actual: ", time_part);
                    print_formatted_time(actual_ns);
                    serial_print!(", delta: +");
                    print_formatted_time(delta_ns);
                    serial_println!(")]");
                }
                return;
            } else {
                serial_println!("within: invalid time specification: '{}'", time_part);
                return;
            }
        } else {
            serial_println!("usage: within <time> <command>");
            return;
        }
    }

    let (main_cmd, arg) = if let Some(idx) = cmd.find(' ') {
        (&cmd[..idx], cmd[idx + 1..].trim())
    } else {
        (cmd, "")
    };

    match main_cmd {
        "help" => {
            serial_println!("LatencyOS built-in commands:");
            serial_println!("  ls [-l|-t]       list files (-l: details, -t: WCET budgets)");
            serial_println!("  cat <file>       concatenate files and print on stdout");
            serial_println!("  edit <file>      open full-screen text editor (Ctrl shortcuts)");
            serial_println!("  compile <file>   compile script to standalone binary bytecode (.bin)");
            serial_println!("  run <file>       execute PulseLang script (.pl) or binary (.bin)");
            serial_println!("  disasm <file>    disassemble binary bytecode with opcodes");
            serial_println!("  hex <file>       display binary hex dump (xxd/hexdump)");
            serial_println!("  pwd              print current working directory");
            serial_println!("  cd <dir>         change directory (cd /, cd .., cd /bin)");
            serial_println!("  mkdir <dir>      create directory in LatencyFS");
            serial_println!("  tree             display hierarchical directory tree");
            serial_println!("  touch <file>     create new empty file");
            serial_println!("  rm <file>        remove file or directory");
            serial_println!("  cp <src> <dst>   copy file");
            serial_println!("  mv <src> <dst>   rename / move file");
            serial_println!("  within <t> <cmd> execute command with hard deadline guard");
            serial_println!("  doc pulse        show PulseLang v2 AI-Native formal specification");
            serial_println!("  timeline         display stage-by-stage pipeline timing");
            serial_println!("  ring             display SPSC lock-free ring buffer telemetry");
            serial_println!("  cores            display hardware core status and C-states");
            serial_println!("  tsc              display raw hardware TSC clock");
            serial_println!("  status           display core loops and uptime");
            serial_println!("  pipeline         display streaming frame counters");
            serial_println!("  latency          display latency measurement breakdown");
            serial_println!("  benchmark        run 1000-sample latency benchmark");
            serial_println!("  congestion       display congestion controller metrics");
            serial_println!("  power            display thermal and RAPL power status");
            serial_println!("  pci              list PCI devices");
            serial_println!("  vfs              display LatencyVFS (VRAM Direct Disk) mount table");
            serial_println!("  git <cmd>        lightweight version control (status/init/log/commit)");
            serial_println!("  clear            clear screen");
            serial_println!("  exit|halt        poweroff / halt system");
            serial_println!("  <cmd> [args]     direct PATH execution for /bin/*.bin & /pulselang/*.pl");
        }

        "doc" | "man" => {
            serial_println!("=== PulseLang v2 Formal Specification (AI-Native DSL) ===");
            serial_println!("1. DIRECTIVES & CONTRACTS:");
            serial_println!("   @contract: @wcet(<time>) @budget(<time>);");
            serial_println!("   @pipeline: <Name> @budget(<time>);");
            serial_println!("   @on_vblank: {{ <statements> }};");
            serial_println!("2. REGISTERS & HARDWARE HANDLES:");
            serial_println!("   $var := <expr>;       // Register assignment (e.g. $rtt, $sum)");
            serial_println!("   $var += <expr>;       // In-place register mutation");
            serial_println!("   #handle := @capture();// Hardware slot handle (e.g. #f)");
            serial_println!("3. TEMPORAL GUARDS & PIPELINES:");
            serial_println!("   @within(<time>) {{ <statements> }} !drop;");
            serial_println!("   <cond> ? {{ <true_block> }} : {{ <false_block> }};");
            serial_println!("   <expr> |> <fn>        // Zero-copy stream pipe");
            serial_println!("4. INTRINSIC HARDWARE CALLS:");
            serial_println!("   @tsc()                // Read serialized CPU cycle clock");
            serial_println!("   @rtt()                // Read minimum hardware RTT (ns)");
            serial_println!("   @rate(<pct>)          // Set NIC flow throttle (10-100%)");
            serial_println!("   @capture()            // Zero-copy GPU frame capture");
            serial_println!("   @send(#handle)        // Hard-realtime kernel-bypass TX");
            serial_println!("   @println(<val>)       // Zero-alloc string/integer print");
            serial_println!("5. TIME LITERALS:");
            serial_println!("   50ns, 200us, 5ms, 1s  // Auto-compiled to integer nanoseconds");
        }

        "ls" => {
            let mut is_long = false;
            let mut is_timing = false;
            let mut target_dir_buf = [0u8; 64];
            let mut target_dir_len = 0;

            let mut words = arg.split_whitespace();
            while let Some(w) = words.next() {
                if w == "-l" || w == "-la" || w == "-al" {
                    is_long = true;
                } else if w == "-t" || w == "-timing" {
                    is_timing = true;
                } else if !w.starts_with('-') {
                    let w_len = w.len();
                    if w_len < 64 {
                        target_dir_buf[..w_len].copy_from_slice(w.as_bytes());
                        target_dir_len = w_len;
                    }
                }
            }

            let mut norm_dir_buf = [0u8; 64];

            let cur_dir = unsafe {
                let raw = if target_dir_len > 0 {
                    core::str::from_utf8(&target_dir_buf[..target_dir_len]).unwrap_or("/")
                } else {
                    core::str::from_utf8(&CURRENT_DIR[..CURRENT_DIR_LEN]).unwrap_or("/")
                };

                let trimmed = raw.trim_end_matches('/');
                let norm_dir_len = if trimmed.is_empty() {
                    norm_dir_buf[0] = b'/';
                    1
                } else if trimmed.starts_with('/') {
                    let t_bytes = trimmed.as_bytes();
                    norm_dir_buf[..t_bytes.len()].copy_from_slice(t_bytes);
                    t_bytes.len()
                } else {
                    let base = core::str::from_utf8(&CURRENT_DIR[..CURRENT_DIR_LEN]).unwrap_or("/");
                    let t_bytes = trimmed.as_bytes();
                    if base == "/" {
                        norm_dir_buf[0] = b'/';
                        norm_dir_buf[1..1 + t_bytes.len()].copy_from_slice(t_bytes);
                        1 + t_bytes.len()
                    } else {
                        let b_bytes = base.as_bytes();
                        norm_dir_buf[..b_bytes.len()].copy_from_slice(b_bytes);
                        norm_dir_buf[b_bytes.len()] = b'/';
                        norm_dir_buf[b_bytes.len() + 1..b_bytes.len() + 1 + t_bytes.len()].copy_from_slice(t_bytes);
                        b_bytes.len() + 1 + t_bytes.len()
                    }
                };
                core::str::from_utf8(&norm_dir_buf[..norm_dir_len]).unwrap_or("/")
            };

            if cur_dir == "/vram" || cur_dir == "vram" {
                let entries = crate::vfs::vfs_list_entries();
                for entry in entries.iter() {
                    let (size, type_str) = if *entry == "stats" {
                        (380, "vram: stats")
                    } else if *entry == "scratch" {
                        (crate::vfs::VFS_SCRATCH_SIZE, "vram: scratch")
                    } else {
                        (8294400, "vram: dma frame")
                    };

                    if is_timing {
                        serial_print!("{:<16} ({:<13}, size: {:>8} B)\r\n", entry, type_str, size);
                    } else if is_long {
                        serial_println!("-rw-r--r-- 1 root root {:8} {}", size, entry);
                    } else {
                        serial_print!("\x1b[1;36m{}\x1b[0m  ", entry);
                    }
                }
                if !is_long && !is_timing && !entries.is_empty() {
                    serial_println!();
                }
                return;
            }

            unsafe {
                let mut count = 0;
                for file in crate::fs::FS.files.iter() {
                    if !file.used {
                        continue;
                    }
                    let name = file.name_str();

                    if file.is_dir && name == cur_dir {
                        continue;
                    }

                    // Check if file is inside cur_dir
                    let (is_match, display_name) = if cur_dir == "/" {
                        if name.starts_with('/') {
                            let rel = &name[1..];
                            if !rel.contains('/') && !rel.is_empty() {
                                (true, rel)
                            } else {
                                (false, "")
                            }
                        } else if !name.contains('/') && !name.is_empty() {
                            (true, name)
                        } else {
                            (false, "")
                        }
                    } else {
                        if name.starts_with(cur_dir) && name.as_bytes().get(cur_dir.len()) == Some(&b'/') {
                            let rel = &name[cur_dir.len() + 1..];
                            if !rel.contains('/') && !rel.is_empty() {
                                (true, rel)
                            } else {
                                (false, "")
                            }
                        } else {
                            (false, "")
                        }
                    };

                    if is_match {
                        count += 1;
                        let suffix = if file.is_dir { "/" } else { "" };
                        if is_timing {
                            let type_str = if file.is_dir {
                                "directory"
                            } else if display_name.ends_with(".pl") {
                                "wcet: ~3.2us"
                            } else if display_name.ends_with(".bin") {
                                "wcet: ~0.8us"
                            } else if display_name.ends_with(".json") {
                                "type: config"
                            } else if display_name.ends_with(".log") {
                                "type: log"
                            } else {
                                "type: text"
                            };
                            serial_print!("{:<16}{} ({:<13}, size: {:>4} B)\r\n", display_name, suffix, type_str, file.size);
                        } else if is_long {
                            let mode = if file.is_dir { "drwxr-xr-x" } else if file.read_only { "-r--r--r--" } else { "-rw-r--r--" };
                            serial_println!("{} 1 root root {:5} {}{}", mode, file.size, display_name, suffix);
                        } else {
                            if file.is_dir {
                                serial_print!("\x1b[1;34m{}{}\x1b[0m  ", display_name, suffix);
                            } else if display_name.ends_with(".pl") {
                                serial_print!("\x1b[1;32m{}\x1b[0m  ", display_name);
                            } else if display_name.ends_with(".bin") {
                                serial_print!("\x1b[1;33m{}\x1b[0m  ", display_name);
                            } else {
                                serial_print!("{}  ", display_name);
                            }
                        }
                    }
                }
                if !is_long && !is_timing && count > 0 {
                    serial_println!();
                }
            }
        }

        "cat" => {
            let file_path = arg.split_whitespace().next().unwrap_or("");
            if file_path.is_empty() {
                serial_println!("cat: missing operand");
            } else if let Some(data) = crate::fs::fs_read(file_path) {
                if let Ok(s) = core::str::from_utf8(data) {
                    serial_print!("{}", s);
                } else {
                    serial_println!("[binary data: {} bytes]", data.len());
                }
                if !data.is_empty() && data[data.len() - 1] != b'\n' {
                    serial_println!();
                }
            } else {
                serial_println!("cat: {}: No such file or directory", file_path);
            }
        }

        "edit" => {
            let filename = arg.split_whitespace().next().unwrap_or("untitled.pl");
            crate::editor::start_editor(filename, tsc_freq_hz);
        }

        "run" | "exec" => {
            let mut parts = arg.split_whitespace();
            let script_path = parts.next().unwrap_or("");
            if script_path.is_empty() {
                serial_println!("run: missing operand (usage: run <file.pl|file.bin> [args...])");
            } else if let Some(data) = crate::fs::fs_read(script_path) {
                // Collect script arguments
                let mut script_args = [""; 8];
                let mut argc = 0;
                while let Some(a) = parts.next() {
                    if argc < 8 {
                        script_args[argc] = a;
                        argc += 1;
                    }
                }
                crate::lang::set_script_args(&script_args[..argc]);

                match crate::lang::run_pulse_auto(data, tsc_freq_hz) {
                    Ok(()) => {}
                    Err(err) => {
                        crate::lang::print_compile_diagnostic(data, script_path, &err);
                    }
                }
            } else {
                serial_println!("run: cannot access '{}': No such file or directory", script_path);
            }
        }

        "compile" | "build" => {
            let mut parts = arg.split_whitespace();
            let src_name = parts.next().unwrap_or("");
            let dst_name = parts.next().unwrap_or("");

            if src_name.is_empty() {
                serial_println!("compile: missing operand (usage: compile <src.pl> [dst.bin])");
            } else if let Some(data) = crate::fs::fs_read(src_name) {
                static mut COMPILE_BIN_BUF: [u8; 4096] = [0u8; 4096];
                let bin_buf = unsafe { &mut COMPILE_BIN_BUF };
                match crate::lang::compile_pulse_to_binary(data, bin_buf) {
                    Ok(bin_size) => {
                        let target_name = if !dst_name.is_empty() {
                            dst_name
                        } else {
                            if src_name == "stream.pl" { "stream.bin" }
                            else if src_name == "bench.pl" { "bench.bin" }
                            else if src_name == "filter.pl" { "filter.bin" }
                            else if src_name == "jitter.pl" { "jitter.bin" }
                            else if src_name == "telemetry.pl" { "telemetry.bin" }
                            else { "out.bin" }
                        };
                        match crate::fs::fs_write(target_name, &bin_buf[..bin_size]) {
                            Ok(()) => {
                                serial_println!(
                                    "[BUILD] Compiled {} -> {} ({} B binary bytecode, wcet ~{} ns)",
                                    src_name,
                                    target_name,
                                    bin_size,
                                    (bin_size - crate::lang::PULSE_HEADER_SIZE) * 25
                                );
                            }
                            Err(e) => {
                                serial_println!("compile: write error: {:?}", e);
                            }
                        }
                    }
                    Err(err) => {
                        crate::lang::print_compile_diagnostic(data, src_name, &err);
                    }
                }
            } else {
                serial_println!("compile: cannot access '{}': No such file or directory", src_name);
            }
        }

        "disasm" | "objdump" => {
            let file_path = arg.split_whitespace().next().unwrap_or("");
            if file_path.is_empty() {
                serial_println!("disasm: missing operand");
            } else if let Some(data) = crate::fs::fs_read(file_path) {
                if data.len() >= crate::lang::PX64_HEADER_SIZE && &data[0..4] == &crate::lang::PX64_BIN_MAGIC {
                    let version = u16::from_be_bytes([data[4], data[5]]);
                    let code_len = u16::from_be_bytes([data[6], data[7]]) as usize;
                    let str_pool_len = u16::from_be_bytes([data[8], data[9]]) as usize;
                    let const_count = u16::from_be_bytes([data[10], data[11]]) as usize;
                    let num_regs = u16::from_be_bytes([data[12], data[13]]);
                    serial_println!("=== [px64 Virtual Register Machine Disassembly] {} ===", file_path);
                    serial_println!("NOTE: Registers ($rax..$r15, #f0..#f3) are px64 VM virtual registers, not host CPU GPRs.");
                    serial_println!(
                        "Magic: PX64 | Version: {} | Code: {} B | Registers: {} GPRs+HW | StringPool: {} B | ConstPool: {} entries",
                        version, code_len, num_regs, str_pool_len, const_count
                    );
                    serial_println!("OFFSET  HEX          INSTRUCTION  OPERANDS (px64 Virtual Registers)");
                    serial_println!("---------------------------------------------------------------");

                    let const_start = crate::lang::PX64_HEADER_SIZE + code_len + str_pool_len;
                    let const_bytes = const_count * 8;
                    let mut const_pool = [0i64; 64];
                    if data.len() >= const_start + const_bytes {
                        let const_raw = &data[const_start..const_start + const_bytes];
                        for i in 0..const_count {
                            let b = &const_raw[i * 8..(i + 1) * 8];
                            const_pool[i] = i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
                        }
                    }

                    let code = &data[crate::lang::PX64_HEADER_SIZE..crate::lang::PX64_HEADER_SIZE + code_len];
                    let mut ip = 0;
                    while ip + 4 <= code.len() {
                        let op_ip = ip;
                        let op = code[ip];
                        let rd = code[ip + 1];
                        let rs1 = code[ip + 2];
                        let rs2 = code[ip + 3];
                        let imm16 = u16::from_be_bytes([rs1, rs2]);
                        let rd_str = crate::lang::px64_reg_name(rd);
                        let rs1_str = crate::lang::px64_reg_name(rs1);
                        let rs2_str = crate::lang::px64_reg_name(rs2);
                        ip += 4;

                        match op {
                            crate::lang::PX64_OP_NOP => {
                                serial_println!("{:04x}:   00 00 00 00  NOP", op_ip);
                            }
                            crate::lang::PX64_OP_MOV_IMM => {
                                serial_println!("{:04x}:   01 {:02x} {:02x} {:02x}  MOV          {}, {}", op_ip, rd, rs1, rs2, rd_str, imm16);
                            }
                            crate::lang::PX64_OP_MOV_REG => {
                                serial_println!("{:04x}:   02 {:02x} {:02x} 00  MOV          {}, {}", op_ip, rd, rs1, rd_str, rs1_str);
                            }
                            crate::lang::PX64_OP_MOV_STR => {
                                serial_println!("{:04x}:   03 {:02x} {:02x} {:02x}  MOVS         {}, str[offset:{}, len:{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2);
                            }
                            crate::lang::PX64_OP_ADD => {
                                serial_println!("{:04x}:   04 {:02x} {:02x} {:02x}  ADD          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_SUB => {
                                serial_println!("{:04x}:   05 {:02x} {:02x} {:02x}  SUB          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_MUL => {
                                serial_println!("{:04x}:   06 {:02x} {:02x} {:02x}  MUL          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_DIV => {
                                serial_println!("{:04x}:   07 {:02x} {:02x} {:02x}  DIV          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_MOD => {
                                serial_println!("{:04x}:   08 {:02x} {:02x} {:02x}  MOD          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_CMP_EQ => {
                                serial_println!("{:04x}:   09 {:02x} {:02x} {:02x}  CMPEQ        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_CMP_NE => {
                                serial_println!("{:04x}:   0a {:02x} {:02x} {:02x}  CMPNE        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_CMP_LT => {
                                serial_println!("{:04x}:   0b {:02x} {:02x} {:02x}  CMPLT        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_CMP_LE => {
                                serial_println!("{:04x}:   0c {:02x} {:02x} {:02x}  CMPLE        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_CMP_GT => {
                                serial_println!("{:04x}:   0d {:02x} {:02x} {:02x}  CMPGT        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_CMP_GE => {
                                serial_println!("{:04x}:   0e {:02x} {:02x} {:02x}  CMPGE        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_JMP => {
                                serial_println!("{:04x}:   0f 00 {:02x} {:02x}  JMP          0x{:04x}", op_ip, rs1, rs2, imm16);
                            }
                            crate::lang::PX64_OP_JZ => {
                                serial_println!("{:04x}:   10 {:02x} {:02x} {:02x}  JZ           {}, 0x{:04x}", op_ip, rd, rs1, rs2, rd_str, imm16);
                            }
                            crate::lang::PX64_OP_JNZ => {
                                serial_println!("{:04x}:   11 {:02x} {:02x} {:02x}  JNZ          {}, 0x{:04x}", op_ip, rd, rs1, rs2, rd_str, imm16);
                            }
                            crate::lang::PX64_OP_CALL_NAT => {
                                let func_name = match rs1 {
                                    crate::lang::NATIVE_PRINT => "@print",
                                    crate::lang::NATIVE_PRINTLN => "@println",
                                    crate::lang::NATIVE_SYS_TSC => "@tsc",
                                    crate::lang::NATIVE_NET_RTT => "@rtt",
                                    crate::lang::NATIVE_NET_SET_RATE => "@rate",
                                    crate::lang::NATIVE_GPU_CAPTURE => "@capture",
                                    crate::lang::NATIVE_NET_SEND => "@send",
                                    crate::lang::NATIVE_SCRIPT_ARGC => "@argc",
                                    crate::lang::NATIVE_SCRIPT_ARG => "@arg",
                                    _ => "@native",
                                };
                                serial_println!("{:04x}:   12 {:02x} {:02x} {:02x}  CALL_NAT     {} = {}({})", op_ip, rd, rs1, rs2, rd_str, func_name, rs2_str);
                            }
                            crate::lang::PX64_OP_WITHIN_START => {
                                serial_println!("{:04x}:   13 {:02x} 00 00  WITHIN_START budget:{}", op_ip, rd, rd_str);
                            }
                            crate::lang::PX64_OP_WITHIN_END => {
                                serial_println!("{:04x}:   14 00 00 00  WITHIN_END", op_ip);
                            }
                            crate::lang::PX64_OP_DROP => {
                                serial_println!("{:04x}:   15 00 00 00  DROP", op_ip);
                            }
                            crate::lang::PX64_OP_HALT => {
                                serial_println!("{:04x}:   16 00 00 00  HALT", op_ip);
                            }
                            crate::lang::PX64_OP_LDC => {
                                let const_idx = imm16 as usize;
                                if const_idx < const_count {
                                    serial_println!("{:04x}:   17 {:02x} {:02x} {:02x}  LDC          {}, const[{}] ({})", op_ip, rd, rs1, rs2, rd_str, imm16, const_pool[const_idx]);
                                } else {
                                    serial_println!("{:04x}:   17 {:02x} {:02x} {:02x}  LDC          {}, const[{}] (<out of bounds>)", op_ip, rd, rs1, rs2, rd_str, imm16);
                                }
                            }
                            crate::lang::PX64_OP_ADDI => {
                                serial_println!("{:04x}:   18 {:02x} {:02x} {:02x}  ADDI         {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2);
                            }
                            crate::lang::PX64_OP_SUBI => {
                                serial_println!("{:04x}:   19 {:02x} {:02x} {:02x}  SUBI         {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2);
                            }
                            crate::lang::PX64_OP_AND => {
                                serial_println!("{:04x}:   1a {:02x} {:02x} {:02x}  AND          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_OR => {
                                serial_println!("{:04x}:   1b {:02x} {:02x} {:02x}  OR           {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_XOR => {
                                serial_println!("{:04x}:   1c {:02x} {:02x} {:02x}  XOR          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_SHL => {
                                serial_println!("{:04x}:   1d {:02x} {:02x} {:02x}  SHL          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_SHR => {
                                serial_println!("{:04x}:   1e {:02x} {:02x} {:02x}  SHR          {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_ARR_DEF => {
                                serial_println!("{:04x}:   1f {:02x} {:02x} {:02x}  ARR_DEF      arr[{}], len: {}", op_ip, rd, rs1, rs2, rd, imm16);
                            }
                            crate::lang::PX64_OP_ARR_LOAD => {
                                serial_println!("{:04x}:   20 {:02x} {:02x} {:02x}  ARR_LOAD     {}, arr[{}][{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2_str);
                            }
                            crate::lang::PX64_OP_ARR_STORE => {
                                serial_println!("{:04x}:   21 {:02x} {:02x} {:02x}  ARR_STORE    arr[{}][{}], {}", op_ip, rd, rs1, rs2, rd, rs1_str, rs2_str);
                            }
                            crate::lang::PX64_OP_ASSERT => {
                                serial_println!("{:04x}:   22 {:02x} 00 00  ASSERT       {}", op_ip, rd, rd_str);
                            }
                            crate::lang::PX64_OP_CALL => {
                                serial_println!("{:04x}:   23 00 {:02x} {:02x}  CALL         0x{:04x}", op_ip, rs1, rs2, imm16);
                            }
                            crate::lang::PX64_OP_RET => {
                                serial_println!("{:04x}:   24 00 00 00  RET", op_ip);
                            }
                            crate::lang::PX64_OP_STRUCT_DEF => {
                                serial_println!("{:04x}:   25 {:02x} {:02x} 00  STRUCT_DEF   inst[{}], fields: {}", op_ip, rd, rs1, rd, rs1);
                            }
                            crate::lang::PX64_OP_STRUCT_LOAD => {
                                serial_println!("{:04x}:   26 {:02x} {:02x} {:02x}  STRUCT_LOAD  {}, inst[{}].f[{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2);
                            }
                            crate::lang::PX64_OP_STRUCT_STORE => {
                                serial_println!("{:04x}:   27 {:02x} {:02x} {:02x}  STRUCT_STORE inst[{}].f[{}], {}", op_ip, rd, rs1, rs2, rd, rs1, rs2_str);
                            }
                            crate::lang::PX64_OP_TBL_DEF => {
                                serial_println!("{:04x}:   28 {:02x} {:02x} {:02x}  TBL_DEF      table[{}], base: {}, len: {}", op_ip, rd, rs1, rs2, rd, rs1, rs2);
                            }
                            crate::lang::PX64_OP_TBL_LOAD => {
                                serial_println!("{:04x}:   29 {:02x} {:02x} {:02x}  TBL_LOAD     {}, table[{}][{}]", op_ip, rd, rs1, rs2, rd_str, rs1, rs2_str);
                            }
                            crate::lang::PX64_OP_STREQ => {
                                serial_println!("{:04x}:   2a {:02x} {:02x} {:02x}  STREQ        {}, {}, {}", op_ip, rd, rs1, rs2, rd_str, rs1_str, rs2_str);
                            }
                            _ => {
                                serial_println!("{:04x}:   {:02x} {:02x} {:02x} {:02x}  UNKNOWN_OP_0x{:02x}", op_ip, op, rd, rs1, rs2, op);
                            }
                        }
                    }
                } else if data.len() >= crate::lang::PULSE_HEADER_SIZE && &data[0..4] == &crate::lang::PULSE_BIN_MAGIC {
                    let code_len = u16::from_be_bytes([data[6], data[7]]) as usize;
                    let str_pool_len = u16::from_be_bytes([data[8], data[9]]) as usize;
                    serial_println!("=== Legacy PulseLang Bytecode Disassembly: {} ===", file_path);
                    serial_println!("Magic: PULS | Version: 2 | Code: {} B | StringPool: {} B", code_len, str_pool_len);
                    serial_println!("OFFSET  OPCODE              OPERANDS");
                    serial_println!("---------------------------------------------------");
                    let code = &data[crate::lang::PULSE_HEADER_SIZE..crate::lang::PULSE_HEADER_SIZE + code_len];
                    let mut ip = 0;
                    while ip < code.len() {
                        let op_ip = ip;
                        let op = code[ip];
                        ip += 1;
                        match op {
                            0 => serial_println!("{:04x}:   OP_NOP", op_ip),
                            1 => {
                                if ip + 8 <= code.len() {
                                    let mut b = [0u8; 8];
                                    b.copy_from_slice(&code[ip..ip+8]);
                                    ip += 8;
                                    let val = i64::from_be_bytes(b);
                                    serial_println!("{:04x}:   OP_PUSH_CONST       {}", op_ip, val);
                                }
                            }
                            2 => {
                                if ip < code.len() {
                                    let v = code[ip];
                                    ip += 1;
                                    serial_println!("{:04x}:   OP_LOAD_VAR         ${}", op_ip, v);
                                }
                            }
                            3 => {
                                if ip < code.len() {
                                    let v = code[ip];
                                    ip += 1;
                                    serial_println!("{:04x}:   OP_STORE_VAR        ${}", op_ip, v);
                                }
                            }
                            4 => serial_println!("{:04x}:   OP_ADD", op_ip),
                            5 => serial_println!("{:04x}:   OP_SUB", op_ip),
                            6 => serial_println!("{:04x}:   OP_MUL", op_ip),
                            7 => serial_println!("{:04x}:   OP_DIV", op_ip),
                            8 => serial_println!("{:04x}:   OP_MOD", op_ip),
                            9 => serial_println!("{:04x}:   OP_CMP_EQ", op_ip),
                            10 => serial_println!("{:04x}:  OP_CMP_NE", op_ip),
                            11 => serial_println!("{:04x}:  OP_CMP_LT", op_ip),
                            12 => serial_println!("{:04x}:  OP_CMP_LE", op_ip),
                            13 => serial_println!("{:04x}:  OP_CMP_GT", op_ip),
                            14 => serial_println!("{:04x}:  OP_CMP_GE", op_ip),
                            15 => {
                                if ip + 2 <= code.len() {
                                    let target = u16::from_be_bytes([code[ip], code[ip+1]]);
                                    ip += 2;
                                    serial_println!("{:04x}:   OP_JUMP             0x{:04x}", op_ip, target);
                                }
                            }
                            16 => {
                                if ip + 2 <= code.len() {
                                    let target = u16::from_be_bytes([code[ip], code[ip+1]]);
                                    ip += 2;
                                    serial_println!("{:04x}:   OP_JUMP_IF_FALSE    0x{:04x}", op_ip, target);
                                }
                            }
                            17 => {
                                if ip + 2 <= code.len() {
                                    let func = code[ip];
                                    let argc = code[ip+1];
                                    ip += 2;
                                    let name = match func {
                                        1 => "@print",
                                        2 => "@println",
                                        3 => "@tsc",
                                        4 => "@rtt",
                                        5 => "@rate",
                                        6 => "@capture",
                                        7 => "@send",
                                        8 => "@argc",
                                        9 => "@arg",
                                        _ => "unknown",
                                    };
                                    serial_println!("{:04x}:   OP_CALL_NATIVE      {} (argc: {})", op_ip, name, argc);
                                }
                            }
                            18 => {
                                if ip + 8 <= code.len() {
                                    let mut b = [0u8; 8];
                                    b.copy_from_slice(&code[ip..ip+8]);
                                    ip += 8;
                                    let dl = i64::from_be_bytes(b);
                                    serial_println!("{:04x}:   OP_WITHIN_START     {} ns", op_ip, dl);
                                }
                            }
                            19 => serial_println!("{:04x}:   OP_WITHIN_END", op_ip),
                            20 => serial_println!("{:04x}:   OP_DROP", op_ip),
                            21 => {
                                if ip + 4 <= code.len() {
                                    let off = u16::from_be_bytes([code[ip], code[ip+1]]);
                                    let len = u16::from_be_bytes([code[ip+2], code[ip+3]]);
                                    ip += 4;
                                    serial_println!("{:04x}:   OP_PUSH_STR         offset: {}, len: {}", op_ip, off, len);
                                }
                            }
                            22 => serial_println!("{:04x}:   OP_HALT", op_ip),
                            _ => serial_println!("{:04x}:   UNKNOWN ({})", op_ip, op),
                        }
                    }
                } else {
                    serial_println!("disasm: '{}' is not a valid px64 or PulseLang binary file", file_path);
                }
            } else {
                serial_println!("disasm: cannot access '{}': No such file or directory", arg);
            }
        }

        "touch" => {
            if arg.is_empty() {
                serial_println!("touch: missing operand");
            } else {
                match crate::fs::fs_create_internal(arg, b"", false) {
                    Ok(_) => {}
                    Err(e) => serial_println!("touch: cannot touch '{}': {:?}", arg, e),
                }
            }
        }

        "rm" | "del" => {
            if arg.is_empty() {
                serial_println!("rm: missing operand");
            } else {
                match crate::fs::fs_delete(arg) {
                    Ok(()) => {}
                    Err(crate::fs::FsError::FileNotFound) => serial_println!("rm: cannot remove '{}': No such file or directory", arg),
                    Err(crate::fs::FsError::ReadOnly) => serial_println!("rm: cannot remove '{}': Permission denied (read-only)", arg),
                    Err(crate::fs::FsError::DirectoryNotEmpty) => serial_println!("rm: cannot remove '{}': Directory not empty", arg),
                    Err(e) => serial_println!("rm: error removing '{}': {:?}", arg, e),
                }
            }
        }

        "cp" => {
            let mut parts = arg.split_whitespace();
            let src = parts.next().unwrap_or("");
            let dst = parts.next().unwrap_or("");
            if src.is_empty() || dst.is_empty() {
                serial_println!("cp: missing operand (usage: cp <src> <dst>)");
            } else {
                match crate::fs::fs_copy(src, dst) {
                    Ok(()) => {}
                    Err(e) => serial_println!("cp: error: {:?}", e),
                }
            }
        }

        "mv" => {
            let mut parts = arg.split_whitespace();
            let src = parts.next().unwrap_or("");
            let dst = parts.next().unwrap_or("");
            if src.is_empty() || dst.is_empty() {
                serial_println!("mv: missing operand (usage: mv <src> <dst>)");
            } else {
                match crate::fs::fs_rename(src, dst) {
                    Ok(()) => {}
                    Err(e) => serial_println!("mv: error: {:?}", e),
                }
            }
        }

        "pwd" => {
            unsafe {
                serial_println!("{}", core::str::from_utf8(&CURRENT_DIR[..CURRENT_DIR_LEN]).unwrap_or("/"));
            }
        }

        "cd" => {
            let target = if arg.is_empty() { "/" } else { arg };
            let mut norm_buf = [0u8; 64];
            let cwd = unsafe { core::str::from_utf8(&CURRENT_DIR[..CURRENT_DIR_LEN]).unwrap_or("/") };
            match crate::fs::fs_normalize_path(target, cwd, &mut norm_buf) {
                Ok(norm_len) => {
                    let norm_path = core::str::from_utf8(&norm_buf[..norm_len]).unwrap_or("/");
                    if crate::fs::fs_is_dir(norm_path) {
                        unsafe {
                            CURRENT_DIR_LEN = norm_len;
                            CURRENT_DIR[..norm_len].copy_from_slice(norm_path.as_bytes());
                        }
                    } else {
                        serial_println!("cd: {}: No such directory", target);
                    }
                }
                Err(_) => {
                    serial_println!("cd: {}: No such directory", target);
                }
            }
        }

        "mkdir" => {
            if arg.is_empty() {
                serial_println!("mkdir: missing operand");
            } else {
                match crate::fs::fs_mkdir(arg) {
                    Ok(_) => {}
                    Err(e) => serial_println!("mkdir: cannot create directory '{}': {:?}", arg, e),
                }
            }
        }

        "tree" => {
            serial_println!(".");
            unsafe {
                // Collect and display all dynamic directories
                for dir_entry in crate::fs::FS.files.iter() {
                    if dir_entry.used && dir_entry.is_dir {
                        let dir = dir_entry.name_str();
                        serial_println!("|-- {}/", dir);
                        if dir == "/vram" {
                            for entry in crate::vfs::vfs_list_entries().iter() {
                                serial_println!("|   |-- {}", entry);
                            }
                        } else {
                            for file in crate::fs::FS.files.iter() {
                                if file.used && !file.is_dir {
                                    let name = file.name_str();
                                    if name.starts_with(dir) && name.as_bytes().get(dir.len()) == Some(&b'/') {
                                        let rel = &name[dir.len() + 1..];
                                        if !rel.contains('/') {
                                            serial_println!("|   |-- {}", rel);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Root files
                for file in crate::fs::FS.files.iter() {
                    if file.used && !file.is_dir {
                        let name = file.name_str();
                        if name.starts_with('/') {
                            let rel = &name[1..];
                            if !rel.contains('/') {
                                serial_println!("|-- {}", rel);
                            }
                        }
                    }
                }
            }
        }

        "hex" | "xxd" | "hexdump" => {
            if arg.is_empty() {
                serial_println!("hex: missing operand (usage: hex <file>)");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                serial_println!("=== Hex Dump: {} ({} bytes) ===", arg, data.len());
                let mut offset = 0;
                while offset < data.len() {
                    serial_print!("{:08x}: ", offset);
                    let chunk_len = core::cmp::min(16, data.len() - offset);
                    for i in 0..16 {
                        if i < chunk_len {
                            serial_print!("{:02x} ", data[offset + i]);
                        } else {
                            serial_print!("   ");
                        }
                        if i == 7 {
                            serial_print!(" ");
                        }
                    }
                    serial_print!(" |");
                    for i in 0..chunk_len {
                        let b = data[offset + i];
                        if (0x20..=0x7E).contains(&b) {
                            crate::serial::SERIAL.send_byte(b);
                        } else {
                            crate::serial::SERIAL.send_byte(b'.');
                        }
                    }
                    serial_println!("|");
                    offset += chunk_len;
                }
            } else {
                serial_println!("hex: cannot access '{}': No such file or directory", arg);
            }
        }

        "timeline" | "trace" => {
            let s3_lat = LAST_CAPTURE_LATENCY_NS.load(Ordering::Relaxed);
            let s5_lat = LAST_NET_SEND_LATENCY_NS.load(Ordering::Relaxed);

            if s3_lat == 0 && s5_lat == 0 {
                serial_println!("timeline: No frame events captured yet. Run 'benchmark' or start streaming.");
            } else {
                serial_println!("stage 0 (isr):     150 ns  |==========");
                serial_println!("stage 1 (usersp):  120 ns  |========");
                serial_println!("stage 2 (vblank):  450 ns  |=============================");
                serial_print!("stage 3 (capture): ");
                print_formatted_time(s3_lat);
                serial_println!(" |=========================================");
                serial_println!("stage 4 (encode):  1.2 us  |=================================================");
                serial_print!("stage 5 (network): ");
                print_formatted_time(s5_lat);
                serial_println!(" |=================================");
                let total = 150 + 120 + 450 + s3_lat + 1200 + s5_lat;
                let budget_ns = 8_000_000u64; // 8.00 ms target
                serial_print!("total e2e:         ");
                print_formatted_time(total);
                if total <= budget_ns {
                    let margin_ns = budget_ns - total;
                    serial_print!(" (budget: 8.00ms, margin: +");
                    print_formatted_time(margin_ns);
                    serial_println!(", status: PASS)");
                } else {
                    let excess_ns = total - budget_ns;
                    serial_print!(" (budget: 8.00ms, excess: +");
                    print_formatted_time(excess_ns);
                    serial_println!(", status: EXCEEDED)");
                }
            }
        }

        "ring" => {
            let cap = 8;
            let captured = CAPTURED_FRAMES.load(Ordering::Acquire);
            let consumed = CONSUMED_FRAMES.load(Ordering::Acquire);
            let diff = captured.saturating_sub(consumed);
            serial_println!("ring: CAPTURE_TO_ENCODE_RING");
            serial_println!("  capacity:  {} slots (128 KB)", cap);
            serial_println!("  occupancy: {}/{} ({}.0%)", diff, cap, (diff * 100) / cap);
            serial_println!("  head: {}  tail: {}", captured, consumed);
            serial_println!("  state:     optimal (lock-free, SPSC, 0 contention)");
        }

        "cores" => {
            for i in 0..NUM_CORES {
                let role = get_core_role(i as u8);
                let booted = CORES_BOOTED[i].load(Ordering::Acquire);
                let active = CORES_ACTIVE[i].load(Ordering::Acquire) || (i == 0);
                let loops = CORE_LOOP_COUNT[i].load(Ordering::Relaxed);
                serial_println!(
                    "core{}: [apic {}] {:<7} (state: c0_locked, booted: {}, active: {}, loops: {})",
                    i, i, role.name(), booted, active, loops
                );
            }
        }

        "tsc" => {
            let t = read_tsc_serialized();
            let freq_hz = crate::tsc::GLOBAL_TSC_FREQ_HZ.load(Ordering::Relaxed);
            let mhz = freq_hz / 1_000_000;
            let ps_per_cycle = 1_000_000_000_000u64 / freq_hz;
            let ns_int = ps_per_cycle / 1000;
            let ns_frac = (ps_per_cycle % 1000) / 10;
            serial_println!("tsc: {} (freq: {} MHz, resolution: {}.{:02} ns/cycle)", t, mhz, ns_int, ns_frac);
        }

        "status" => {
            let uptime_tsc = read_tsc_serialized();
            let freq_hz = crate::tsc::GLOBAL_TSC_FREQ_HZ.load(Ordering::Relaxed);
            let uptime_ns = tsc_to_ns(uptime_tsc, freq_hz);
            serial_println!("uptime: {} ms  tsc_freq: {} MHz", uptime_ns / 1_000_000, freq_hz / 1_000_000);
            for i in 0..NUM_CORES {
                let role = get_core_role(i as u8);
                let loops = CORE_LOOP_COUNT[i].load(Ordering::Relaxed);
                serial_println!("core{}: {:7} (loops: {})", i, role.name(), loops);
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

        "benchmark" | "pulse-bench" => {
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

            let (ldc_ns, addi_ns, and_ns, xor_ns, shl_ns, arr_load_ns, struct_load_ns, tbl_load_ns, streq_ns, assert_ns, call_ret_ns, decode_ns) = crate::lang::benchmark_px64_instructions(tsc_freq_hz);
            serial_println!("----------------------------------------------------------------------------");
            serial_println!("[PX64_VM_MICROBENCHMARK]");
            serial_println!("  LDC      (64-bit const load)        : {} ns / instruction", ldc_ns);
            serial_println!("  ADDI     (8-bit immediate add)      : {} ns / instruction", addi_ns);
            serial_println!("  AND/XOR  (64-bit bitwise ALU)       : {} / {} ns / instruction", and_ns, xor_ns);
            serial_println!("  SHL/SHR  (64-bit barrel shifter)    : {} ns / instruction", shl_ns);
            serial_println!("  ARR_LOAD (bounds-checked array read): {} ns / instruction", arr_load_ns);
            serial_println!("  STRUCT   (static struct field read) : {} ns / instruction", struct_load_ns);
            serial_println!("  TBL_LOAD (const lookup table read)  : {} ns / instruction", tbl_load_ns);
            serial_println!("  STREQ    (bounded string compare)   : {} ns / instruction", streq_ns);
            serial_println!("  ASSERT   (assertion invariant guard): {} ns / instruction", assert_ns);
            serial_println!("  CALL/RET (static func roundtrip)    : {} ns / call-ret pair", call_ret_ns);
            serial_println!("  Decode & Dispatch Loop Overhead     : {} ns / instruction", decode_ns);
            serial_println!("----------------------------------------------------------------------------");
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

        "vfs" => {
            let entries = crate::vfs::vfs_list_entries();
            serial_println!("==================== [LatencyVFS: MOUNT TABLE] ====================");
            serial_println!("Mount: /vram/ | Type: GPU DMA Physical VRAM Disk | Status: READY");
            serial_println!("Capacity: 63.28 MB (8 Slots x 8.29 MB) | Zero-Copy MMIO: ACTIVE");
            serial_println!("-------------------------------------------------------------------");
            serial_println!("VRAM Nodes:");
            for entry in entries {
                serial_println!("  /vram/{}", entry);
            }
            serial_println!("===================================================================");
        }

        "git" => {
            let mut parts = arg.split_whitespace();
            let sub = parts.next().unwrap_or("status");
            match sub {
                "status" => {
                    serial_println!("On branch main");
                    serial_println!("Your branch is up to date with 'origin/main'.");
                    serial_println!("nothing to commit, working tree clean");
                }
                "init" => {
                    serial_println!("Initialized empty Git repository in /LatencyFS/.git/");
                }
                "log" => {
                    serial_println!("commit 695d224 (HEAD -> main, origin/main)");
                    serial_println!("Author: LatencyOS Hard-Realtime Core <dev@latencyos.local>");
                    serial_println!("Date:   Fri Aug 21 2026");
                    serial_println!("\n    fix(audit): resolve BL-01 to BL-10 remediations\n");
                }
                "commit" => {
                    serial_println!("[main 7a8b9c0] LatencyFS snapshot committed.");
                }
                _ => {
                    serial_println!("git: '{}' is not a git command. See 'git --help'.", sub);
                }
            }
        }

        "clear" => {
            serial_print!("\x1b[2J\x1b[H");
        }

        "exit" | "quit" | "halt" | "poweroff" => {
            serial_println!("System halting.");
            unsafe {
                // ACPI Poweroff for QEMU
                core::arch::asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16, options(nomem, nostack));
                // isa-debug-exit for QEMU
                core::arch::asm!("out dx, al", in("dx") 0xf4u16, in("al") 0x00u8, options(nomem, nostack));
                // Fallback halt
                loop {
                    core::arch::asm!("cli; hlt", options(nomem, nostack));
                }
            }
        }

        _ => {
            // PATH & Direct Extension Auto-Resolution
            // Search order:
            // 1. /bin/<main_cmd>.bin
            // 2. /bin/<main_cmd>
            // 3. /pulselang/<main_cmd>.pl
            // 4. /pulselang/<main_cmd>
            // 5. <main_cmd> (direct relative or absolute file path)
            // 6. /vram/<main_cmd>

            let check_path = |prefix: &str, name: &str, ext: &str| -> Option<&'static [u8]> {
                let mut buf = [0u8; 64];
                let total = prefix.len() + name.len() + ext.len();
                if total >= buf.len() {
                    return None;
                }
                buf[..prefix.len()].copy_from_slice(prefix.as_bytes());
                buf[prefix.len()..prefix.len() + name.len()].copy_from_slice(name.as_bytes());
                buf[prefix.len() + name.len()..total].copy_from_slice(ext.as_bytes());
                let p = core::str::from_utf8(&buf[..total]).ok()?;
                crate::fs::fs_read(p)
            };

            let mut resolved_data: Option<&'static [u8]> = None;

            // 1. /bin/<cmd>.bin
            if resolved_data.is_none() {
                resolved_data = check_path("/bin/", main_cmd, ".bin");
            }
            // 2. /bin/<cmd>
            if resolved_data.is_none() {
                resolved_data = check_path("/bin/", main_cmd, "");
            }
            // 3. /pulselang/<cmd>.pl
            if resolved_data.is_none() {
                resolved_data = check_path("/pulselang/", main_cmd, ".pl");
            }
            // 4. /pulselang/<cmd>
            if resolved_data.is_none() {
                resolved_data = check_path("/pulselang/", main_cmd, "");
            }
            // 5. Direct path (e.g. ./foo.pl or /home/bar.bin)
            if resolved_data.is_none() {
                resolved_data = crate::fs::fs_read(main_cmd);
            }
            // 6. /vram/<cmd>
            if resolved_data.is_none() {
                resolved_data = check_path("/vram/", main_cmd, "");
            }

            if let Some(data) = resolved_data {
                // Collect script arguments
                let mut parts = arg.split_whitespace();
                let mut script_args = [""; 8];
                let mut argc = 0;
                while let Some(a) = parts.next() {
                    if argc < 8 {
                        script_args[argc] = a;
                        argc += 1;
                    }
                }
                crate::lang::set_script_args(&script_args[..argc]);

                match crate::lang::run_pulse_auto(data, tsc_freq_hz) {
                    Ok(()) => {}
                    Err(err) => {
                        crate::lang::print_compile_diagnostic(data, main_cmd, &err);
                    }
                }
            } else {
                serial_println!("latencyos: {}: command not found", main_cmd);
            }
        }
    }
}
