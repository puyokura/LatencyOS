//! Pulse Extended 64-bit Real-Time Architecture (px64) Register Virtual Machine
//!
//! Provides a zero-dynamic-memory allocation runtime engine capable of executing
//! pre-compiled `px64` bytecode binaries (.bin) and compiled PulseLang source scripts (.pul).

use crate::error::CompileError;
use crate::isa::*;

/// Precomputed CRC32 lookup table (IEEE 802.3 polynomial: 0xEDB88320)
static CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// Compute standard IEEE 802.3 CRC32 checksum over a byte slice.
pub fn compute_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let idx = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[idx];
    }
    !crc
}

/// Null writer that discards all output (for `no_std` zero-allocation runs without output buffer).
pub struct NullWriter;

impl core::fmt::Write for NullWriter {
    #[inline(always)]
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        Ok(())
    }
}

/// Direct standard output writer for `std` environments.
#[cfg(any(feature = "std", test))]
pub struct StdoutWriter;

#[cfg(any(feature = "std", test))]
impl core::fmt::Write for StdoutWriter {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        use std::io::Write;
        std::io::stdout()
            .write_all(s.as_bytes())
            .map_err(|_| core::fmt::Error)?;
        Ok(())
    }
}

/// `PX64VM` - 64-bit Register Virtual Machine for LatencyOS.
///
/// Features:
/// - 20 64-bit virtual registers (`$rax`..`$r15`, `#f0`..`#f3`).
/// - 10,000 instruction steps WCET limit (configurable).
/// - Tagged 64-bit values (`STR_TAG`, `ARG_TAG`, `ERR_TAG`).
/// - Tagged results (`@ok`, `@err`, `@is_ok`, `@is_err`, `@unwrap`).
/// - Call stack with up to 8 nested call levels and register frame preservation.
/// - Static struct slots, constant lookup tables, and array slots.
/// - Full set of 43 `PX64_OP_*` instruction opcodes.
/// - Comprehensive native intrinsics (`@print`, `@println`, `@argc`, `@arg`, `@tsc`, `@tsc_freq`, `@min`, `@max`, etc.).
pub struct PX64VM<'a> {
    pub code: &'a [u8],
    pub str_pool: &'a [u8],
    pub const_pool: &'a [i64],
    pub args: &'a [&'a str],
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
    pub vram: [[u8; 4096]; 4],
    pub steps: usize,
    pub max_steps: usize,
}

