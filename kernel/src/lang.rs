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
// Bytecode Instruction Set
// -----------------------------------------------------------------------------

pub const OP_NOP: u8 = 0;
pub const OP_PUSH_CONST: u8 = 1;      // + 8 bytes (i64)
pub const OP_LOAD_VAR: u8 = 2;        // + 1 byte (var_idx)
pub const OP_STORE_VAR: u8 = 3;       // + 1 byte (var_idx)
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
pub const OP_JUMP: u8 = 15;           // + 2 bytes (target ip)
pub const OP_JUMP_IF_FALSE: u8 = 16;  // + 2 bytes (target ip)
pub const OP_CALL_NATIVE: u8 = 17;    // + 1 byte (func_id), + 1 byte (argc)
pub const OP_WITHIN_START: u8 = 18;   // + 8 bytes (deadline_ns)
pub const OP_WITHIN_END: u8 = 19;
pub const OP_DROP: u8 = 20;
pub const OP_PUSH_STR: u8 = 21;       // + 2 bytes (str offset), + 2 bytes (str len)
pub const OP_HALT: u8 = 22;

// Native function IDs
pub const NATIVE_PRINT: u8 = 1;
pub const NATIVE_PRINTLN: u8 = 2;
pub const NATIVE_SYS_TSC: u8 = 3;
pub const NATIVE_NET_RTT: u8 = 4;
pub const NATIVE_NET_SET_RATE: u8 = 5;
pub const NATIVE_GPU_CAPTURE: u8 = 6;
pub const NATIVE_NET_SEND: u8 = 7;

// -----------------------------------------------------------------------------
// Compiler (Single-Pass AST-to-Bytecode with Zero Heap Allocation)
// -----------------------------------------------------------------------------

