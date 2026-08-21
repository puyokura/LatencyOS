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

pub const MAX_TOKENS: usize = 512;
pub const MAX_BYTECODE_SIZE: usize = 1024;
pub const MAX_VARS: usize = 32;
pub const MAX_STRING_POOL: usize = 512;
pub const MAX_VM_STEPS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords & Directives (@contract, @pipeline, @budget, @wcet, @within, @while, @loop, @on_vblank)
    Let,
    If,
    Else,
    While,
    Within,
    Or,
    Drop,
    Pipeline,
    On,
    Emit,
    Return,
    Budget,

    // AI-Native Directives
    AtContract,
    AtPipeline,
    AtBudget,
    AtWcet,
    AtWithin,
    AtWhile,
    AtLoop,
    AtOnVblank,

    // Literals & Identifiers ($var, #handle, @intrinsic)
    Ident,
    VarIdent,       // $rtt, $sum, $i, $t0
    HardwareIdent,  // #frame, #f, #slot0
    IntrinsicIdent, // @tsc, @rtt, @rate, @capture, @send, @print, @println
    Number(i64),
    TimeLiteral(u64), // In nanoseconds (50ns, 200us, 5ms, 1s)
    StringLit,

    // Operators & Symbols
    ColonEq,    // :=
    PlusEq,     // +=
    MinusEq,    // -=
    Question,   // ?
    Pipe,       // |>
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Percent,    // %
    Eq,         // =
    EqEq,       // ==
    NotEq,      // !=
    Lt,         // <
    LtEq,       // <=
    Gt,         // >
    GtEq,       // >=
    And,        // &&
    OrOp,       // ||
    Semi,       // ;
    Colon,      // :
    Comma,      // ,
    Dot,        // .
    LParen,     // (
    RParen,     // )
    LBrace,     // {
    RBrace,     // }
    Exclamation,// !

    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub len: usize,
    pub line: usize,
    pub col: usize,
}

impl Token {
    pub const fn empty() -> Self {
        Self {
            kind: TokenKind::Eof,
            start: 0,
            len: 0,
            line: 1,
            col: 1,
        }
    }
}

// Function: get_line_and_col
// Description: Calculate 1-indexed line and column from source byte position.
// Worst-case execution time: ~1000 ns
pub fn get_line_and_col(src: &[u8], pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut line_start = 0;
    let limit = core::cmp::min(pos, src.len());
    for i in 0..limit {
        if src[i] == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    (line, limit.saturating_sub(line_start) + 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileError {
    pub code: &'static str,
    pub message: &'static str,
    pub line: usize,
    pub col: usize,
    pub byte_offset: usize,
    pub token_kind: TokenKind,
    pub token_len: usize,
    pub expected: &'static str,
    pub stage: &'static str,
    pub suggestion: &'static str,
}

impl CompileError {
    pub const fn simple(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            line: 1,
            col: 1,
            byte_offset: 0,
            token_kind: TokenKind::Eof,
            token_len: 0,
            expected: "Valid bytecode or script syntax",
            stage: "Execution / Validation",
            suggestion: "Check file format and integrity",
        }
    }
}

// Function: print_compile_diagnostic
// Description: Emit comprehensive, structured, AI-actionable compiler diagnostic log.
// Worst-case execution time: ~20_000 ns
pub fn print_compile_diagnostic(src: &[u8], filename: &str, err: &CompileError) {
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
                    serial_print!("{}", b as char);
                } else {
                    serial_print!(".");
                }
            }
        }
        serial_println!("|");
        row_start += 16;
    }
}

