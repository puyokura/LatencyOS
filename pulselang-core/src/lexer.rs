//! PulseLang zero-allocation lexer (tokenizer)

use crate::error::CompileError;
use crate::token::{get_line_and_col, Token, TokenKind};

/// Zero-allocation lexer for PulseLang source scripts.
pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Self { src, pos: 0, line: 1 }
    }

    /// Tokenize input source into token buffer without dynamic allocation.
    pub fn tokenize(&mut self, tokens: &mut [Token]) -> Result<usize, CompileError> {
        let mut count = 0;
        let max_tokens = tokens.len();

        if max_tokens == 0 {
            return Err(CompileError::simple(
                "ERR_TOKEN_BUFFER_EMPTY",
                "Supplied token slice has 0 capacity",
            ));
        }

        while self.pos < self.src.len() && count + 1 < max_tokens {
            self.skip_whitespace_and_comments();
            if self.pos >= self.src.len() {
                break;
            }

            let start = self.pos;
            let (line, col) = get_line_and_col(self.src, start);
            let b = self.src[self.pos];

            let kind = match b {
                b'(' => {
                    self.pos += 1;
                    TokenKind::LParen
                }
                b')' => {
                    self.pos += 1;
                    TokenKind::RParen
                }
                b'{' => {
                    self.pos += 1;
                    TokenKind::LBrace
                }
                b'}' => {
                    self.pos += 1;
                    TokenKind::RBrace
                }
                b';' => {
                    self.pos += 1;
                    TokenKind::Semi
                }
                b',' => {
                    self.pos += 1;
                    TokenKind::Comma
                }
                b'.' => {
                    if self.peek_next() == Some(b'.') {
                        self.pos += 2;
                        TokenKind::DotDot
                    } else {
                        self.pos += 1;
                        TokenKind::Dot
                    }
                }
                b'?' => {
                    self.pos += 1;
                    TokenKind::Question
                }

                b':' => {
                    if self.peek_next() == Some(b'=') {
                        self.pos += 2;
                        TokenKind::ColonEq
                    } else if self.peek_next() == Some(b':') {
                        self.pos += 2;
                        TokenKind::ColonColon
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

                b'*' => {
                    self.pos += 1;
                    TokenKind::Star
                }
                b'/' => {
                    self.pos += 1;
                    TokenKind::Slash
                }
                b'%' => {
                    self.pos += 1;
                    TokenKind::Percent
                }

                b'[' => {
                    self.pos += 1;
                    TokenKind::LBracket
                }
                b']' => {
                    self.pos += 1;
                    TokenKind::RBracket
                }
                b'^' => {
                    self.pos += 1;
                    TokenKind::Caret
                }

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
                    if b == b'0'
                        && self.pos + 1 < self.src.len()
                        && (self.src[self.pos + 1] == b'x' || self.src[self.pos + 1] == b'X')
                    {
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
                        if self.pos < self.src.len()
                            && self.src[self.pos] == b'.'
                            && self.pos + 1 < self.src.len()
                            && self.src[self.pos + 1].is_ascii_digit()
                        {
                            self.pos += 1; // consume '.'
                            let mut scaled_val = num;
                            let mut frac_digits: u8 = 0;
                            while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                                scaled_val = scaled_val.saturating_mul(10).saturating_add((self.src[self.pos] - b'0') as i64);
                                frac_digits = frac_digits.saturating_add(1);
                                self.pos += 1;
                            }
                            TokenKind::FloatLit(scaled_val, frac_digits)
                        } else if self.match_suffix(b"ns") {
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
                        && (self.src[self.pos].is_ascii_alphanumeric()
                            || self.src[self.pos] == b'_')
                    {
                        self.pos += 1;
                    }
                    TokenKind::VarIdent
                }

                b'#' => {
                    // Hardware Handle: #f, #frame, #slot0
                    self.pos += 1;
                    while self.pos < self.src.len()
                        && (self.src[self.pos].is_ascii_alphanumeric()
                            || self.src[self.pos] == b'_')
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
                        && (self.src[self.pos].is_ascii_alphanumeric()
                            || self.src[self.pos] == b'_'
                            || self.src[self.pos] == b'.')
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
                        b"pool_size" => TokenKind::AtPoolSize,
                        b"test" => TokenKind::AtTest,
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
                        b"enum" => TokenKind::Enum,
                        b"fixed" => TokenKind::Fixed,
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
        self.skip_whitespace_and_comments();
        if self.pos < self.src.len() {
            let (line, col) = get_line_and_col(self.src, self.pos);
            return Err(CompileError {
                code: "ERR_MAX_TOKENS_EXCEEDED",
                message: "Source script exceeds maximum tokens capacity",
                line,
                col,
                byte_offset: self.pos,
                token_kind: TokenKind::Eof,
                token_len: 0,
                expected: "Script with fewer tokens or split into smaller functions/modules",
                stage: "Lexer -> Tokenization",
                suggestion: "Reduce code size or split logic into modules",
            });
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
                b'/' if self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'*' => {
                    self.pos += 2;
                    while self.pos + 1 < self.src.len() {
                        if self.src[self.pos] == b'\n' {
                            self.line += 1;
                        }
                        if self.src[self.pos] == b'*' && self.src[self.pos + 1] == b'/' {
                            self.pos += 2;
                            break;
                        }
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