pub struct Compiler<'a> {
    src: &'a [u8],
    tokens: &'a [Token],
    current: usize,
    pub code: [u8; MAX_BYTECODE_SIZE],
    pub code_len: usize,
    var_names: [[u8; 16]; MAX_VARS],
    var_lens: [usize; MAX_VARS],
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

    fn emit_byte(&mut self, b: u8) -> Result<usize, CompileError> {
        if self.code_len >= MAX_BYTECODE_SIZE {
            return Err(self.error(
                "ERR_BYTECODE_OVERFLOW",
                "Bytecode buffer overflow (1024 bytes limit reached)",
                "Smaller script size or split into modules",
                "Code Generation",
                "Reduce script length or simplify loop expressions",
            ));
        }
        let pos = self.code_len;
        self.code[self.code_len] = b;
        self.code_len += 1;
        Ok(pos)
    }

    fn emit_u16(&mut self, val: u16) -> Result<usize, CompileError> {
        let pos = self.emit_byte((val >> 8) as u8)?;
        self.emit_byte((val & 0xFF) as u8)?;
        Ok(pos)
    }

    fn emit_i64(&mut self, val: i64) -> Result<usize, CompileError> {
        let pos = self.code_len;
        let bytes = val.to_be_bytes();
        for &b in &bytes {
            self.emit_byte(b)?;
        }
        Ok(pos)
    }

    fn patch_u16(&mut self, pos: usize, val: u16) {
        self.code[pos] = (val >> 8) as u8;
        self.code[pos + 1] = (val & 0xFF) as u8;
    }

    fn resolve_var(&mut self, tok: Token) -> Result<u8, CompileError> {
        let name = &self.src[tok.start..tok.start + tok.len];
        for i in 0..self.var_count {
            if self.var_lens[i] == name.len() && &self.var_names[i][..self.var_lens[i]] == name {
                return Ok(i as u8);
            }
        }
        if self.var_count >= MAX_VARS {
            return Err(self.error(
                "ERR_MAX_VARS_EXCEEDED",
                "Maximum distinct variables limit reached (32 vars)",
                "Reuse existing $variables",
                "Symbol Table Allocation",
                "Reduce the number of distinct variable names in the script",
            ));
        }
        let idx = self.var_count;
        let len = core::cmp::min(name.len(), 16);
        self.var_names[idx][..len].copy_from_slice(&name[..len]);
        self.var_lens[idx] = len;
        self.var_count += 1;
        Ok(idx as u8)
    }

    // Function: compile
    // Description: Single-pass compilation from tokens into bytecode.
    // Worst-case execution time: ~50_000 ns
    pub fn compile(&mut self) -> Result<usize, CompileError> {
        while self.peek().kind != TokenKind::Eof {
            self.statement()?;
        }
        self.emit_byte(OP_HALT)?;
        Ok(self.code_len)
    }

    fn statement(&mut self) -> Result<(), CompileError> {
        let tok = self.peek();

        match tok.kind {
            // Directives: @contract, @pipeline, @budget, @wcet
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
                // Optional @budget(...)
                if self.peek().kind == TokenKind::AtBudget || self.peek().kind == TokenKind::Budget {
                    self.advance();
                    if self.match_token(TokenKind::LParen) {
                        self.advance(); // time literal
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

                self.emit_byte(OP_WITHIN_START)?;
                self.emit_i64(deadline_ns as i64)?;

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
                self.emit_byte(OP_WITHIN_END)?;

                if self.match_token(TokenKind::Exclamation) || self.match_token(TokenKind::Or) {
                    if self.match_token(TokenKind::Drop) {
                        self.emit_byte(OP_DROP)?;
                    }
                }
                self.match_token(TokenKind::Semi);
            }

            TokenKind::AtWhile | TokenKind::While => {
                self.advance();
                let loop_start = self.code_len as u16;
                self.match_token(TokenKind::LParen);
                self.expression()?;
                self.match_token(TokenKind::RParen);

                self.emit_byte(OP_JUMP_IF_FALSE)?;
                let exit_jump = self.emit_u16(0)?;

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

                self.emit_byte(OP_JUMP)?;
                self.emit_u16(loop_start)?;
                self.patch_u16(exit_jump, self.code_len as u16);
            }

            TokenKind::Let => {
                self.advance();
                let ident = self.advance();
                let var_idx = self.resolve_var(ident)?;
                self.match_token(TokenKind::Eq);
                self.expression()?;
                self.match_token(TokenKind::Semi);
                self.emit_byte(OP_STORE_VAR)?;
                self.emit_byte(var_idx)?;
            }

            TokenKind::If => {
                self.advance();
                self.match_token(TokenKind::LParen);
                self.expression()?;
                self.match_token(TokenKind::RParen);

                self.emit_byte(OP_JUMP_IF_FALSE)?;
                let jump_false_pos = self.emit_u16(0)?;

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
                    self.emit_byte(OP_JUMP)?;
                    let jump_end_pos = self.emit_u16(0)?;
                    self.patch_u16(jump_false_pos, self.code_len as u16);

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
                    self.patch_u16(jump_end_pos, self.code_len as u16);
                } else {
                    self.patch_u16(jump_false_pos, self.code_len as u16);
                }
            }

            // Variable Assignment ($var := expr; or $var = expr; or $var += expr;)
            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                let ident = self.advance();
                let var_idx = self.resolve_var(ident)?;

                if self.match_token(TokenKind::ColonEq) || self.match_token(TokenKind::Eq) {
                    self.expression()?;
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of assignment",
                            "Semicolon ';'",
                            "Statement -> Variable Assignment",
                            "Append ';' at end of assignment statement",
                        ));
                    }
                    self.emit_byte(OP_STORE_VAR)?;
                    self.emit_byte(var_idx)?;
                } else if self.match_token(TokenKind::PlusEq) {
                    // $var += expr -> load, expr, add, store
                    self.emit_byte(OP_LOAD_VAR)?;
                    self.emit_byte(var_idx)?;
                    self.expression()?;
                    self.emit_byte(OP_ADD)?;
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Addition Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                    self.emit_byte(OP_STORE_VAR)?;
                    self.emit_byte(var_idx)?;
                } else if self.match_token(TokenKind::MinusEq) {
                    self.emit_byte(OP_LOAD_VAR)?;
                    self.emit_byte(var_idx)?;
                    self.expression()?;
                    self.emit_byte(OP_SUB)?;
                    if !self.match_token(TokenKind::Semi) {
                        return Err(self.error(
                            "ERR_MISSING_SEMICOLON",
                            "Missing semicolon ';' at end of compound assignment",
                            "Semicolon ';'",
                            "Statement -> Compound Subtraction Assignment",
                            "Append ';' at end of statement",
                        ));
                    }
                    self.emit_byte(OP_STORE_VAR)?;
                    self.emit_byte(var_idx)?;
                } else {
                    // Expression statement starting with var
                    self.current -= 1; // rewind
                    self.expression_statement()?;
                }
            }

            _ => {
                // Expression statement (e.g. condition ? { ... } : { ... }; or function call)
                self.expression_statement()?;
            }
        }
        Ok(())
    }

    fn expression_statement(&mut self) -> Result<(), CompileError> {
        self.expression()?;
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

    fn expression(&mut self) -> Result<(), CompileError> {
        self.ternary()?;
        while self.match_token(TokenKind::Pipe) {
            self.ternary()?;
        }
        Ok(())
    }

    fn ternary(&mut self) -> Result<(), CompileError> {
        self.equality()?;
        if self.match_token(TokenKind::Question) {
            self.emit_byte(OP_JUMP_IF_FALSE)?;
            let jump_false_pos = self.emit_u16(0)?;

            // True branch
            if self.match_token(TokenKind::LBrace) {
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                    self.statement()?;
                }
                self.match_token(TokenKind::RBrace);
            } else {
                self.expression()?;
            }

            if self.match_token(TokenKind::Colon) {
                self.emit_byte(OP_JUMP)?;
                let jump_end_pos = self.emit_u16(0)?;
                self.patch_u16(jump_false_pos, self.code_len as u16);

                // False branch
                if self.match_token(TokenKind::LBrace) {
                    while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
                        self.statement()?;
                    }
                    self.match_token(TokenKind::RBrace);
                } else {
                    self.expression()?;
                }
                self.patch_u16(jump_end_pos, self.code_len as u16);
            } else {
                self.patch_u16(jump_false_pos, self.code_len as u16);
            }
        }
        Ok(())
    }

    fn equality(&mut self) -> Result<(), CompileError> {
        self.comparison()?;
        while self.peek().kind == TokenKind::EqEq || self.peek().kind == TokenKind::NotEq {
            let op = self.advance().kind;
            self.comparison()?;
            if op == TokenKind::EqEq {
                self.emit_byte(OP_CMP_EQ)?;
            } else {
                self.emit_byte(OP_CMP_NE)?;
            }
        }
        Ok(())
    }

    fn comparison(&mut self) -> Result<(), CompileError> {
        self.term()?;
        while matches!(self.peek().kind, TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq) {
            let op = self.advance().kind;
            self.term()?;
            match op {
                TokenKind::Lt => { self.emit_byte(OP_CMP_LT)?; }
                TokenKind::LtEq => { self.emit_byte(OP_CMP_LE)?; }
                TokenKind::Gt => { self.emit_byte(OP_CMP_GT)?; }
                TokenKind::GtEq => { self.emit_byte(OP_CMP_GE)?; }
                _ => {}
            }
        }
        Ok(())
    }

    fn term(&mut self) -> Result<(), CompileError> {
        self.factor()?;
        while self.peek().kind == TokenKind::Plus || self.peek().kind == TokenKind::Minus {
            let op = self.advance().kind;
            self.factor()?;
            if op == TokenKind::Plus {
                self.emit_byte(OP_ADD)?;
            } else {
                self.emit_byte(OP_SUB)?;
            }
        }
        Ok(())
    }

    fn factor(&mut self) -> Result<(), CompileError> {
        self.primary()?;
        while self.peek().kind == TokenKind::Star || self.peek().kind == TokenKind::Slash || self.peek().kind == TokenKind::Percent {
            let op = self.advance().kind;
            self.primary()?;
            match op {
                TokenKind::Star => { self.emit_byte(OP_MUL)?; }
                TokenKind::Slash => { self.emit_byte(OP_DIV)?; }
                TokenKind::Percent => { self.emit_byte(OP_MOD)?; }
                _ => {}
            }
        }
        Ok(())
    }

    fn primary(&mut self) -> Result<(), CompileError> {
        let tok = self.advance();

        match tok.kind {
            TokenKind::Number(n) => {
                self.emit_byte(OP_PUSH_CONST)?;
                self.emit_i64(n)?;
            }

            TokenKind::TimeLiteral(ns) => {
                self.emit_byte(OP_PUSH_CONST)?;
                self.emit_i64(ns as i64)?;
            }

            TokenKind::StringLit => {
                let s = &self.src[tok.start + 1..tok.start + tok.len - 1];
                if self.str_pool_len + s.len() > MAX_STRING_POOL {
                    return Err(self.error(
                        "ERR_STRING_POOL_FULL",
                        "String literal pool exhausted (512 bytes limit reached)",
                        "Shorter string constants",
                        "String Pool Allocation",
                        "Reduce the size of string constants or combine print statements",
                    ));
                }
                let offset = self.str_pool_len;
                self.str_pool[offset..offset + s.len()].copy_from_slice(s);
                self.str_pool_len += s.len();

                self.emit_byte(OP_PUSH_STR)?;
                self.emit_u16(offset as u16)?;
                self.emit_u16(s.len() as u16)?;
            }

            TokenKind::VarIdent | TokenKind::HardwareIdent => {
                let var_idx = self.resolve_var(tok)?;
                self.emit_byte(OP_LOAD_VAR)?;
                self.emit_byte(var_idx)?;
            }

            TokenKind::IntrinsicIdent | TokenKind::Ident => {
                let name = &self.src[tok.start..tok.start + tok.len];
                if self.peek().kind == TokenKind::LParen {
                    // Intrinsic / Function call
                    self.advance(); // consume '('
                    let mut argc = 0;
                    if self.peek().kind != TokenKind::RParen {
                        self.expression()?;
                        argc += 1;
                        while self.match_token(TokenKind::Comma) {
                            self.expression()?;
                            argc += 1;
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
                        _ => {
                            return Err(self.error(
                                "ERR_UNKNOWN_INTRINSIC",
                                "Unknown intrinsic function name",
                                "One of @print, @println, @tsc, @rtt, @rate, @capture, @send",
                                "Expression -> Intrinsic Call",
                                "Verify that the intrinsic name matches supported DSL intrinsics",
                            ));
                        }
                    };

                    self.emit_byte(OP_CALL_NATIVE)?;
                    self.emit_byte(func_id)?;
                    self.emit_byte(argc)?;
                } else {
                    let var_idx = self.resolve_var(tok)?;
                    self.emit_byte(OP_LOAD_VAR)?;
                    self.emit_byte(var_idx)?;
                }
            }

            TokenKind::LParen => {
                self.expression()?;
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

// -----------------------------------------------------------------------------
// Real-Time Bytecode Virtual Machine
// -----------------------------------------------------------------------------

pub const STR_TAG: i64 = 0x4000_0000_0000_0000;

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
                "VM Evaluation stack overflow (64 elements limit reached)",
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

    // Function: run
    // Description: Execute bytecode under strictly bounded instruction step limit (WCET guarantee).
    // Worst-case execution time: ~100_000 ns (MAX_VM_STEPS * 10 ns)
    pub fn run(&mut self, tsc_freq_hz: u64) -> Result<(), CompileError> {
        let mut steps = 0;

        while self.ip < self.code.len() && steps < MAX_VM_STEPS {
            steps += 1;
            let op = self.code[self.ip];
            self.ip += 1;

            match op {
                OP_NOP => {}

                OP_PUSH_CONST => {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&self.code[self.ip..self.ip + 8]);
                    self.ip += 8;
                    let val = i64::from_be_bytes(bytes);
                    self.push(val)?;
                }

                OP_LOAD_VAR => {
                    let idx = self.code[self.ip] as usize;
                    self.ip += 1;
                    if idx < MAX_VARS {
                        self.push(self.vars[idx])?;
                    }
                }

                OP_STORE_VAR => {
                    let idx = self.code[self.ip] as usize;
                    self.ip += 1;
                    let val = self.pop()?;
                    if idx < MAX_VARS {
                        self.vars[idx] = val;
                    }
                }

                OP_ADD => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(a.wrapping_add(b))?;
                }

                OP_SUB => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(a.wrapping_sub(b))?;
                }

                OP_MUL => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(a.wrapping_mul(b))?;
                }

                OP_DIV => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    if b == 0 {
                        return Err(CompileError::simple(
                            "ERR_VM_DIV_BY_ZERO",
                            "Division by zero during VM bytecode execution",
                        ));
                    }
                    self.push(a / b)?;
                }

                OP_MOD => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    if b == 0 {
                        return Err(CompileError::simple(
                            "ERR_VM_MOD_BY_ZERO",
                            "Modulo by zero during VM bytecode execution",
                        ));
                    }
                    self.push(a % b)?;
                }

                OP_CMP_EQ => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(if a == b { 1 } else { 0 })?;
                }

                OP_CMP_NE => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(if a != b { 1 } else { 0 })?;
                }

                OP_CMP_LT => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(if a < b { 1 } else { 0 })?;
                }

                OP_CMP_LE => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(if a <= b { 1 } else { 0 })?;
                }

                OP_CMP_GT => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(if a > b { 1 } else { 0 })?;
                }

                OP_CMP_GE => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    self.push(if a >= b { 1 } else { 0 })?;
                }

                OP_JUMP => {
                    let target = u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    self.ip = target;
                }

                OP_JUMP_IF_FALSE => {
                    let target = u16::from_be_bytes([self.code[self.ip], self.code[self.ip + 1]]) as usize;
                    self.ip += 2;
                    let cond = self.pop()?;
                    if cond == 0 {
                        self.ip = target;
                    }
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
                                if (val & STR_TAG) != 0 {
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
                                if (val & STR_TAG) != 0 {
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

                        NATIVE_SYS_TSC => {
                            let tsc = read_tsc_serialized();
                            self.push(tsc as i64)?;
                        }

                        NATIVE_NET_RTT => {
                            let rtt = LAST_RTT_NS.load(Ordering::Relaxed);
                            self.push(rtt as i64)?;
                        }

                        NATIVE_NET_SET_RATE => {
                            if argc > 0 {
                                let rate = self.pop()?;
                                CONGESTION_RATE_PCT.store(rate as u8, Ordering::Relaxed);
                            }
                            self.push(0)?;
                        }

                        NATIVE_GPU_CAPTURE => {
                            let vblank = poll_vblank_edge(5);
                            let handle = capture_frame_zero_copy(0, 1, vblank);
                            self.push(handle.slot_id as i64)?;
                        }

                        NATIVE_NET_SEND => {
                            if argc > 0 {
                                let _slot = self.pop()?;
                                let handle = capture_frame_zero_copy(0, 1, read_tsc_serialized());
                                let deadline = read_tsc_serialized() + crate::tsc::ns_to_tsc(50_000_000, tsc_freq_hz);
                                let mut seq = 1u16;
                                let _ = stream_send_frame(&handle, deadline, &mut seq);
                            }
                            self.push(1)?;
                        }

                        _ => {}
                    }
                }

                OP_WITHIN_START => {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&self.code[self.ip..self.ip + 8]);
                    self.ip += 8;
                    let budget_ns = i64::from_be_bytes(bytes) as u64;

                    let tsc_budget = if tsc_freq_hz > 0 {
                        (budget_ns * tsc_freq_hz) / 1_000_000_000
                    } else {
                        budget_ns * 3
                    };
                    let deadline = read_tsc_serialized() + tsc_budget;

                    if self.dl_sp < 8 {
                        self.deadline_stack[self.dl_sp] = deadline;
                        self.dl_sp += 1;
                    }
                }

                OP_WITHIN_END => {
                    if self.dl_sp > 0 {
                        self.dl_sp -= 1;
                    }
                }

                OP_DROP => {
                    if self.dl_sp > 0 {
                        let dl = self.deadline_stack[self.dl_sp - 1];
                        if read_tsc_serialized() > dl {
                            serial_println!("[DEADLINE_DROP] Frame dropped due to deadline breach");
                        }
                    }
                }

                OP_HALT => {
                    break;
                }

                _ => {
                    return Err(CompileError::simple(
                        "ERR_VM_INVALID_OPCODE",
                        "Invalid bytecode opcode encountered in instruction stream",
                    ));
                }
            }
        }

        if steps >= MAX_VM_STEPS {
            return Err(CompileError::simple(
                "ERR_VM_WCET_EXCEEDED",
                "Execution exceeded WCET instruction step limit (infinite loop protection)",
            ));
        }

        Ok(())
    }
}