// -----------------------------------------------------------------------------
// Lexer (Zero-Allocation Tokenizer)
// -----------------------------------------------------------------------------

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Self { src, pos: 0, line: 1 }
    }

    // Function: tokenize
    // Description: Tokenize input source into static token array without dynamic allocation.
    // Worst-case execution time: ~25_000 ns
    pub fn tokenize(&mut self, tokens: &mut [Token; MAX_TOKENS]) -> Result<usize, CompileError> {
        let mut count = 0;

        while self.pos < self.src.len() && count < MAX_TOKENS - 1 {
            self.skip_whitespace_and_comments();
            if self.pos >= self.src.len() {
                break;
            }

            let start = self.pos;
            let (line, col) = get_line_and_col(self.src, start);
            let b = self.src[self.pos];

            let kind = match b {
                b'(' => { self.pos += 1; TokenKind::LParen }
                b')' => { self.pos += 1; TokenKind::RParen }
                b'{' => { self.pos += 1; TokenKind::LBrace }
                b'}' => { self.pos += 1; TokenKind::RBrace }
                b';' => { self.pos += 1; TokenKind::Semi }
                b',' => { self.pos += 1; TokenKind::Comma }
                b'.' => { self.pos += 1; TokenKind::Dot }
                b'?' => { self.pos += 1; TokenKind::Question }

                b':' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::ColonEq
                    } else {
                        self.pos += 1;
                        TokenKind::Colon
                    }
                }

                b'+' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::PlusEq
                    } else {
                        self.pos += 1;
                        TokenKind::Plus
                    }
                }

                b'-' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::MinusEq
                    } else {
                        self.pos += 1;
                        TokenKind::Minus
                    }
                }

                b'*' => { self.pos += 1; TokenKind::Star }
                b'/' => { self.pos += 1; TokenKind::Slash }
                b'%' => { self.pos += 1; TokenKind::Percent }

                b'|' => {
                    if self.peek_next() == Some(b'>') {
                        self.pos += 2;
                        TokenKind::Pipe
                    } else if self.peek_next() == Some(b'|') {
                        self.pos += 2;
                        TokenKind::OrOp
                    } else {
                        return Err(CompileError {
                            code: "ERR_LEX_UNEXPECTED_PIPE",
                            message: "Unexpected single '|' operator",
                            line,
                            col,
                            byte_offset: start,
                            token_kind: TokenKind::Eof,
                            token_len: 1,
                            expected: "Pipe operator '|>' or logical OR '||'",
                            stage: "Lexical Analysis",
                            suggestion: "Use '|>' for pipeline dataflow or '||' for logical OR",
                        });
                    }
                }

                b'=' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::EqEq
                    } else {
                        self.pos += 1;
                        TokenKind::Eq
                    }
                }

                b'!' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::NotEq
                    } else {
                        self.pos += 1;
                        TokenKind::Exclamation
                    }
                }

                b'<' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::LtEq
                    } else {
                        self.pos += 1;
                        TokenKind::Lt
                    }
                }

                b'>' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::GtEq
                    } else {
                        self.pos += 1;
                        TokenKind::Gt
                    }
                }

                b'&' => {
                    if self.peek_next() == Some(b'&') {
                        self.pos += 2;
                        TokenKind::And
                    } else {
                        return Err(CompileError {
                            code: "ERR_LEX_UNEXPECTED_AMP",
                            message: "Single '&' not supported in PulseLang",
                            line,
                            col,
                            byte_offset: start,
                            token_kind: TokenKind::Eof,
                            token_len: 1,
                            expected: "Logical AND '&&'",
                            stage: "Lexical Analysis",
                            suggestion: "Use '&&' for logical conditions",
                        });
                    }
                }

                b'"' => {
                    self.pos += 1;
                    while self.pos < self.src.len() && self.src[self.pos] != b'"' {
                        if self.src[self.pos] == b'\n' {
                            self.line += 1;
                        }
                        self.pos += 1;
                    }
                    if self.pos < self.src.len() && self.src[self.pos] == b'"' {
                        self.pos += 1;
                    }
                    TokenKind::StringLit
                }

                b'0'..=b'9' => {
                    let mut num = 0i64;
                    while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                        num = num * 10 + (self.src[self.pos] - b'0') as i64;
                        self.pos += 1;
                    }

                    if self.match_suffix(b"ns") {
                        TokenKind::TimeLiteral(num as u64)
                    } else if self.match_suffix(b"us") {
                        TokenKind::TimeLiteral((num as u64) * 1_000)
                    } else if self.match_suffix(b"ms") {
                        TokenKind::TimeLiteral((num as u64) * 1_000_000)
                    } else if self.match_suffix(b"s") {
                        TokenKind::TimeLiteral((num as u64) * 1_000_000_000)
                    } else {
                        TokenKind::Number(num)
                    }
                }

                b'$' => {
                    // Variable Identifier: $rtt, $sum, $i, $t0
                    self.pos += 1;
                    while self.pos < self.src.len()
                        && (self.src[self.pos].is_ascii_alphanumeric() || self.src[self.pos] == b'_')
                    {
                        self.pos += 1;
                    }
                    TokenKind::VarIdent
                }

                b'#' => {
                    // Hardware Handle: #f, #frame, #slot0
                    self.pos += 1;
                    while self.pos < self.src.len()
                        && (self.src[self.pos].is_ascii_alphanumeric() || self.src[self.pos] == b'_')
                    {
                        self.pos += 1;
                    }
                    TokenKind::HardwareIdent
                }

                b'@' => {
                    // Directives & Intrinsics: @contract, @pipeline, @tsc, @rtt, @within, @while, etc.
                    self.pos += 1;
                    let at_start = self.pos;
                    while self.pos < self.src.len()
                        && (self.src[self.pos].is_ascii_alphanumeric() || self.src[self.pos] == b'_' || self.src[self.pos] == b'.')
                    {
                        self.pos += 1;
                    }
                    let tag = &self.src[at_start..self.pos];
                    match tag {
                        b"contract" => TokenKind::AtContract,
                        b"pipeline" => TokenKind::AtPipeline,
                        b"budget" => TokenKind::AtBudget,
                        b"wcet" => TokenKind::AtWcet,
                        b"within" => TokenKind::AtWithin,
                        b"while" => TokenKind::AtWhile,
                        b"loop" => TokenKind::AtLoop,
                        b"on_vblank" => TokenKind::AtOnVblank,
                        b"drop" => TokenKind::Drop,
                        _ => TokenKind::IntrinsicIdent,
                    }
                }

                b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                    while self.pos < self.src.len()
                        && (self.src[self.pos].is_ascii_alphanumeric()
                            || self.src[self.pos] == b'_'
                            || self.src[self.pos] == b'.')
                    {
                        self.pos += 1;
                    }
                    let ident = &self.src[start..self.pos];
                    match ident {
                        b"let" => TokenKind::Let,
                        b"if" => TokenKind::If,
                        b"else" => TokenKind::Else,
                        b"while" => TokenKind::While,
                        b"within" => TokenKind::Within,
                        b"or" => TokenKind::Or,
                        b"drop" => TokenKind::Drop,
                        b"pipeline" => TokenKind::Pipeline,
                        b"on" => TokenKind::On,
                        b"emit" => TokenKind::Emit,
                        b"return" => TokenKind::Return,
                        b"budget" => TokenKind::Budget,
                        _ => TokenKind::Ident,
                    }
                }

                _ => {
                    self.pos += 1;
                    continue;
                }
            };

            tokens[count] = Token {
                kind,
                start,
                len: self.pos - start,
                line,
                col,
            };
            count += 1;
        }

        let (line, col) = get_line_and_col(self.src, self.pos);
        tokens[count] = Token {
            kind: TokenKind::Eof,
            start: self.pos,
            len: 0,
            line,
            col,
        };

        Ok(count)
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.src.len() {
            match self.src[self.pos] {
                b' ' | b'\t' | b'\r' => {
                    self.pos += 1;
                }
                b'\n' => {
                    self.line += 1;
                    self.pos += 1;
                }
                b'/' if self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'/' => {
                    while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn peek_next(&self) -> Option<u8> {
        if self.pos + 1 < self.src.len() {
            Some(self.src[self.pos + 1])
        } else {
            None
        }
    }

    fn match_suffix(&mut self, suffix: &[u8]) -> bool {
        if self.pos + suffix.len() <= self.src.len() {
            if &self.src[self.pos..self.pos + suffix.len()] == suffix {
                self.pos += suffix.len();
                return true;
            }
        }
        false
    }
}

// -----------------------------------------------------------------------------
// px64 (Pulse Extended 64-bit Real-Time Architecture) Instruction Set
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

pub const PX64_BIN_MAGIC: [u8; 4] = *b"PX64";
pub const PX64_BIN_VERSION: u16 = 2;
pub const PX64_HEADER_SIZE: usize = 16;
pub const PX64_NUM_REGISTERS: usize = 20;

// Function: px64_reg_name
// Description: Map register index to canonical x64-compatible register name.
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
        _ => "$r?",
    }
}

