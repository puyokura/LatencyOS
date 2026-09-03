//! Pulse Extended 64-bit Real-Time Architecture (px64) Instruction Set Architecture (ISA)

// -----------------------------------------------------------------------------
// px64 Instruction Set Opcode Constants
// -----------------------------------------------------------------------------

pub const PX64_OP_NOP: u8 = 0;
pub const PX64_OP_MOV_IMM: u8 = 1;      // [1, Rd, Imm_hi, Imm_lo] -> Rd = Imm16
pub const PX64_OP_MOV_REG: u8 = 2;      // [2, Rd, Rs1, 0]         -> Rd = Rs1
pub const PX64_OP_MOV_STR: u8 = 3;      // [3, Rd, Offset, Len]    -> Rd = STR_TAG | (Offset << 32) | Len
pub const PX64_OP_ADD: u8 = 4;          // [4, Rd, Rs1, Rs2]       -> Rd = Rs1 + Rs2
pub const PX64_OP_SUB: u8 = 5;          // [5, Rd, Rs1, Rs2]       -> Rd = Rs1 - Rs2
pub const PX64_OP_MUL: u8 = 6;          // [6, Rd, Rs1, Rs2]       -> Rd = Rs1 * Rs2
pub const PX64_OP_DIV: u8 = 7;          // [7, Rd, Rs1, Rs2]       -> Rd = Rs1 / Rs2 (0 protection)
pub const PX64_OP_MOD: u8 = 8;          // [8, Rd, Rs1, Rs2]       -> Rd = Rs1 % Rs2 (0 protection)
pub const PX64_OP_CMP_EQ: u8 = 9;       // [9, Rd, Rs1, Rs2]       -> Rd = (Rs1 == Rs2) ? 1 : 0
pub const PX64_OP_CMP_NE: u8 = 10;      // [10, Rd, Rs1, Rs2]      -> Rd = (Rs1 != Rs2) ? 1 : 0
pub const PX64_OP_CMP_LT: u8 = 11;      // [11, Rd, Rs1, Rs2]      -> Rd = (Rs1 < Rs2) ? 1 : 0
pub const PX64_OP_CMP_LE: u8 = 12;      // [12, Rd, Rs1, Rs2]      -> Rd = (Rs1 <= Rs2) ? 1 : 0
pub const PX64_OP_CMP_GT: u8 = 13;      // [13, Rd, Rs1, Rs2]      -> Rd = (Rs1 > Rs2) ? 1 : 0
pub const PX64_OP_CMP_GE: u8 = 14;      // [14, Rd, Rs1, Rs2]      -> Rd = (Rs1 >= Rs2) ? 1 : 0
pub const PX64_OP_JMP: u8 = 15;         // [15, 0, Target_hi, Target_lo] -> IP = Target
pub const PX64_OP_JZ: u8 = 16;          // [16, Rs1, Target_hi, Target_lo] -> if Rs1 == 0 { IP = Target }
pub const PX64_OP_JNZ: u8 = 17;         // [17, Rs1, Target_hi, Target_lo] -> if Rs1 != 0 { IP = Target }
pub const PX64_OP_CALL_NAT: u8 = 18;    // [18, Rd, FuncId, ArgReg] -> Rd = call_native(FuncId, ArgReg)
pub const PX64_OP_WITHIN_START: u8 = 19; // [19, Rs1, 0, 0]        -> Push deadline (Rs1 = us)
pub const PX64_OP_WITHIN_END: u8 = 20;  // [20, 0, 0, 0]           -> Pop deadline
pub const PX64_OP_DROP: u8 = 21;        // [21, 0, 0, 0]           -> Drop frame on deadline overrun
pub const PX64_OP_HALT: u8 = 22;        // [22, 0, 0, 0]           -> Halt VM
pub const PX64_OP_LDC: u8 = 23;         // [23, Rd, ConstIdx_hi, ConstIdx_lo] -> Rd = const_pool[ConstIdx] (0x17)
pub const PX64_OP_ADDI: u8 = 24;        // [24, Rd, Rs1, Imm8]     -> Rd = Rs1 + Imm8 (0x18)
pub const PX64_OP_SUBI: u8 = 25;        // [25, Rd, Rs1, Imm8]     -> Rd = Rs1 - Imm8 (0x19)
pub const PX64_OP_AND: u8 = 26;         // [26, Rd, Rs1, Rs2]      -> Rd = Rs1 & Rs2 (0x1a)
pub const PX64_OP_OR: u8 = 27;          // [27, Rd, Rs1, Rs2]      -> Rd = Rs1 | Rs2 (0x1b)
pub const PX64_OP_XOR: u8 = 28;         // [28, Rd, Rs1, Rs2]      -> Rd = Rs1 ^ Rs2 (0x1c)
pub const PX64_OP_SHL: u8 = 29;         // [29, Rd, Rs1, Rs2]      -> Rd = Rs1 << (Rs2 & 63) (0x1d)
pub const PX64_OP_SHR: u8 = 30;         // [30, Rd, Rs1, Rs2]      -> Rd = (Rs1 as u64 >> (Rs2 & 63)) as i64 (0x1e)
pub const PX64_OP_ARR_DEF: u8 = 31;     // [31, ArrId, Len_hi, Len_lo] -> array_lens[ArrId] = Len16 (0x1f)
pub const PX64_OP_ARR_LOAD: u8 = 32;    // [32, Rd, ArrId, Rs_idx] -> Rd = array_slots[base + Rs_idx] (0x20)
pub const PX64_OP_ARR_STORE: u8 = 33;   // [33, ArrId, Rs_idx, Rs_val] -> array_slots[base + Rs_idx] = Rs_val (0x21)
pub const PX64_OP_ASSERT: u8 = 34;      // [34, Rs1, 0, 0]         -> if Rs1 == 0 { halt with ERR_PX64_ASSERTION_FAILED } (0x22)
pub const PX64_OP_CALL: u8 = 35;        // [35, 0, Target_hi, Target_lo] -> IP = Target, push ret IP (0x23)
pub const PX64_OP_RET: u8 = 36;         // [36, 0, 0, 0]           -> pop ret IP from call stack (0x24)
pub const PX64_OP_STRUCT_DEF: u8 = 37;   // [37, InstId, FieldCount, 0] -> struct_field_counts[InstId] = FieldCount (0x25)
pub const PX64_OP_STRUCT_LOAD: u8 = 38;  // [38, Rd, InstId, FieldOffset] -> Rd = struct_slots[base + FieldOffset] (0x26)
pub const PX64_OP_STRUCT_STORE: u8 = 39; // [39, InstId, FieldOffset, Rs_val] -> struct_slots[base + FieldOffset] = Rs_val (0x27)
pub const PX64_OP_TBL_DEF: u8 = 40;      // [40, TblId, Base8, Len8]    -> table_bases[TblId] = Base, table_lens[TblId] = Len (0x28)
pub const PX64_OP_TBL_LOAD: u8 = 41;     // [41, Rd, TblId, Rs_idx]     -> Rd = const_pool[base + Rs_idx] (0x29)
pub const PX64_OP_STREQ: u8 = 42;        // [42, Rd, Rs1, Rs2]          -> Rd = (streq(Rs1, Rs2)) ? 1 : 0 (0x2a)
pub const PX64_OP_SPILL_STORE: u8 = 43;  // [43, SlotId, Rs_val, 0]     -> spill_slots[SlotId] = Rs_val (0x2b)
pub const PX64_OP_SPILL_LOAD: u8 = 44;   // [44, Rd, SlotId, 0]         -> Rd = spill_slots[SlotId] (0x2c)

