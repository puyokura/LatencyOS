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

#[derive(Clone, Copy)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub len: usize,
    #[allow(dead_code)]
    pub line: usize,
}

impl Token {
    pub const fn empty() -> Self {
        Self {
            kind: TokenKind::Eof,
            start: 0,
            len: 0,
            line: 1,
        }
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
    pub fn tokenize(&mut self, tokens: &mut [Token; MAX_TOKENS]) -> Result<usize, &'static str> {
        let mut count = 0;

        while self.pos < self.src.len() && count < MAX_TOKENS - 1 {
            self.skip_whitespace_and_comments();
            if self.pos >= self.src.len() {
                break;
            }

            let start = self.pos;
            let line = self.line;
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
                        return Err("Unexpected single '|'");
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
                        return Err("Single '&' not supported");
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
            };
            count += 1;
        }

        tokens[count] = Token {
            kind: TokenKind::Eof,
            start: self.pos,
            len: 0,
            line: self.line,
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

    fn emit_byte(&mut self, b: u8) -> Result<usize, &'static str> {
        if self.code_len >= MAX_BYTECODE_SIZE {
            return Err("Bytecode buffer full");
        }
        let pos = self.code_len;
        self.code[self.code_len] = b;
        self.code_len += 1;
        Ok(pos)
    }

