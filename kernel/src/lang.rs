// lang.rs - PulseLang v2: AI-Native Temporal Reactive DSL, Compiler & Real-Time VM
//
// Worst-case execution time: Documented per function.

use crate::gpu::{capture_frame_zero_copy, poll_vblank_edge};
use crate::net::{
    stream_send_frame, CONGESTION_RATE_PCT, LAST_RTT_NS,
};
use crate::serial_print;
use crate::serial_println;
use crate::tsc::read_tsc_serialized;
use core::sync::atomic::Ordering;

// -----------------------------------------------------------------------------
// Re-exports from pulselang-core
// -----------------------------------------------------------------------------

pub use pulselang_core::*;

// Global Script Arguments for CLI passing
pub static mut SCRIPT_ARGS: [[u8; 128]; 8] = [[0; 128]; 8];
pub static mut SCRIPT_ARG_LENS: [usize; 8] = [0; 8];
pub static mut SCRIPT_ARGC: usize = 0;

// Function: set_script_args
// Description: Store command-line arguments for PulseLang scripts with zero dynamic allocation.
// Worst-case execution time: ~500 ns
pub fn set_script_args(args: &[&str]) {
    unsafe {
        SCRIPT_ARGC = core::cmp::min(args.len(), 8);
        for (i, arg) in args.iter().take(8).enumerate() {
            let raw = arg.trim_matches('"').trim_matches('\'');
            let len = core::cmp::min(raw.len(), 128);
            SCRIPT_ARGS[i][..len].copy_from_slice(&raw.as_bytes()[..len]);
            SCRIPT_ARG_LENS[i] = len;
        }
    }
}

pub static mut COMPILER_TOKENS: [Token; MAX_TOKENS] = [Token::empty(); MAX_TOKENS];

pub fn compile_pulse_to_binary(src: &[u8], out_buf: &mut [u8]) -> Result<usize, CompileError> {
    let tokens = unsafe { &mut COMPILER_TOKENS };
    pulselang_core::compile_pulse_to_binary_with_tokens(src, tokens, out_buf)
}
// Function: print_compile_diagnostic
// Description: Emit comprehensive, structured, AI-actionable compiler or runtime diagnostic log.
// Worst-case execution time: ~20_000 ns
pub fn print_compile_diagnostic(src: &[u8], filename: &str, err: &CompileError) {
    if is_runtime_error(err.code) {
        print_runtime_diagnostic(filename, err);
        return;
    }

    serial_println!("==================== [PULSELANG COMPILE ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================");
    serial_println!("[ERROR_CODE]: {}", err.code);
    serial_println!("[MESSAGE]: {}", err.message);
    serial_println!("[FILE]: {}", filename);
    serial_println!("[LOCATION]: Line {}, Column {} (ByteOffset: {})", err.line, err.col, err.byte_offset);
    serial_print!("[TOKEN_FOUND]: Kind: {:?}, Value: \"", err.token_kind);
    if err.byte_offset + err.token_len <= src.len() && err.token_len > 0 {
        if let Ok(tok_str) = core::str::from_utf8(&src[err.byte_offset..err.byte_offset + err.token_len]) {
            serial_print!("{}", tok_str);
        }
    }
    serial_println!("\"");
    serial_println!("[EXPECTED]: {}", err.expected);
    serial_println!("[PARSER_STAGE]: {}", err.stage);
    serial_println!("[SOURCE_CONTEXT]:");

    // Print source context with 3-line window and pointer caret
    print_source_context_lines(src, err.line, err.col, err.token_len);

    let hex_start = (err.byte_offset.saturating_sub(16) / 16) * 16;
    let hex_end = core::cmp::min(hex_start + 32, src.len());
    serial_println!("[HEX_DUMP (offset 0x{:04x}..0x{:04x})]:", hex_start, hex_end);
    print_byte_hex_dump(src, hex_start, hex_end);

    serial_println!("[AI_REPAIR_HINT]: {}", err.suggestion);
    serial_println!("=============================================================================================");
}

fn is_runtime_error(code: &str) -> bool {
    code.starts_with("ERR_PX64_") || code.starts_with("ERR_BINARY_") || code.starts_with("ERR_VM_")
}

fn print_runtime_diagnostic(filename: &str, err: &CompileError) {
    serial_println!("==================== [PULSELANG RUNTIME ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================");
    serial_println!("[ERROR_CODE]: {}", err.code);
    serial_println!("[MESSAGE]: {}", err.message);
    serial_println!("[FILE]: {}", filename);
    serial_println!("[EXECUTION_DOMAIN]: px64 Real-Time Register Virtual Machine");

    match err.code {
        "ERR_PX64_TIMEOUT_EXCEEDED" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Wall-Clock Watchdog Deadline Violation");
            serial_println!("[TIMEOUT_LIMIT]: 5,000,000 ns (5.0 ms wall-clock)");
            serial_println!("[ROOT_CAUSE]: Script execution exceeded 5.0ms wall-clock threshold (infinite loop or long-running intrinsics)");
            serial_println!("[AI_REPAIR_HINT]: Bound while loops with finite counter or insert @within temporal deadline guards");
        }
        "ERR_PX64_WCET_EXCEEDED" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Instruction Step Limit Exceeded");
            serial_println!("[STEP_LIMIT]: 10,000 instruction steps (MAX_VM_STEPS)");
            serial_println!("[ROOT_CAUSE]: Pure arithmetic or branching loop executed without terminating within 10,000 steps");
            serial_println!("[AI_REPAIR_HINT]: Ensure loop condition decrements towards termination condition within 10,000 steps");
        }
        "ERR_BINARY_VERSION_MISMATCH" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Binary Version Incompatibility");
            serial_println!("[EXPECTED_VERSION]: PX64 Version 3");
            serial_println!("[ROOT_CAUSE]: Binary was compiled with an incompatible or outdated toolchain version");
            serial_println!("[AI_REPAIR_HINT]: Recompile source file with 'compile <src.pul> <dst.bin>'");
        }
        "ERR_BINARY_TRUNCATED" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Truncated Binary Payload");
            serial_println!("[ROOT_CAUSE]: Binary file payload is smaller than declared header code + string pool + const pool length");
            serial_println!("[AI_REPAIR_HINT]: Re-generate binary artifact or check file system storage integrity");
        }
        "ERR_PX64_CONST_OUT_OF_BOUNDS" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Constant Pool Access Violation");
            serial_println!("[ROOT_CAUSE]: Instruction attempted to load from an invalid 64-bit constant pool index");
            serial_println!("[AI_REPAIR_HINT]: Recompile source file or inspect binary with 'disasm <file.bin>'");
        }
        "ERR_PX64_ARRAY_OUT_OF_BOUNDS" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Fixed-Length Array Boundary Violation");
            serial_println!("[ROOT_CAUSE]: Array index expression evaluated to an index outside [0..N-1]");
            serial_println!("[AI_REPAIR_HINT]: Ensure array indexing expression is bounded with a static for loop (0..N) or bounds check");
        }
        "ERR_PX64_STRUCT_OUT_OF_BOUNDS" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Static Struct Field Access Violation");
            serial_println!("[ROOT_CAUSE]: Struct field offset or instance ID is out of bounds");
            serial_println!("[AI_REPAIR_HINT]: Verify struct field definitions and ensure instance ID is within [0..7]");
        }
        "ERR_PX64_TABLE_OUT_OF_BOUNDS" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Read-Only Const Table Boundary Violation");
            serial_println!("[ROOT_CAUSE]: Const table lookup index evaluated to an index outside [0..N-1]");
            serial_println!("[AI_REPAIR_HINT]: Bound table lookup index with a static range for loop (0..N) or check bounds");
        }
        "ERR_PX64_ASSERTION_FAILED" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Runtime Assertion Contract Failure");
            serial_println!("[ROOT_CAUSE]: @assert() condition evaluated to false (0)");
            serial_println!("[AI_REPAIR_HINT]: Check preceding computational pipeline and verify expected state invariants");
        }
        "ERR_PX64_STACK_OVERFLOW" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Static Call Stack Overflow Violation");
            serial_println!("[STACK_DEPTH_LIMIT]: 8 nested function call frames (MAX_CALL_DEPTH)");
            serial_println!("[ROOT_CAUSE]: Recursion or nested call depth exceeded static 8-frame call stack");
            serial_println!("[AI_REPAIR_HINT]: Eliminate recursive function calls or refactor into static bounded for-loops");
        }
        "ERR_PX64_UNWRAP_FAILED" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Tagged Result Unwrap Fault");
            serial_println!("[ROOT_CAUSE]: Attempted to unwrap an Err tagged value without checking @is_ok()");
            serial_println!("[AI_REPAIR_HINT]: Guard @unwrap($res) with 'if (@is_ok($res))' check");
        }
        "ERR_PX64_INVALID_OPCODE" => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Invalid Opcode Execution Fault");
            serial_println!("[ROOT_CAUSE]: Virtual machine encountered an unrecognized or unregistered instruction opcode");
            serial_println!("[AI_REPAIR_HINT]: Verify compiler code generator or inspect bytecode with 'disasm <file.bin>'");
        }
        _ => {
            serial_println!("[RUNTIME_FAULT_CATEGORY]: Virtual Machine Execution Fault");
            serial_println!("[ROOT_CAUSE]: Virtual machine execution fault or internal VM state corruption");
            serial_println!("[AI_REPAIR_HINT]: Recompile source file or inspect binary with 'disasm <file.bin>'");
        }
    }
    serial_println!("=============================================================================================");
}