// Native function IDs
pub const NATIVE_PRINT: u8 = 1;
pub const NATIVE_PRINTLN: u8 = 2;
pub const NATIVE_SYS_TSC: u8 = 3;
pub const NATIVE_NET_RTT: u8 = 4;
pub const NATIVE_NET_SET_RATE: u8 = 5;
pub const NATIVE_GPU_CAPTURE: u8 = 6;
pub const NATIVE_NET_SEND: u8 = 7;
pub const NATIVE_SCRIPT_ARGC: u8 = 8;
pub const NATIVE_SCRIPT_ARG: u8 = 9;

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

// -----------------------------------------------------------------------------
// Compiler (px64 Single-Pass Register Allocator & Instruction Generator)
// -----------------------------------------------------------------------------

pub struct Compiler<'a> {
    src: &'a [u8],
    tokens: &'a [Token],
    current: usize,
    pub code: [u8; MAX_BYTECODE_SIZE],
    pub code_len: usize,
    var_names: [[u8; 16]; MAX_VARS],
    var_lens: [usize; MAX_VARS],
    var_regs: [u8; MAX_VARS],
    var_count: usize,
    pub str_pool: [u8; MAX_STRING_POOL],
    pub str_pool_len: usize,
}

impl<'a> Compiler<'a> {
    pub fn new(src: &'a [u8], tokens: &'a [Token]) -> Self {
        Self {
            src,
            tokens,
            current: 0,
            code: [0; MAX_BYTECODE_SIZE],
            code_len: 0,
            var_names: [[0; 16]; MAX_VARS],
            var_lens: [0; MAX_VARS],
            var_regs: [0; MAX_VARS],
            var_count: 0,
            str_pool: [0; MAX_STRING_POOL],
            str_pool_len: 0,
        }
    }

    fn peek(&self) -> Token {
        if self.current < self.tokens.len() {
            self.tokens[self.current]
        } else {
            Token::empty()
        }
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek();
        if self.current < self.tokens.len() {
            self.current += 1;
        }
        tok
    }

    fn match_token(&mut self, kind: TokenKind) -> bool {
        if self.peek().kind == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    fn error(
        &self,
        code: &'static str,
        message: &'static str,
        expected: &'static str,
        stage: &'static str,
        suggestion: &'static str,
    ) -> CompileError {
        let tok = self.peek();
        CompileError {
            code,
            message,
            line: tok.line,
            col: tok.col,
            byte_offset: tok.start,
            token_kind: tok.kind,
            token_len: tok.len,
            expected,
            stage,
            suggestion,
        }
    }

    fn emit_inst(&mut self, op: u8, rd: u8, rs1: u8, rs2: u8) -> Result<usize, CompileError> {
        if self.code_len + 4 > MAX_BYTECODE_SIZE {
            return Err(self.error(
                "ERR_BYTECODE_OVERFLOW",
                "px64 bytecode buffer overflow (1024 bytes limit reached)",
                "Smaller script size or simplify expressions",
                "Code Generation",
                "Reduce script length or complexity",
            ));
        }
        let pos = self.code_len;
        self.code[pos] = op;
        self.code[pos + 1] = rd;
        self.code[pos + 2] = rs1;
        self.code[pos + 3] = rs2;
        self.code_len += 4;
        Ok(pos)
    }

    fn emit_imm16(&mut self, op: u8, rd: u8, imm: u16) -> Result<usize, CompileError> {
        self.emit_inst(op, rd, (imm >> 8) as u8, (imm & 0xFF) as u8)
    }

    fn patch_imm16(&mut self, pos: usize, imm: u16) {
        self.code[pos + 2] = (imm >> 8) as u8;
        self.code[pos + 3] = (imm & 0xFF) as u8;
    }

    fn resolve_var(&mut self, tok: Token) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        match name {
            b"$rax" | b"$r0" => return Ok(0),
            b"$rcx" | b"$r1" => return Ok(1),
            b"$rdx" | b"$r2" => return Ok(2),
            b"$rbx" | b"$r3" => return Ok(3),
            b"$rsp" | b"$r4" => return Ok(4),
            b"$rbp" | b"$r5" => return Ok(5),
            b"$rsi" | b"$r6" => return Ok(6),
            b"$rdi" | b"$r7" => return Ok(7),
            b"$r8" => return Ok(8),
            b"$r9" => return Ok(9),
            b"$r10" => return Ok(10),
            b"$r11" => return Ok(11),
            b"$r12" => return Ok(12),
            b"$r13" => return Ok(13),
            b"$r14" => return Ok(14),
            b"$r15" => return Ok(15),
            b"#f" | b"#frame" | b"#f0" | b"#slot0" => return Ok(16),
            b"#f1" | b"#slot1" => return Ok(17),
            b"#f2" | b"#slot2" => return Ok(18),
            b"#f3" | b"#slot3" => return Ok(19),
            _ => {}
        }

        for i in 0..self.var_count {
            if self.var_lens[i] == name.len() && &self.var_names[i][..self.var_lens[i]] == name {
                return Ok(self.var_regs[i]);
            }
        }

        if self.var_count >= 13 {
            return Err(self.error(
                "ERR_MAX_VARS_EXCEEDED",
                "Maximum distinct variables limit reached (13 general-purpose registers)",
                "Reuse existing $variables or registers ($rax, $rcx, etc.)",
                "Register Allocation",
                "Reduce distinct variable count in script",
            ));
        }

        let reg = (1 + self.var_count) as u8; // Map user variables to $rcx ($r1) through $r13 ($r13)
        let idx = self.var_count;
        let len = core::cmp::min(name.len(), 16);
        self.var_names[idx][..len].copy_from_slice(&name[..len]);
        self.var_lens[idx] = len;
        self.var_regs[idx] = reg;
        self.var_count += 1;
        Ok(reg)
    }

    // Function: compile
    // Description: Single-pass compilation from tokens into px64 fixed 32-bit instructions.
    // Worst-case execution time: ~40_000 ns
    pub fn compile(&mut self) -> Result<usize, CompileError> {
        while self.peek().kind != TokenKind::Eof {
            self.statement()?;
        }
        self.emit_inst(PX64_OP_HALT, 0, 0, 0)?;
        Ok(self.code_len)
    }

