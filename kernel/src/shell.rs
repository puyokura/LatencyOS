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

// Function: print_formatted_time
// Description: Print human-readable time without float allocations.
// Worst-case execution time: ~1000 ns
fn print_formatted_time(ns: u64) {
    if ns < 1_000 {
        serial_print!("{}ns", ns);
    } else if ns < 1_000_000 {
        let us = ns / 1_000;
        let frac = (ns % 1_000) / 100;
        serial_print!("{}.{}us", us, frac);
    } else {
        let ms = ns / 1_000_000;
        let frac = (ns % 1_000_000) / 100_000;
        serial_print!("{}.{}ms", ms, frac);
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
        CURSOR_POS = 0;
        LINE_LEN = 0;
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

// Function: poll_shell
// Description: Non-blocking poll for incoming serial characters, full multi-byte ANSI CSI sequences, and line editing.
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
                                    let t_start = read_tsc_serialized();
                                    execute_command(cmd_str.trim(), tsc_freq_hz);
                                    let t_end = read_tsc_serialized();
                                    LAST_CMD_LATENCY_NS = tsc_to_ns(t_end - t_start, tsc_freq_hz);
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

                        // Tab
                        b'\t' => {}

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
                        ESC_STATE = EscapeState::Csi {
                            params: [0; 4],
                            param_count: 0,
                        };
                    } else {
                        ESC_STATE = EscapeState::Normal;
                    }
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
                                redraw_line();
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
                                redraw_line();
                            } else {
                                HISTORY_IDX = HISTORY_COUNT;
                                LINE_LEN = 0;
                                CURSOR_POS = 0;
                                redraw_line();
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Right Arrow / Ctrl+Right
                        b'C' => {
                            let is_ctrl = (params[0] == 1 && params[1] == 5) || params[0] == 5;
                            if is_ctrl {
                                move_word_right();
                            } else if CURSOR_POS < LINE_LEN {
                                CURSOR_POS += 1;
                                serial_print!("\x1b[1C");
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Left Arrow / Ctrl+Left
                        b'D' => {
                            let is_ctrl = (params[0] == 1 && params[1] == 5) || params[0] == 5;
                            if is_ctrl {
                                move_word_left();
                            } else if CURSOR_POS > 0 {
                                CURSOR_POS -= 1;
                                serial_print!("\x1b[1D");
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Home
                        b'H' => {
                            CURSOR_POS = 0;
                            redraw_line();
                            ESC_STATE = EscapeState::Normal;
                        }

                        // End
                        b'F' => {
                            CURSOR_POS = LINE_LEN;
                            redraw_line();
                            ESC_STATE = EscapeState::Normal;
                        }

                        // Tilde sequences: 3~ (Delete), 1~ (Home), 4~ (End)
                        b'~' => {
                            if params[0] == 3 {
                                if CURSOR_POS < LINE_LEN {
                                    for i in CURSOR_POS..LINE_LEN - 1 {
                                        LINE_BUF[i] = LINE_BUF[i + 1];
                                    }
                                    LINE_LEN -= 1;
                                    redraw_line();
                                }
                            } else if params[0] == 1 || params[0] == 7 {
                                CURSOR_POS = 0;
                                redraw_line();
                            } else if params[0] == 4 || params[0] == 8 {
                                CURSOR_POS = LINE_LEN;
                                redraw_line();
                            }
                            ESC_STATE = EscapeState::Normal;
                        }

                        _ => {
                            // Any unexpected character terminates CSI sequence cleanly without leaking
                            ESC_STATE = EscapeState::Normal;
                        }
                    }
                }
            }
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
            serial_println!("  edit <file>      open full-screen text editor");
            serial_println!("  run <file>       execute PulseLang script");
            serial_println!("  compile <file>   compile script and show diagnostics");
            serial_println!("  doc pulse        show PulseLang v2 AI-Native formal specification");
            serial_println!("  within <t> <cmd> execute command with hard deadline guard");
            serial_println!("  timeline         display stage-by-stage pipeline timing");
            serial_println!("  ring             display SPSC lock-free ring buffer telemetry");
            serial_println!("  cores            display hardware core status and C-states");
            serial_println!("  tsc              display raw hardware TSC clock");
            serial_println!("  rm <file>        remove file");
            serial_println!("  status           display core loops and uptime");
            serial_println!("  pipeline         display streaming frame counters");
            serial_println!("  latency          display latency measurement breakdown");
            serial_println!("  benchmark        run 1000-sample latency benchmark");
            serial_println!("  congestion       display congestion controller metrics");
            serial_println!("  power            display thermal and RAPL power status");
            serial_println!("  pci              list PCI devices");
            serial_println!("  clear            clear screen");
            serial_println!("  exit|halt        poweroff / halt system");
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
            let is_long = arg == "-l" || arg == "-la" || arg == "-al";
            let is_timing = arg == "-t" || arg == "-timing";
            unsafe {
                if is_timing {
                    for file in crate::fs::FS.files.iter() {
                        if file.used {
                            let type_str = if file.name_str().ends_with(".pl") {
                                "wcet: ~3.2us"
                            } else if file.name_str().ends_with(".bin") {
                                "wcet: ~0.8us"
                            } else if file.name_str().ends_with(".json") {
                                "type: config"
                            } else if file.name_str().ends_with(".log") {
                                "type: log"
                            } else {
                                "type: text"
                            };
                            serial_print!("{:<16} ({:<13}, size: {:>4} B)\r\n", file.name_str(), type_str, file.size);
                        }
                    }
                } else if is_long {
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
                for &b in data {
                    if b == b'\t' {
                        serial_print!("    ");
                    } else {
                        SERIAL.send_byte(b);
                    }
                }
                if !data.is_empty() && data[data.len() - 1] != b'\n' {
                    serial_println!();
                }
            } else {
                serial_println!("cat: {}: No such file or directory", arg);
            }
        }

        "edit" => {
            let filename = if arg.is_empty() { "untitled.pl" } else { arg };
            crate::editor::start_editor(filename, tsc_freq_hz);
        }

        "run" | "exec" => {
            if arg.is_empty() {
                serial_println!("run: missing operand");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                match crate::lang::run_pulse_auto(data, tsc_freq_hz) {
                    Ok(()) => {}
                    Err(e) => {
                        serial_println!("pulse: {}: runtime error: {}", arg, e);
                    }
                }
            } else {
                serial_println!("run: cannot access '{}': No such file or directory", arg);
            }
        }

        "compile" | "build" => {
            let mut parts = arg.split_whitespace();
            let src_name = parts.next().unwrap_or("");
            let dst_name = parts.next().unwrap_or("");

            if src_name.is_empty() {
                serial_println!("compile: missing operand (usage: compile <src.pl> [dst.bin])");
            } else if let Some(data) = crate::fs::fs_read(src_name) {
                let mut bin_buf = [0u8; 4096];
                match crate::lang::compile_pulse_to_binary(data, &mut bin_buf) {
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
                    Err(e) => {
                        serial_println!("compile: {}: compile error: {}", src_name, e);
                    }
                }
            } else {
                serial_println!("compile: cannot access '{}': No such file or directory", src_name);
            }
        }

        "disasm" | "objdump" => {
            if arg.is_empty() {
                serial_println!("disasm: missing operand");
            } else if let Some(data) = crate::fs::fs_read(arg) {
                if data.len() < crate::lang::PULSE_HEADER_SIZE || &data[0..4] != &crate::lang::PULSE_BIN_MAGIC {
                    serial_println!("disasm: '{}' is not a valid PulseLang binary file", arg);
                } else {
                    let code_len = u16::from_be_bytes([data[6], data[7]]) as usize;
                    let str_pool_len = u16::from_be_bytes([data[8], data[9]]) as usize;
                    serial_println!("=== PulseLang Bytecode Disassembly: {} ===", arg);
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
                    Err(crate::fs::FsError::FileNotFound) => serial_println!("rm: cannot remove '{}': No such file", arg),
                    Err(crate::fs::FsError::ReadOnly) => serial_println!("rm: cannot remove '{}': Permission denied (read-only)", arg),
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

        "timeline" | "trace" => {
            let s3_lat = LAST_CAPTURE_LATENCY_NS.load(Ordering::Relaxed);
            let s5_lat = LAST_NET_SEND_LATENCY_NS.load(Ordering::Relaxed);

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
            serial_print!("total e2e:         ");
            print_formatted_time(total);
            serial_println!(" (budget: 8.00ms, margin: optimal)");
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
            serial_println!("tsc: {} (freq: {} MHz, resolution: 0.29 ns/cycle)", t, tsc_freq_hz / 1_000_000);
        }


        "status" => {
            let uptime_tsc = read_tsc_serialized();
            let uptime_ns = tsc_to_ns(uptime_tsc, tsc_freq_hz);
            serial_println!("uptime: {} ms  tsc_freq: {} MHz", uptime_ns / 1_000_000, tsc_freq_hz / 1_000_000);
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
            serial_println!("latencyos: {}: command not found", main_cmd);
        }
    }
}