fn print_source_context_lines(src: &[u8], error_line: usize, error_col: usize, tok_len: usize) {
    let mut cur_line = 1;
    let mut line_start = 0;
    let mut i = 0;

    while i <= src.len() {
        if i == src.len() || src[i] == b'\n' {
            let line_end = i;
            if cur_line + 1 >= error_line && cur_line <= error_line + 1 {
                let line_bytes = &src[line_start..line_end];
                let line_str = core::str::from_utf8(line_bytes).unwrap_or("");
                if cur_line == error_line {
                    serial_println!("> Line {:3}: {}", cur_line, line_str);
                    serial_print!("         ");
                    let caret_col = error_col.saturating_sub(1);
                    for _ in 0..caret_col {
                        serial_print!(" ");
                    }
                    let caret_len = core::cmp::max(tok_len, 1);
                    for _ in 0..caret_len {
                        serial_print!("^");
                    }
                    serial_println!(" [Syntax Error Here]");
                } else {
                    serial_println!("  Line {:3}: {}", cur_line, line_str);
                }
            }
            cur_line += 1;
            line_start = i + 1;
        }
        i += 1;
    }
}

fn print_byte_hex_dump(src: &[u8], start: usize, end: usize) {
    let mut row_start = start;
    while row_start < end {
        let row_end = core::cmp::min(row_start + 16, src.len());
        serial_print!("  {:08x}: ", row_start);

        for j in 0..16 {
            if row_start + j < row_end {
                serial_print!("{:02x} ", src[row_start + j]);
            } else {
                serial_print!("   ");
            }
        }
        serial_print!(" |");
        for j in 0..16 {
            if row_start + j < row_end {
                let b = src[row_start + j];
                if (0x20..=0x7E).contains(&b) {
                    crate::serial::SERIAL.send_byte(b);
                } else {
                    crate::serial::SERIAL.send_byte(b'.');
                }
            }
        }
        serial_println!("|");
        row_start += 16;
    }
}

// -----------------------------------------------------------------------------
// px64 Real-Time Register Virtual Machine
// -----------------------------------------------------------------------------

pub struct PX64VM<'a> {
    pub code: &'a [u8],
    pub str_pool: &'a [u8],
    pub const_pool: &'a [i64],
    pub ip: usize,
    pub regs: [i64; PX64_NUM_REGISTERS],
    pub call_stack: [u16; MAX_CALL_DEPTH],
    pub frame_stack: [[i64; 16]; MAX_CALL_DEPTH],
    pub call_sp: usize,
    pub deadline_stack: [u64; 8],
    pub dl_sp: usize,
    pub array_slots: [i64; 256],
    pub array_lens: [u16; 8],
    pub array_bases: [u16; 8],
    pub struct_slots: [i64; 256],
    pub struct_field_counts: [u8; 8],
    pub struct_bases: [u16; 8],
    pub table_lens: [u8; 8],
    pub table_bases: [u8; 8],
}

impl<'a> PX64VM<'a> {
    pub fn new(code: &'a [u8], str_pool: &'a [u8], const_pool: &'a [i64]) -> Self {
        Self {
            code,
            str_pool,
            const_pool,
            ip: 0,
            regs: [0; PX64_NUM_REGISTERS],
            call_stack: [0; MAX_CALL_DEPTH],
            frame_stack: [[0; 16]; MAX_CALL_DEPTH],
            call_sp: 0,
            deadline_stack: [0; 8],
            dl_sp: 0,
            array_slots: [0; 256],
            array_lens: [0; 8],
            array_bases: [0; 8],
            struct_slots: [0; 256],
            struct_field_counts: [0; 8],
            struct_bases: [0; 8],
            table_lens: [0; 8],
            table_bases: [0; 8],
        }
    }