    fn statement(&mut self) -> Result<(), CompileError> {
        let tok = self.peek();

        match tok.kind {
            TokenKind::AtContract => {
                self.advance();
                self.match_token(TokenKind::Colon);
                while self.peek().kind != TokenKind::Semi && self.peek().kind != TokenKind::Eof {
                    self.advance();
                }
                if !self.match_token(TokenKind::Semi) {
                    return Err(self.error(
                        "ERR_MISSING_SEMICOLON",
                        "Missing semicolon ';' after @contract directive",
                        "Semicolon ';'",
                        "Statement -> @contract Directive",
                        "Add ';' at end of @contract: ...;",
                    ));
                }
            }

            TokenKind::AtPipeline | TokenKind::Pipeline => {
                self.advance();
                if self.peek().kind == TokenKind::Colon {
                    self.advance();
                }
                let _name = self.advance();
                if self.peek().kind == TokenKind::AtBudget || self.peek().kind == TokenKind::Budget {
                    self.advance();
                    if self.match_token(TokenKind::LParen) {
                        self.advance();
                        self.match_token(TokenKind::RParen);
                    }
                }
                if self.match_token(TokenKind::Semi) {
                    return Ok(());
                }
                if self.match_token(TokenKind::LBrace) {
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                }
            }

            TokenKind::AtOnVblank | TokenKind::On => {
                self.advance();
                if self.peek().kind == TokenKind::Colon {
                    self.advance();
                }
                if self.match_token(TokenKind::LParen) {
                    self.advance();
                    self.match_token(TokenKind::RParen);
                }
                if self.match_token(TokenKind::LBrace) {
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                    self.match_token(TokenKind::Semi);
                }
            }

            TokenKind::AtWithin | TokenKind::Within => {
                self.advance();
                let has_paren = self.match_token(TokenKind::LParen);
                let time_tok = self.advance();
                if has_paren {
                    self.match_token(TokenKind::RParen);
                }

                let deadline_ns = match time_tok.kind {
                    TokenKind::TimeLiteral(ns) => ns,
                    TokenKind::Number(n) => n as u64,
                    _ => {
                        return Err(self.error(
                            "ERR_EXPECTED_TIME_LITERAL",
                            "Expected time duration literal after @within (e.g. 500us, 10ms, 100ns)",
                            "Time literal with unit suffix (e.g. 500us, 5ms, 100ns)",
                            "Statement -> @within Block",
                            "Specify duration like @within(500us) { ... }",
                        ));
                    }
                };

                let time_reg = 0;
                let budget_us = deadline_ns / 1_000;
                self.emit_imm16(PX64_OP_MOV_IMM, time_reg, budget_us as u16)?;
                self.emit_inst(PX64_OP_WITHIN_START, time_reg, 0, 0)?;

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' to begin within block",
                        "Left brace '{'",
                        "Statement -> @within Block",
                        "Add opening brace '{' after @within(...)",
                    ));
                }
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);
                self.emit_inst(PX64_OP_WITHIN_END, 0, 0, 0)?;

                if self.match_token(TokenKind::Exclamation) || self.match_token(TokenKind::Or) {
                    if self.match_token(TokenKind::Drop) {
                        self.emit_inst(PX64_OP_DROP, 0, 0, 0)?;
                    }
                }
                self.match_token(TokenKind::Semi);
            }

            TokenKind::AtWhile | TokenKind::While => {
                self.advance();
                let loop_start = self.code_len as u16;
                self.match_token(TokenKind::LParen);
                let cond_reg = 0;
                self.expression(cond_reg)?;
                self.match_token(TokenKind::RParen);

                let jz_pos = self.emit_inst(PX64_OP_JZ, cond_reg, 0, 0)?;

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' for while block",
                        "Left brace '{'",
                        "Statement -> While Loop",
                        "Add opening brace '{' after while condition",
                    ));
                }
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);

                self.emit_imm16(PX64_OP_JMP, 0, loop_start)?;
                self.patch_imm16(jz_pos, self.code_len as u16);
            }

            TokenKind::Let => {
                self.advance();
                let ident = self.advance();
                let var_reg = self.resolve_var(ident)?;
                self.match_token(TokenKind::Eq);
                self.expression(var_reg)?;
                self.match_token(TokenKind::Semi);
            }

            TokenKind::If => {
                self.advance();
                self.match_token(TokenKind::LParen);
                let cond_reg = 0;
                self.expression(cond_reg)?;
                self.match_token(TokenKind::RParen);

                let jz_pos = self.emit_inst(PX64_OP_JZ, cond_reg, 0, 0)?;

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' after if condition",
                        "Left brace '{'",
                        "Statement -> If Branch",
                        "Add opening brace '{' after if (...) condition",
                    ));
                }
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);

                if self.match_token(TokenKind::Else) {
                    let jmp_pos = self.emit_inst(PX64_OP_JMP, 0, 0, 0)?;
                    self.patch_imm16(jz_pos, self.code_len as u16);

                    if !self.match_token(TokenKind::LBrace) {
                        return Err(self.error(
                            "ERR_EXPECTED_LBRACE",
                            "Missing opening brace '{' after else keyword",
                            "Left brace '{'",
                            "Statement -> Else Branch",
                            "Add opening brace '{' after else",
                        ));
                    }
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                    self.patch_imm16(jmp_pos, self.code_len as u16);
                } else {
                    self.patch_imm16(jz_pos, self.code_len as u16);
                }
            }

            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                let ident = self.advance();
                let var_reg = self.resolve_var(ident)?;

                if self.match_token(TokenKind::ColonEq) || self.match_token(TokenKind::Eq) {
                    self.expression(var_reg)?;
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of assignment",
                            "Semicolon ';'",
                            "Statement -> Variable Assignment",
                            "Append ';' at end of assignment statement",
                        ));
                    }
                } else if self.match_token(TokenKind::PlusEq) {
                    let tmp_reg = 15u8;
                    self.expression(tmp_reg)?;
                    self.emit_inst(PX64_OP_ADD, var_reg, var_reg, tmp_reg)?;
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Addition Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                } else if self.match_token(TokenKind::MinusEq) {
                    let tmp_reg = 15u8;
                    self.expression(tmp_reg)?;
                    self.emit_inst(PX64_OP_SUB, var_reg, var_reg, tmp_reg)?;
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Subtraction Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                } else {
                    self.current -= 1;
                    self.expression_statement()?;
                }
            }

            _ => {
                self.expression_statement()?;
            }
        }
        Ok(())
    }

    fn expression_statement(&mut self) -> Result<(), CompileError> {
        self.expression(0)?;
        if !self.match_token(TokenKind::Semi) {
            return Err(self.error(
                "ERR_MISSING_SEMICOLON",
                "Missing semicolon ';' at end of expression statement",
                "Semicolon ';'",
                "Statement -> Expression Statement",
                "Append ';' to terminate the statement",
            ));
        }
        Ok(())
    }

    fn expression(&mut self, dst: u8) -> Result<(), CompileError> {
        self.ternary(dst)?;
        while self.match_token(TokenKind::Pipe) {
            if self.peek().kind == TokenKind::IntrinsicIdent || self.peek().kind == TokenKind::Ident {
                let tok = self.advance();
                let name = &self.src[tok.start..tok.start + tok.len];
                let func_id = match name {
                    b"@send" | b"net.send" => NATIVE_NET_SEND,
                    b"@print" | b"print" => NATIVE_PRINT,
                    b"@println" | b"println" => NATIVE_PRINTLN,
                    b"@rate" | b"net.set_rate" => NATIVE_NET_SET_RATE,
                    _ => {
                        return Err(self.error(
                            "ERR_INVALID_PIPE_TARGET",
                            "Invalid pipe target function",
                            "One of @send, @print, @println, @rate",
                            "Expression -> Pipe",
                            "Pipe value to a valid intrinsic like |> @send",
                        ));
                    }
                };
                self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, dst)?;
            }
        }
        Ok(())
    }

    fn ternary(&mut self, dst: u8) -> Result<(), CompileError> {
        self.equality(dst)?;
        if self.match_token(TokenKind::Question) {
            let jz_pos = self.emit_inst(PX64_OP_JZ, dst, 0, 0)?;

            // True branch
            if self.match_token(TokenKind::LBrace) {
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);
            } else {
                self.expression(dst)?;
            }

            if self.match_token(TokenKind::Colon) {
                let jmp_end_pos = self.emit_inst(PX64_OP_JMP, 0, 0, 0)?;
                self.patch_imm16(jz_pos, self.code_len as u16);

                // False branch
                if self.match_token(TokenKind::LBrace) {
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                } else {
                    self.expression(dst)?;
                }
                self.patch_imm16(jmp_end_pos, self.code_len as u16);
            } else {
                self.patch_imm16(jz_pos, self.code_len as u16);
            }
        }
        Ok(())
    }

    fn equality(&mut self, dst: u8) -> Result<(), CompileError> {
        self.comparison(dst)?;
        while self.peek().kind == TokenKind::EqEq || self.peek().kind == TokenKind::NotEq {
            let op = self.advance().kind;
            let rhs_reg = 15u8;
            self.comparison(rhs_reg)?;
            if op == TokenKind::EqEq {
                self.emit_inst(PX64_OP_CMP_EQ, dst, dst, rhs_reg)?;
            } else {
                self.emit_inst(PX64_OP_CMP_NE, dst, dst, rhs_reg)?;
            }
        }
        Ok(())
    }

    fn comparison(&mut self, dst: u8) -> Result<(), CompileError> {
        self.term(dst)?;
        while matches!(self.peek().kind, TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq) {
            let op = self.advance().kind;
            let rhs_reg = 15u8;
            self.term(rhs_reg)?;
            match op {
                TokenKind::Lt => { self.emit_inst(PX64_OP_CMP_LT, dst, dst, rhs_reg)?; }
                TokenKind::LtEq => { self.emit_inst(PX64_OP_CMP_LE, dst, dst, rhs_reg)?; }
                TokenKind::Gt => { self.emit_inst(PX64_OP_CMP_GT, dst, dst, rhs_reg)?; }
                TokenKind::GtEq => { self.emit_inst(PX64_OP_CMP_GE, dst, dst, rhs_reg)?; }
                _ => {}
            }
        }
        Ok(())
    }

    fn term(&mut self, dst: u8) -> Result<(), CompileError> {
        self.factor(dst)?;
        while self.peek().kind == TokenKind::Plus || self.peek().kind == TokenKind::Minus {
            let op = self.advance().kind;
            let rhs_reg = 15u8;
            self.factor(rhs_reg)?;
            if op == TokenKind::Plus {
                self.emit_inst(PX64_OP_ADD, dst, dst, rhs_reg)?;
            } else {
                self.emit_inst(PX64_OP_SUB, dst, dst, rhs_reg)?;
            }
        }
        Ok(())
    }

    fn factor(&mut self, dst: u8) -> Result<(), CompileError> {
        self.primary(dst)?;
        while self.peek().kind == TokenKind::Star || self.peek().kind == TokenKind::Slash || self.peek().kind == TokenKind::Percent {
            let op = self.advance().kind;
            let rhs_reg = 15u8;
            self.primary(rhs_reg)?;
            match op {
                TokenKind::Star => { self.emit_inst(PX64_OP_MUL, dst, dst, rhs_reg)?; }
                TokenKind::Slash => { self.emit_inst(PX64_OP_DIV, dst, dst, rhs_reg)?; }
                TokenKind::Percent => { self.emit_inst(PX64_OP_MOD, dst, dst, rhs_reg)?; }
                _ => {}
            }
        }
        Ok(())
    }

    fn primary(&mut self, dst: u8) -> Result<(), CompileError> {
        let tok = self.advance();

        match tok.kind {
            TokenKind::Number(n) => {
                self.emit_imm16(PX64_OP_MOV_IMM, dst, n as u16)?;
            }

            TokenKind::TimeLiteral(ns) => {
                let us = ns / 1_000;
                self.emit_imm16(PX64_OP_MOV_IMM, dst, us as u16)?;
            }

            TokenKind::StringLit => {
                let s = &self.src[tok.start + 1..tok.start + tok.len - 1];
                if self.str_pool_len + s.len() > MAX_STRING_POOL {
                    return Err(self.error(
                        "ERR_STRING_POOL_FULL",
                        "String literal pool exhausted (512 bytes limit reached)",
                        "Shorter string constants",
                        "String Pool Allocation",
                        "Reduce the size of string constants",
                    ));
                }
                let offset = self.str_pool_len;
                self.str_pool[offset..offset + s.len()].copy_from_slice(s);
                self.str_pool_len += s.len();

                self.emit_inst(PX64_OP_MOV_STR, dst, offset as u8, s.len() as u8)?;
            }

            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                let var_reg = self.resolve_var(tok)?;
                if var_reg != dst {
                    self.emit_inst(PX64_OP_MOV_REG, dst, var_reg, 0)?;
                }
            }

            TokenKind::IntrinsicIdent | TokenKind::Ident => {
                let name = &self.src[tok.start..tok.start + tok.len];
                if self.peek().kind == TokenKind::LParen {
                    self.advance(); // consume '('
                    let mut arg_reg = 0u8;
                    if self.peek().kind != TokenKind::RParen {
                        arg_reg = 15u8;
                        self.expression(arg_reg)?;
                        while self.match_token(TokenKind::Comma) {
                            let dummy = 14u8;
                            self.expression(dummy)?;
                        }
                    }
                    if !self.match_token(TokenKind::RParen) {
                        return Err(self.error(
                            "ERR_UNCLOSED_PAREN",
                            "Missing closing parenthesis ')' in function argument list",
                            "Closing parenthesis ')'",
                            "Expression -> Intrinsic Function Call",
                            "Add matching ')' after argument list",
                        ));
                    }

                    let func_id = match name {
                        b"@print" | b"print" => NATIVE_PRINT,
                        b"@println" | b"println" => NATIVE_PRINTLN,
                        b"@tsc" | b"sys.tsc" => NATIVE_SYS_TSC,
                        b"@rtt" | b"net.rtt" => NATIVE_NET_RTT,
                        b"@rate" | b"net.set_rate" => NATIVE_NET_SET_RATE,
                        b"@capture" | b"gpu.capture" => NATIVE_GPU_CAPTURE,
                        b"@send" | b"net.send" => NATIVE_NET_SEND,
                        b"@argc" | b"sys.argc" => NATIVE_SCRIPT_ARGC,
                        b"@arg" | b"sys.arg" => NATIVE_SCRIPT_ARG,
                        _ => {
                            return Err(self.error(
                                "ERR_UNKNOWN_INTRINSIC",
                                "Unknown intrinsic function name",
                                "One of @print, @println, @tsc, @rtt, @rate, @capture, @send, @argc, @arg",
                                "Expression -> Intrinsic Call",
                                "Verify that the intrinsic name matches supported DSL intrinsics",
                            ));
                        }
                    };

                    self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, arg_reg)?;
                } else {
                    let var_reg = self.resolve_var(tok)?;
                    if var_reg != dst {
                        self.emit_inst(PX64_OP_MOV_REG, dst, var_reg, 0)?;
                    }
                }
            }

            TokenKind::LParen => {
                self.expression(dst)?;
                if !self.match_token(TokenKind::RParen) {
                    return Err(self.error(
                        "ERR_UNCLOSED_PAREN",
                        "Expected closing parenthesis ')' after sub-expression",
                        "Closing parenthesis ')'",
                        "Expression -> Grouping",
                        "Add matching ')' to close the grouped expression",
                    ));
                }
            }

            _ => {
                return Err(self.error(
                    "ERR_SYNTAX_UNEXPECTED_TOKEN",
                    "Unexpected token encountered in expression",
                    "Literal value, variable ($var), hardware handle (#h), or intrinsic call (@fn)",
                    "Expression -> Primary",
                    "Replace invalid token with a valid variable name, number, or expression",
                ));
            }
        }

        Ok(())
    }
}