impl<'a> PX64VM<'a> {
    /// Construct a new `PX64VM` instance with code, string pool, constant pool, and CLI arguments.
    pub fn new(
        code: &'a [u8],
        str_pool: &'a [u8],
        const_pool: &'a [i64],
        args: &'a [&'a str],
    ) -> Self {
        Self {
            code,
            str_pool,
            const_pool,
            args,
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
            vram: [
                [0x5A; 4096],
                [0xA5; 4096],
                [0x3C; 4096],
                [0xC3; 4096],
            ],
            steps: 0,
            max_steps: MAX_VM_STEPS,
        }
    }

    /// Construct a new `PX64VM` instance without command-line arguments.
    pub fn new_without_args(
        code: &'a [u8],
        str_pool: &'a [u8],
        const_pool: &'a [i64],
    ) -> Self {
        Self::new(code, str_pool, const_pool, &[])
    }

    /// Retrieve the byte slice corresponding to a tagged string or CLI argument pointer.
    #[inline(always)]
    pub fn get_str_bytes<'b>(&self, val: i64) -> Option<&'b [u8]>
    where
        'a: 'b,
    {
        if (val & STR_TAG) != 0 {
            let raw = val & !STR_TAG;
            let offset = ((raw as u64) >> 32) as usize;
            let len = (raw & 0xFFFF_FFFF) as usize;
            if offset + len <= self.str_pool.len() {
                Some(&self.str_pool[offset..offset + len])
            } else {
                None
            }
        } else if (val & ARG_TAG) != 0 {
            let idx = (val & 0xFF) as usize;
            if idx < self.args.len() {
                Some(self.args[idx].as_bytes())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Read host serialized timestamp counter.
    #[inline(always)]
    fn get_tsc(&self) -> i64 {
        #[cfg(target_arch = "x86_64")]
        {
            unsafe { core::arch::x86_64::_rdtsc() as i64 }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            (self.steps as i64).wrapping_mul(10)
        }
    }

    /// Read host uptime in nanoseconds.
    #[inline(always)]
    fn get_uptime_ns(&self) -> i64 {
        #[cfg(target_arch = "x86_64")]
        {
            let tsc = unsafe { core::arch::x86_64::_rdtsc() as u64 };
            // Standard 3GHz conversion: tsc * 1_000_000_000 / 3_000_000_000 = tsc / 3
            (tsc / 3) as i64
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            (self.steps as i64).wrapping_mul(15)
        }
    }

    /// Execute the loaded px64 bytecode instructions directly with custom output writer.
    pub fn run_with_output(&mut self, output: &mut dyn core::fmt::Write) -> Result<(), CompileError> {
        while self.ip + 4 <= self.code.len() {
            if self.steps >= self.max_steps {
                return Err(CompileError::simple(
                    "ERR_PX64_WCET_EXCEEDED",
                    "Execution exceeded px64 WCET instruction step limit (infinite loop protection)",
                ));
            }
            self.steps += 1;

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
                    let inst_id = rd;
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
                    let inst_id = rs1;
                    let offset = rs2;
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
                    let inst_id = rd;
                    let offset = rs1;
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
                    let tbl_id = rd;
                    let base_idx = rs1 as u8;
                    let len = rs2 as u8;
                    if tbl_id < 8 {
                        self.table_bases[tbl_id] = base_idx;
                        self.table_lens[tbl_id] = len;
                    }
                }

                PX64_OP_TBL_LOAD => {
                    let tbl_id = rs1;
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
                    let const_idx = base + idx as usize;
                    if const_idx >= self.const_pool.len() {
                        return Err(CompileError::simple(
                            "ERR_PX64_TABLE_OUT_OF_BOUNDS",
                            "Const table lookup index out of bounds in const pool",
                        ));
                    }
                    let val = self.const_pool[const_idx];
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
                    let offset = rs1 as u64;
                    let len = rs2 as u64;
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] =
                            STR_TAG | (((offset) as i64) << 32) | ((len) as i64);
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
                                if idx < self.args.len() {
                                    let _ = output.write_str(self.args[idx]);
                                }
                            } else if (arg_val & STR_TAG) != 0 {
                                let raw = arg_val & !STR_TAG;
                                let offset = ((raw as u64) >> 32) as usize;
                                let len = (raw & 0xFFFF_FFFF) as usize;
                                if offset + len <= self.str_pool.len() {
                                    if let Ok(s) = core::str::from_utf8(&self.str_pool[offset..offset + len]) {
                                        let _ = output.write_str(s);
                                    }
                                }
                            } else {
                                let _ = write!(output, "{}", arg_val);
                            }
                            0
                        }

                        NATIVE_PRINTLN => {
                            if (arg_val & ARG_TAG) != 0 {
                                let idx = (arg_val & 0xFF) as usize;
                                if idx < self.args.len() {
                                    let _ = output.write_str(self.args[idx]);
                                }
                                let _ = output.write_str("\n");
                            } else if (arg_val & STR_TAG) != 0 {
                                let raw = arg_val & !STR_TAG;
                                let offset = ((raw as u64) >> 32) as usize;
                                let len = (raw & 0xFFFF_FFFF) as usize;
                                if offset + len <= self.str_pool.len() {
                                    if let Ok(s) = core::str::from_utf8(&self.str_pool[offset..offset + len]) {
                                        let _ = output.write_str(s);
                                    }
                                }
                                let _ = output.write_str("\n");
                            } else if arg_reg != 0 || arg_val != 0 {
                                let _ = writeln!(output, "{}", arg_val);
                            } else {
                                let _ = output.write_str("\n");
                            }
                            0
                        }

                        NATIVE_SYS_TSC => self.get_tsc(),

                        NATIVE_NET_RTT => 100,

                        NATIVE_NET_SET_RATE => 0,

                        NATIVE_GPU_CAPTURE => 0,

                        NATIVE_NET_SEND => 1,

                        NATIVE_SCRIPT_ARGC => self.args.len() as i64,

                        NATIVE_SCRIPT_ARG => {
                            if arg_val >= 0 && (arg_val as usize) < self.args.len() && (arg_val as usize) < 256 {
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

                        NATIVE_STREQ => {
                            let s1 = arg_val;
                            let s2 = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize]
                            } else {
                                0
                            };
                            if s1 == s2 {
                                1
                            } else {
                                match (self.get_str_bytes(s1), self.get_str_bytes(s2)) {
                                    (Some(b1), Some(b2)) => {
                                        if b1 == b2 {
                                            1
                                        } else {
                                            0
                                        }
                                    }
                                    _ => 0,
                                }
                            }
                        }

                        NATIVE_CORE_ID => 0,

                        NATIVE_TSC_FREQ => 3_000_000_000,

                        NATIVE_UPTIME_NS => self.get_uptime_ns(),

                        NATIVE_BUSY_WAIT => {
                            if arg_val > 0 {
                                let spins = (arg_val as usize).min(100_000);
                                for _ in 0..spins {
                                    core::hint::spin_loop();
                                }
                            }
                            0
                        }

                        NATIVE_RING_DEPTH => 0,

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
                            let crc = compute_crc32(&bytes);
                            (crc ^ seed) as i64
                        }

                        NATIVE_VRAM_READ => {
                            let slot_id = (arg_val as usize) % 4;
                            let offset = if arg_reg > 0 {
                                self.regs[(arg_reg - 1) as usize] as usize
                            } else {
                                0
                            };
                            let slot_data = &self.vram[slot_id];
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
                            let slot_id = (arg_val as usize) % 4;
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
                            let slot_data = &mut self.vram[slot_id];
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
                    let deadline = (self.get_uptime_ns() as u64).wrapping_add(budget_ns);
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
                        if (self.get_uptime_ns() as u64) > dl {
                            let _ = output.write_str("[DEADLINE_DROP] Frame dropped due to deadline breach\n");
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

        Ok(())
    }

    /// Execute the loaded px64 bytecode instructions.
    ///
    /// When the `std` feature is active, output is printed to standard output.
    /// In `no_std` mode, output is discarded unless `run_with_output` is called.
    pub fn run(&mut self) -> Result<(), CompileError> {
        #[cfg(feature = "std")]
        {
            let mut writer = StdoutWriter;
            self.run_with_output(&mut writer)
        }
        #[cfg(not(feature = "std"))]
        {
            let mut writer = NullWriter;
            self.run_with_output(&mut writer)
        }
    }
}

/// Execute a pre-compiled `px64` bytecode binary buffer with command-line arguments and custom output writer.
pub fn run_binary_with_output(
    bin: &[u8],
    args: &[&str],
    output: &mut dyn core::fmt::Write,
) -> Result<(), CompileError> {
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
        let str_pool = &bin[PX64_HEADER_SIZE + code_len..PX64_HEADER_SIZE + code_len + str_pool_len];

        let mut const_pool = [0i64; 64];
        let const_start = PX64_HEADER_SIZE + code_len + str_pool_len;
        let const_slice_raw = &bin[const_start..const_start + const_bytes];
        let count = core::cmp::min(const_count, 64);
        for i in 0..count {
            let b = &const_slice_raw[i * 8..(i + 1) * 8];
            const_pool[i] = i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        }

        let mut vm = PX64VM::new(code, str_pool, &const_pool[..count], args);
        vm.run_with_output(output)
    } else {
        Err(CompileError::simple(
            "ERR_BINARY_INVALID_MAGIC",
            "Invalid executable binary magic (expected PX64)",
        ))
    }
}

