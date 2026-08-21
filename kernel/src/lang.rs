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

pub const MAX_TOKENS: usize = 256;
pub const MAX_BYTECODE_SIZE: usize = 1024;
pub const MAX_VARS: usize = 32;
pub const MAX_STRING_POOL: usize = 512;
pub const MAX_VM_STEPS: usize = 10_000;
pub const MAX_SCRIPT_TIMEOUT_NS: u64 = 20_000_000; // 20.0 ms (20,000,000 ns) wall-clock hard watchdog limit

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords & Directives (@contract, @pipeline, @budget, @wcet, @within, @while, @loop, @on_vblank)
    Let,
    Mut,
    Match,
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
    For,
    In,
    Fn,
    Struct,
    Const,

    // AI-Native Directives & Contracts
    AtContract,
    AtPipeline,
    AtBudget,
    AtWcet,
    AtWithin,
    AtWhile,
    AtFor,
    AtLoop,
    AtOnVblank,
    AtAssert,
    AtRequires,
    AtEnsures,
    AtInvariant,

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
    PipeSingle, // |
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
    Shl,        // <<
    Gt,         // >
    GtEq,       // >=
    Shr,        // >>
    And,        // &&
    Amp,        // &
    Caret,      // ^
    OrOp,       // ||
    Semi,       // ;
    Colon,      // :
    Comma,      // ,
    Dot,        // .
    DotDot,     // ..
    Arrow,      // ->
    FatArrow,   // =>
    Underscore, // _
    LParen,     // (
    RParen,     // )
    LBrace,     // {
    RBrace,     // }
    LBracket,   // [
    RBracket,   // ]
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
            serial_println!("[AI_REPAIR_HINT]: Recompile source file with 'compile <src.pl> <dst.bin>'");
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
                b'.' => {
                    if self.peek_next() == Some(b'.') {
                        self.pos += 2;
                        TokenKind::DotDot
                    } else {
                        self.pos += 1;
                        TokenKind::Dot
                    }
                }
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
                    } else if self.peek_next() == Some(b'>') {
                        self.pos += 2;
                        TokenKind::Arrow
                    } else {
                        self.pos += 1;
                        TokenKind::Minus
                    }
                }

                b'*' => { self.pos += 1; TokenKind::Star }
                b'/' => { self.pos += 1; TokenKind::Slash }
                b'%' => { self.pos += 1; TokenKind::Percent }

                b'[' => { self.pos += 1; TokenKind::LBracket }
                b']' => { self.pos += 1; TokenKind::RBracket }
                b'^' => { self.pos += 1; TokenKind::Caret }

                b'|' => {
                    if self.peek_next() == Some(b'>') {
                        self.pos += 2;
                        TokenKind::Pipe
                    } else if self.peek_next() == Some(b'|') {
                        self.pos += 2;
                        TokenKind::OrOp
                    } else {
                        self.pos += 1;
                        TokenKind::PipeSingle
                    }
                }

                b'=' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::EqEq
                    } else if self.peek_next() == Some(b'>') {
                        self.pos += 2;
                        TokenKind::FatArrow
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
                    if self.peek_next() == Some(b'<') {
                        self.pos += 2;
                        TokenKind::Shl
                    } else if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::LtEq
                    } else {
                        self.pos += 1;
                        TokenKind::Lt
                    }
                }

                b'>' => {
                    if self.peek_next() == Some(b'>') {
                        self.pos += 2;
                        TokenKind::Shr
                    } else if self.peek_next() == Some(b'=') {
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
                        self.pos += 1;
                        TokenKind::Amp
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
                    if b == b'0' && self.pos + 1 < self.src.len() && (self.src[self.pos + 1] == b'x' || self.src[self.pos + 1] == b'X') {
                        self.pos += 2;
                        let mut num = 0i64;
                        while self.pos < self.src.len() && self.src[self.pos].is_ascii_hexdigit() {
                            let digit = match self.src[self.pos] {
                                b'0'..=b'9' => (self.src[self.pos] - b'0') as i64,
                                b'a'..=b'f' => (self.src[self.pos] - b'a' + 10) as i64,
                                b'A'..=b'F' => (self.src[self.pos] - b'A' + 10) as i64,
                                _ => 0,
                            };
                            num = (num << 4) | digit;
                            self.pos += 1;
                        }
                        TokenKind::Number(num)
                    } else {
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
                        b"for" => TokenKind::AtFor,
                        b"assert" => TokenKind::AtAssert,
                        b"requires" => TokenKind::AtRequires,
                        b"ensures" => TokenKind::AtEnsures,
                        b"invariant" => TokenKind::AtInvariant,
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
                        b"mut" => TokenKind::Mut,
                        b"match" => TokenKind::Match,
                        b"if" => TokenKind::If,
                        b"else" => TokenKind::Else,
                        b"while" => TokenKind::While,
                        b"for" => TokenKind::For,
                        b"in" => TokenKind::In,
                        b"within" => TokenKind::Within,
                        b"or" => TokenKind::Or,
                        b"drop" => TokenKind::Drop,
                        b"pipeline" => TokenKind::Pipeline,
                        b"on" => TokenKind::On,
                        b"emit" => TokenKind::Emit,
                        b"return" => TokenKind::Return,
                        b"budget" => TokenKind::Budget,
                        b"fn" => TokenKind::Fn,
                        b"struct" => TokenKind::Struct,
                        b"const" => TokenKind::Const,
                        b"_" => TokenKind::Underscore,
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
pub const PX64_BIN_VERSION: u16 = 3;
pub const PX64_HEADER_SIZE: usize = 16;
pub const PX64_NUM_REGISTERS: usize = 20;
pub const MAX_CONST_POOL: usize = 64;

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
pub const NATIVE_TAG_OK: u8 = 10;
pub const NATIVE_TAG_ERR: u8 = 11;
pub const NATIVE_IS_OK: u8 = 12;
pub const NATIVE_IS_ERR: u8 = 13;
pub const NATIVE_UNWRAP: u8 = 14;
pub const NATIVE_STREQ: u8 = 15;

pub const ERR_TAG: i64 = 0x1000_0000_0000_0000;

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HandleState {
    Unallocated,
    Allocated { line: usize, col: usize },
    Consumed,
}

pub static mut COMPILER_CODE: [u8; MAX_BYTECODE_SIZE] = [0; MAX_BYTECODE_SIZE];
pub static mut COMPILER_STR_POOL: [u8; MAX_STRING_POOL] = [0; MAX_STRING_POOL];
pub static mut COMPILER_CONST_POOL: [i64; MAX_CONST_POOL] = [0; MAX_CONST_POOL];
pub static mut COMPILER_VAR_NAMES: [[u8; 16]; MAX_VARS] = [[0; 16]; MAX_VARS];

#[derive(Clone, Copy)]
pub struct FnMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub entry_pc: u16,
    pub param_count: u8,
    pub param_names: [[u8; 16]; 4],
    pub param_lens: [usize; 4],
}

impl FnMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            entry_pc: 0,
            param_count: 0,
            param_names: [[0; 16]; 4],
            param_lens: [0; 4],
        }
    }
}

#[derive(Clone, Copy)]
pub struct ArrayMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub arr_id: u8,
    pub base: u16,
    pub len: u16,
}

impl ArrayMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            arr_id: 0,
            base: 0,
            len: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct StructFieldMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub offset: u8,
}

impl StructFieldMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            offset: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct StructDefMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub field_count: u8,
    pub fields: [StructFieldMeta; 8],
}

impl StructDefMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            field_count: 0,
            fields: [StructFieldMeta::empty(); 8],
        }
    }
}

#[derive(Clone, Copy)]
pub struct StructInstMeta {
    pub var_name: [u8; 16],
    pub var_name_len: usize,
    pub struct_def_idx: u8,
    pub inst_id: u8,
    pub base_slot: u16,
    pub field_count: u8,
}