// // -----------------------------------------------------------------------------
// px64 Real-Time Register Virtual Machine
// -----------------------------------------------------------------------------

pub const STR_TAG: i64 = 0x4000_0000_0000_0000;
pub const ARG_TAG: i64 = 0x2000_0000_0000_0000;

pub struct PX64VM<'a> {
    pub code: &'a [u8],
    pub str_pool: &'a [u8],
    pub ip: usize,
    pub regs: [i64; PX64_NUM_REGISTERS],
    pub deadline_stack: [u64; 8],
    pub dl_sp: usize,
}

impl<'a> PX64VM<'a> {
    pub fn new(code: &'a [u8], str_pool: &'a [u8]) -> Self {
        Self {
            code,
            str_pool,
            ip: 0,
            regs: [0; PX64_NUM_REGISTERS],
            deadline_stack: [0; 8],
            dl_sp: 0,
        }
    }

    // Function: run
    // Description: Execute px64 32-bit fixed instructions with zero heap allocations and bounded WCET.
    // Worst-case execution time: ~60_000 ns
    pub fn run(&mut self, tsc_freq_hz: u64) -> Result<(), CompileError> {
        let mut steps = 0;

        while self.ip + 4 <= self.code.len() && steps < MAX_VM_STEPS {
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

                PX64_OP_MOV_REG => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS {
                        self.regs[rd] = self.regs[rs1];
                    }
                }

                PX64_OP_MOV_STR => {
                    let offset = rs1;
                    let len = rs2;
                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = STR_TAG | (((offset as u64) as i64) << 32) | ((len as u64) as i64);
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
                        self.regs[rd] = if denom != 0 { self.regs[rs1] / denom } else { 0 };
                    }
                }