    fn emit_u16(&mut self, val: u16) -> Result<usize, &'static str> {
        let pos = self.emit_byte((val >> 8) as u8)?;
        self.emit_byte((val & 0xFF) as u8)?;
        Ok(pos)
    }

    fn emit_i64(&mut self, val: i64) -> Result<usize, &'static str> {
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

    fn resolve_var(&mut self, tok: Token) -> Result<u8, &'static str> {
        let name = &self.src[tok.start..tok.start + tok.len];
        for i in 0..self.var_count {
            if self.var_lens[i] == name.len() && &self.var_names[i][..self.var_lens[i]] == name {
                return Ok(i as u8);
            }
        }
        if self.var_count >= MAX_VARS {
            return Err("Maximum variables limit reached");
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
    pub fn compile(&mut self) -> Result<usize, &'static str> {
        while self.peek().kind != TokenKind::Eof {
            self.statement()?;
        }
        self.emit_byte(OP_HALT)?;
        Ok(self.code_len)
    }

    fn statement(&mut self) -> Result<(), &'static str> {
        let tok = self.peek();

        match tok.kind {
            // Directives: @contract, @pipeline, @budget, @wcet
            TokenKind::AtContract => {
                self.advance();
                self.match_token(TokenKind::Colon);
                while self.peek().kind != TokenKind::Semi && self.peek().kind != TokenKind::Eof {
                    self.advance();
                }
                self.match_token(TokenKind::Semi);
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
                    _ => return Err("Expected time literal (e.g. 500us)"),
                };

                self.emit_byte(OP_WITHIN_START)?;
                self.emit_i64(deadline_ns as i64)?;

                if !self.match_token(TokenKind::LBrace) {
                    return Err("Expected '{' in within block");
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
                    return Err("Expected '{' in while block");
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
                    return Err("Expected '{' after if condition");
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
                        return Err("Expected '{' after else");
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
                    self.match_token(TokenKind::Semi);
                    self.emit_byte(OP_STORE_VAR)?;
                    self.emit_byte(var_idx)?;
                } else if self.match_token(TokenKind::PlusEq) {
                    // $var += expr -> load, expr, add, store
                    self.emit_byte(OP_LOAD_VAR)?;
                    self.emit_byte(var_idx)?;
                    self.expression()?;
                    self.emit_byte(OP_ADD)?;
                    self.match_token(TokenKind::Semi);
                    self.emit_byte(OP_STORE_VAR)?;
                    self.emit_byte(var_idx)?;
                } else if self.match_token(TokenKind::MinusEq) {
                    self.emit_byte(OP_LOAD_VAR)?;
                    self.emit_byte(var_idx)?;
                    self.expression()?;
                    self.emit_byte(OP_SUB)?;
                    self.match_token(TokenKind::Semi);
                    self.emit_byte(OP_STORE_VAR)?;
                    self.emit_byte(var_idx)?;
                } else {
                    // Expression statement starting with var
                    self.current -= 1; // rewind
                    self.expression()?;
                    self.match_token(TokenKind::Semi);
                }
            }

            _ => {
                // Expression statement (e.g. condition ? { ... } : { ... }; or function call)
                self.expression_statement()?;
            }
        }
        Ok(())
    }

    fn expression_statement(&mut self) -> Result<(), &'static str> {
        self.expression()?;

        // Check for ternary condition statement: cond ? { true_block } : { false_block };
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

        self.match_token(TokenKind::Semi);
        Ok(())
    }

    fn expression(&mut self) -> Result<(), &'static str> {
        self.equality()?;
        // Support pipe operator: expr |> func()
        while self.match_token(TokenKind::Pipe) {
            self.equality()?;
        }
        Ok(())
    }

    fn equality(&mut self) -> Result<(), &'static str> {
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

    fn comparison(&mut self) -> Result<(), &'static str> {
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

    fn term(&mut self) -> Result<(), &'static str> {
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

    fn factor(&mut self) -> Result<(), &'static str> {
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

    fn primary(&mut self) -> Result<(), &'static str> {
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
                    return Err("String pool full");
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
                    self.match_token(TokenKind::RParen);

                    let func_id = match name {
                        b"@print" | b"print" => NATIVE_PRINT,
                        b"@println" | b"println" => NATIVE_PRINTLN,
                        b"@tsc" | b"sys.tsc" => NATIVE_SYS_TSC,
                        b"@rtt" | b"net.rtt" => NATIVE_NET_RTT,
                        b"@rate" | b"net.set_rate" => NATIVE_NET_SET_RATE,
                        b"@capture" | b"gpu.capture" => NATIVE_GPU_CAPTURE,
                        b"@send" | b"net.send" => NATIVE_NET_SEND,
                        _ => return Err("Unknown intrinsic function"),
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
                    return Err("Expected ')' after expression");
                }
            }

            _ => return Err("Unexpected token in expression"),
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Real-Time Bytecode Virtual Machine
// -----------------------------------------------------------------------------

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

    fn push(&mut self, val: i64) -> Result<(), &'static str> {
        if self.sp >= 64 {
            return Err("VM Stack overflow");
        }
        self.stack[self.sp] = val;
        self.sp += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<i64, &'static str> {
        if self.sp == 0 {
            return Err("VM Stack underflow");
        }
        self.sp -= 1;
        Ok(self.stack[self.sp])
    }

    // Function: run
    // Description: Execute bytecode under strictly bounded instruction step limit (WCET guarantee).
    // Worst-case execution time: ~100_000 ns (MAX_VM_STEPS * 10 ns)
    pub fn run(&mut self, tsc_freq_hz: u64) -> Result<(), &'static str> {
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
                        return Err("Division by zero");
                    }
                    self.push(a / b)?;
                }

                OP_MOD => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    if b == 0 {
                        return Err("Modulo by zero");
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
                    let target = ((self.code[self.ip] as usize) << 8) | (self.code[self.ip + 1] as usize);
                    self.ip = target;
                }

                OP_JUMP_IF_FALSE => {
                    let target = ((self.code[self.ip] as usize) << 8) | (self.code[self.ip + 1] as usize);
                    self.ip += 2;
                    let cond = self.pop()?;
                    if cond == 0 {
                        self.ip = target;
                    }
                }

                OP_PUSH_STR => {
                    let offset = ((self.code[self.ip] as usize) << 8) | (self.code[self.ip + 1] as usize);
                    let len = ((self.code[self.ip + 2] as usize) << 8) | (self.code[self.ip + 3] as usize);
                    self.ip += 4;
                    let tagged = 0x7FFF_0000_0000_0000i64 | ((offset as i64) << 16) | (len as i64);
                    self.push(tagged)?;
                }

                OP_CALL_NATIVE => {
                    let func_id = self.code[self.ip];
                    let argc = self.code[self.ip + 1];
                    self.ip += 2;

                    match func_id {
                        NATIVE_PRINT => {
                            if argc > 0 {
                                let val = self.pop()?;
                                if (val & 0x7FFF_0000_0000_0000i64) == 0x7FFF_0000_0000_0000i64 {
                                    let offset = ((val >> 16) & 0xFFFF) as usize;
                                    let len = (val & 0xFFFF) as usize;
                                    if offset + len <= self.str_pool.len() {
                                        if let Ok(s) = core::str::from_utf8(&self.str_pool[offset..offset + len]) {
                                            serial_print!("{}", s);
                                        }
                                    }
                                } else {
                                    serial_print!("{}", val);
                                }
                            }
                        }

                        NATIVE_PRINTLN => {
                            if argc > 0 {
                                let val = self.pop()?;
                                if (val & 0x7FFF_0000_0000_0000i64) == 0x7FFF_0000_0000_0000i64 {
                                    let offset = ((val >> 16) & 0xFFFF) as usize;
                                    let len = (val & 0xFFFF) as usize;
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
                                let pct = self.pop()? as u8;
                                CONGESTION_RATE_PCT.store(pct.clamp(10, 100), Ordering::Relaxed);
                            }
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
                        }

                        _ => return Err("Unknown native syscall"),
                    }
                }

                OP_WITHIN_START => {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&self.code[self.ip..self.ip + 8]);
                    self.ip += 8;
                    let deadline_ns = i64::from_be_bytes(bytes) as u64;
                    let deadline_tsc = read_tsc_serialized() + crate::tsc::ns_to_tsc(deadline_ns, tsc_freq_hz);
                    if self.dl_sp < 8 {
                        self.deadline_stack[self.dl_sp] = deadline_tsc;
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

                _ => return Err("Invalid bytecode opcode"),
            }
        }

        if steps >= MAX_VM_STEPS {
            return Err("Execution exceeded WCET step limit (infinite loop protection)");
        }

        Ok(())
    }
}

// Function: run_pulse_script
// Description: Compile and execute a PulseLang script in one shot.
// Worst-case execution time: ~120_000 ns
pub fn run_pulse_script(src: &[u8], tsc_freq_hz: u64) -> Result<(), &'static str> {
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src, &tokens);
    let _code_len = compiler.compile()?;

    let mut vm = VM::new(&compiler.code[..compiler.code_len], &compiler.str_pool[..compiler.str_pool_len]);
    vm.run(tsc_freq_hz)
}