impl StructInstMeta {
    pub const fn empty() -> Self {
        Self {
            var_name: [0; 16],
            var_name_len: 0,
            struct_def_idx: 0,
            inst_id: 0,
            base_slot: 0,
            field_count: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ConstTableMeta {
    pub name: [u8; 16],
    pub name_len: usize,
    pub tbl_id: u8,
    pub base_idx: u8,
    pub len: u8,
}

impl ConstTableMeta {
    pub const fn empty() -> Self {
        Self {
            name: [0; 16],
            name_len: 0,
            tbl_id: 0,
            base_idx: 0,
            len: 0,
        }
    }
}

pub struct Compiler<'a> {
    src: &'a [u8],
    tokens: &'a [Token],
    current: usize,
    pub code_len: usize,
    var_lens: [usize; MAX_VARS],
    var_regs: [u8; MAX_VARS],
    var_mut: [bool; MAX_VARS],
    var_count: usize,
    pub str_pool_len: usize,
    pub const_pool_len: usize,
    temp_depth: u8,
    handle_states: [HandleState; 4],
    pub arrays: [ArrayMeta; 8],
    pub array_count: usize,
    pub total_array_elements: usize,
    pub functions: [FnMeta; 16],
    pub fn_count: usize,
    pub current_fn: Option<usize>,
    pub struct_defs: [StructDefMeta; 8],
    pub struct_def_count: usize,
    pub struct_insts: [StructInstMeta; 8],
    pub struct_inst_count: usize,
    pub total_struct_fields: usize,
    pub const_tables: [ConstTableMeta; 8],
    pub const_table_count: usize,
}

impl<'a> Compiler<'a> {
    pub fn new(src: &'a [u8], tokens: &'a [Token]) -> Self {
        unsafe {
            COMPILER_CODE.fill(0);
            COMPILER_STR_POOL.fill(0);
            COMPILER_CONST_POOL.fill(0);
            for v in COMPILER_VAR_NAMES.iter_mut() {
                v.fill(0);
            }
        }
        Self {
            src,
            tokens,
            current: 0,
            code_len: 0,
            var_lens: [0; MAX_VARS],
            var_regs: [0; MAX_VARS],
            var_mut: [true; MAX_VARS],
            var_count: 0,
            str_pool_len: 0,
            const_pool_len: 0,
            temp_depth: 0,
            handle_states: [HandleState::Unallocated; 4],
            arrays: [ArrayMeta::empty(); 8],
            array_count: 0,
            total_array_elements: 0,
            functions: [FnMeta::empty(); 16],
            fn_count: 0,
            current_fn: None,
            struct_defs: [StructDefMeta::empty(); 8],
            struct_def_count: 0,
            struct_insts: [StructInstMeta::empty(); 8],
            struct_inst_count: 0,
            total_struct_fields: 0,
            const_tables: [ConstTableMeta::empty(); 8],
            const_table_count: 0,
        }
    }

    pub fn declare_array(&mut self, tok: Token, len: usize) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        if self.array_count >= 8 {
            return Err(self.error(
                "ERR_MAX_ARRAYS_EXCEEDED",
                "Maximum distinct arrays limit reached (8 arrays limit)",
                "Fewer distinct arrays",
                "Array Allocation",
                "Reduce distinct array declarations across script",
            ));
        }
        if self.total_array_elements + len > 256 {
            return Err(self.error(
                "ERR_ARRAY_CAPACITY_EXCEEDED",
                "Total static array capacity exceeded (max 256 elements)",
                "Smaller array size",
                "Array Allocation",
                "Reduce total array elements in script",
            ));
        }
        let arr_id = self.array_count as u8;
        let mut meta = ArrayMeta::empty();
        meta.name_len = core::cmp::min(name.len(), 16);
        meta.name[..meta.name_len].copy_from_slice(&name[..meta.name_len]);
        meta.arr_id = arr_id;
        meta.base = self.total_array_elements as u16;
        meta.len = len as u16;
        self.arrays[self.array_count] = meta;
        self.array_count += 1;
        self.total_array_elements += len;
        Ok(arr_id)
    }

    pub fn lookup_array(&self, tok: Token) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        for i in 0..self.array_count {
            let meta = &self.arrays[i];
            if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                return Ok(meta.arr_id);
            }
        }
        Err(self.error(
            "ERR_ARRAY_UNDEFINED",
            "Array variable is not defined",
            "Declare array with 'let $buf: [i64; N];'",
            "Array Access",
            "Define array buffer before indexing elements",
        ))
    }

    pub fn declare_struct_def(&mut self) -> Result<(), CompileError> {
        let name_tok = self.advance();
        if name_tok.kind != TokenKind::Ident {
            return Err(self.error(
                "ERR_EXPECTED_STRUCT_NAME",
                "Expected struct identifier name after 'struct' keyword",
                "Struct name identifier",
                "Statement -> Struct Definition",
                "Specify struct name, e.g. 'struct Point'",
            ));
        }
        let s_name = &self.src[name_tok.start..name_tok.start + name_tok.len];
        if self.struct_def_count >= 8 {
            return Err(self.error(
                "ERR_MAX_STRUCTS_EXCEEDED",
                "Maximum distinct struct definitions reached (8 struct types limit)",
                "Fewer struct definitions",
                "Struct Definition",
                "Reduce distinct struct declarations across script",
            ));
        }

        for i in 0..self.struct_def_count {
            let def = &self.struct_defs[i];
            if def.name_len == s_name.len() && &def.name[..def.name_len] == s_name {
                return Err(self.error(
                    "ERR_STRUCT_REDEFINED",
                    "Struct type is already defined",
                    "Unique struct type name",
                    "Struct Definition",
                    "Rename duplicate struct type",
                ));
            }
        }

        if !self.match_token(TokenKind::LBrace) {
            return Err(self.error(
                "ERR_EXPECTED_LBRACE",
                "Expected opening brace '{' after struct name",
                "Left brace '{'",
                "Statement -> Struct Definition",
                "Add '{' to start struct field list",
            ));
        }

        let mut def = StructDefMeta::empty();
        def.name_len = core::cmp::min(s_name.len(), 16);
        def.name[..def.name_len].copy_from_slice(&s_name[..def.name_len]);

        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            let f_tok = self.advance();
            if f_tok.kind != TokenKind::Ident {
                return Err(self.error(
                    "ERR_EXPECTED_FIELD_NAME",
                    "Expected field identifier in struct definition",
                    "Field name identifier",
                    "Statement -> Struct Fields",
                    "Specify field name, e.g. 'x: i64'",
                ));
            }
            let f_name = &self.src[f_tok.start..f_tok.start + f_tok.len];
            let f_idx = def.field_count as usize;
            if f_idx >= 8 {
                return Err(self.error(
                    "ERR_MAX_FIELDS_EXCEEDED",
                    "Maximum fields per struct exceeded (8 fields limit)",
                    "<= 8 fields",
                    "Statement -> Struct Fields",
                    "Reduce field count to 8 or fewer per struct",
                ));
            }

            if self.match_token(TokenKind::Colon) {
                self.advance(); // consume type (e.g. i64)
            }

            def.fields[f_idx].name_len = core::cmp::min(f_name.len(), 16);
            def.fields[f_idx].name[..def.fields[f_idx].name_len].copy_from_slice(&f_name[..def.fields[f_idx].name_len]);
            def.fields[f_idx].offset = f_idx as u8;
            def.field_count += 1;

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }

        if !self.match_token(TokenKind::RBrace) {
            return Err(self.error(
                "ERR_EXPECTED_RBRACE",
                "Expected closing brace '}' after struct field list",
                "Right brace '}'",
                "Statement -> Struct Definition",
                "Add '}' to close struct definition",
            ));
        }
        self.match_token(TokenKind::Semi);

        self.struct_defs[self.struct_def_count] = def;
        self.struct_def_count += 1;
        Ok(())
    }

    pub fn is_struct_type(&self, type_tok: Token) -> bool {
        let type_name = &self.src[type_tok.start..type_tok.start + type_tok.len];
        for i in 0..self.struct_def_count {
            let def = &self.struct_defs[i];
            if def.name_len == type_name.len() && &def.name[..def.name_len] == type_name {
                return true;
            }
        }
        false
    }

    pub fn declare_struct_inst(&mut self, var_tok: Token, type_tok: Token) -> Result<u8, CompileError> {
        let var_name = &self.src[var_tok.start..var_tok.start + var_tok.len];
        let type_name = &self.src[type_tok.start..type_tok.start + type_tok.len];

        let mut def_match = None;
        for i in 0..self.struct_def_count {
            let def = &self.struct_defs[i];
            if def.name_len == type_name.len() && &def.name[..def.name_len] == type_name {
                def_match = Some((i as u8, *def));
                break;
            }
        }

        let (def_idx, def) = match def_match {
            Some(d) => d,
            None => {
                return Err(self.error(
                    "ERR_UNKNOWN_STRUCT_TYPE",
                    "Struct type name is not defined",
                    "Declared struct type name",
                    "Statement -> Struct Instantiation",
                    "Declare struct before instantiating, e.g. 'struct Point { ... }'",
                ));
            }
        };

        if self.struct_inst_count >= 8 {
            return Err(self.error(
                "ERR_MAX_STRUCT_INSTS_EXCEEDED",
                "Maximum struct instances limit reached (8 struct instances limit)",
                "Fewer struct instances",
                "Struct Instantiation",
                "Reduce distinct struct instances across script",
            ));
        }

        let field_count = def.field_count as usize;
        if self.total_struct_fields + field_count > 256 {
            return Err(self.error(
                "ERR_STRUCT_CAPACITY_EXCEEDED",
                "Total static struct field capacity exceeded (max 256 fields)",
                "Fewer struct fields",
                "Struct Allocation",
                "Reduce number of fields across struct instances",
            ));
        }

        let inst_id = self.struct_inst_count as u8;
        let base_slot = self.total_struct_fields as u16;
        self.total_struct_fields += field_count;

        let mut inst = StructInstMeta::empty();
        inst.var_name_len = core::cmp::min(var_name.len(), 16);
        inst.var_name[..inst.var_name_len].copy_from_slice(&var_name[..inst.var_name_len]);
        inst.struct_def_idx = def_idx;
        inst.inst_id = inst_id;
        inst.base_slot = base_slot;
        inst.field_count = def.field_count;

        self.struct_insts[self.struct_inst_count] = inst;
        self.struct_inst_count += 1;

        self.emit_inst(PX64_OP_STRUCT_DEF, inst_id, def.field_count, 0)?;

        Ok(inst_id)
    }

    pub fn lookup_struct_field(&self, var_name: &[u8], field_name: &[u8]) -> Result<(u8, u8), CompileError> {
        let mut inst_match = None;
        for i in 0..self.struct_inst_count {
            let inst = &self.struct_insts[i];
            if inst.var_name_len == var_name.len() && &inst.var_name[..inst.var_name_len] == var_name {
                inst_match = Some(*inst);
                break;
            }
        }

        let inst = match inst_match {
            Some(i) => i,
            None => {
                return Err(self.error(
                    "ERR_UNKNOWN_STRUCT_VAR",
                    "Variable is not a declared struct instance",
                    "Struct variable",
                    "Expression -> Field Access",
                    "Declare struct instance with 'let $var: StructName;'",
                ));
            }
        };

        let def = &self.struct_defs[inst.struct_def_idx as usize];
        for f in 0..def.field_count as usize {
            let field = &def.fields[f];
            if field.name_len == field_name.len() && &field.name[..field.name_len] == field_name {
                return Ok((inst.inst_id, field.offset));
            }
        }

        Err(self.error(
            "ERR_UNKNOWN_STRUCT_FIELD",
            "Field does not exist on struct type",
            "Valid struct field name",
            "Expression -> Field Access",
            "Check field name in struct definition",
        ))
    }

    #[allow(dead_code)]
    pub fn is_struct_inst(&self, var_name: &[u8]) -> bool {
        for i in 0..self.struct_inst_count {
            let inst = &self.struct_insts[i];
            if inst.var_name_len == var_name.len() && &inst.var_name[..inst.var_name_len] == var_name {
                return true;
            }
        }
        false
    }

    #[allow(dead_code)]
    pub fn is_array(&self, tok: Token) -> bool {
        let name = &self.src[tok.start..tok.start + tok.len];
        for i in 0..self.array_count {
            let meta = &self.arrays[i];
            if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                return true;
            }
        }
        false
    }

    pub fn declare_const_table(&mut self) -> Result<(), CompileError> {
        let name_tok = self.advance();
        if name_tok.kind != TokenKind::Ident {
            return Err(self.error(
                "ERR_EXPECTED_CONST_NAME",
                "Expected table identifier name after 'const' keyword",
                "Const table identifier name",
                "Statement -> Const Table Declaration",
                "Specify const table name, e.g. 'const LUT: [i64; 4] = [1, 2, 3, 4];'",
            ));
        }
        let t_name = &self.src[name_tok.start..name_tok.start + name_tok.len];
        if self.const_table_count >= 8 {
            return Err(self.error(
                "ERR_MAX_TABLES_EXCEEDED",
                "Maximum distinct const table limit reached (8 const tables limit)",
                "Fewer const tables",
                "Const Table Allocation",
                "Reduce distinct const tables across script",
            ));
        }

        for i in 0..self.const_table_count {
            let tbl = &self.const_tables[i];
            if tbl.name_len == t_name.len() && &tbl.name[..tbl.name_len] == t_name {
                return Err(self.error(
                    "ERR_TABLE_REDEFINED",
                    "Const table name is already defined",
                    "Unique const table name",
                    "Const Table Declaration",
                    "Rename duplicate const table",
                ));
            }
        }

        if !self.match_token(TokenKind::Colon) {
            return Err(self.error(
                "ERR_EXPECTED_COLON",
                "Expected ':' after const table name",
                "Colon ':'",
                "Statement -> Const Table Declaration",
                "Specify type annotation, e.g. 'const LUT: [i64; 4]'",
            ));
        }

        if !self.match_token(TokenKind::LBracket) {
            return Err(self.error(
                "ERR_EXPECTED_LBRACKET",
                "Expected '[' in const table type annotation",
                "Left bracket '['",
                "Statement -> Const Table Declaration",
                "Specify table type as '[i64; N]'",
            ));
        }

        self.advance(); // consume type (e.g. i64)

        if !self.match_token(TokenKind::Semi) {
            return Err(self.error(
                "ERR_TABLE_SYNTAX",
                "Expected ';' in table type [i64; N]",
                "Semicolon ';'",
                "Statement -> Const Table Declaration",
                "Specify table type as '[i64; N]'",
            ));
        }

        let len_tok = self.advance();
        let expected_len = match len_tok.kind {
            TokenKind::Number(n) if n > 0 && n <= 64 => n as usize,
            _ => return Err(self.error(
                "ERR_TABLE_INVALID_LEN",
                "Const table size must be a positive constant integer (1..64)",
                "Positive integer (1..64)",
                "Statement -> Const Table Declaration",
                "Specify valid table size, e.g. '[i64; 16]'",
            )),
        };

        if !self.match_token(TokenKind::RBracket) {
            return Err(self.error(
                "ERR_TABLE_SYNTAX",
                "Expected ']' after table length",
                "Right bracket ']'",
                "Statement -> Const Table Declaration",
                "Close table type with ']'",
            ));
        }

        if !self.match_token(TokenKind::Eq) {
            return Err(self.error(
                "ERR_EXPECTED_EQ",
                "Expected '=' before table initialization array",
                "Equals sign '='",
                "Statement -> Const Table Declaration",
                "Assign initial values, e.g. '= [10, 20, 30]'",
            ));
        }

        if !self.match_token(TokenKind::LBracket) {
            return Err(self.error(
                "ERR_EXPECTED_LBRACKET",
                "Expected '[' to start table elements list",
                "Left bracket '['",
                "Statement -> Const Table Declaration",
                "Start elements list with '['",
            ));
        }

        if self.const_pool_len + expected_len > 256 {
            return Err(self.error(
                "ERR_CONST_POOL_EXCEEDED",
                "Constant pool capacity exceeded (max 256 constants)",
                "Fewer constants",
                "Constant Pool Allocation",
                "Reduce total constants and table elements across script",
            ));
        }

        let tbl_id = self.const_table_count as u8;
        let base_idx = self.const_pool_len as u8;
        let mut actual_len = 0;

        while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
            let mut is_neg = false;
            if self.peek().kind == TokenKind::Minus {
                self.advance();
                is_neg = true;
            }

            let elem_tok = self.advance();
            let val: i64 = match elem_tok.kind {
                TokenKind::Number(n) => {
                    if is_neg { -n } else { n }
                }
                TokenKind::TimeLiteral(ns) => {
                    ns as i64
                }
                _ => {
                    return Err(self.error(
                        "ERR_TABLE_NON_CONSTANT_ELEMENT",
                        "Const table elements must be compile-time constant integer or time literals",
                        "Constant literal value",
                        "Statement -> Const Table Declaration",
                        "Use constant literals for all table elements",
                    ));
                }
            };

            let _ = self.append_table_constant(val)?;
            actual_len += 1;

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }

        if actual_len != expected_len {
            return Err(self.error(
                "ERR_TABLE_LENGTH_MISMATCH",
                "Number of table elements does not match declared type length",
                "Matching number of elements",
                "Statement -> Const Table Declaration",
                "Ensure array element count matches declared length [i64; N]",
            ));
        }

        if !self.match_token(TokenKind::RBracket) {
            return Err(self.error(
                "ERR_EXPECTED_RBRACKET",
                "Expected closing bracket ']' after table elements list",
                "Right bracket ']'",
                "Statement -> Const Table Declaration",
                "Close elements list with ']'",
            ));
        }
        self.match_token(TokenKind::Semi);

        let mut meta = ConstTableMeta::empty();
        meta.name_len = core::cmp::min(t_name.len(), 16);
        meta.name[..meta.name_len].copy_from_slice(&t_name[..meta.name_len]);
        meta.tbl_id = tbl_id;
        meta.base_idx = base_idx;
        meta.len = expected_len as u8;

        self.const_tables[self.const_table_count] = meta;
        self.const_table_count += 1;

        self.emit_inst(PX64_OP_TBL_DEF, tbl_id, base_idx, expected_len as u8)?;

        Ok(())
    }

    pub fn lookup_const_table(&self, name: &[u8]) -> Result<(u8, u8), CompileError> {
        for i in 0..self.const_table_count {
            let meta = &self.const_tables[i];
            if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                return Ok((meta.tbl_id, meta.len));
            }
        }
        Err(self.error(
            "ERR_UNKNOWN_CONST_TABLE",
            "Const table identifier is not defined",
            "Declared const table name",
            "Expression -> Const Table Access",
            "Define const table with 'const TABLE: [i64; N] = [...];'",
        ))
    }

    fn peek_ahead(&self, offset: usize) -> Token {
        if self.current + offset < self.tokens.len() {
            self.tokens[self.current + offset]
        } else {
            Token::empty()
        }
    }

    fn emit_const(&mut self, dst: u8, val: i64) -> Result<(), CompileError> {
        if val >= 0 && val <= 65535 {
            self.emit_imm16(PX64_OP_MOV_IMM, dst, val as u16)?;
        } else {
            let idx = self.add_constant(val)?;
            self.emit_imm16(PX64_OP_LDC, dst, idx)?;
        }
        Ok(())
    }

    pub fn alloc_temp(&mut self) -> Result<u8, CompileError> {
        if self.temp_depth >= 6 {
            return Err(self.error(
                "ERR_EXPR_TOO_COMPLEX",
                "Expression nesting too deep (exceeded register scratch pool)",
                "Simpler expression or intermediate variables",
                "Expression Evaluation",
                "Split complex expression into intermediate variables",
            ));
        }
        let reg = 15 - self.temp_depth;
        self.temp_depth += 1;
        Ok(reg)
    }

    pub fn free_temp(&mut self, reg: u8) {
        if self.temp_depth > 0 && reg == 15 - (self.temp_depth - 1) {
            self.temp_depth -= 1;
        }
    }

    pub fn add_constant(&mut self, val: i64) -> Result<u16, CompileError> {
        unsafe {
            for i in 0..self.const_pool_len {
                if COMPILER_CONST_POOL[i] == val {
                    return Ok(i as u16);
                }
            }
            if self.const_pool_len >= MAX_CONST_POOL {
                return Err(self.error(
                    "ERR_CONST_POOL_FULL",
                    "64-bit constant pool exhausted (64 entries limit reached)",
                    "Fewer unique 64-bit constants",
                    "Constant Pool Allocation",
                    "Reduce large constant literals",
                ));
            }
            let idx = self.const_pool_len;
            COMPILER_CONST_POOL[idx] = val;
            self.const_pool_len += 1;
            Ok(idx as u16)
        }
    }

    pub fn append_table_constant(&mut self, val: i64) -> Result<u16, CompileError> {
        unsafe {
            if self.const_pool_len >= MAX_CONST_POOL {
                return Err(self.error(
                    "ERR_CONST_POOL_FULL",
                    "64-bit constant pool exhausted (64 entries limit reached)",
                    "Fewer unique 64-bit constants",
                    "Constant Pool Allocation",
                    "Reduce large constant literals or table size",
                ));
            }
            let idx = self.const_pool_len;
            COMPILER_CONST_POOL[idx] = val;
            self.const_pool_len += 1;
            Ok(idx as u16)
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
        unsafe {
            COMPILER_CODE[pos] = op;
            COMPILER_CODE[pos + 1] = rd;
            COMPILER_CODE[pos + 2] = rs1;
            COMPILER_CODE[pos + 3] = rs2;
        }
        self.code_len += 4;
        Ok(pos)
    }

    fn emit_imm16(&mut self, op: u8, rd: u8, imm: u16) -> Result<usize, CompileError> {
        self.emit_inst(op, rd, (imm >> 8) as u8, (imm & 0xFF) as u8)
    }

    fn patch_imm16(&mut self, pos: usize, imm: u16) {
        unsafe {
            COMPILER_CODE[pos + 2] = (imm >> 8) as u8;
            COMPILER_CODE[pos + 3] = (imm & 0xFF) as u8;
        }
    }

    fn declare_var(&mut self, tok: Token, is_mut: bool) -> Result<u8, CompileError> {
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

        unsafe {
            for i in 0..self.var_count {
                if self.var_lens[i] == name.len() && &COMPILER_VAR_NAMES[i][..self.var_lens[i]] == name {
                    self.var_mut[i] = is_mut;
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

            let reg = (1 + self.var_count) as u8;
            let idx = self.var_count;
            let len = core::cmp::min(name.len(), 16);
            COMPILER_VAR_NAMES[idx][..len].copy_from_slice(&name[..len]);
            self.var_lens[idx] = len;
            self.var_regs[idx] = reg;
            self.var_mut[idx] = is_mut;
            self.var_count += 1;
            Ok(reg)
        }
    }

    fn check_var_mutation(&self, tok: Token) -> Result<(), CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        if name.starts_with(b"#") || name == b"$rax" || name == b"$rcx" || name == b"$rdx" || name == b"$rbx" || name == b"$rsp" || name == b"$rbp" || name == b"$rsi" || name == b"$rdi" || name.starts_with(b"$r") {
            return Ok(());
        }
        unsafe {
            for i in 0..self.var_count {
                if self.var_lens[i] == name.len() && &COMPILER_VAR_NAMES[i][..self.var_lens[i]] == name {
                    if !self.var_mut[i] {
                        return Err(self.error(
                            "ERR_MUTABILITY_VIOLATION",
                            "Cannot reassign/mutate immutable variable; variables are immutable by default in PulseLang",
                            "Mutable variable declaration ('let mut')",
                            "Statement -> Variable Assignment",
                            "Declare variable with 'let mut $var = ...;' to permit mutation",
                        ));
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
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

        // Check if variable is a parameter of current function
        if let Some(fn_idx) = self.current_fn {
            let meta = &self.functions[fn_idx];
            let param_regs: [u8; 4] = [7, 6, 2, 1]; // $rdi, $rsi, $rdx, $rcx
            for p in 0..meta.param_count as usize {
                if meta.param_lens[p] == name.len() && &meta.param_names[p][..meta.param_lens[p]] == name {
                    return Ok(param_regs[p]);
                }
            }
        }

        unsafe {
            for i in 0..self.var_count {
                if self.var_lens[i] == name.len() && &COMPILER_VAR_NAMES[i][..self.var_lens[i]] == name {
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
            COMPILER_VAR_NAMES[idx][..len].copy_from_slice(&name[..len]);
            self.var_lens[idx] = len;
            self.var_regs[idx] = reg;
            self.var_mut[idx] = true;
            self.var_count += 1;
            Ok(reg)
        }
    }

    // Function: compile
    // Description: Single-pass compilation from tokens into px64 fixed 32-bit instructions.
    // Worst-case execution time: ~40_000 ns
    pub fn compile(&mut self) -> Result<usize, CompileError> {
        while self.peek().kind != TokenKind::Eof {
            self.statement()?;
        }

        // BL-03: Verify all allocated hardware handles were sent/consumed!
        for i in 0..4 {
            if let HandleState::Allocated { line, col } = self.handle_states[i] {
                return Err(CompileError {
                    code: "ERR_LINEAR_UNCONSUMED_HANDLE",
                    message: "Hardware handle captured but never sent/consumed (linear ownership violation)",
                    line,
                    col,
                    byte_offset: 0,
                    token_kind: TokenKind::HardwareIdent,
                    token_len: 2,
                    expected: "Consume handle with @send(#handle)",
                    stage: "Linear Ownership Verification",
                    suggestion: "Add '@send(#handle);' along all execution paths to release DMA buffer",
                });
            }
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
                let val = deadline_ns as i64;
                if val >= 0 && val <= 65535 {
                    self.emit_imm16(PX64_OP_MOV_IMM, time_reg, val as u16)?;
                } else {
                    let idx = self.add_constant(val)?;
                    self.emit_imm16(PX64_OP_LDC, time_reg, idx)?;
                }
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

                // BL-04: Static Loop Boundary Verification
                let cond_tok = self.peek();
                if let TokenKind::Number(n) = cond_tok.kind {
                    if n != 0 {
                        return Err(self.error(
                            "ERR_UNBOUNDED_LOOP",
                            "Constant infinite loop (@while(1)) rejected: loops must have statically bounded monotonic conditions",
                            "Bounded condition e.g. @while($i < 100)",
                            "Static Loop Bound Verification",
                            "Replace infinite loop with bounded loop counter",
                        ));
                    }
                }

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

            TokenKind::For | TokenKind::AtFor => {
                self.advance();
                let has_paren = self.match_token(TokenKind::LParen);

                // 1. Loop variable: must be a VarIdent ($i, $idx, etc.)
                let var_tok = self.peek();
                if var_tok.kind != TokenKind::VarIdent {
                    return Err(self.error(
                        "ERR_FOR_EXPECTED_VAR",
                        "Expected variable identifier (e.g., $i) after 'for'",
                        "Variable identifier '$var'",
                        "Statement -> For Loop",
                        "Specify loop variable, e.g., 'for $i in 0..10'",
                    ));
                }
                let var_tok = self.advance();
                let var_reg = self.resolve_var(var_tok)?;

                // 2. 'in' keyword
                if !self.match_token(TokenKind::In) {
                    return Err(self.error(
                        "ERR_FOR_EXPECTED_IN",
                        "Expected 'in' keyword after loop variable",
                        "'in'",
                        "Statement -> For Loop",
                        "Add 'in' keyword after variable, e.g., 'for $i in 0..10'",
                    ));
                }

                // 3. Start bound: Must be compile-time constant integer or time literal
                let start_tok = self.peek();
                let start_val: i64 = match start_tok.kind {
                    TokenKind::Number(n) => {
                        self.advance();
                        n
                    }
                    TokenKind::TimeLiteral(ns) => {
                        self.advance();
                        ns as i64
                    }
                    _ => {
                        return Err(self.error(
                            "ERR_FOR_NON_CONSTANT_BOUND",
                            "Static range for loop requires compile-time constant start bound (0..N)",
                            "Integer literal e.g. '0'",
                            "Statement -> For Loop",
                            "Use constant literal for loop range start, or use 'while' for dynamic bounds",
                        ));
                    }
                };

                // 4. '..' range operator
                if !self.match_token(TokenKind::DotDot) {
                    return Err(self.error(
                        "ERR_FOR_EXPECTED_DOTDOT",
                        "Expected range operator '..' between loop bounds",
                        "'..'",
                        "Statement -> For Loop",
                        "Specify range with '..', e.g., '0..100'",
                    ));
                }

                // 5. End bound: Must be compile-time constant integer or time literal
                let end_tok = self.peek();
                let end_val: i64 = match end_tok.kind {
                    TokenKind::Number(n) => {
                        self.advance();
                        n
                    }
                    TokenKind::TimeLiteral(ns) => {
                        self.advance();
                        ns as i64
                    }
                    _ => {
                        return Err(self.error(
                            "ERR_FOR_NON_CONSTANT_BOUND",
                            "Static range for loop requires compile-time constant end bound (0..N)",
                            "Integer literal e.g. '100'",
                            "Statement -> For Loop",
                            "Use constant literal for loop range end, or use 'while' for dynamic bounds",
                        ));
                    }
                };

                if has_paren {
                    self.match_token(TokenKind::RParen);
                }

                // Calculate iterations
                let iterations: u64 = if end_val > start_val {
                    (end_val - start_val) as u64
                } else {
                    0
                };

                // Emit start initialization: $i = start_val
                if start_val >= 0 && start_val <= 65535 {
                    self.emit_imm16(PX64_OP_MOV_IMM, var_reg, start_val as u16)?;
                } else {
                    let idx = self.add_constant(start_val)?;
                    self.emit_imm16(PX64_OP_LDC, var_reg, idx)?;
                }

                // Emit end value into a temp register
                let end_reg = self.alloc_temp()?;
                if end_val >= 0 && end_val <= 65535 {
                    self.emit_imm16(PX64_OP_MOV_IMM, end_reg, end_val as u16)?;
                } else {
                    let idx = self.add_constant(end_val)?;
                    self.emit_imm16(PX64_OP_LDC, end_reg, idx)?;
                }

                let loop_start = self.code_len as u16;

                // Condition: $rax = ($i < end_reg)
                self.emit_inst(PX64_OP_CMP_LT, 0, var_reg, end_reg)?;
                let jz_pos = self.emit_inst(PX64_OP_JZ, 0, 0, 0)?;

                // Parse loop body
                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' for for loop body",
                        "Left brace '{'",
                        "Statement -> For Loop",
                        "Add opening brace '{' after for loop range",
                    ));
                }

                let body_code_start = self.code_len;
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);
                let body_code_end = self.code_len;

                // Static WCET validation:
                // Body instructions = (body_code_end - body_code_start) / 4
                // Loop overhead per iteration: CMPLT (1) + JZ (1) + ADDI (1) + JMP (1) = 4 instructions
                let body_inst_count = (body_code_end - body_code_start) / 4;
                let insts_per_iter = body_inst_count + 4;
                let total_estimated_steps = (insts_per_iter as u64).saturating_mul(iterations);

                if total_estimated_steps > MAX_VM_STEPS as u64 {
                    return Err(self.error(
                        "ERR_FOR_WCET_EXCEEDED",
                        "Static loop WCET exceeds MAX_VM_STEPS step limit (10,000 steps)",
                        "Loop with total steps <= 10,000",
                        "Static Loop WCET Verification",
                        "Reduce loop upper bound or simplify loop body to satisfy real-time step budget",
                    ));
                }

                // Increment: $i += 1
                self.emit_inst(PX64_OP_ADDI, var_reg, var_reg, 1)?;

                // Jump back to loop start
                self.emit_imm16(PX64_OP_JMP, 0, loop_start)?;

                // Patch exit jump
                self.patch_imm16(jz_pos, self.code_len as u16);

                // Free temp register
                self.free_temp(end_reg);
            }

            TokenKind::Let => {
                self.advance();
                let is_mut = self.match_token(TokenKind::Mut);
                let ident = self.advance();
                if self.match_token(TokenKind::Colon) {
                    // Array declaration: let $buf: [i64; N];
                    if self.match_token(TokenKind::LBracket) {
                        let _type_tok = self.advance(); // i64 or Ident
                        if !self.match_token(TokenKind::Semi) {
                            return Err(self.error(
                                "ERR_ARRAY_SYNTAX",
                                "Expected ';' in array type [i64; N]",
                                "Semicolon ';'",
                                "Statement -> Array Declaration",
                                "Specify array type as '[i64; N]'",
                            ));
                        }
                        let len_tok = self.advance();
                        let len = match len_tok.kind {
                            TokenKind::Number(n) if n > 0 => n as usize,
                            _ => return Err(self.error(
                                "ERR_ARRAY_INVALID_LEN",
                                "Array size must be a positive constant integer",
                                "Positive integer",
                                "Statement -> Array Declaration",
                                "Specify constant array size, e.g. '[i64; 16]'",
                            )),
                        };
                        if !self.match_token(TokenKind::RBracket) {
                            return Err(self.error(
                                "ERR_ARRAY_SYNTAX",
                                "Expected ']' after array length",
                                "Right bracket ']'",
                                "Statement -> Array Declaration",
                                "Close array type with ']'",
                            ));
                        }
                        self.match_token(TokenKind::Semi);

                        let arr_id = self.declare_array(ident, len)?;
                        self.emit_imm16(PX64_OP_ARR_DEF, arr_id, len as u16)?;
                        return Ok(());
                    } else {
                        // Struct instance declaration: let $pt: Point; or type annotation let $x: i64 = 10;
                        let type_tok = self.advance();
                        if self.is_struct_type(type_tok) {
                            self.declare_struct_inst(ident, type_tok)?;
                            self.match_token(TokenKind::Semi);
                            return Ok(());
                        }
                        // Primitive type annotation, e.g. let $x: i64 = 10;
                    }
                }
                let var_reg = self.declare_var(ident, is_mut)?;
                if self.match_token(TokenKind::ColonEq) || self.match_token(TokenKind::Eq) {
                    self.expression(var_reg)?;
                }
                self.match_token(TokenKind::Semi);
            }

            TokenKind::AtAssert => {
                self.advance();
                self.match_token(TokenKind::LParen);
                let cond_reg = 0;
                self.expression(cond_reg)?;
                self.match_token(TokenKind::RParen);
                self.match_token(TokenKind::Semi);
                self.emit_inst(PX64_OP_ASSERT, cond_reg, 0, 0)?;
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

            TokenKind::Match => {
                self.advance(); // consume 'match'
                let match_reg = self.alloc_temp()?;
                self.expression(match_reg)?;
                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Missing opening brace '{' after match expression",
                        "Left brace '{'",
                        "Statement -> Match Block",
                        "Add '{' to open match arms",
                    ));
                }

                let mut arm_jmp_ends = [0usize; 16];
                let mut arm_count = 0;
                let mut has_ok_arm = false;
                let mut has_err_arm = false;
                let mut has_wildcard = false;

                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    if arm_count >= 16 {
                        return Err(self.error(
                            "ERR_MAX_MATCH_ARMS_EXCEEDED",
                            "Maximum match arms exceeded (up to 16 arms supported)",
                            "<= 16 arms",
                            "Statement -> Match Arm",
                            "Reduce the number of match arms",
                        ));
                    }

                    let pat_tok = self.peek();
                    let name = &self.src[pat_tok.start..pat_tok.start + pat_tok.len];

                    if pat_tok.kind == TokenKind::Underscore || name == b"_" {
                        self.advance(); // consume '_'
                        has_wildcard = true;
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'pattern => { ... },'",
                            ));
                        }

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);
                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;
                        break;
                    } else if name == b"Ok" {
                        self.advance(); // consume 'Ok'
                        has_ok_arm = true;
                        if !self.match_token(TokenKind::LParen) {
                            return Err(self.error(
                                "ERR_EXPECTED_LPAREN",
                                "Expected '(' in Ok($var) pattern",
                                "Left parenthesis '('",
                                "Match Pattern -> Ok Variant",
                                "Specify as 'Ok($val) => { ... }'",
                            ));
                        }
                        let bind_tok = self.advance();
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Expected ')' after Ok pattern binding variable",
                                "Right parenthesis ')'",
                                "Match Pattern -> Ok Variant",
                                "Close Ok pattern with ')'",
                            ));
                        }
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'Ok($var) => { ... },'",
                            ));
                        }

                        let test_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CALL_NAT, test_reg, NATIVE_IS_ERR, match_reg)?;
                        let jnz_skip = self.emit_imm16(PX64_OP_JNZ, test_reg, 0)?;
                        self.free_temp(test_reg);

                        let bind_reg = self.resolve_var(bind_tok)?;
                        let unwrap_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CALL_NAT, unwrap_reg, NATIVE_UNWRAP, match_reg)?;
                        self.emit_inst(PX64_OP_MOV_REG, bind_reg, unwrap_reg, 0)?;
                        self.free_temp(unwrap_reg);

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);

                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;

                        self.patch_imm16(jnz_skip, self.code_len as u16);
                    } else if name == b"Err" {
                        self.advance(); // consume 'Err'
                        has_err_arm = true;
                        if !self.match_token(TokenKind::LParen) {
                            return Err(self.error(
                                "ERR_EXPECTED_LPAREN",
                                "Expected '(' in Err($var) pattern",
                                "Left parenthesis '('",
                                "Match Pattern -> Err Variant",
                                "Specify as 'Err($code) => { ... }'",
                            ));
                        }
                        let bind_tok = self.advance();
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Expected ')' after Err pattern binding variable",
                                "Right parenthesis ')'",
                                "Match Pattern -> Err Variant",
                                "Close Err pattern with ')'",
                            ));
                        }
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'Err($var) => { ... },'",
                            ));
                        }

                        let test_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CALL_NAT, test_reg, NATIVE_IS_OK, match_reg)?;
                        let jnz_skip = self.emit_imm16(PX64_OP_JNZ, test_reg, 0)?;
                        self.free_temp(test_reg);

                        let bind_reg = self.resolve_var(bind_tok)?;
                        let tag_reg = self.alloc_temp()?;
                        self.emit_const(tag_reg, ERR_TAG)?;
                        self.emit_inst(PX64_OP_XOR, bind_reg, match_reg, tag_reg)?;
                        self.free_temp(tag_reg);

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);

                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;

                        self.patch_imm16(jnz_skip, self.code_len as u16);
                    } else {
                        let pat_reg = self.alloc_temp()?;
                        self.expression(pat_reg)?;
                        if !self.match_token(TokenKind::FatArrow) {
                            return Err(self.error(
                                "ERR_EXPECTED_FAT_ARROW",
                                "Expected '=>' after match pattern",
                                "Fat arrow '=>'",
                                "Statement -> Match Arm",
                                "Specify arm as 'value => { ... },'",
                            ));
                        }

                        let cmp_reg = self.alloc_temp()?;
                        self.emit_inst(PX64_OP_CMP_EQ, cmp_reg, match_reg, pat_reg)?;
                        self.free_temp(pat_reg);
                        let jz_skip = self.emit_imm16(PX64_OP_JZ, cmp_reg, 0)?;
                        self.free_temp(cmp_reg);

                        if self.match_token(TokenKind::LBrace) {
                            while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                                self.statement()?;
                            }
                            self.match_token(TokenKind::RBrace);
                        } else {
                            self.statement()?;
                        }
                        self.match_token(TokenKind::Comma);

                        let jmp_end = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                        arm_jmp_ends[arm_count] = jmp_end;
                        arm_count += 1;

                        self.patch_imm16(jz_skip, self.code_len as u16);
                    }
                }

                if !self.match_token(TokenKind::RBrace) {
                    return Err(self.error(
                        "ERR_UNCLOSED_BRACE",
                        "Missing closing brace '}' in match statement",
                        "Right brace '}'",
                        "Statement -> Match Block",
                        "Add '}' to close match statement",
                    ));
                }

                if (has_ok_arm || has_err_arm) && (!has_ok_arm || !has_err_arm) && !has_wildcard {
                    return Err(self.error(
                        "ERR_NON_EXHAUSTIVE_MATCH",
                        "Result pattern match is not exhaustive: must handle both 'Ok($val)' and 'Err($code)', or provide wildcard '_' arm",
                        "Exhaustive Ok and Err arms or wildcard '_'",
                        "Statement -> Match Block",
                        "Add missing 'Ok(...)' / 'Err(...)' or wildcard '_ => { ... }' arm",
                    ));
                } else if !has_ok_arm && !has_err_arm && !has_wildcard {
                    return Err(self.error(
                        "ERR_NON_EXHAUSTIVE_MATCH",
                        "Value pattern match is not exhaustive: must provide a wildcard '_' fallback arm to ensure deterministic safety",
                        "Wildcard '_' arm",
                        "Statement -> Match Block",
                        "Add wildcard arm '_ => { ... }' at the end of match statement",
                    ));
                }

                for i in 0..arm_count {
                    self.patch_imm16(arm_jmp_ends[i], self.code_len as u16);
                }

                self.free_temp(match_reg);
            }

            TokenKind::Fn => {
                self.advance(); // consume 'fn'
                let name_tok = self.advance();
                if name_tok.kind != TokenKind::Ident {
                    return Err(self.error(
                        "ERR_EXPECTED_FN_NAME",
                        "Expected function identifier name after 'fn' keyword",
                        "Function name identifier",
                        "Statement -> Function Declaration",
                        "Specify function name e.g. 'fn my_func()'",
                    ));
                }
                let fn_name = &self.src[name_tok.start..name_tok.start + name_tok.len];
                if self.fn_count >= 16 {
                    return Err(self.error(
                        "ERR_MAX_FNS_EXCEEDED",
                        "Maximum static function limit reached (16 functions limit)",
                        "Fewer functions",
                        "Function Declaration",
                        "Reduce distinct function declarations in script",
                    ));
                }

                // Check redefinition
                for i in 0..self.fn_count {
                    let meta = &self.functions[i];
                    if meta.name_len == fn_name.len() && &meta.name[..meta.name_len] == fn_name {
                        return Err(self.error(
                            "ERR_FN_REDEFINED",
                            "Function is already defined",
                            "Unique function name",
                            "Function Declaration",
                            "Rename duplicate function",
                        ));
                    }
                }

                // Emit jump over function body so top-level control flow skips past it
                let jmp_skip = self.emit_imm16(PX64_OP_JMP, 0, 0)?;
                let entry_pc = self.code_len as u16;

                // Parameter list
                if !self.match_token(TokenKind::LParen) {
                    return Err(self.error(
                        "ERR_EXPECTED_LPAREN",
                        "Expected '(' after function name",
                        "Opening parenthesis '('",
                        "Statement -> Function Declaration",
                        "Add '(' after function name",
                    ));
                }

                let mut fn_meta = FnMeta::empty();
                fn_meta.name_len = core::cmp::min(fn_name.len(), 16);
                fn_meta.name[..fn_meta.name_len].copy_from_slice(&fn_name[..fn_meta.name_len]);
                fn_meta.entry_pc = entry_pc;

                while self.peek().kind != TokenKind::RParen && self.peek().kind != TokenKind::Eof {
                    let p_tok = self.advance();
                    if p_tok.kind != TokenKind::VarIdent {
                        return Err(self.error(
                            "ERR_EXPECTED_PARAM_VAR",
                            "Expected variable identifier '$param' in parameter list",
                            "Variable identifier '$param'",
                            "Statement -> Function Parameters",
                            "Use '$param' name in parameter list",
                        ));
                    }
                    let p_name = &self.src[p_tok.start..p_tok.start + p_tok.len];
                    let p_idx = fn_meta.param_count as usize;
                    if p_idx >= 4 {
                        return Err(self.error(
                            "ERR_MAX_PARAMS_EXCEEDED",
                            "Maximum parameter count exceeded (up to 4 parameters supported)",
                            "<= 4 parameters",
                            "Statement -> Function Parameters",
                            "Reduce parameter count to 4 or fewer",
                        ));
                    }
                    fn_meta.param_lens[p_idx] = core::cmp::min(p_name.len(), 16);
                    fn_meta.param_names[p_idx][..fn_meta.param_lens[p_idx]].copy_from_slice(&p_name[..fn_meta.param_lens[p_idx]]);
                    fn_meta.param_count += 1;
                    if !self.match_token(TokenKind::Comma) {
                        break;
                    }
                }

                if !self.match_token(TokenKind::RParen) {
                    return Err(self.error(
                        "ERR_EXPECTED_RPAREN",
                        "Expected ')' after parameter list",
                        "Closing parenthesis ')'",
                        "Statement -> Function Declaration",
                        "Add ')' to close parameter list",
                    ));
                }

                // Optional return type: '->' Ident
                if self.match_token(TokenKind::Arrow) {
                    self.advance(); // consume return type (e.g. i64)
                }

                let fn_idx = self.fn_count;
                self.functions[self.fn_count] = fn_meta;
                self.fn_count += 1;

                let prev_fn = self.current_fn;
                self.current_fn = Some(fn_idx);
                let saved_var_count = self.var_count;

                // Parse 0 or more @requires(condition) clauses
                while self.match_token(TokenKind::AtRequires) {
                    if !self.match_token(TokenKind::LParen) {
                        return Err(self.error(
                            "ERR_CONTRACT_SYNTAX",
                            "Expected '(' after @requires",
                            "Left parenthesis '('",
                            "Function Contract -> Precondition",
                            "Specify precondition as '@requires(condition)'",
                        ));
                    }
                    let cond_reg = self.alloc_temp()?;
                    self.expression(cond_reg)?;
                    if !self.match_token(TokenKind::RParen) {
                        return Err(self.error(
                            "ERR_UNCLOSED_PAREN",
                            "Missing closing parenthesis ')' in @requires condition",
                            "Closing parenthesis ')'",
                            "Function Contract -> Precondition",
                            "Add matching ')' after @requires condition",
                        ));
                    }
                    self.emit_inst(PX64_OP_ASSERT, cond_reg, 0, 0)?;
                    self.free_temp(cond_reg);
                }

                if !self.match_token(TokenKind::LBrace) {
                    return Err(self.error(
                        "ERR_EXPECTED_LBRACE",
                        "Expected opening brace '{' before function body",
                        "Left brace '{'",
                        "Statement -> Function Body",
                        "Add '{' to start function body",
                    ));
                }

                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);

                // Default return at end of function
                self.emit_inst(PX64_OP_RET, 0, 0, 0)?;

                self.var_count = saved_var_count;
                self.current_fn = prev_fn;

                // Patch jump over function body
                self.patch_imm16(jmp_skip, self.code_len as u16);
            }

            TokenKind::Return => {
                self.advance(); // consume 'return'
                if self.peek().kind != TokenKind::Semi {
                    let ret_reg = 0; // $rax
                    self.expression(ret_reg)?;
                }
                self.match_token(TokenKind::Semi);
                self.emit_inst(PX64_OP_RET, 0, 0, 0)?;
            }

            TokenKind::Struct => {
                self.advance(); // consume 'struct'
                self.declare_struct_def()?;
            }

            TokenKind::Const => {
                self.advance(); // consume 'const'
                self.declare_const_table()?;
            }

            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                if self.peek_ahead(1).kind == TokenKind::Dot {
                    let var_tok = self.advance();
                    self.match_token(TokenKind::Dot);
                    let field_tok = self.advance();
                    if field_tok.kind != TokenKind::Ident {
                        return Err(self.error(
                            "ERR_EXPECTED_FIELD_NAME",
                            "Expected field name identifier after '.'",
                            "Field identifier name",
                            "Statement -> Struct Field Assignment",
                            "Specify field name, e.g. '$pt.x := 10;'",
                        ));
                    }
                    let var_name = &self.src[var_tok.start..var_tok.start + var_tok.len];
                    let field_name = &self.src[field_tok.start..field_tok.start + field_tok.len];
                    let (inst_id, field_offset) = self.lookup_struct_field(var_name, field_name)?;

                    if !self.match_token(TokenKind::ColonEq) && !self.match_token(TokenKind::Eq) {
                        return Err(self.error(
                            "ERR_EXPECTED_COLON_EQ",
                            "Expected ':=' or '=' in struct field assignment",
                            "':=' or '='",
                            "Statement -> Struct Field Assignment",
                            "Assign field value with '$inst.field := val;'",
                        ));
                    }

                    let val_reg = self.alloc_temp()?;
                    self.expression(val_reg)?;
                    self.match_token(TokenKind::Semi);
                    self.emit_inst(PX64_OP_STRUCT_STORE, inst_id, field_offset, val_reg)?;
                    self.free_temp(val_reg);
                    return Ok(());
                }

                if self.peek_ahead(1).kind == TokenKind::LBracket {
                    let ident = self.advance();
                    let arr_id = self.lookup_array(ident)?;
                    self.match_token(TokenKind::LBracket);
                    let idx_reg = self.alloc_temp()?;
                    self.expression(idx_reg)?;
                    self.match_token(TokenKind::RBracket);
                    if !self.match_token(TokenKind::ColonEq) && !self.match_token(TokenKind::Eq) {
                        return Err(self.error(
                            "ERR_EXPECTED_COLON_EQ",
                            "Expected ':=' in array element assignment",
                            "':='",
                            "Statement -> Array Assignment",
                            "Assign value with '$arr[$idx] := val;'",
                        ));
                    }
                    let val_reg = self.alloc_temp()?;
                    self.expression(val_reg)?;
                    self.match_token(TokenKind::Semi);
                    self.emit_inst(PX64_OP_ARR_STORE, arr_id, idx_reg, val_reg)?;
                    self.free_temp(val_reg);
                    self.free_temp(idx_reg);
                    return Ok(());
                }

                let ident = self.advance();
                let var_reg = self.resolve_var(ident)?;

                if self.match_token(TokenKind::ColonEq) || self.match_token(TokenKind::Eq) {
                    self.check_var_mutation(ident)?;
                    // BL-03: Handle hardware handle allocation tracking
                    if var_reg >= 16 && var_reg <= 19 {
                        let slot = (var_reg - 16) as usize;
                        if let HandleState::Allocated { .. } = self.handle_states[slot] {
                            return Err(self.error(
                                "ERR_LINEAR_OVERWRITE",
                                "Hardware handle overwritten before previous buffer was consumed/sent",
                                "Consume prior handle with @send(#h) before reassigning",
                                "Linear Ownership Verification",
                                "Ensure '@send(#h)' is invoked before overwriting '#h := @capture()'",
                            ));
                        }
                        self.handle_states[slot] = HandleState::Allocated { line: ident.line, col: ident.col };
                    }
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
                    return Ok(());
                } else if self.match_token(TokenKind::PlusEq) {
                    self.check_var_mutation(ident)?;
                    if let TokenKind::Number(n) = self.peek().kind {
                        if n >= 0 && n <= 255 {
                            self.advance();
                            self.emit_inst(PX64_OP_ADDI, var_reg, var_reg, n as u8)?;
                            if !self.match_token(TokenKind::Semi) {
                                return Err(self.error(
                                    "ERR_MISSING_SEMICOLON",
                                    "Missing semicolon ';' at end of compound assignment",
                                    "Semicolon ';'",
                                    "Statement -> Compound Addition Assignment",
                                    "Append ';' at end of statement",
                                ));
                            }
                            return Ok(());
                        }
                    }
                    let tmp_reg = self.alloc_temp()?;
                    self.expression(tmp_reg)?;
                    self.emit_inst(PX64_OP_ADD, var_reg, var_reg, tmp_reg)?;
                    self.free_temp(tmp_reg);
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Addition Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                    return Ok(());
                } else if self.match_token(TokenKind::MinusEq) {
                    self.check_var_mutation(ident)?;
                    if let TokenKind::Number(n) = self.peek().kind {
                        if n >= 0 && n <= 255 {
                            self.advance();
                            self.emit_inst(PX64_OP_SUBI, var_reg, var_reg, n as u8)?;
                            if !self.match_token(TokenKind::Semi) {
                                return Err(self.error(
                                    "ERR_MISSING_SEMICOLON",
                                    "Missing semicolon ';' at end of compound assignment",
                                    "Semicolon ';'",
                                    "Statement -> Compound Subtraction Assignment",
                                    "Append ';' at end of statement",
                                ));
                            }
                            return Ok(());
                        }
                    }
                    let tmp_reg = self.alloc_temp()?;
                    self.expression(tmp_reg)?;
                    self.emit_inst(PX64_OP_SUB, var_reg, var_reg, tmp_reg)?;
                    self.free_temp(tmp_reg);
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Subtraction Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                    return Ok(());
                } else {
                    self.current -= 1;
                    self.expression_statement()?;
                    return Ok(());
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
        let val = self.ternary(dst)?;
        if let Some(v) = val {
            self.emit_const(dst, v)?;
        }
        while self.match_token(TokenKind::Pipe) {
            if self.peek().kind == TokenKind::IntrinsicIdent || self.peek().kind == TokenKind::Ident {
                let tok = self.advance();
                let name = &self.src[tok.start..tok.start + tok.len];
                let func_id = match name {
                    b"@send" | b"net.send" => {
                        if dst >= 16 && dst <= 19 {
                            let slot = (dst - 16) as usize;
                            match self.handle_states[slot] {
                                HandleState::Unallocated => {
                                    return Err(self.error(
                                        "ERR_LINEAR_USE_BEFORE_ALLOC",
                                        "Hardware handle sent before being captured/allocated",
                                        "Capture handle with '#f := @capture()' before sending",
                                        "Linear Ownership Verification",
                                        "Initialize hardware handle with '@capture()' before '@send()'",
                                    ));
                                }
                                HandleState::Consumed => {
                                    return Err(self.error(
                                        "ERR_LINEAR_DOUBLE_SEND",
                                        "Hardware handle sent multiple times (double-send / double-free violation)",
                                        "Single @send per allocated handle",
                                        "Linear Ownership Verification",
                                        "Remove duplicate '@send(#f);' calls on the same handle",
                                    ));
                                }
                                HandleState::Allocated { .. } => {
                                    self.handle_states[slot] = HandleState::Consumed;
                                }
                            }
                        }
                        NATIVE_NET_SEND
                    }
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

    fn ternary(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let val = self.logic_or(dst)?;
        if self.match_token(TokenKind::Question) {
            if let Some(v) = val {
                self.emit_const(dst, v)?;
            }
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
            return Ok(None);
        }
        Ok(val)
    }

    fn logic_or(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.logic_and(dst)?;
        while self.peek().kind == TokenKind::OrOp {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.logic_and(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(if a != 0 || b != 0 { 1 } else { 0 });
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                let zero_reg = self.alloc_temp()?;
                self.emit_const(zero_reg, 0)?;
                self.emit_inst(PX64_OP_CMP_NE, dst, dst, zero_reg)?;
                let rhs_bool = self.alloc_temp()?;
                self.emit_inst(PX64_OP_CMP_NE, rhs_bool, rhs_reg, zero_reg)?;
                self.emit_inst(PX64_OP_OR, dst, dst, rhs_bool)?;
                self.free_temp(rhs_bool);
                self.free_temp(zero_reg);
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn logic_and(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.equality(dst)?;
        while self.peek().kind == TokenKind::And {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.equality(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(if a != 0 && b != 0 { 1 } else { 0 });
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                let zero_reg = self.alloc_temp()?;
                self.emit_const(zero_reg, 0)?;
                self.emit_inst(PX64_OP_CMP_NE, dst, dst, zero_reg)?;
                let rhs_bool = self.alloc_temp()?;
                self.emit_inst(PX64_OP_CMP_NE, rhs_bool, rhs_reg, zero_reg)?;
                self.emit_inst(PX64_OP_AND, dst, dst, rhs_bool)?;
                self.free_temp(rhs_bool);
                self.free_temp(zero_reg);
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn equality(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.comparison(dst)?;
        while self.peek().kind == TokenKind::EqEq || self.peek().kind == TokenKind::NotEq {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.comparison(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(if op == TokenKind::EqEq { if a == b { 1 } else { 0 } } else { if a != b { 1 } else { 0 } });
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                if op == TokenKind::EqEq {
                    self.emit_inst(PX64_OP_CMP_EQ, dst, dst, rhs_reg)?;
                } else {
                    self.emit_inst(PX64_OP_CMP_NE, dst, dst, rhs_reg)?;
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn comparison(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.bitwise_or(dst)?;
        while matches!(self.peek().kind, TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq) {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.bitwise_or(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Lt => if a < b { 1 } else { 0 },
                    TokenKind::LtEq => if a <= b { 1 } else { 0 },
                    TokenKind::Gt => if a > b { 1 } else { 0 },
                    TokenKind::GtEq => if a >= b { 1 } else { 0 },
                    _ => 0,
                });
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                match op {
                    TokenKind::Lt => { self.emit_inst(PX64_OP_CMP_LT, dst, dst, rhs_reg)?; }
                    TokenKind::LtEq => { self.emit_inst(PX64_OP_CMP_LE, dst, dst, rhs_reg)?; }
                    TokenKind::Gt => { self.emit_inst(PX64_OP_CMP_GT, dst, dst, rhs_reg)?; }
                    TokenKind::GtEq => { self.emit_inst(PX64_OP_CMP_GE, dst, dst, rhs_reg)?; }
                    _ => {}
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn bitwise_or(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.bitwise_xor(dst)?;
        while self.peek().kind == TokenKind::PipeSingle {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.bitwise_xor(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(a | b);
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                self.emit_inst(PX64_OP_OR, dst, dst, rhs_reg)?;
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn bitwise_xor(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.bitwise_and(dst)?;
        while self.peek().kind == TokenKind::Caret {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.bitwise_and(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(a ^ b);
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                self.emit_inst(PX64_OP_XOR, dst, dst, rhs_reg)?;
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn bitwise_and(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.shift(dst)?;
        while self.peek().kind == TokenKind::Amp {
            self.advance();
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.shift(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(a & b);
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                self.emit_inst(PX64_OP_AND, dst, dst, rhs_reg)?;
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn shift(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.term(dst)?;
        while matches!(self.peek().kind, TokenKind::Shl | TokenKind::Shr) {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.term(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Shl => a.wrapping_shl((b & 63) as u32),
                    TokenKind::Shr => (a as u64 >> (b & 63)) as i64,
                    _ => 0,
                });
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                match op {
                    TokenKind::Shl => { self.emit_inst(PX64_OP_SHL, dst, dst, rhs_reg)?; }
                    TokenKind::Shr => { self.emit_inst(PX64_OP_SHR, dst, dst, rhs_reg)?; }
                    _ => {}
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn term(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.factor(dst)?;
        while self.peek().kind == TokenKind::Plus || self.peek().kind == TokenKind::Minus {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.factor(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Plus => a.wrapping_add(b),
                    TokenKind::Minus => a.wrapping_sub(b),
                    _ => 0,
                });
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                if op == TokenKind::Plus {
                    self.emit_inst(PX64_OP_ADD, dst, dst, rhs_reg)?;
                } else {
                    self.emit_inst(PX64_OP_SUB, dst, dst, rhs_reg)?;
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn factor(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let mut val = self.primary(dst)?;
        while self.peek().kind == TokenKind::Star || self.peek().kind == TokenKind::Slash || self.peek().kind == TokenKind::Percent {
            let op = self.advance().kind;
            let rhs_reg = self.alloc_temp()?;
            let rhs_val = self.primary(rhs_reg)?;
            if let (Some(a), Some(b)) = (val, rhs_val) {
                val = Some(match op {
                    TokenKind::Star => a.wrapping_mul(b),
                    TokenKind::Slash => if b != 0 { a.wrapping_div(b) } else { 0 },
                    TokenKind::Percent => if b != 0 { a.wrapping_rem(b) } else { 0 },
                    _ => 0,
                });
            } else {
                if let Some(a) = val { self.emit_const(dst, a)?; val = None; }
                if let Some(b) = rhs_val { self.emit_const(rhs_reg, b)?; }
                match op {
                    TokenKind::Star => { self.emit_inst(PX64_OP_MUL, dst, dst, rhs_reg)?; }
                    TokenKind::Slash => { self.emit_inst(PX64_OP_DIV, dst, dst, rhs_reg)?; }
                    TokenKind::Percent => { self.emit_inst(PX64_OP_MOD, dst, dst, rhs_reg)?; }
                    _ => {}
                }
            }
            self.free_temp(rhs_reg);
        }
        Ok(val)
    }

    fn primary(&mut self, dst: u8) -> Result<Option<i64>, CompileError> {
        let tok = self.advance();

        match tok.kind {
            TokenKind::Number(n) => {
                Ok(Some(n))
            }

            TokenKind::TimeLiteral(ns) => {
                Ok(Some(ns as i64))
            }

            TokenKind::StringLit => {
                let s = if tok.len >= 2 {
                    &self.src[tok.start + 1..tok.start + tok.len - 1]
                } else {
                    &[]
                };
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
                unsafe {
                    COMPILER_STR_POOL[offset..offset + s.len()].copy_from_slice(s);
                }
                self.str_pool_len += s.len();

                self.emit_inst(PX64_OP_MOV_STR, dst, offset as u8, s.len() as u8)?;
                Ok(None)
            }

            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                if self.peek().kind == TokenKind::LBracket {
                    let arr_id = self.lookup_array(tok)?;
                    self.advance(); // consume '['
                    let idx_reg = self.alloc_temp()?;
                    self.expression(idx_reg)?;
                    self.match_token(TokenKind::RBracket);
                    self.emit_inst(PX64_OP_ARR_LOAD, dst, arr_id, idx_reg)?;
                    self.free_temp(idx_reg);
                    Ok(None)
                } else if self.peek().kind == TokenKind::Dot {
                    self.advance(); // consume '.'
                    let field_tok = self.advance();
                    if field_tok.kind != TokenKind::Ident {
                        return Err(self.error(
                            "ERR_EXPECTED_FIELD_NAME",
                            "Expected field name identifier after '.'",
                            "Field identifier name",
                            "Expression -> Struct Field Access",
                            "Specify field name, e.g. '$pt.x'",
                        ));
                    }
                    let var_name = &self.src[tok.start..tok.start + tok.len];
                    let field_name = &self.src[field_tok.start..field_tok.start + field_tok.len];
                    let (inst_id, field_offset) = self.lookup_struct_field(var_name, field_name)?;
                    self.emit_inst(PX64_OP_STRUCT_LOAD, dst, inst_id, field_offset)?;
                    Ok(None)
                } else {
                    let var_reg = self.resolve_var(tok)?;
                    if var_reg != dst {
                        self.emit_inst(PX64_OP_MOV_REG, dst, var_reg, 0)?;
                    }
                    Ok(None)
                }
            }

            TokenKind::IntrinsicIdent | TokenKind::Ident => {
                let name = &self.src[tok.start..tok.start + tok.len];
                if self.peek().kind == TokenKind::LBracket {
                    let (tbl_id, _len) = self.lookup_const_table(name)?;
                    self.advance(); // consume '['
                    let idx_reg = self.alloc_temp()?;
                    self.expression(idx_reg)?;
                    self.match_token(TokenKind::RBracket);
                    self.emit_inst(PX64_OP_TBL_LOAD, dst, tbl_id, idx_reg)?;
                    self.free_temp(idx_reg);
                    return Ok(None);
                }

                if self.peek().kind == TokenKind::LParen {
                    // Check if calling a user-defined static function
                    let mut fn_match = None;
                    for i in 0..self.fn_count {
                        let meta = &self.functions[i];
                        if meta.name_len == name.len() && &meta.name[..meta.name_len] == name {
                            fn_match = Some(*meta);
                            break;
                        }
                    }

                    if let Some(fn_meta) = fn_match {
                        self.advance(); // consume '('
                        let param_regs: [u8; 4] = [7, 6, 2, 1]; // $rdi, $rsi, $rdx, $rcx
                        let mut arg_idx = 0;
                        while self.peek().kind != TokenKind::RParen && self.peek().kind != TokenKind::Eof {
                            if arg_idx >= fn_meta.param_count as usize {
                                return Err(self.error(
                                    "ERR_TOO_MANY_ARGS",
                                    "Too many arguments passed to static function",
                                    "Matching number of function parameters",
                                    "Expression -> Function Call",
                                    "Check function parameter signature",
                                ));
                            }
                            let target_reg = param_regs[arg_idx];
                            self.expression(target_reg)?;
                            arg_idx += 1;
                            if !self.match_token(TokenKind::Comma) {
                                break;
                            }
                        }
                        if arg_idx < fn_meta.param_count as usize {
                            return Err(self.error(
                                "ERR_TOO_FEW_ARGS",
                                "Too few arguments passed to static function",
                                "Matching number of function parameters",
                                "Expression -> Function Call",
                                "Provide all required function arguments",
                            ));
                        }
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Missing closing parenthesis ')' in function argument list",
                                "Closing parenthesis ')'",
                                "Expression -> Function Call",
                                "Add matching ')' after argument list",
                            ));
                        }
                        self.emit_imm16(PX64_OP_CALL, 0, fn_meta.entry_pc)?;
                        if dst != 0 {
                            self.emit_inst(PX64_OP_MOV_REG, dst, 0, 0)?;
                        }
                        return Ok(None);
                    }

                    if name == b"@streq" || name == b"str.eq" {
                        self.advance(); // consume '('
                        let s1_reg = self.alloc_temp()?;
                        self.expression(s1_reg)?;
                        if !self.match_token(TokenKind::Comma) {
                            return Err(self.error(
                                "ERR_EXPECTED_COMMA",
                                "Expected ',' between arguments in @streq($s1, $s2)",
                                "Comma ','",
                                "Expression -> Intrinsic Function Call",
                                "Provide two string arguments, e.g. '@streq($s1, $s2)'",
                            ));
                        }
                        let s2_reg = self.alloc_temp()?;
                        self.expression(s2_reg)?;
                        if !self.match_token(TokenKind::RParen) {
                            return Err(self.error(
                                "ERR_UNCLOSED_PAREN",
                                "Missing closing parenthesis ')' in @streq($s1, $s2)",
                                "Closing parenthesis ')'",
                                "Expression -> Intrinsic Function Call",
                                "Add matching ')' after argument list",
                            ));
                        }
                        self.emit_inst(PX64_OP_STREQ, dst, s1_reg, s2_reg)?;
                        self.free_temp(s2_reg);
                        self.free_temp(s1_reg);
                        return Ok(None);
                    }

                    self.advance(); // consume '('
                    let mut arg_reg = 0u8;
                    let mut raw_h_reg = 0u8;
                    if self.peek().kind != TokenKind::RParen {
                        let arg_tok = self.peek();
                        if arg_tok.kind == TokenKind::HardwareIdent {
                            if let Ok(reg) = self.resolve_var(arg_tok) {
                                raw_h_reg = reg;
                            }
                        }
                        arg_reg = self.alloc_temp()?;
                        self.expression(arg_reg)?;
                        while self.match_token(TokenKind::Comma) {
                            let dummy = self.alloc_temp()?;
                            self.expression(dummy)?;
                            self.free_temp(dummy);
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
                        b"@send" | b"net.send" => {
                            let target_reg = if raw_h_reg >= 16 && raw_h_reg <= 19 { raw_h_reg } else { arg_reg };
                            if target_reg >= 16 && target_reg <= 19 {
                                let slot = (target_reg - 16) as usize;
                                match self.handle_states[slot] {
                                    HandleState::Unallocated => {
                                        return Err(self.error(
                                            "ERR_LINEAR_USE_BEFORE_ALLOC",
                                            "Hardware handle sent before being captured/allocated",
                                            "Capture handle with '#f := @capture()' before sending",
                                            "Linear Ownership Verification",
                                            "Initialize hardware handle with '@capture()' before '@send()'",
                                        ));
                                    }
                                    HandleState::Consumed => {
                                        return Err(self.error(
                                            "ERR_LINEAR_DOUBLE_SEND",
                                            "Hardware handle sent multiple times (double-send / double-free violation)",
                                            "Single @send per allocated handle",
                                            "Linear Ownership Verification",
                                            "Remove duplicate '@send(#f);' calls on the same handle",
                                        ));
                                    }
                                    HandleState::Allocated { .. } => {
                                        self.handle_states[slot] = HandleState::Consumed;
                                    }
                                }
                            }
                            NATIVE_NET_SEND
                        }
                        b"@argc" | b"sys.argc" => NATIVE_SCRIPT_ARGC,
                        b"@arg" | b"sys.arg" => NATIVE_SCRIPT_ARG,
                        b"@ok" => NATIVE_TAG_OK,
                        b"@err" => NATIVE_TAG_ERR,
                        b"@is_ok" => NATIVE_IS_OK,
                        b"@is_err" => NATIVE_IS_ERR,
                        b"@unwrap" => NATIVE_UNWRAP,
                        _ => {
                            return Err(self.error(
                                "ERR_UNKNOWN_INTRINSIC",
                                "Unknown intrinsic function name",
                                "One of @print, @println, @tsc, @rtt, @rate, @capture, @send, @argc, @arg, @ok, @err, @is_ok, @is_err, @unwrap",
                                "Expression -> Intrinsic Call",
                                "Verify that the intrinsic name matches supported DSL intrinsics",
                            ));
                        }
                    };

                    self.emit_inst(PX64_OP_CALL_NAT, dst, func_id, arg_reg)?;
                    if arg_reg != 0 {
                        self.free_temp(arg_reg);
                    }
                    Ok(None)
                } else {
                    let var_reg = self.resolve_var(tok)?;
                    if var_reg != dst {
                        self.emit_inst(PX64_OP_MOV_REG, dst, var_reg, 0)?;
                    }
                    Ok(None)
                }
            }

            TokenKind::Minus => {
                let zero_reg = self.alloc_temp()?;
                self.emit_const(zero_reg, 0)?;
                let rhs_reg = self.alloc_temp()?;
                let rhs_val = self.primary(rhs_reg)?;
                if let Some(b) = rhs_val {
                    self.free_temp(rhs_reg);
                    self.free_temp(zero_reg);
                    Ok(Some(-b))
                } else {
                    self.emit_inst(PX64_OP_SUB, dst, zero_reg, rhs_reg)?;
                    self.free_temp(rhs_reg);
                    self.free_temp(zero_reg);
                    Ok(None)
                }
            }

            TokenKind::Exclamation => {
                let rhs_reg = self.alloc_temp()?;
                let rhs_val = self.primary(rhs_reg)?;
                if let Some(b) = rhs_val {
                    self.free_temp(rhs_reg);
                    Ok(Some(if b == 0 { 1 } else { 0 }))
                } else {
                    let zero_reg = self.alloc_temp()?;
                    self.emit_const(zero_reg, 0)?;
                    self.emit_inst(PX64_OP_CMP_EQ, dst, rhs_reg, zero_reg)?;
                    self.free_temp(zero_reg);
                    self.free_temp(rhs_reg);
                    Ok(None)
                }
            }

            TokenKind::LParen => {
                let val = self.ternary(dst)?;
                if !self.match_token(TokenKind::RParen) {
                    return Err(self.error(
                        "ERR_UNCLOSED_PAREN",
                        "Expected closing parenthesis ')' after sub-expression",
                        "Closing parenthesis ')'",
                        "Expression -> Grouping",
                        "Add matching ')' to close the grouped expression",
                    ));
                }
                Ok(val)
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
    }
}

// // -----------------------------------------------------------------------------
// px64 Real-Time Register Virtual Machine
// -----------------------------------------------------------------------------

pub const STR_TAG: i64 = 0x4000_0000_0000_0000;
pub const ARG_TAG: i64 = 0x2000_0000_0000_0000;
pub const MAX_CALL_DEPTH: usize = 8;

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
    fn get_str_bytes<'b>(&self, val: i64) -> Option<&'b [u8]> where 'a: 'b {
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
                    let idx = if idx_reg < PX64_NUM_REGISTERS { self.regs[idx_reg] } else { -1 };
                    if arr_id >= 8 {
                        return Err(CompileError::simple("ERR_PX64_ARRAY_INVALID_ID", "Array ID is invalid"));
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
                    let idx = if idx_reg < PX64_NUM_REGISTERS { self.regs[idx_reg] } else { -1 };
                    let val = if val_reg < PX64_NUM_REGISTERS { self.regs[val_reg] } else { 0 };
                    if arr_id >= 8 {
                        return Err(CompileError::simple("ERR_PX64_ARRAY_INVALID_ID", "Array ID is invalid"));
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
                    let val = if val_reg < PX64_NUM_REGISTERS { self.regs[val_reg] } else { 0 };
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
                    let idx = if idx_reg < PX64_NUM_REGISTERS { self.regs[idx_reg] } else { -1 };
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
                    let cond = if rd < PX64_NUM_REGISTERS { self.regs[rd] } else { 0 };
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
                        let v1 = self.regs[rs1];
                        let v2 = self.regs[rs2];
                        let eq = if v1 == v2 {
                            1
                        } else if ((v1 & STR_TAG) != 0 || (v1 & ARG_TAG) != 0) && ((v2 & STR_TAG) != 0 || (v2 & ARG_TAG) != 0) {
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {
                                (Some(b1), Some(b2)) => if b1 == b2 { 1 } else { 0 },
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
                        } else if ((v1 & STR_TAG) != 0 || (v1 & ARG_TAG) != 0) && ((v2 & STR_TAG) != 0 || (v2 & ARG_TAG) != 0) {
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {
                                (Some(b1), Some(b2)) => if b1 == b2 { 0 } else { 1 },
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
                                (Some(b1), Some(b2)) => if b1 == b2 { 1 } else { 0 },
                                _ => 0,
                            }
                        };
                        self.regs[rd] = eq;
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

                        NATIVE_TAG_OK => {
                            arg_val & !ERR_TAG
                        }

                        NATIVE_TAG_ERR => {
                            ERR_TAG | (arg_val & !ERR_TAG)
                        }

                        NATIVE_IS_OK => {
                            if (arg_val & ERR_TAG) == 0 { 1 } else { 0 }
                        }

                        NATIVE_IS_ERR => {
                            if (arg_val & ERR_TAG) != 0 { 1 } else { 0 }
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
                            0
                        }

                        _ => 0,
                    };

                    if rd < PX64_NUM_REGISTERS {
                        self.regs[rd] = ret;
                    }
                }

                PX64_OP_WITHIN_START => {
                    let budget_ns = if rd < PX64_NUM_REGISTERS { self.regs[rd] as u64 } else { 500_000 };
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

// Function: benchmark_px64_instructions
// Description: Benchmark real cycle count and nanosecond execution times for px64 instructions.
// Worst-case execution time: ~200_000 ns
pub fn benchmark_px64_instructions(tsc_freq_hz: u64) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) {
    let const_pool = [123456789012345i64; 4];
    let mut code = [0u8; 4004];

    // 1. Measure 1,000 iterations of LDC
    for i in 0..1000 {
        code[i * 4] = PX64_OP_LDC;
        code[i * 4 + 1] = 0; // $rax
        code[i * 4 + 2] = 0;
        code[i * 4 + 3] = 0; // const[0]
    }
    code[4000] = PX64_OP_HALT;
    let mut vm = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm2 = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_and = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_xor = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_shl = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_arr = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_assert = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_nop = PX64VM::new(&code, &[], &const_pool);
    let t4 = read_tsc_serialized();
    let _ = vm_nop.run(tsc_freq_hz);
    let t5 = read_tsc_serialized();
    let decode_ns = crate::tsc::tsc_to_ns(t5 - t4, tsc_freq_hz) / 1000;

    // 9. Measure 500 iterations of CALL/RET pairs (1,000 instructions total)
    let mut call_code = [0u8; 2008];
    for i in 0..500 {
        call_code[i * 4] = PX64_OP_CALL;
        call_code[i * 4 + 1] = 0;
        call_code[i * 4 + 2] = (2000 >> 8) as u8;
        call_code[i * 4 + 3] = (2000 & 0xFF) as u8;
    }
    call_code[2000] = PX64_OP_RET;
    call_code[2004] = PX64_OP_HALT;
    let mut vm_call = PX64VM::new(&call_code, &[], &const_pool);
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
    let mut vm_struct = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_tbl = PX64VM::new(&code, &[], &const_pool);
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
    let mut vm_streq = PX64VM::new(&code, str_bytes_pool, &const_pool);
    vm_streq.regs[1] = STR_TAG | (((0u64) as i64) << 32) | 20;
    vm_streq.regs[2] = STR_TAG | (((0u64) as i64) << 32) | 20;
    let t_str0 = read_tsc_serialized();
    let _ = vm_streq.run(tsc_freq_hz);
    let t_str1 = read_tsc_serialized();
    let streq_ns = crate::tsc::tsc_to_ns(t_str1 - t_str0, tsc_freq_hz) / 1000;

    (ldc_ns, addi_ns, and_ns, xor_ns, shl_ns, arr_load_ns, struct_load_ns, tbl_load_ns, streq_ns, assert_ns, call_ret_ns, decode_ns)
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

static mut COMPILER_TOKENS: [Token; MAX_TOKENS] = [Token::empty(); MAX_TOKENS];

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
        unsafe { &COMPILER_CODE[..code_len] },
        unsafe { &COMPILER_STR_POOL[..compiler.str_pool_len] },
        unsafe { &COMPILER_CONST_POOL[..compiler.const_pool_len] },
    );
    vm.run(tsc_freq_hz)
}

pub const PULSE_BIN_MAGIC: [u8; 4] = *b"PX64";
pub const PULSE_BIN_VERSION: u16 = 3;
pub const PULSE_HEADER_SIZE: usize = 16;

// Function: compile_pulse_to_binary
// Description: Compile PulseLang source code into px64 binary format (PX64).
// Worst-case execution time: ~50_000 ns
pub fn compile_pulse_to_binary(src: &[u8], out_buf: &mut [u8]) -> Result<usize, CompileError> {
    let tokens = unsafe { &mut COMPILER_TOKENS };
    for tok in tokens.iter_mut() {
        *tok = Token::empty();
    }
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(tokens)?;

    let mut compiler = Compiler::new(src, tokens);
    let code_len = compiler.compile()?;
    let str_pool_len = compiler.str_pool_len;
    let const_pool_count = compiler.const_pool_len;
    let const_pool_bytes = const_pool_count * 8;

    let total_size = PX64_HEADER_SIZE + code_len + str_pool_len + const_pool_bytes;
    if total_size > out_buf.len() {
        return Err(CompileError::simple(
            "ERR_BINARY_BUFFER_OVERFLOW",
            "Target binary output buffer is too small for compiled px64 artifact",
        ));
    }

    // Header (16 bytes)
    out_buf[0..4].copy_from_slice(&PX64_BIN_MAGIC); // b"PX64"
    out_buf[4..6].copy_from_slice(&PX64_BIN_VERSION.to_be_bytes()); // 3
    out_buf[6..8].copy_from_slice(&(code_len as u16).to_be_bytes());
    out_buf[8..10].copy_from_slice(&(str_pool_len as u16).to_be_bytes());
    out_buf[10..12].copy_from_slice(&(const_pool_count as u16).to_be_bytes());
    out_buf[12..14].copy_from_slice(&(PX64_NUM_REGISTERS as u16).to_be_bytes()); // 20
    out_buf[14..16].fill(0); // Reserved

    // Payload: Code + String Pool + Constant Pool
    out_buf[PX64_HEADER_SIZE..PX64_HEADER_SIZE + code_len].copy_from_slice(unsafe { &COMPILER_CODE[..code_len] });
    out_buf[PX64_HEADER_SIZE + code_len..PX64_HEADER_SIZE + code_len + str_pool_len].copy_from_slice(unsafe { &COMPILER_STR_POOL[..str_pool_len] });

    let const_start = PX64_HEADER_SIZE + code_len + str_pool_len;
    for (i, &c) in unsafe { &COMPILER_CONST_POOL[..const_pool_count] }.iter().enumerate() {
        out_buf[const_start + i * 8..const_start + (i + 1) * 8].copy_from_slice(&c.to_be_bytes());
    }

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
        let const_count = u16::from_be_bytes([bin[10], bin[11]]) as usize;
        let const_bytes = const_count * 8;

        if bin.len() < PX64_HEADER_SIZE + code_len + str_pool_len + const_bytes {
            return Err(CompileError::simple("ERR_BINARY_TRUNCATED", "Truncated px64 binary payload"));
        }

        let code = &bin[PX64_HEADER_SIZE..PX64_HEADER_SIZE + code_len];
        let str_pool = &bin[PX64_HEADER_SIZE + code_len..PX64_HEADER_SIZE + code_len + str_pool_len];

        let mut const_pool = [0i64; 64];
        let const_slice_raw = &bin[PX64_HEADER_SIZE + code_len + str_pool_len..PX64_HEADER_SIZE + code_len + str_pool_len + const_bytes];
        for i in 0..const_count {
            let b = &const_slice_raw[i * 8..(i + 1) * 8];
            const_pool[i] = i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        }

        let mut vm = PX64VM::new(code, str_pool, &const_pool[..const_count]);
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