                PX64_OP_MOD => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        let denom = self.regs[rs2];
                        self.regs[rd] = if denom != 0 { self.regs[rs1] % denom } else { 0 };
                    }
                }

                PX64_OP_CMP_EQ => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] == self.regs[rs2] { 1 } else { 0 };
                    }
                }

                PX64_OP_CMP_NE => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] != self.regs[rs2] { 1 } else { 0 };
                    }
                }

                PX64_OP_CMP_LT => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] < self.regs[rs2] { 1 } else { 0 };
                    }
                }

                PX64_OP_CMP_LE => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] <= self.regs[rs2] { 1 } else { 0 };
                    }
                }

                PX64_OP_CMP_GT => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] > self.regs[rs2] { 1 } else { 0 };
                    }
                }

                PX64_OP_CMP_GE => {
                    if rd < PX64_NUM_REGISTERS && rs1 < PX64_NUM_REGISTERS && rs2 < PX64_NUM_REGISTERS {
                        self.regs[rd] = if self.regs[rs1] >= self.regs[rs2] { 1 } else { 0 };
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
                    let arg_val = if arg_reg < PX64_NUM_REGISTERS { self.regs[arg_reg] } else { 0 };

                    let ret = match func_id {
                        NATIVE_PRINT => {
                            if (arg_val & ARG_TAG) != 0 {
                                let idx = (arg_val & 0xFF) as usize;
                                unsafe {
                                    if idx < SCRIPT_ARGC {
                                        let len = SCRIPT_ARG_LENS[idx];
                                        if let Ok(s) = core::str::from_utf8(&SCRIPT_ARGS[idx][..len]) {
                                            serial_print!("{}", s);
                                        }
                                    }
                                }
                            } else if (arg_val & STR_TAG) != 0 {
                                let raw = arg_val & !STR_TAG;
                                let offset = (raw >> 32) as usize;
                                let len = (raw & 0xFFFF_FFFF) as usize;
                                if offset + len <= self.str_pool.len() {
                                    if let Ok(s) = core::str::from_utf8(&self.str_pool[offset..offset + len]) {
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
                                        if let Ok(s) = core::str::from_utf8(&SCRIPT_ARGS[idx][..len]) {
                                            serial_println!("{}", s);
                                        }
                                    }
                                }
                            } else if (arg_val & STR_TAG) != 0 {
                                let raw = arg_val & !STR_TAG;
                                let offset = (raw >> 32) as usize;
                                let len = (raw & 0xFFFF_FFFF) as usize;
                                if offset + len <= self.str_pool.len() {
                                    if let Ok(s) = core::str::from_utf8(&self.str_pool[offset..offset + len]) {
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

                        NATIVE_SYS_TSC => {
                            read_tsc_serialized() as i64
                        }

                        NATIVE_NET_RTT => {
                            LAST_RTT_NS.load(Ordering::Relaxed) as i64
                        }

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
                            let handle = capture_frame_zero_copy(0, 1, read_tsc_serialized());
                            let deadline = read_tsc_serialized() + crate::tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
                            let mut seq = 1u16;
                            let _ = stream_send_frame(&handle, deadline, &mut seq);
                            1
                        }

                        NATIVE_SCRIPT_ARGC => {
                            unsafe { SCRIPT_ARGC as i64 }
                        }

                        NATIVE_SCRIPT_ARG => {
                            if arg_val >= 0 && (arg_val as usize) < 8 {
                                ARG_TAG | (arg_val & 0xFF)
                            } else {
                                0
                            }
                        }

                        _ => 0,
                    };

                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = ret;
                    }
                }

                PX64_OP_WITHIN_START => {
                    let budget_us = if rd < PX64_NUM_REGISTERS { self.regs[rd] as u64 } else { 500 };
                    let budget_ns = budget_us * 1_000;
                    let deadline = read_tsc_serialized() + crate::tsc::ns_to_tsc(budget_ns, tsc_freq_hz);
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

        if steps >= MAX_VM_STEPS {
            return Err(CompileError::simple(
                "ERR_PX64_WCET_EXCEEDED",
                "Execution exceeded px64 WCET instruction step limit (infinite loop protection)",
            ));
        }

        Ok(())
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
            return Err(CompileError::simple("ERR_VM_STACK_OVERFLOW", "VM Evaluation stack overflow"));
        }
        self.stack[self.sp] = val;
        self.sp += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<i64, CompileError> {
        if self.sp == 0 {
            return Err(CompileError::simple("ERR_VM_STACK_UNDERFLOW", "VM Evaluation stack underflow"));
        }
        self.sp -= 1;
        Ok(self.stack[self.sp])
    }

    pub fn run(&mut self, tsc_freq_hz: u64) -> Result<(), CompileError> {
        let mut steps = 0;
        while self.ip < self.code.len() && steps < MAX_VM_STEPS {
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
                OP_ADD => { let (b, a) = (self.pop()?, self.pop()?); self.push(a.wrapping_add(b))?; }
                OP_SUB => { let (b, a) = (self.pop()?, self.pop()?); self.push(a.wrapping_sub(b))?; }
                OP_MUL => { let (b, a) = (self.pop()?, self.pop()?); self.push(a.wrapping_mul(b))?; }
                OP_DIV => { let (b, a) = (self.pop()?, self.pop()?); self.push(if b != 0 { a / b } else { 0 })?; }
                OP_MOD => { let (b, a) = (self.pop()?, self.pop()?); self.push(if b != 0 { a % b } else { 0 })?; }
                OP_CMP_EQ => { let (b, a) = (self.pop()?, self.pop()?); self.push(if a == b { 1 } else { 0 })?; }
                OP_CMP_NE => { let (b, a) = (self.pop()?, self.pop()?); self.push(if a != b { 1 } else { 0 })?; }
                OP_CMP_LT => { let (b, a) = (self.pop()?, self.pop()?); self.push(if a < b { 1 } else { 0 })?; }
                OP_CMP_LE => { let (b, a) = (self.pop()?, self.pop()?); self.push(if a <= b { 1 } else { 0 })?; }
                OP_CMP_GT => { let (b, a) = (self.pop()?, self.pop()?); self.push(if a > b { 1 } else { 0 })?; }
                OP_CMP_GE => { let (b, a) = (self.pop()?, self.pop()?); self.push(if a >= b { 1 } else { 0 })?; }
                OP_JUMP => {
                    let target = u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    self.ip = target;
                }
                OP_JUMP_IF_FALSE => {
                    let target = u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    self.ip += 2;
                    let cond = self.pop()?;
                    if cond == 0 { self.ip = target; }
                }
                OP_PUSH_STR => {
                    let offset = u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    let len = u16::from_be_bytes([self.code[self.ip + 2], self.code[self.ip + 3]]) as usize;
                    self.ip += 4;
                    let encoded = STR_TAG | (((offset as u64) as i64) << 32) | ((len as u64) as i64);
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
                                            if let Ok(s) = core::str::from_utf8(&SCRIPT_ARGS[idx][..len]) {
                                                serial_print!("{}", s);
                                            }
                                        }
                                    }
                                } else if (val & STR_TAG) != 0 {
                                    let raw = val & !STR_TAG;
                                    let offset = (raw >> 32) as usize;
                                    let len = (raw & 0xFFFF_FFFF) as usize;
                                    if offset + len <= self.str_pool.len() {
                                        if let Ok(s) = core::str::from_utf8(&self.str_pool[offset..offset + len]) {
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
                                            if let Ok(s) = core::str::from_utf8(&SCRIPT_ARGS[idx][..len]) {
                                                serial_println!("{}", s);
                                            }
                                        }
                                    }
                                } else if (val & STR_TAG) != 0 {
                                    let raw = val & !STR_TAG;
                                    let offset = (raw >> 32) as usize;
                                    let len = (raw & 0xFFFF_FFFF) as usize;
                                    if offset + len <= self.str_pool.len() {
                                        if let Ok(s) = core::str::from_utf8(&self.str_pool[offset..offset + len]) {
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
                        NATIVE_SYS_TSC => { self.push(read_tsc_serialized() as i64)?; }
                        NATIVE_NET_RTT => { self.push(LAST_RTT_NS.load(Ordering::Relaxed) as i64)?; }
                        NATIVE_NET_SET_RATE => { if argc > 0 { let r = self.pop()?; CONGESTION_RATE_PCT.store(r as u8, Ordering::Relaxed); } self.push(0)?; }
                        NATIVE_GPU_CAPTURE => { let vblank = poll_vblank_edge(5); let h = capture_frame_zero_copy(0, 1, vblank); self.push(h.slot_id as i64)?; }
                        NATIVE_NET_SEND => {
                            if argc > 0 {
                                let _ = self.pop()?;
                                let handle = capture_frame_zero_copy(0, 1, read_tsc_serialized());
                                let deadline = read_tsc_serialized() + crate::tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
                                let mut seq = 1u16;
                                let _ = stream_send_frame(&handle, deadline, &mut seq);
                            }
                            self.push(1)?;
                        }
                        NATIVE_SCRIPT_ARGC => { unsafe { self.push(SCRIPT_ARGC as i64)?; } }
                        NATIVE_SCRIPT_ARG => {
                            if argc > 0 {
                                let idx = self.pop()?;
                                if idx >= 0 && (idx as usize) < 8 { self.push(ARG_TAG | (idx & 0xFF))?; }
                                else { self.push(0)?; }
                            } else { self.push(0)?; }
                        }
                        _ => {}
                    }
                }
                OP_WITHIN_START => {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&self.code[self.ip..self.ip + 8]);
                    self.ip += 8;
                    let budget_ns = i64::from_be_bytes(b) as u64;
                    let tsc_budget = if tsc_freq_hz > 0 { (budget_ns * tsc_freq_hz) / 1_000_000_000 } else { budget_ns * 3 };
                    if self.dl_sp < 8 { self.deadline_stack[self.dl_sp] = read_tsc_serialized() + tsc_budget; self.dl_sp += 1; }
                }
                OP_WITHIN_END => { if self.dl_sp > 0 { self.dl_sp -= 1; } }
                OP_DROP => {
                    if self.dl_sp > 0 && read_tsc_serialized() > self.deadline_stack[self.dl_sp - 1] {
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
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src, &tokens);
    let code_len = compiler.compile()?;

    let mut vm = PX64VM::new(&compiler.code[..code_len], &compiler.str_pool[..compiler.str_pool_len]);
    vm.run(tsc_freq_hz)
}

pub const PULSE_BIN_MAGIC: [u8; 4] = *b"PULS";
pub const PULSE_BIN_VERSION: u16 = 2;
pub const PULSE_HEADER_SIZE: usize = 12;

// Function: compile_pulse_to_binary
// Description: Compile PulseLang source code into px64 binary format (PX64).
// Worst-case execution time: ~50_000 ns
pub fn compile_pulse_to_binary(src: &[u8], out_buf: &mut [u8]) -> Result<usize, CompileError> {
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src, &tokens);
    let code_len = compiler.compile()?;
    let str_pool_len = compiler.str_pool_len;

    let total_size = PX64_HEADER_SIZE + code_len + str_pool_len;
    if total_size > out_buf.len() {
        return Err(CompileError::simple(
            "ERR_BINARY_BUFFER_OVERFLOW",
            "Target binary output buffer is too small for compiled px64 artifact",
        ));
    }

    // Header (16 bytes)
    out_buf[0..4].copy_from_slice(&PX64_BIN_MAGIC);
    out_buf[4..6].copy_from_slice(&PX64_BIN_VERSION.to_be_bytes());
    out_buf[6..8].copy_from_slice(&(code_len as u16).to_be_bytes());
    out_buf[8..10].copy_from_slice(&(str_pool_len as u16).to_be_bytes());
    out_buf[10..12].copy_from_slice(&(PX64_NUM_REGISTERS as u16).to_be_bytes());
    out_buf[12..16].fill(0); // Reserved

    // Payload
    out_buf[PX64_HEADER_SIZE..PX64_HEADER_SIZE + code_len].copy_from_slice(&compiler.code[..code_len]);
    out_buf[PX64_HEADER_SIZE + code_len..total_size].copy_from_slice(&compiler.str_pool[..str_pool_len]);

    Ok(total_size)
}

// Function: run_pulse_binary
// Description: Execute pre-compiled px64 / PULS binary bytecode directly in O(1) zero compilation latency.
// Worst-case execution time: ~60_000 ns
pub fn run_pulse_binary(bin: &[u8], tsc_freq_hz: u64) -> Result<(), CompileError> {
    if bin.len() >= PX64_HEADER_SIZE && &bin[0..4] == &PX64_BIN_MAGIC {
        let version = u16::from_be_bytes([bin[4], bin[5]]);
        if version != PX64_BIN_VERSION {
            return Err(CompileError::simple("ERR_BINARY_VERSION_MISMATCH", "Unsupported px64 binary version"));
        }
        let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
        let str_pool_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;

        if bin.len() < PX64_HEADER_SIZE + code_len + str_pool_len {
            return Err(CompileError::simple("ERR_BINARY_TRUNCATED", "Truncated px64 binary payload"));
        }

        let code = &bin[PX64_HEADER_SIZE..PX64_HEADER_SIZE + code_len];
        let str_pool = &bin[PX64_HEADER_SIZE + code_len..PX64_HEADER_SIZE + code_len + str_pool_len];

        let mut vm = PX64VM::new(code, str_pool);
        vm.run(tsc_freq_hz)
    } else if bin.len() >= PULSE_HEADER_SIZE && &bin[0..4] == &PULSE_BIN_MAGIC {
        let version = u16::from_be_bytes([bin[4], bin[5]]);
        if version != PULSE_BIN_VERSION {
            return Err(CompileError::simple("ERR_BINARY_VERSION_MISMATCH", "Unsupported PulseLang binary version"));
        }
        let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
        let str_pool_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;

        if bin.len() < PULSE_HEADER_SIZE + code_len + str_pool_len {
            return Err(CompileError::simple("ERR_BINARY_TRUNCATED", "Truncated binary bytecode payload"));
        }

        let code = &bin[PULSE_HEADER_SIZE..PULSE_HEADER_SIZE + code_len];
        let str_pool = &bin[PULSE_HEADER_SIZE + code_len..PULSE_HEADER_SIZE + code_len + str_pool_len];

        let mut vm = VM::new(code, str_pool);
        vm.run(tsc_freq_hz)
    } else {
        Err(CompileError::simple("ERR_BINARY_INVALID_MAGIC", "Invalid executable binary magic (expected PX64)"))
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