/// Execute a pre-compiled `px64` bytecode binary buffer with command-line arguments.
pub fn run_binary(bin: &[u8], args: &[&str]) -> Result<(), CompileError> {
    #[cfg(any(feature = "std", test))]
    {
        let mut writer = StdoutWriter;
        run_binary_with_output(bin, args, &mut writer)
    }
    #[cfg(not(any(feature = "std", test)))]
    {
        let mut writer = NullWriter;
        run_binary_with_output(bin, args, &mut writer)
    }
}

/// Compile and execute PulseLang source code with command-line arguments and custom output writer (`alloc`/`std` API).
#[cfg(any(feature = "alloc", test))]
pub fn run_source_with_output(
    src: &str,
    args: &[&str],
    output: &mut dyn core::fmt::Write,
) -> Result<(), CompileError> {
    let bin = crate::compile(src)?;
    run_binary_with_output(&bin, args, output)
}

/// Compile and execute PulseLang source code with command-line arguments (`alloc`/`std` API).
#[cfg(any(feature = "alloc", test))]
pub fn run_source(src: &str, args: &[&str]) -> Result<(), CompileError> {
    #[cfg(any(feature = "std", test))]
    {
        let mut writer = StdoutWriter;
        run_source_with_output(src, args, &mut writer)
    }
    #[cfg(not(any(feature = "std", test)))]
    {
        let mut writer = NullWriter;
        run_source_with_output(src, args, &mut writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn test_vm_arithmetic_and_bitwise() {
        let src = r#"
            let $a = 100;
            let $b = 30;
            let $add = $a + $b;
            let $sub = $a - $b;
            let $mul = $a * $b;
            let $div = $a / $b;
            let $mod = $a % $b;
            let $and = $a & $b;
            let $or = $a | $b;
            let $xor = $a ^ $b;
            let $shl = 1 << 4;
            let $shr = 64 >> 2;

            @assert($add == 130);
            @assert($sub == 70);
            @assert($mul == 3000);
            @assert($div == 3);
            @assert($mod == 10);
            @assert($shl == 16);
            @assert($shr == 16);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Arithmetic execution failed");
    }

    #[test]
    fn test_vm_while_loop() {
        let src = r#"
            let mut $i = 1;
            let mut $sum = 0;
            while ($i <= 10) {
                $sum = $sum + $i;
                $i += 1;
            }
            @assert($sum == 55);
            @assert($i == 11);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("While loop execution failed");
    }

    #[test]
    fn test_vm_for_loop() {
        let src = r#"
            let mut $sum = 0;
            for $i in 0..10 {
                $sum = $sum + $i;
            }
            @assert($sum == 45);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("For loop execution failed");
    }

    #[test]
    fn test_vm_print_and_println() {
        let src = r#"
            @print("Hello, ");
            @println("PulseLang VM!");
            @print("Value: ");
            @println(42);
            @println();
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Print execution failed");
        assert_eq!(out, "Hello, PulseLang VM!\nValue: 42\n\n");
    }

    #[test]
    fn test_vm_fizzbuzz_execution() {
        let src = r#"
            let mut $i = 1;
            while ($i <= 15) {
                if ($i % 15 == 0) {
                    @println("FizzBuzz");
                } else {
                    if ($i % 3 == 0) {
                        @println("Fizz");
                    } else {
                        if ($i % 5 == 0) {
                            @println("Buzz");
                        } else {
                            @println($i);
                        }
                    }
                }
                $i += 1;
            }
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("FizzBuzz execution failed");
        let expected = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn test_vm_cli_arguments() {
        let src = r#"
            let $c = @argc();
            @assert($c == 3);
            let $a0 = @arg(0);
            let $a1 = @arg(1);
            let $a2 = @arg(2);

            @println($a0);
            @println($a1);
            @println($a2);

            if ($a0 == "alpha") {
                @println("arg0 matched alpha");
            }
            if ($a1 == "beta") {
                @println("arg1 matched beta");
            }
        "#;
        let args = ["alpha", "beta", "gamma"];
        let mut out = String::new();
        run_source_with_output(src, &args, &mut out).expect("CLI args execution failed");
        assert_eq!(
            out,
            "alpha\nbeta\ngamma\narg0 matched alpha\narg1 matched beta\n"
        );
    }

    #[test]
    fn test_vm_tagged_results_ok() {
        let src = r#"
            let $res = @ok(100);
            @assert(@is_ok($res) == 1);
            @assert(@is_err($res) == 0);
            let $val = @unwrap($res);
            @assert($val == 100);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Tagged result ok failed");
    }

    #[test]
    fn test_vm_tagged_results_err() {
        let src = r#"
            let $res = @err(404);
            @assert(@is_ok($res) == 0);
            @assert(@is_err($res) == 1);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Tagged result err failed");
    }

    #[test]
    fn test_vm_tagged_unwrap_failure() {
        let src = r#"
            let $res = @err(500);
            let $bad = @unwrap($res);
        "#;
        let mut out = String::new();
        let res = run_source_with_output(src, &[], &mut out);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, "ERR_PX64_UNWRAP_FAILED");
    }

    #[test]
    fn test_vm_assertion_failure() {
        let src = r#"
            let $x = 10;
            @assert($x == 999);
        "#;
        let mut out = String::new();
        let res = run_source_with_output(src, &[], &mut out);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, "ERR_PX64_ASSERTION_FAILED");
    }

    #[test]
    fn test_vm_structs() {
        let src = r#"
            struct Point {
                x: i64,
                y: i64,
            }
            let $pt: Point;
            $pt.x = 123;
            $pt.y = 456;
            @assert($pt.x == 123);
            @assert($pt.y == 456);
            @assert($pt.x + $pt.y == 579);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Struct execution failed");
    }

    #[test]
    fn test_vm_const_tables() {
        let src = r#"
            const LUT: [i64; 5] = [10, 20, 30, 40, 50];
            let $v0 = LUT[0];
            let $v2 = LUT[2];
            let $v4 = LUT[4];
            @assert($v0 == 10);
            @assert($v2 == 30);
            @assert($v4 == 50);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Const table execution failed");
    }

    #[test]
    fn test_vm_arrays() {
        let src = r#"
            let $arr: [i64; 4];
            $arr[0] = 111;
            $arr[1] = 222;
            $arr[2] = 333;
            $arr[3] = 444;
            @assert($arr[0] == 111);
            @assert($arr[1] == 222);
            @assert($arr[2] == 333);
            @assert($arr[3] == 444);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Array execution failed");
    }

    #[test]
    fn test_vm_functions() {
        let src = r#"
            fn add_three($a, $b, $c) {
                return $a + $b + $c;
            }
            let $res = add_three(10, 20, 30);
            @assert($res == 60);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Function execution failed");
    }

    #[test]
    fn test_vm_math_and_bit_intrinsics() {
        let src = r#"
            let $min = @min(10, 20);
            let $max = @max(10, 20);
            let $abs = @abs(-55);
            let $clamp1 = @clamp(15, 0, 10);
            let $clamp2 = @clamp(-5, 0, 10);
            let $clamp3 = @clamp(5, 0, 10);
            let $popcnt = @popcnt(45);
            let $core = @core_id();
            let $freq = @tsc_freq();

            @assert($min == 10);
            @assert($max == 20);
            @assert($abs == 55);
            @assert($clamp1 == 10);
            @assert($clamp2 == 0);
            @assert($clamp3 == 5);
            @assert($popcnt == 4);
            @assert($core == 0);
            @assert($freq == 3000000000);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("Math intrinsics execution failed");
    }

    #[test]
    fn test_vm_vram_read_write() {
        let src = r#"
            let $slot = 0;
            let $offset = 64;
            let $val = 0x1234567890ABCDEF;
            @vram_write($slot, $offset, $val);
            let $read = @vram_read($slot, $offset);
            @assert($read == $val);
        "#;
        let mut out = String::new();
        run_source_with_output(src, &[], &mut out).expect("VRAM execution failed");
    }

    #[test]
    fn test_vm_wcet_limit() {
        let src = r#"
            let mut $i = 0;
            while ($i >= 0) {
                $i += 1;
            }
        "#;
        let mut out = String::new();
        let res = run_source_with_output(src, &[], &mut out);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, "ERR_PX64_WCET_EXCEEDED");
    }
}