// Legacy Bytecode Instruction Set (PULS v1/v2 backward compatibility)
pub const OP_NOP: u8 = 0;
pub const OP_PUSH_CONST: u8 = 1;
pub const OP_LOAD_VAR: u8 = 2;
pub const OP_STORE_VAR: u8 = 3;
pub const OP_ADD: u8 = 4;
pub const OP_SUB: u8 = 5;
pub const OP_MUL: u8 = 6;
pub const OP_DIV: u8 = 7;
pub const OP_MOD: u8 = 8;
pub const OP_CMP_EQ: u8 = 9;
pub const OP_CMP_NE: u8 = 10;
pub const OP_CMP_LT: u8 = 11;
pub const OP_CMP_LE: u8 = 12;
pub const OP_CMP_GT: u8 = 13;
pub const OP_CMP_GE: u8 = 14;
pub const OP_JUMP: u8 = 15;
pub const OP_JUMP_IF_FALSE: u8 = 16;
pub const OP_CALL_NATIVE: u8 = 17;
pub const OP_WITHIN_START: u8 = 18;
pub const OP_WITHIN_END: u8 = 19;
pub const OP_DROP: u8 = 20;
pub const OP_PUSH_STR: u8 = 21;
pub const OP_HALT: u8 = 22;

// Binary container constants
pub const PX64_BIN_MAGIC: [u8; 4] = *b"PX64";
pub const PX64_BIN_VERSION: u16 = 3;
pub const PX64_HEADER_SIZE: usize = 16;
pub const PX64_NUM_REGISTERS: usize = 20;
pub const MAX_CONST_POOL: usize = 64;