    #[inline(always)]
    fn get_str_bytes<'b>(&self, val: i64) -> Option<&'b [u8]>
    where
        'a: 'b,
    {
        if (val & STR_TAG) != 0 {
            let offset = ((val & 0x00FF_FFFF_0000_0000) >> 32) as usize;
            let len = (val & 0xFFFF_FFFF) as usize;
            if offset + len <= self.str_pool.len() {
                Some(&self.str_pool[offset..offset + len])
            } else {
                None
            }
        } else if (val & ARG_TAG) != 0 {
            let idx = (val & 0xFF) as usize;
            unsafe {
                if idx < SCRIPT_ARGC {
                    let len = SCRIPT_ARG_LENS[idx];
                    Some(&SCRIPT_ARGS[idx][..len])
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    // Function: run
    // Description: Execute px64 32-bit fixed instructions with zero heap allocations and bounded WCET.
    // Worst-case execution time: ~60_000 ns
    pub fn run(&mut self, tsc_freq_hz: u64) -> Result<(), CompileError> {
        let start_tsc = read_tsc_serialized();
        let timeout_tsc = start_tsc + crate::tsc::ns_to_tsc(MAX_SCRIPT_TIMEOUT_NS, tsc_freq_hz);
        let mut steps = 0;

        while self.ip + 4 <= self.code.len() {
            if read_tsc_serialized() > timeout_tsc {
                return Err(CompileError::simple(
                    "ERR_PX64_TIMEOUT_EXCEEDED",
                    "Execution exceeded wall-clock execution deadline (watchdog safety violation)",
                ));
            }

            if steps >= MAX_VM_STEPS {
                return Err(CompileError::simple(
                    "ERR_PX64_WCET_EXCEEDED",
                    "Execution exceeded px64 WCET instruction step limit (infinite loop protection)",
                ));
            }

            steps += 1;
            let op = self.code[self.ip];
            let rd = self.code[self.ip + 1] as usize;
            let rs1 = self.code[self.ip + 2] as usize;
            let rs2 = self.code[self.ip + 3] as usize;
            let imm16 = u16::from_be_bytes([self.code[self.ip + 2], self.code[self.ip + 3]]);
            self.ip += 4;

            match op {
                PX64_OP_NOP => {}
                PX64_OP_HALT => break,

                PX64_OP_MOV_IMM => {
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = imm16 as i64;
                    }
                }

                PX64_OP_LDC => {
                    let const_idx = imm16 as usize;
                    if const_idx >= self.const_pool.len() {
                        return Err(CompileError::simple(
                            "ERR_PX64_CONST_OUT_OF_BOUNDS",
                            "Constant pool index out of bounds during LDC execution",
                        ));
                    }
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.const_pool[const_idx];
                    }
                }

                PX64_OP_ADDI => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1].wrapping_add(rs2 as i64);
                    }
                }

                PX64_OP_SUBI => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1].wrapping_sub(rs2 as i64);
                    }
                }

                PX64_OP_AND => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1] & self.regs[rs2];
                    }
                }

                PX64_OP_OR => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1] | self.regs[rs2];
                    }
                }

                PX64_OP_XOR => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1] ^ self.regs[rs2];
                    }
                }

                PX64_OP_SHL => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1].wrapping_shl((self.regs[rs2] & 63) as u32);
                    }
                }

                PX64_OP_SHR => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = (self.regs[rs1] as u64 >> (self.regs[rs2] & 63)) as i64;
                    }
                }

                PX64_OP_ARR_DEF => {
                    let arr_id = rd;
                    let len = imm16 as usize;
                    if arr_id < 8 {
                        self.array_lens[arr_id] = len as u16;
                        let mut base = 0;
                        for i in 0..arr_id {
                            base += self.array_lens[i] as usize;
                        }
                        self.array_bases[arr_id] = base as u16;
                        if base + len <= 256 {
                            self.array_slots[base..base + len].fill(0);
                        }
                    }
                }

                PX64_OP_ARR_LOAD => {
                    let arr_id = rs1;
                    let idx_reg = rs2;
                    let idx = if idx_reg < PX64_NUM_REGISTERS {
                        self.regs[idx_reg]
                    } else {
                        -1
                    };
                    if arr_id >= 8 {
                        return Err(CompileError::simple(
                            "ERR_PX64_ARRAY_INVALID_ID",
                            "Array ID is invalid",
                        ));
                    }
                    let max_len = self.array_lens[arr_id] as i64;
                    if idx < 0 || idx >= max_len {
                        return Err(CompileError::simple(
                            "ERR_PX64_ARRAY_OUT_OF_BOUNDS",
                            "Array element access out of bounds",
                        ));
                    }
                    let base = self.array_bases[arr_id] as usize;
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.array_slots[base + idx as usize];
                    }
                }

                PX64_OP_ARR_STORE => {
                    let arr_id = rd;
                    let idx_reg = rs1;
                    let val_reg = rs2;
                    let idx = if idx_reg < PX64_NUM_REGISTERS {
                        self.regs[idx_reg]
                    } else {
                        -1
                    };
                    let val = if val_reg < PX64_NUM_REGISTERS {
                        self.regs[val_reg]
                    } else {
                        0
                    };
                    if arr_id >= 8 {
                        return Err(CompileError::simple(
                            "ERR_PX64_ARRAY_INVALID_ID",
                            "Array ID is invalid",
                        ));
                    }
                    let max_len = self.array_lens[arr_id] as i64;
                    if idx < 0 || idx >= max_len {
                        return Err(CompileError::simple(
                            "ERR_PX64_ARRAY_OUT_OF_BOUNDS",
                            "Array element access out of bounds",
                        ));
                    }
                    let base = self.array_bases[arr_id] as usize;
                    self.array_slots[base + idx as usize] = val;
                }

                PX64_OP_STRUCT_DEF => {
                    let inst_id = rd as usize;
                    let field_count = rs1 as u8;
                    if inst_id < 8 {
                        self.struct_field_counts[inst_id] = field_count;
                        let mut base = 0u16;
                        for i in 0..inst_id {
                            base += self.struct_field_counts[i] as u16;
                        }
                        self.struct_bases[inst_id] = base;
                    }
                }

                PX64_OP_STRUCT_LOAD => {
                    let inst_id = rs1 as usize;
                    let offset = rs2 as usize;
                    if inst_id >= 8 || offset >= self.struct_field_counts[inst_id] as usize {
                        return Err(CompileError::simple(
                            "ERR_PX64_STRUCT_OUT_OF_BOUNDS",
                            "Struct field offset out of bounds",
                        ));
                    }
                    let base = self.struct_bases[inst_id] as usize;
                    let val = self.struct_slots[base + offset];
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = val;
                    }
                }

                PX64_OP_STRUCT_STORE => {
                    let inst_id = rd as usize;
                    let offset = rs1 as usize;
                    let val_reg = rs2;
                    let val = if val_reg < PX64_NUM_REGISTERS {
                        self.regs[val_reg]
                    } else {
                        0
                    };
                    if inst_id >= 8 || offset >= self.struct_field_counts[inst_id] as usize {
                        return Err(CompileError::simple(
                            "ERR_PX64_STRUCT_OUT_OF_BOUNDS",
                            "Struct field offset out of bounds",
                        ));
                    }
                    let base = self.struct_bases[inst_id] as usize;
                    self.struct_slots[base + offset] = val;
                }

                PX64_OP_TBL_DEF => {
                    let tbl_id = rd as usize;
                    let base_idx = rs1 as u8;
                    let len = rs2 as u8;
                    if tbl_id < 8 {
                        self.table_bases[tbl_id] = base_idx;
                        self.table_lens[tbl_id] = len;
                    }
                }

                PX64_OP_TBL_LOAD => {
                    let tbl_id = rs1 as usize;
                    let idx_reg = rs2;
                    let idx = if idx_reg < PX64_NUM_REGISTERS {
                        self.regs[idx_reg]
                    } else {
                        -1
                    };
                    if tbl_id >= 8 {
                        return Err(CompileError::simple(
                            "ERR_PX64_TABLE_INVALID_ID",
                            "Const table ID is invalid",
                        ));
                    }
                    let max_len = self.table_lens[tbl_id] as i64;
                    if idx < 0 || idx >= max_len {
                        return Err(CompileError::simple(
                            "ERR_PX64_TABLE_OUT_OF_BOUNDS",
                            "Const table lookup index out of bounds",
                        ));
                    }
                    let base = self.table_bases[tbl_id] as usize;
                    let val = self.const_pool[base + idx as usize];
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = val;
                    }
                }

                PX64_OP_ASSERT => {
                    let cond = if rd < PX64_NUM_REGISTERS {
                        self.regs[rd]
                    } else {
                        0
                    };
                    if cond == 0 {
                        return Err(CompileError::simple(
                            "ERR_PX64_ASSERTION_FAILED",
                            "PulseLang assertion failed: condition evaluated to false (0)",
                        ));
                    }
                }

                PX64_OP_CALL => {
                    let target_ip = imm16 as usize;
                    if self.call_sp >= MAX_CALL_DEPTH {
                        return Err(CompileError::simple(
                            "ERR_PX64_STACK_OVERFLOW",
                            "Call stack overflow: exceeded MAX_CALL_DEPTH limit (8 nested calls maximum, unbounded recursion prohibited)",
                        ));
                    }
                    self.call_stack[self.call_sp] = self.ip as u16;
                    self.frame_stack[self.call_sp].copy_from_slice(&self.regs[..16]);
                    self.call_sp += 1;
                    self.ip = target_ip;
                }

                PX64_OP_RET => {
                    if self.call_sp == 0 {
                        break;
                    }
                    self.call_sp -= 1;
                    let return_val = self.regs[0]; // $rax
                    let return_ip = self.call_stack[self.call_sp] as usize;
                    self.regs[..16].copy_from_slice(&self.frame_stack[self.call_sp]);
                    self.regs[0] = return_val;
                    self.ip = return_ip;
                }

                PX64_OP_MOV_REG => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1];
                    }
                }

                PX64_OP_MOV_STR => {
                    let offset = rs1;
                    let len = rs2;
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] =
                            STR_TAG | (((offset as u64) as i64) << 32) | ((len as u64) as i64);
                    }
                }

                PX64_OP_ADD => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1].wrapping_add(self.regs[rs2]);
                    }
                }

                PX64_OP_SUB => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1].wrapping_sub(self.regs[rs2]);
                    }
                }

                PX64_OP_MUL => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1].wrapping_mul(self.regs[rs2]);
                    }
                }

                PX64_OP_DIV => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        let denom = self.regs[rs2];
                        if denom == 0 {
                            return Err(CompileError::simple(
                                "ERR_PX64_DIV_BY_ZERO",
                                "Division by zero in px64 virtual register machine",
                            ));
                        }
                        self.regs[rd] = self.regs[rs1].wrapping_div(denom);
                    }
                }

                PX64_OP_MOD => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        let denom = self.regs[rs2];
                        if denom == 0 {
                            return Err(CompileError::simple(
                                "ERR_PX64_DIV_BY_ZERO",
                                "Modulo by zero in px64 virtual register machine",
                            ));
                        }
                        self.regs[rd] = self.regs[rs1].wrapping_rem(denom);
                    }
                }

                PX64_OP_CMP_EQ => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        let v1 = self.regs[rs1];
                        let v2 = self.regs[rs2];
                        let eq = if v1 == v2 {
                            1
                        } else if ((v1 & STR_TAG) != 0 || (v1 & ARG_TAG) != 0)
                            && ((v2 & STR_TAG) != 0 || (v2 & ARG_TAG) != 0)
                        {
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {
                                (Some(b1), Some(b2)) => {
                                    if b1 == b2 {
                                        1
                                    } else {
                                        0
                                    }
                                }
                                _ => 0,
                            }
                        } else {
                            0
                        };
                        self.regs[rd] = eq;
                    }
                }

                PX64_OP_CMP_NE => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        let v1 = self.regs[rs1];
                        let v2 = self.regs[rs2];
                        let ne = if v1 == v2 {
                            0
                        } else if ((v1 & STR_TAG) != 0 || (v1 & ARG_TAG) != 0)
                            && ((v2 & STR_TAG) != 0 || (v2 & ARG_TAG) != 0)
                        {
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {
                                (Some(b1), Some(b2)) => {
                                    if b1 == b2 {
                                        0
                                    } else {
                                        1
                                    }
                                }
                                _ => 1,
                            }
                        } else {
                            1
                        };
                        self.regs[rd] = ne;
                    }
                }

                PX64_OP_STREQ => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        let v1 = self.regs[rs1];
                        let v2 = self.regs[rs2];
                        let eq = if v1 == v2 {
                            1
                        } else {
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {
                                (Some(b1), Some(b2)) => {
                                    if b1 == b2 {
                                        1
                                    } else {
                                        0
                                    }
                                }
                                _ => 0,
                            }
                        };
                        self.regs[rd] = eq;
                    }
                }

                PX64_OP_CMP_LT => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] < self.regs[rs2] {
                            1
                        } else {
                            0
                        };
                    }
                }

                PX64_OP_CMP_LE => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] <= self.regs[rs2] {
                            1
                        } else {
                            0
                        };
                    }
                }

                PX64_OP_CMP_GT => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] > self.regs[rs2] {
                            1
                        } else {
                            0
                        };
                    }
                }

                PX64_OP_CMP_GE => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] >= self.regs[rs2] {
                            1
                        } else {
                            0
                        };
                    }
                }

                PX64_OP_JMP => {
                    self.ip = imm16 as usize;
                }

                PX64_OP_JZ => {
                    if rd < PX64_NUM_REGISTERS && self.regs[rd] == 0 {
                        self.ip = imm16 as usize;
                    }
                }

                PX64_OP_JNZ => {
                    if rd < PX64_NUM_REGISTERS && self.regs[rd] != 0 {
                        self.ip = imm16 as usize;
                    }
                }

                PX64_OP_CALL_NAT => {
                    let func_id = rs1 as u8;
                    let arg_reg = rs2;
                    let arg_val = if arg_reg < PX64_NUM_REGISTERS {
                        self.regs[arg_reg]
                    } else {
                        0
                    };

                    let ret = match func_id {
                        NATIVE_PRINT => {
                            if (arg_val & ARG_TAG) != 0 {
                                let idx = (arg_val & 0xFF) as usize;
                                unsafe {
                                    if idx < SCRIPT_ARGC {
                                        let len = SCRIPT_ARG_LENS[idx];
                                        if let Ok(s) =
                                            core::str::from_utf8(&SCRIPT_ARGS[idx][..len])
                                        {
                                            serial_print!("{}", s);
                                        }
                                    }
                                }
                            } else if (arg_val & STR_TAG) != 0 {
                                let raw = arg_val & !STR_TAG;
                                let offset = (raw >> 32) as usize;
                                let len = (raw & 0xFFFF_FFFF) as usize;
                                if offset + len <= self.str_pool.len() {
                                    if let Ok(s) = core::str::from_utf8(
                                        &self.str_pool[offset..offset + len],
                                    ) {
                                        serial_print!("{}", s);
                                    }
                                }
                            } else {
                                serial_print!("{}", arg_val);
                            }
                            0
                        }

                        NATIVE_PRINTLN => {
                            if (arg_val & ARG_TAG) != 0 {
                                let idx = (arg_val & 0xFF) as usize;
                                unsafe {
                                    if idx < SCRIPT_ARGC {
                                        let len = SCRIPT_ARG_LENS[idx];
                                        if let Ok(s) =
                                            core::str::from_utf8(&SCRIPT_ARGS[idx][..len])
                                        {
                                            serial_println!("{}", s);
                                        }
                                    }
                                }
                            } else if (arg_val & STR_TAG) != 0 {
                                let raw = arg_val & !STR_TAG;
                                let offset = (raw >> 32) as usize;
                                let len = (raw & 0xFFFF_FFFF) as usize;
                                if offset + len <= self.str_pool.len() {
                                    if let Ok(s) = core::str::from_utf8(
                                        &self.str_pool[offset..offset + len],
                                    ) {
                                        serial_println!("{}", s);
                                    }
                                }
                            } else if arg_reg != 0 || arg_val != 0 {
                                serial_println!("{}", arg_val);
                            } else {
                                serial_println!();
                            }
                            0
                        }

                        NATIVE_SYS_TSC => read_tsc_serialized() as i64,

                        NATIVE_NET_RTT => LAST_RTT_NS.load(Ordering::Relaxed) as i64,

                        NATIVE_NET_SET_RATE => {
                            CONGESTION_RATE_PCT.store(arg_val as u8, Ordering::Relaxed);
                            0
                        }

                        NATIVE_GPU_CAPTURE => {
                            let vblank = poll_vblank_edge(5);
                            let handle = capture_frame_zero_copy(0, 1, vblank);
                            handle.slot_id as i64
                        }

                        NATIVE_NET_SEND => {
                            let slot_id = (arg_val as u8) % crate::gpu::NUM_FRAME_SLOTS as u8;
                            let now = read_tsc_serialized();
                            let data_slice = crate::gpu::get_frame_slot_data(slot_id);
                            let send_size = core::cmp::min(1400, data_slice.len());
                            let handle = crate::gpu::FrameHandle {
                                slot_id,
                                frame_id: 1,
                                width: crate::gpu::FRAME_WIDTH,
                                height: crate::gpu::FRAME_HEIGHT,
                                stride: crate::gpu::FRAME_STRIDE,
                                phys_addr: data_slice.as_ptr() as u64,
                                size: send_size,
                                crc32: 0x12345678,
                                vblank_tsc: now,
                                capture_done_tsc: now,
                            };
                            let deadline = now + crate::tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
                            let mut seq = 1u16;
                            let _ = stream_send_frame(&handle, deadline, &mut seq);
                            1
                        }

                        NATIVE_SCRIPT_ARGC => unsafe { SCRIPT_ARGC as i64 },

                        NATIVE_SCRIPT_ARG => {
                            if arg_val >= 0 && (arg_val as usize) < 8 {
                                ARG_TAG | (arg_val & 0xFF)
                            } else {
                                0
                            }
                        }

                        NATIVE_TAG_OK => arg_val & !ERR_TAG,

                        NATIVE_TAG_ERR => ERR_TAG | (arg_val & !ERR_TAG),

                        NATIVE_IS_OK => {
                            if (arg_val & ERR_TAG) == 0 {
                                1
                            } else {
                                0
                            }
                        }

                        NATIVE_IS_ERR => {
                            if (arg_val & ERR_TAG) != 0 {
                                1
                            } else {
                                0
                            }
                        }

                        NATIVE_UNWRAP => {
                            if (arg_val & ERR_TAG) != 0 {
                                return Err(CompileError::simple(
                                    "ERR_PX64_UNWRAP_FAILED",
                                    "PulseLang unwrap failed: called @unwrap() on an Err result value",
                                ));
                            }
                            arg_val
                        }

                        NATIVE_STREQ => 0,

                        NATIVE_CORE_ID => crate::apic::get_lapic_id() as i64,

                        NATIVE_TSC_FREQ => (tsc_freq_hz / 1_000_000) as i64,

                        NATIVE_UPTIME_NS => {
                            crate::tsc::tsc_to_ns(read_tsc_serialized(), tsc_freq_hz) as i64
                        }

                        NATIVE_BUSY_WAIT => {
                            if arg_val > 0 {
                                let wait_tsc = crate::tsc::ns_to_tsc(arg_val as u64, tsc_freq_hz);
                                let start = read_tsc_serialized();
                                while read_tsc_serialized() < start + wait_tsc {
                                    core::hint::spin_loop();
                                }
                            }
                            0
                        }

                        NATIVE_RING_DEPTH => {
                            if arg_val == 0 {
                                crate::ring_buffer::CAPTURE_TO_ENCODE_RING.len() as i64
                            } else if arg_val == 1 {
                                crate::ring_buffer::ENCODE_TO_NET_RING.len() as i64
                            } else {
                                0
                            }
                        }

                        NATIVE_MATH_MIN => {
                            let a = arg_val;
                            let b = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize]
                            } else {
                                0
                            };
                            core::cmp::min(a, b)
                        }

                        NATIVE_MATH_MAX => {
                            let a = arg_val;
                            let b = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize]
                            } else {
                                0
                            };
                            core::cmp::max(a, b)
                        }

                        NATIVE_MATH_ABS => arg_val.saturating_abs(),

                        NATIVE_MATH_CLAMP => {
                            let v = arg_val;
                            let min_v = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize]
                            } else {
                                0
                            };
                            let max_v = if arg_reg > 1 {
                                self.regs[(arg_reg - 2) as usize]
                            } else {
                                0
                            };
                            core::cmp::min(core::cmp::max(v, min_v), max_v)
                        }

                        NATIVE_BIT_POPCNT => (arg_val as u64).count_ones() as i64,

                        NATIVE_BIT_LZCNT => (arg_val as u64).leading_zeros() as i64,

                        NATIVE_CRC32 => {
                            let seed = arg_val as u32;
                            let val = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize]
                            } else {
                                0
                            };
                            let bytes = val.to_le_bytes();
                            let crc = crate::gpu::compute_crc32(&bytes);
                            (crc ^ seed) as i64
                        }

                        NATIVE_VRAM_READ => {
                            let slot_id = (arg_val as usize) % crate::gpu::NUM_FRAME_SLOTS;
                            let offset = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize] as usize
                            } else {
                                0
                            };
                            let slot_data = crate::gpu::get_frame_slot_data(slot_id as u8);
                            if offset + 8 <= slot_data.len() {
                                i64::from_le_bytes([
                                    slot_data[offset],
                                    slot_data[offset + 1],
                                    slot_data[offset + 2],
                                    slot_data[offset + 3],
                                    slot_data[offset + 4],
                                    slot_data[offset + 5],
                                    slot_data[offset + 6],
                                    slot_data[offset + 7],
                                ])
                            } else if offset + 4 <= slot_data.len() {
                                i32::from_le_bytes([
                                    slot_data[offset],
                                    slot_data[offset + 1],
                                    slot_data[offset + 2],
                                    slot_data[offset + 3],
                                ]) as i64
                            } else {
                                0
                            }
                        }

                        NATIVE_VRAM_WRITE => {
                            let slot_id = (arg_val as usize) % crate::gpu::NUM_FRAME_SLOTS;
                            let offset = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize] as usize
                            } else {
                                0
                            };
                            let val = if arg_reg > 1 {
                                self.regs[(arg_reg - 2) as usize]
                            } else {
                                0
                            };
                            let slot_data = crate::gpu::get_frame_slot_data_mut(slot_id as u8);
                            if offset + 8 <= slot_data.len() {
                                let bytes = val.to_le_bytes();
                                slot_data[offset..offset + 8].copy_from_slice(&bytes);
                            }
                            0
                        }

                        _ => 0,
                    };

                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = ret;
                    }
                }

                PX64_OP_WITHIN_START => {
                    let budget_ns = if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] as u64
                    } else {
                        500_000
                    };
                    let deadline =
                        read_tsc_serialized() + crate::tsc::ns_to_tsc(budget_ns, tsc_freq_hz);
                    if self.dl_sp < 8 {
                        self.deadline_stack[self.dl_sp] = deadline;
                        self.dl_sp += 1;
                    }
                }

                PX64_OP_WITHIN_END => {
                    if self.dl_sp > 0 {
                        self.dl_sp -= 1;
                    }
                }

                PX64_OP_DROP => {
                    if self.dl_sp > 0 {
                        let dl = self.deadline_stack[self.dl_sp - 1];
                        if read_tsc_serialized() > dl {
                            serial_println!("[DEADLINE_DROP] Frame dropped due to deadline breach");
                        }
                    }
                }

                _ => {
                    return Err(CompileError::simple(
                        "ERR_PX64_INVALID_OPCODE",
                        "Invalid px64 instruction opcode encountered",
                    ));
                }
            }
        }
        if read_tsc_serialized() > timeout_tsc {
            return Err(CompileError::simple(
                "ERR_PX64_TIMEOUT_EXCEEDED",
                "Execution exceeded wall-clock execution deadline (watchdog safety violation)",
            ));
        }

        if steps >= MAX_VM_STEPS {
            return Err(CompileError::simple(
                "ERR_PX64_WCET_EXCEEDED",
                "Execution exceeded px64 WCET instruction step limit (infinite loop protection)",
            ));
        }

        Ok(())
    }
}