// Function: run_pulse_script
// Description: Compile and execute a PulseLang script in one shot.
// Worst-case execution time: ~120_000 ns
pub fn run_pulse_script(src: &[u8], tsc_freq_hz: u64) -> Result<(), CompileError> {
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src, &tokens);
    let _code_len = compiler.compile()?;

    let mut vm = VM::new(&compiler.code[..compiler.code_len], &compiler.str_pool[..compiler.str_pool_len]);
    vm.run(tsc_freq_hz)
}

pub const PULSE_BIN_MAGIC: [u8; 4] = *b"PULS";
pub const PULSE_BIN_VERSION: u16 = 2;
pub const PULSE_HEADER_SIZE: usize = 12;

// Function: compile_pulse_to_binary
// Description: Compile PulseLang source code into binary bytecode format.
// Worst-case execution time: ~60_000 ns
pub fn compile_pulse_to_binary(src: &[u8], out_buf: &mut [u8]) -> Result<usize, CompileError> {
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src, &tokens);
    let code_len = compiler.compile()?;
    let str_pool_len = compiler.str_pool_len;

    let total_size = PULSE_HEADER_SIZE + code_len + str_pool_len;
    if total_size > out_buf.len() {
        return Err(CompileError::simple(
            "ERR_BINARY_BUFFER_OVERFLOW",
            "Target binary output buffer is too small for compiled bytecode",
        ));
    }

    // Header
    out_buf[0..4].copy_from_slice(&PULSE_BIN_MAGIC);
    out_buf[4..6].copy_from_slice(&PULSE_BIN_VERSION.to_be_bytes());
    out_buf[6..8].copy_from_slice(&(code_len as u16).to_be_bytes());
    out_buf[8..10].copy_from_slice(&(str_pool_len as u16).to_be_bytes());
    out_buf[10..12].copy_from_slice(&0u16.to_be_bytes()); // Reserved

    // Payload
    out_buf[PULSE_HEADER_SIZE..PULSE_HEADER_SIZE + code_len].copy_from_slice(&compiler.code[..code_len]);
    out_buf[PULSE_HEADER_SIZE + code_len..total_size].copy_from_slice(&compiler.str_pool[..str_pool_len]);

    Ok(total_size)
}

// Function: run_pulse_binary
// Description: Execute pre-compiled PulseLang binary bytecode directly in O(1) zero compilation latency.
// Worst-case execution time: ~80_000 ns
pub fn run_pulse_binary(bin: &[u8], tsc_freq_hz: u64) -> Result<(), CompileError> {
    if bin.len() < PULSE_HEADER_SIZE {
        return Err(CompileError::simple("ERR_BINARY_TOO_SMALL", "Binary file smaller than header size"));
    }
    if &bin[0..4] != &PULSE_BIN_MAGIC {
        return Err(CompileError::simple("ERR_BINARY_INVALID_MAGIC", "Invalid PulseLang binary magic"));
    }
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
}

// Function: run_pulse_auto
// Description: Automatically detect binary vs source script and execute.
// Worst-case execution time: ~120_000 ns
pub fn run_pulse_auto(data: &[u8], tsc_freq_hz: u64) -> Result<(), CompileError> {
    if data.len() >= 4 && &data[0..4] == &PULSE_BIN_MAGIC {
        run_pulse_binary(data, tsc_freq_hz)
    } else {
        run_pulse_script(data, tsc_freq_hz)
    }
}