pub const PULSE_BIN_MAGIC: [u8; 4] = *b"PX64";
pub const PULSE_BIN_VERSION: u16 = 3;
pub const PULSE_HEADER_SIZE: usize = 16;

// Resource limits
pub const MAX_TOKENS: usize = 2048;
pub const MAX_BYTECODE_SIZE: usize = 4096;
pub const MAX_VARS: usize = 48;
pub const MAX_STRING_POOL: usize = 512;
pub const MAX_VM_STEPS: usize = 10_000;
pub const MAX_SCRIPT_TIMEOUT_NS: u64 = 5_000_000; // 5.0 ms wall-clock hard watchdog limit

// Tag masks
pub const STR_TAG: i64 = 0x4000_0000_0000_0000;
pub const ARG_TAG: i64 = 0x2000_0000_0000_0000;
pub const ERR_TAG: i64 = 0x1000_0000_0000_0000;
pub const MAX_CALL_DEPTH: usize = 8;

// -----------------------------------------------------------------------------
// Native Function IDs
// -----------------------------------------------------------------------------

pub const NATIVE_PRINT: u8 = 1;
pub const NATIVE_PRINTLN: u8 = 2;
pub const NATIVE_SYS_TSC: u8 = 3;
pub const NATIVE_NET_RTT: u8 = 4;
pub const NATIVE_NET_SET_RATE: u8 = 5;
pub const NATIVE_GPU_CAPTURE: u8 = 6;
pub const NATIVE_NET_SEND: u8 = 7;
pub const NATIVE_SCRIPT_ARGC: u8 = 8;
pub const NATIVE_SCRIPT_ARG: u8 = 9;
pub const NATIVE_TAG_OK: u8 = 10;
pub const NATIVE_TAG_ERR: u8 = 11;
pub const NATIVE_IS_OK: u8 = 12;
pub const NATIVE_IS_ERR: u8 = 13;
pub const NATIVE_UNWRAP: u8 = 14;
pub const NATIVE_STREQ: u8 = 15;
pub const NATIVE_CORE_ID: u8 = 16;
pub const NATIVE_TSC_FREQ: u8 = 17;
pub const NATIVE_UPTIME_NS: u8 = 18;
pub const NATIVE_BUSY_WAIT: u8 = 19;
pub const NATIVE_RING_DEPTH: u8 = 20;
pub const NATIVE_MATH_MIN: u8 = 21;
pub const NATIVE_MATH_MAX: u8 = 22;
pub const NATIVE_MATH_ABS: u8 = 23;
pub const NATIVE_MATH_CLAMP: u8 = 24;
pub const NATIVE_BIT_POPCNT: u8 = 25;
pub const NATIVE_BIT_LZCNT: u8 = 26;
pub const NATIVE_CRC32: u8 = 27;
pub const NATIVE_VRAM_READ: u8 = 28;
pub const NATIVE_VRAM_WRITE: u8 = 29;

/// Map register index to canonical x64-compatible register name ($rax..$r15, #f0..#f3).
pub fn px64_reg_name(reg_id: u8) -> &'static str {
    match reg_id {
        0 => "$rax",
        1 => "$rcx",
        2 => "$rdx",
        3 => "$rbx",
        4 => "$rsp",
        5 => "$rbp",
        6 => "$rsi",
        7 => "$rdi",
        8 => "$r8",
        9 => "$r9",
        10 => "$r10",
        11 => "$r11",
        12 => "$r12",
        13 => "$r13",
        14 => "$r14",
        15 => "$r15",
        16 => "#f0",
        17 => "#f1",
        18 => "#f2",
        19 => "#f3",
        _ => "$unk",
    }
}