static mut BENCH_CODE: [u8; 4004] = [0u8; 4004];
static mut BENCH_CALL_CODE: [u8; 2008] = [0u8; 2008];

// Function: benchmark_px64_instructions
// Description: Benchmark real cycle count and nanosecond execution times for px64 instructions.
// Worst-case execution time: ~200_000 ns
pub fn benchmark_px64_instructions(
    tsc_freq_hz: u64,
) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) {
    let const_pool = [123456789012345i64; 4];
    unsafe {
        let code = &mut BENCH_CODE;

        // 1. Measure 1,000 iterations of LDC
        for i in 0..1000 {
            code[i * 4] = PX64_OP_LDC;
            code[i * 4 + 1] = 0; // $rax
            code[i * 4 + 2] = 0;
            code[i * 4 + 3] = 0; // const[0]
        }
        code[4000] = PX64_OP_HALT;
        let mut vm = PX64VM::new(code, &[], &const_pool);
        let t0 = read_tsc_serialized();
        let _ = vm.run(tsc_freq_hz);
        let t1 = read_tsc_serialized();
        let ldc_ns = crate::tsc::tsc_to_ns(t1 - t0, tsc_freq_hz) / 1000;

        // 2. Measure 1,000 iterations of ADDI
        for i in 0..1000 {
            code[i * 4] = PX64_OP_ADDI;
            code[i * 4 + 1] = 0; // $rax
            code[i * 4 + 2] = 0; // $rax
            code[i * 4 + 3] = 1; // +1
        }
        code[4000] = PX64_OP_HALT;
        let mut vm2 = PX64VM::new(code, &[], &const_pool);
        let t2 = read_tsc_serialized();
        let _ = vm2.run(tsc_freq_hz);
        let t3 = read_tsc_serialized();
        let addi_ns = crate::tsc::tsc_to_ns(t3 - t2, tsc_freq_hz) / 1000;

        // 3. Measure 1,000 iterations of bitwise AND
        for i in 0..1000 {
            code[i * 4] = PX64_OP_AND;
            code[i * 4 + 1] = 0; // $rax
            code[i * 4 + 2] = 0; // $rax
            code[i * 4 + 3] = 1; // $rcx
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_and = PX64VM::new(code, &[], &const_pool);
        let t_and0 = read_tsc_serialized();
        let _ = vm_and.run(tsc_freq_hz);
        let t_and1 = read_tsc_serialized();
        let and_ns = crate::tsc::tsc_to_ns(t_and1 - t_and0, tsc_freq_hz) / 1000;

        // 4. Measure 1,000 iterations of bitwise XOR
        for i in 0..1000 {
            code[i * 4] = PX64_OP_XOR;
            code[i * 4 + 1] = 0;
            code[i * 4 + 2] = 0;
            code[i * 4 + 3] = 1;
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_xor = PX64VM::new(code, &[], &const_pool);
        let t_xor0 = read_tsc_serialized();
        let _ = vm_xor.run(tsc_freq_hz);
        let t_xor1 = read_tsc_serialized();
        let xor_ns = crate::tsc::tsc_to_ns(t_xor1 - t_xor0, tsc_freq_hz) / 1000;

        // 5. Measure 1,000 iterations of bitwise SHL
        for i in 0..1000 {
            code[i * 4] = PX64_OP_SHL;
            code[i * 4 + 1] = 0;
            code[i * 4 + 2] = 0;
            code[i * 4 + 3] = 1;
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_shl = PX64VM::new(code, &[], &const_pool);
        let t_shl0 = read_tsc_serialized();
        let _ = vm_shl.run(tsc_freq_hz);
        let t_shl1 = read_tsc_serialized();
        let shl_ns = crate::tsc::tsc_to_ns(t_shl1 - t_shl0, tsc_freq_hz) / 1000;

        // 6. Measure 1,000 iterations of array load (ARR_LOAD)
        for i in 0..1000 {
            code[i * 4] = PX64_OP_ARR_LOAD;
            code[i * 4 + 1] = 0; // $rax
            code[i * 4 + 2] = 0; // arr_id 0
            code[i * 4 + 3] = 2; // $rdx (index 0)
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_arr = PX64VM::new(code, &[], &const_pool);
        vm_arr.array_lens[0] = 64;
        let t_arr0 = read_tsc_serialized();
        let _ = vm_arr.run(tsc_freq_hz);
        let t_arr1 = read_tsc_serialized();
        let arr_load_ns = crate::tsc::tsc_to_ns(t_arr1 - t_arr0, tsc_freq_hz) / 1000;

        // 7. Measure 1,000 iterations of ASSERT (passing condition)
        for i in 0..1000 {
            code[i * 4] = PX64_OP_ASSERT;
            code[i * 4 + 1] = 0; // $rax (val 1)
            code[i * 4 + 2] = 0;
            code[i * 4 + 3] = 0;
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_assert = PX64VM::new(code, &[], &const_pool);
        vm_assert.regs[0] = 1;
        let t_as0 = read_tsc_serialized();
        let _ = vm_assert.run(tsc_freq_hz);
        let t_as1 = read_tsc_serialized();
        let assert_ns = crate::tsc::tsc_to_ns(t_as1 - t_as0, tsc_freq_hz) / 1000;

        // 8. Measure 1,000 iterations of NOP (pure decode & loop overhead)
        for i in 0..1000 {
            code[i * 4] = PX64_OP_NOP;
            code[i * 4 + 1] = 0;
            code[i * 4 + 2] = 0;
            code[i * 4 + 3] = 0;
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_nop = PX64VM::new(code, &[], &const_pool);
        let t4 = read_tsc_serialized();
        let _ = vm_nop.run(tsc_freq_hz);
        let t5 = read_tsc_serialized();
        let decode_ns = crate::tsc::tsc_to_ns(t5 - t4, tsc_freq_hz) / 1000;

        // 9. Measure 500 iterations of CALL/RET pairs (1,000 instructions total)
        let call_code = &mut BENCH_CALL_CODE;
        for i in 0..500 {
            call_code[i * 4] = PX64_OP_CALL;
            call_code[i * 4 + 1] = 0;
            call_code[i * 4 + 2] = (2000 >> 8) as u8;
            call_code[i * 4 + 3] = (2000 & 0xFF) as u8;
        }
        call_code[2000] = PX64_OP_RET;
        call_code[2004] = PX64_OP_HALT;
        let mut vm_call = PX64VM::new(call_code, &[], &const_pool);
        let t_call0 = read_tsc_serialized();
        let _ = vm_call.run(tsc_freq_hz);
        let t_call1 = read_tsc_serialized();
        let call_ret_ns = crate::tsc::tsc_to_ns(t_call1 - t_call0, tsc_freq_hz) / 500;

        // 10. Measure 1,000 iterations of STRUCT_LOAD
        for i in 0..1000 {
            code[i * 4] = PX64_OP_STRUCT_LOAD;
            code[i * 4 + 1] = 0; // $rax
            code[i * 4 + 2] = 0; // inst_id 0
            code[i * 4 + 3] = 0; // field offset 0
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_struct = PX64VM::new(code, &[], &const_pool);
        vm_struct.struct_field_counts[0] = 4;
        let t_st0 = read_tsc_serialized();
        let _ = vm_struct.run(tsc_freq_hz);
        let t_st1 = read_tsc_serialized();
        let struct_load_ns = crate::tsc::tsc_to_ns(t_st1 - t_st0, tsc_freq_hz) / 1000;

        // 11. Measure 1,000 iterations of TBL_LOAD
        for i in 0..1000 {
            code[i * 4] = PX64_OP_TBL_LOAD;
            code[i * 4 + 1] = 0; // $rax
            code[i * 4 + 2] = 0; // tbl_id 0
            code[i * 4 + 3] = 0; // $rax (idx 0)
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_tbl = PX64VM::new(code, &[], &const_pool);
        vm_tbl.table_lens[0] = 4;
        let t_tb0 = read_tsc_serialized();
        let _ = vm_tbl.run(tsc_freq_hz);
        let t_tb1 = read_tsc_serialized();
        let tbl_load_ns = crate::tsc::tsc_to_ns(t_tb1 - t_tb0, tsc_freq_hz) / 1000;

        // 12. Measure 1,000 iterations of STREQ
        let str_bytes_pool = b"PulseLangRealTimeStringComparisonOptimization";
        for i in 0..1000 {
            code[i * 4] = PX64_OP_STREQ;
            code[i * 4 + 1] = 0; // $rax
            code[i * 4 + 2] = 1; // $rcx
            code[i * 4 + 3] = 2; // $rdx
        }
        code[4000] = PX64_OP_HALT;
        let mut vm_streq = PX64VM::new(code, str_bytes_pool, &const_pool);
        vm_streq.regs[1] = STR_TAG | (((0u64) as i64) << 32) | 20;
        vm_streq.regs[2] = STR_TAG | (((0u64) as i64) << 32) | 20;
        let t_str0 = read_tsc_serialized();
        let _ = vm_streq.run(tsc_freq_hz);
        let t_str1 = read_tsc_serialized();
        let streq_ns = crate::tsc::tsc_to_ns(t_str1 - t_str0, tsc_freq_hz) / 1000;

        (
            ldc_ns,
            addi_ns,
            and_ns,
            xor_ns,
            shl_ns,
            arr_load_ns,
            struct_load_ns,
            tbl_load_ns,
            streq_ns,
            assert_ns,
            call_ret_ns,
            decode_ns,
        )
    }
}

// Legacy VM for PULS v1/v2 backward compatibility
pub struct VM<'a> {
    code: &'a [u8],
    str_pool: &'a [u8],
    ip: usize,
    stack: [i64; 64],
    sp: usize,
    vars: [i64; MAX_VARS],
    deadline_stack: [u64; 8],
    dl_sp: usize,
}

impl<'a> VM<'a> {
    pub fn new(code: &'a [u8], str_pool: &'a [u8]) -> Self {
        Self {
            code,
            str_pool,
            ip: 0,
            stack: [0; 64],
            sp: 0,
            vars: [0; MAX_VARS],
            deadline_stack: [0; 8],
            dl_sp: 0,
        }
    }

    fn push(&mut self, val: i64) -> Result<(), CompileError> {
        if self.sp >= 64 {
            return Err(CompileError::simple(
                "ERR_VM_STACK_OVERFLOW",
                "VM Evaluation stack overflow",
            ));
        }
        self.stack[self.sp] = val;
        self.sp += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<i64, CompileError> {
        if self.sp == 0 {
            return Err(CompileError::simple(
                "ERR_VM_STACK_UNDERFLOW",
                "VM Evaluation stack underflow",
            ));
        }
        self.sp -= 1;
        Ok(self.stack[self.sp])
    }

    pub fn run(&mut self, tsc_freq_hz: u64) -> Result<(), CompileError> {
        let start_tsc = read_tsc_serialized();
        let timeout_tsc = start_tsc + crate::tsc::ns_to_tsc(MAX_SCRIPT_TIMEOUT_NS, tsc_freq_hz);
        let mut steps = 0;
        while self.ip < self.code.len() {
            if steps >= MAX_VM_STEPS {
                return Err(CompileError::simple(
                    "ERR_PX64_WCET_EXCEEDED",
                    "Execution exceeded px64 WCET instruction step limit (infinite loop protection)",
                ));
            }

            if read_tsc_serialized() > timeout_tsc {
                return Err(CompileError::simple(
                    "ERR_PX64_TIMEOUT_EXCEEDED",
                    "Execution exceeded 5.0ms wall-clock execution deadline (watchdog safety violation)",
                ));
            }

            steps += 1;
            let op = self.code[self.ip];
            self.ip += 1;

            match op {
                OP_NOP => {}
                OP_PUSH_CONST => {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&self.code[self.ip..self.ip + 8]);
                    self.ip += 8;
                    self.push(i64::from_be_bytes(b))?;
                }
                OP_LOAD_VAR => {
                    let idx = self.code[self.ip] as usize;
                    self.ip += 1;
                    self.push(self.vars[idx])?;
                }
                OP_STORE_VAR => {
                    let idx = self.code[self.ip] as usize;
                    self.ip += 1;
                    let val = self.pop()?;
                    self.vars[idx] = val;
                }
                OP_ADD => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(a.wrapping_add(b))?;
                }
                OP_SUB => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(a.wrapping_sub(b))?;
                }
                OP_MUL => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(a.wrapping_mul(b))?;
                }
                OP_DIV => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if b != 0 { a / b } else { 0 })?;
                }
                OP_MOD => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if b != 0 { a % b } else { 0 })?;
                }
                OP_CMP_EQ => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if a == b { 1 } else { 0 })?;
                }
                OP_CMP_NE => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if a != b { 1 } else { 0 })?;
                }
                OP_CMP_LT => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if a < b { 1 } else { 0 })?;
                }
                OP_CMP_LE => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if a <= b { 1 } else { 0 })?;
                }
                OP_CMP_GT => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if a > b { 1 } else { 0 })?;
                }
                OP_CMP_GE => {
                    let (b, a) = (self.pop()?, self.pop()?);
                    self.push(if a >= b { 1 } else { 0 })?;
                }
                OP_JUMP => {
                    let target =
                        u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    self.ip = target;
                }
                OP_JUMP_IF_FALSE => {
                    let target =
                        u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    self.ip += 2;
                    let cond = self.pop()?;
                    if cond == 0 {
                        self.ip = target;
                    }
                }
                OP_PUSH_STR => {
                    let offset =
                        u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    let len =
                        u16::from_be_bytes([self.code[self.ip + 2], self.code[self.ip + 3]])
                            as usize;
                    self.ip += 4;
                    let encoded =
                        STR_TAG | (((offset as u64) as i64) << 32) | ((len as u64) as i64);
                    self.push(encoded)?;
                }
                OP_CALL_NATIVE => {
                    let func_id = self.code[self.ip];
                    let argc = self.code[self.ip + 1] as usize;
                    self.ip += 2;
                    match func_id {
                        NATIVE_PRINT => {
                            if argc > 0 {
                                let val = self.pop()?;
                                if (val & ARG_TAG) != 0 {
                                    let idx = (val & 0xFF) as usize;
                                    unsafe {
                                        if idx < SCRIPT_ARGC {
                                            let len = SCRIPT_ARG_LENS[idx];
                                            if let Ok(s) =
                                                core::str::from_utf8(&SCRIPT_ARGS[idx][..len])
                                            {
                                                serial_print!("{}", s);
                                            }
                                        }
                                    }
                                } else if (val & STR_TAG) != 0 {
                                    let raw = val & !STR_TAG;
                                    let offset = (raw >> 32) as usize;
                                    let len = (raw & 0xFFFF_FFFF) as usize;
                                    if offset + len <= self.str_pool.len() {
                                        if let Ok(s) = core::str::from_utf8(
                                            &self.str_pool[offset..offset + len],
                                        ) {
                                            serial_print!("{}", s);
                                        }
                                    }
                                } else {
                                    serial_print!("{}", val);
                                }
                            }
                            self.push(0)?;
                        }
                        NATIVE_PRINTLN => {
                            if argc > 0 {
                                let val = self.pop()?;
                                if (val & ARG_TAG) != 0 {
                                    let idx = (val & 0xFF) as usize;
                                    unsafe {
                                        if idx < SCRIPT_ARGC {
                                            let len = SCRIPT_ARG_LENS[idx];
                                            if let Ok(s) =
                                                core::str::from_utf8(&SCRIPT_ARGS[idx][..len])
                                            {
                                                serial_println!("{}", s);
                                            }
                                        }
                                    }
                                } else if (val & STR_TAG) != 0 {
                                    let raw = val & !STR_TAG;
                                    let offset = (raw >> 32) as usize;
                                    let len = (raw & 0xFFFF_FFFF) as usize;
                                    if offset + len <= self.str_pool.len() {
                                        if let Ok(s) = core::str::from_utf8(
                                            &self.str_pool[offset..offset + len],
                                        ) {
                                            serial_println!("{}", s);
                                        }
                                    }
                                } else {
                                    serial_println!("{}", val);
                                }
                            } else {
                                serial_println!();
                            }
                            self.push(0)?;
                        }
                        NATIVE_SYS_TSC => {
                            self.push(read_tsc_serialized() as i64)?;
                        }
                        NATIVE_NET_RTT => {
                            self.push(LAST_RTT_NS.load(Ordering::Relaxed) as i64)?;
                        }
                        NATIVE_NET_SET_RATE => {
                            if argc > 0 {
                                let r = self.pop()?;
                                CONGESTION_RATE_PCT.store(r as u8, Ordering::Relaxed);
                            }
                            self.push(0)?;
                        }
                        NATIVE_GPU_CAPTURE => {
                            let vblank = poll_vblank_edge(5);
                            let h = capture_frame_zero_copy(0, 1, vblank);
                            self.push(h.slot_id as i64)?;
                        }
                        NATIVE_NET_SEND => {
                            if argc > 0 {
                                let _ = self.pop()?;
                                let handle = capture_frame_zero_copy(0, 1, read_tsc_serialized());
                                let deadline =
                                    read_tsc_serialized() + crate::tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
                                let mut seq = 1u16;
                                let _ = stream_send_frame(&handle, deadline, &mut seq);
                            }
                            self.push(1)?;
                        }
                        NATIVE_SCRIPT_ARGC => {
                            unsafe {
                                self.push(SCRIPT_ARGC as i64)?;
                            }
                        }
                        NATIVE_SCRIPT_ARG => {
                            if argc > 0 {
                                let idx = self.pop()?;
                                if idx >= 0 && (idx as usize) < 8 {
                                    self.push(ARG_TAG | (idx & 0xFF))?;
                                } else {
                                    self.push(0)?;
                                }
                            } else {
                                self.push(0)?;
                            }
                        }
                        _ => {}
                    }
                }
                OP_WITHIN_START => {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&self.code[self.ip..self.ip + 8]);
                    self.ip += 8;
                    let budget_ns = i64::from_be_bytes(b) as u64;
                    let tsc_budget = if tsc_freq_hz > 0 {
                        (budget_ns * tsc_freq_hz) / 1_000_000_000
                    } else {
                        budget_ns * 3
                    };
                    if self.dl_sp < 8 {
                        self.deadline_stack[self.dl_sp] = read_tsc_serialized() + tsc_budget;
                        self.dl_sp += 1;
                    }
                }
                OP_WITHIN_END => {
                    if self.dl_sp > 0 {
                        self.dl_sp -= 1;
                    }
                }
                OP_DROP => {
                    if self.dl_sp > 0 && read_tsc_serialized() > self.deadline_stack[self.dl_sp - 1]
                    {
                        serial_println!("[DEADLINE_DROP] Frame dropped due to deadline breach");
                    }
                }
                OP_HALT => break,
                _ => return Err(CompileError::simple("ERR_VM_INVALID_OPCODE", "Invalid opcode")),
            }
        }
        Ok(())
    }
}