/// Map native function ID to canonical DSL intrinsic name.
pub fn px64_native_name(func_id: u8) -> &'static str {
    match func_id {
        NATIVE_PRINT => "@print",
        NATIVE_PRINTLN => "@println",
        NATIVE_SYS_TSC => "@tsc",
        NATIVE_NET_RTT => "@rtt",
        NATIVE_NET_SET_RATE => "@rate",
        NATIVE_GPU_CAPTURE => "@capture",
        NATIVE_NET_SEND => "@send",
        NATIVE_SCRIPT_ARGC => "@argc",
        NATIVE_SCRIPT_ARG => "@arg",
        NATIVE_TAG_OK => "@ok",
        NATIVE_TAG_ERR => "@err",
        NATIVE_IS_OK => "@is_ok",
        NATIVE_IS_ERR => "@is_err",
        NATIVE_UNWRAP => "@unwrap",
        NATIVE_STREQ => "@streq",
        NATIVE_CORE_ID => "@core_id",
        NATIVE_TSC_FREQ => "@tsc_freq",
        NATIVE_UPTIME_NS => "@uptime_ns",
        NATIVE_BUSY_WAIT => "@busy_wait",
        NATIVE_RING_DEPTH => "@ring_depth",
        NATIVE_MATH_MIN => "@min",
        NATIVE_MATH_MAX => "@max",
        NATIVE_MATH_ABS => "@abs",
        NATIVE_MATH_CLAMP => "@clamp",
        NATIVE_BIT_POPCNT => "@popcnt",
        NATIVE_BIT_LZCNT => "@lzcnt",
        NATIVE_CRC32 => "@crc32",
        NATIVE_VRAM_READ => "@vram_read",
        NATIVE_VRAM_WRITE => "@vram_write",
        _ => "@native",
    }
}

/// Map opcode byte to opcode mnemonic string.
pub fn px64_op_name(op: u8) -> &'static str {
    match op {
        PX64_OP_NOP => "NOP",
        PX64_OP_MOV_IMM => "MOV",
        PX64_OP_MOV_REG => "MOV",
        PX64_OP_MOV_STR => "MOVS",
        PX64_OP_ADD => "ADD",
        PX64_OP_SUB => "SUB",
        PX64_OP_MUL => "MUL",
        PX64_OP_DIV => "DIV",
        PX64_OP_MOD => "MOD",
        PX64_OP_CMP_EQ => "CMPEQ",
        PX64_OP_CMP_NE => "CMPNE",
        PX64_OP_CMP_LT => "CMPLT",
        PX64_OP_CMP_LE => "CMPLE",
        PX64_OP_CMP_GT => "CMPGT",
        PX64_OP_CMP_GE => "CMPGE",
        PX64_OP_JMP => "JMP",
        PX64_OP_JZ => "JZ",
        PX64_OP_JNZ => "JNZ",
        PX64_OP_CALL_NAT => "CALL_NAT",
        PX64_OP_WITHIN_START => "WITHIN_START",
        PX64_OP_WITHIN_END => "WITHIN_END",
        PX64_OP_DROP => "DROP",
        PX64_OP_HALT => "HALT",
        PX64_OP_LDC => "LDC",
        PX64_OP_ADDI => "ADDI",
        PX64_OP_SUBI => "SUBI",
        PX64_OP_AND => "AND",
        PX64_OP_OR => "OR",
        PX64_OP_XOR => "XOR",
        PX64_OP_SHL => "SHL",
        PX64_OP_SHR => "SHR",
        PX64_OP_ARR_DEF => "ARR_DEF",
        PX64_OP_ARR_LOAD => "ARR_LOAD",
        PX64_OP_ARR_STORE => "ARR_STORE",
        PX64_OP_ASSERT => "ASSERT",
        PX64_OP_CALL => "CALL",
        PX64_OP_RET => "RET",
        PX64_OP_STRUCT_DEF => "STRUCT_DEF",
        PX64_OP_STRUCT_LOAD => "STRUCT_LOAD",
        PX64_OP_STRUCT_STORE => "STRUCT_STORE",
        PX64_OP_TBL_DEF => "TBL_DEF",
        PX64_OP_TBL_LOAD => "TBL_LOAD",
        PX64_OP_STREQ => "STREQ",
        PX64_OP_SPILL_STORE => "SPILL_STORE",
        PX64_OP_SPILL_LOAD => "SPILL_LOAD",
        _ => "UNKNOWN_OP",
    }
}