// Function: run_pulse_script
// Description: Compile and execute a PulseLang script using the px64 architecture engine.
// Worst-case execution time: ~100_000 ns
pub fn run_pulse_script(src: &[u8], tsc_freq_hz: u64) -> Result<(), CompileError> {
    let tokens = unsafe { &mut COMPILER_TOKENS };
    for tok in tokens.iter_mut() {
        *tok = Token::empty();
    }
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(tokens)?;

    let mut compiler = Compiler::new(src, tokens);
    let code_len = compiler.compile()?;

    let mut vm = PX64VM::new(
        &compiler.code[..code_len],
        &compiler.str_pool[..compiler.str_pool_len],
        &compiler.const_pool[..compiler.const_pool_len],
    );
    vm.run(tsc_freq_hz)
}

// Function: run_pulse_binary
// Description: Execute pre-compiled px64 / PULS binary bytecode directly in O(1) zero compilation latency.
// Worst-case execution time: ~60_000 ns
pub fn run_pulse_binary(bin: &[u8], tsc_freq_hz: u64) -> Result<(), CompileError> {
    if bin.len() >= PX64_HEADER_SIZE && &bin[0..4] == &PX64_BIN_MAGIC {
        let version = u16::from_be_bytes([bin[4], bin[5]]);
        if version != PX64_BIN_VERSION {
            return Err(CompileError::simple(
                "ERR_BINARY_VERSION_MISMATCH",
                "Unsupported px64 binary version",
            ));
        }
        let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
        let str_pool_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;
        let const_count = u16::from_be_bytes([bin[10], bin[11]]) as usize;
        let const_bytes = const_count * 8;

        if bin.len() < PX64_HEADER_SIZE + code_len + str_pool_len + const_bytes {
            return Err(CompileError::simple(
                "ERR_BINARY_TRUNCATED",
                "Truncated px64 binary payload",
            ));
        }

        let code = &bin[PX64_HEADER_SIZE..PX64_HEADER_SIZE + code_len];
        let str_pool =
            &bin[PX64_HEADER_SIZE + code_len..PX64_HEADER_SIZE + code_len + str_pool_len];

        let mut const_pool = [0i64; 64];
        let const_slice_raw = &bin[PX64_HEADER_SIZE + code_len + str_pool_len
            ..PX64_HEADER_SIZE + code_len + str_pool_len + const_bytes];
        for i in 0..core::cmp::min(const_count, 64) {
            let b = &const_slice_raw[i * 8..(i + 1) * 8];
            const_pool[i] = i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        }

        let mut vm = PX64VM::new(code, str_pool, &const_pool[..const_count]);
        vm.run(tsc_freq_hz)
    } else if bin.len() >= PULSE_HEADER_SIZE && &bin[0..4] == &PULSE_BIN_MAGIC {
        let version = u16::from_be_bytes([bin[4], bin[5]]);
        if version != PULSE_BIN_VERSION {
            return Err(CompileError::simple(
                "ERR_BINARY_VERSION_MISMATCH",
                "Unsupported PulseLang binary version",
            ));
        }
        let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
        let str_pool_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;

        if bin.len() < PULSE_HEADER_SIZE + code_len + str_pool_len {
            return Err(CompileError::simple(
                "ERR_BINARY_TRUNCATED",
                "Truncated binary bytecode payload",
            ));
        }

        let code = &bin[PULSE_HEADER_SIZE..PULSE_HEADER_SIZE + code_len];
        let str_pool =
            &bin[PULSE_HEADER_SIZE + code_len..PULSE_HEADER_SIZE + code_len + str_pool_len];

        let mut vm = VM::new(code, str_pool);
        vm.run(tsc_freq_hz)
    } else {
        Err(CompileError::simple(
            "ERR_BINARY_INVALID_MAGIC",
            "Invalid executable binary magic (expected PX64)",
        ))
    }
}

// Function: run_pulse_auto
// Description: Automatically detect px64/PULS binary vs source script and execute.
// Worst-case execution time: ~100_000 ns
pub fn run_pulse_auto(data: &[u8], tsc_freq_hz: u64) -> Result<(), CompileError> {
    if data.len() >= 4 && (&data[0..4] == &PX64_BIN_MAGIC || &data[0..4] == &PULSE_BIN_MAGIC) {
        run_pulse_binary(data, tsc_freq_hz)
    } else {
        run_pulse_script(data, tsc_freq_hz)
    }
}
