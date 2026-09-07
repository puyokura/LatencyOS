//! PulseLang Source Code Formatter (`pulc fmt`)
//!
//! Provides deterministic, standard-conformant formatting for PulseLang source scripts:
//! - 4-space block indentation
//! - Normalized operator and punctuation spacing
//! - Contract clause alignment (@wcet, @requires, @ensures, @invariant)
//! - Single-line comment and string literal preservation
//! - Normalization of empty lines to at most one
//! - Clean terminating newline

use crate::error::CompileError;
use crate::lexer::Lexer;
use crate::isa::MAX_TOKENS;
use crate::token::{Token, TokenKind};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenCategory {
    IdentOrLiteral,
    Colon,
    ColonColon,
    Comma,
    Semi,
    Dot,
    DotDot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    BinaryOp,
    UnaryMinus,
    Exclamation,
    Question,
}

#[derive(Debug, Clone)]
struct LineToken<'a> {
    text: &'a str,
    category: TokenCategory,
}

/// Tokenize a code segment on a single line into classified tokens.
fn tokenize_line<'a>(line: &'a str) -> Vec<LineToken<'a>> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut after_fixed = false;
    let mut in_fixed_lt = false;

    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' => {
                i += 1;
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i < bytes.len() && bytes[i] == b'"' {
                    i += 1;
                }
                tokens.push(LineToken {
                    text: &line[start..i],
                    category: TokenCategory::IdentOrLiteral,
                });
            }
            b':' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::ColonColon,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::Colon,
                    });
                    i += 1;
                }
            }
            b';' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::Semi,
                });
                i += 1;
            }
            b',' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::Comma,
                });
                i += 1;
            }
            b'.' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'.' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::DotDot,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::Dot,
                    });
                    i += 1;
                }
            }
            b'(' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::LParen,
                });
                i += 1;
            }
            b')' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::RParen,
                });
                i += 1;
            }
            b'[' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::LBracket,
                });
                i += 1;
            }
            b']' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::RBracket,
                });
                i += 1;
            }
            b'{' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::LBrace,
                });
                i += 1;
            }
            b'}' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::RBrace,
                });
                i += 1;
            }
            b'!' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::Exclamation,
                    });
                    i += 1;
                }
            }
            b'?' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::Question,
                });
                i += 1;
            }
            b'=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                }
            }
            b'+' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                }
            }
            b'-' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    let is_unary = match tokens.last() {
                        None => true,
                        Some(prev) => match prev.category {
                            TokenCategory::BinaryOp
                            | TokenCategory::UnaryMinus
                            | TokenCategory::LParen
                            | TokenCategory::LBracket
                            | TokenCategory::Comma
                            | TokenCategory::Colon
                            | TokenCategory::Semi => true,
                            TokenCategory::IdentOrLiteral => {
                                prev.text == "return" || prev.text == "in"
                            }
                            _ => false,
                        },
                    };
                    if is_unary {
                        tokens.push(LineToken {
                            text: &line[i..i + 1],
                            category: TokenCategory::UnaryMinus,
                        });
                    } else {
                        tokens.push(LineToken {
                            text: &line[i..i + 1],
                            category: TokenCategory::BinaryOp,
                        });
                    }
                    i += 1;
                }
            }
            b'*' | b'/' | b'%' | b'^' => {
                tokens.push(LineToken {
                    text: &line[i..i + 1],
                    category: TokenCategory::BinaryOp,
                });
                i += 1;
            }
            b'&' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'&' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                }
            }
            b'|' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                }
            }
            b'<' => {
                if after_fixed {
                    in_fixed_lt = true;
                    after_fixed = false;
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                }
            }
            b'>' => {
                if in_fixed_lt {
                    in_fixed_lt = false;
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(LineToken {
                        text: &line[i..i + 2],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 2;
                } else {
                    tokens.push(LineToken {
                        text: &line[i..i + 1],
                        category: TokenCategory::BinaryOp,
                    });
                    i += 1;
                }
            }
            b'$' | b'#' | b'@' => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric()
                        || bytes[i] == b'_'
                        || bytes[i] == b'.')
                {
                    i += 1;
                }
                tokens.push(LineToken {
                    text: &line[start..i],
                    category: TokenCategory::IdentOrLiteral,
                });
            }
            b'0'..=b'9' => {
                let start = i;
                if bytes[i] == b'0'
                    && i + 1 < bytes.len()
                    && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
                {
                    i += 2;
                    while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                } else {
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i < bytes.len()
                        && bytes[i] == b'.'
                        && i + 1 < bytes.len()
                        && bytes[i + 1].is_ascii_digit()
                    {
                        i += 1;
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    if i + 2 <= bytes.len()
                        && (&line[i..i + 2] == "ns"
                            || &line[i..i + 2] == "us"
                            || &line[i..i + 2] == "ms")
                    {
                        i += 2;
                    } else if i < bytes.len() && bytes[i] == b's' {
                        i += 1;
                    }
                }
                tokens.push(LineToken {
                    text: &line[start..i],
                    category: TokenCategory::IdentOrLiteral,
                });
            }
            _ => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric()
                        || bytes[i] == b'_'
                        || bytes[i] == b'.')
                {
                    i += 1;
                }
                if i > start {
                    let word = &line[start..i];
                    after_fixed = word == "fixed";
                    tokens.push(LineToken {
                        text: word,
                        category: TokenCategory::IdentOrLiteral,
                    });
                } else {
                    i += 1;
                }
            }
        }
    }
    tokens
}

/// Determines whether a space is required between `prev` and `curr`.
fn is_space_needed(prev: &LineToken, curr: &LineToken, has_fixed_lt: bool) -> bool {
    if curr.category == TokenCategory::Semi || curr.category == TokenCategory::Comma {
        return false;
    }
    if curr.category == TokenCategory::Colon {
        return false;
    }
    if curr.category == TokenCategory::Dot || prev.category == TokenCategory::Dot {
        return false;
    }
    if curr.category == TokenCategory::DotDot || prev.category == TokenCategory::DotDot {
        return false;
    }
    if curr.category == TokenCategory::ColonColon || prev.category == TokenCategory::ColonColon {
        return false;
    }
    if curr.category == TokenCategory::Question {
        return false;
    }
    if prev.category == TokenCategory::Exclamation {
        return false;
    }
    if prev.category == TokenCategory::Colon {
        return true;
    }
    if prev.category == TokenCategory::Semi {
        return true;
    }
    if prev.category == TokenCategory::Comma {
        return true;
    }
    if curr.category == TokenCategory::RParen {
        return false;
    }
    if prev.category == TokenCategory::LParen {
        return false;
    }
    if curr.category == TokenCategory::RBracket {
        return false;
    }
    if prev.category == TokenCategory::LBracket {
        return false;
    }
    if curr.category == TokenCategory::LParen {
        if prev.category == TokenCategory::IdentOrLiteral {
            return matches!(
                prev.text,
                "if" | "while" | "match" | "for" | "return" | "in"
            );
        }
        if prev.category == TokenCategory::BinaryOp {
            return true;
        }
        if prev.category == TokenCategory::LParen || prev.category == TokenCategory::LBracket {
            return false;
        }
        return true;
    }
    if curr.category == TokenCategory::LBracket {
        if prev.category == TokenCategory::IdentOrLiteral {
            if prev.text == "return" || prev.text == "in" {
                return true;
            }
            return false;
        }
        if prev.category == TokenCategory::RParen || prev.category == TokenCategory::RBracket {
            return false;
        }
        return true;
    }
    // fixed<16>
    if curr.text == "<" && prev.text == "fixed" {
        return false;
    }
    if prev.text == "<" && has_fixed_lt {
        return false;
    }
    if curr.text == ">" && has_fixed_lt {
        return false;
    }
    if curr.category == TokenCategory::UnaryMinus {
        if prev.category == TokenCategory::LParen || prev.category == TokenCategory::LBracket {
            return false;
        }
        return true;
    }
    if prev.category == TokenCategory::UnaryMinus {
        return false;
    }
    if curr.category == TokenCategory::BinaryOp || prev.category == TokenCategory::BinaryOp {
        return true;
    }
    if curr.category == TokenCategory::LBrace {
        return true;
    }
    if prev.category == TokenCategory::LBrace {
        return true;
    }
    if curr.category == TokenCategory::RBrace {
        return true;
    }
    if prev.category == TokenCategory::RBrace {
        if curr.category == TokenCategory::Semi || curr.category == TokenCategory::Comma {
            return false;
        }
        return true;
    }

    true
}

/// Format tokens on a single line into a normalized string.
fn format_tokens(tokens: &[LineToken]) -> String {
    let mut out = String::new();
    let mut in_fixed = false;

    for i in 0..tokens.len() {
        let curr = &tokens[i];
        if curr.text == "<" && i > 0 && tokens[i - 1].text == "fixed" {
            in_fixed = true;
        }

        if i > 0 {
            let prev = &tokens[i - 1];
            if is_space_needed(prev, curr, in_fixed) {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
        }

        out.push_str(curr.text);

        if in_fixed && curr.text == ">" {
            in_fixed = false;
        }
    }

    out
}


/// Split line into code and optional single-line comment, taking string literals into account.
fn split_line_comment(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => {
                i += 2;
            }
            b'"' => {
                in_str = !in_str;
                i += 1;
            }
            b'/' if !in_str && i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                return (&line[..i], Some(&line[i..]));
            }
            _ => {
                i += 1;
            }
        }
    }
    (line, None)
}

/// Count unquoted '{' and '}' occurrences in code.
fn count_braces(code: &str) -> (usize, usize) {
    let bytes = code.as_bytes();
    let mut in_str = false;
    let mut open = 0;
    let mut close = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => {
                i += 2;
            }
            b'"' => {
                in_str = !in_str;
                i += 1;
            }
            b'{' if !in_str => {
                open += 1;
                i += 1;
            }
            b'}' if !in_str => {
                close += 1;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    (open, close)
}

/// Count unquoted '[' and ']' occurrences in code.
fn count_brackets(code: &str) -> (usize, usize) {
    let bytes = code.as_bytes();
    let mut in_str = false;
    let mut open = 0;
    let mut close = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => {
                i += 2;
            }
            b'"' => {
                in_str = !in_str;
                i += 1;
            }
            b'[' if !in_str => {
                open += 1;
                i += 1;
            }
            b']' if !in_str => {
                close += 1;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    (open, close)
}

/// Format PulseLang source code into a standardized, deterministic representation.
///
/// Returns `Ok(formatted_string)` or `Err(CompileError)` if source contains lexical or delimiter syntax errors.
pub fn format_source(src: &str) -> Result<String, CompileError> {
    if src.trim().is_empty() {
        return Ok(String::new());
    }

    // 1. Lexical & Delimiter Validation
    let mut tokens = alloc::vec![Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src.as_bytes());
    let tok_count = lexer.tokenize(&mut tokens)?;

    let mut brace_count: isize = 0;
    let mut paren_count: isize = 0;
    let mut bracket_count: isize = 0;

    for tok in &tokens[..tok_count] {
        match tok.kind {
            TokenKind::LBrace => brace_count += 1,
            TokenKind::RBrace => {
                brace_count -= 1;
                if brace_count < 0 {
                    return Err(CompileError {
                        code: "ERR_UNMATCHED_BRACE",
                        message: "Unexpected closing brace '}' without matching '{'",
                        line: tok.line,
                        col: tok.col,
                        byte_offset: tok.start,
                        token_kind: tok.kind,
                        token_len: tok.len,
                        expected: "Matching opening brace '{'",
                        stage: "Formatter -> Validation",
                        suggestion: "Remove redundant '}' or add opening '{'",
                    });
                }
            }
            TokenKind::LParen => paren_count += 1,
            TokenKind::RParen => {
                paren_count -= 1;
                if paren_count < 0 {
                    return Err(CompileError {
                        code: "ERR_UNMATCHED_PAREN",
                        message: "Unexpected closing parenthesis ')' without matching '('",
                        line: tok.line,
                        col: tok.col,
                        byte_offset: tok.start,
                        token_kind: tok.kind,
                        token_len: tok.len,
                        expected: "Matching opening parenthesis '('",
                        stage: "Formatter -> Validation",
                        suggestion: "Remove redundant ')' or add opening '('",
                    });
                }
            }
            TokenKind::LBracket => bracket_count += 1,
            TokenKind::RBracket => {
                bracket_count -= 1;
                if bracket_count < 0 {
                    return Err(CompileError {
                        code: "ERR_UNMATCHED_BRACKET",
                        message: "Unexpected closing bracket ']' without matching '['",
                        line: tok.line,
                        col: tok.col,
                        byte_offset: tok.start,
                        token_kind: tok.kind,
                        token_len: tok.len,
                        expected: "Matching opening bracket '['",
                        stage: "Formatter -> Validation",
                        suggestion: "Remove redundant ']' or add opening '['",
                    });
                }
            }
            TokenKind::StringLit => {
                let src_bytes = src.as_bytes();
                if tok.len < 2
                    || src_bytes[tok.start] != b'"'
                    || src_bytes[tok.start + tok.len - 1] != b'"'
                {
                    return Err(CompileError {
                        code: "ERR_UNCLOSED_STRING",
                        message: "Unclosed string literal in source script",
                        line: tok.line,
                        col: tok.col,
                        byte_offset: tok.start,
                        token_kind: tok.kind,
                        token_len: tok.len,
                        expected: "Closing quote '\"'",
                        stage: "Formatter -> Validation",
                        suggestion: "Add closing quote '\"' to terminate string literal",
                    });
                }
            }
            _ => {}
        }
    }

    if brace_count != 0 {
        return Err(CompileError::simple(
            "ERR_UNCLOSED_BRACE",
            "Unclosed opening brace '{' in source script",
        ));
    }
    if paren_count != 0 {
        return Err(CompileError::simple(
            "ERR_UNCLOSED_PAREN",
            "Unclosed opening parenthesis '(' in source script",
        ));
    }
    if bracket_count != 0 {
        return Err(CompileError::simple(
            "ERR_UNCLOSED_BRACKET",
            "Unclosed opening bracket '[' in source script",
        ));
    }

    // 2. Line-by-line formatting
    let mut out = String::new();
    let mut depth: usize = 0;
    let mut last_was_empty = true; // suppress initial empty lines

    let lines: Vec<&str> = src.lines().collect();

    for raw_line in lines {
        let (code_part, comment_opt) = split_line_comment(raw_line);
        let trimmed_code = code_part.trim();
        let trimmed_comment = comment_opt.map(|c| c.trim());

        // Blank line check
        if trimmed_code.is_empty() && trimmed_comment.is_none() {
            if !last_was_empty {
                out.push('\n');
                last_was_empty = true;
            }
            continue;
        }

        // Pure comment line
        if trimmed_code.is_empty() {
            if let Some(comment) = trimmed_comment {
                let indent_spaces = depth * 4;
                for _ in 0..indent_spaces {
                    out.push(' ');
                }
                out.push_str(comment);
                out.push('\n');
                last_was_empty = false;
            }
            continue;
        }

        // Code line (with or without trailing comment)
        let (open_braces, close_braces) = count_braces(trimmed_code);
        let (open_brackets, close_brackets) = count_brackets(trimmed_code);

        let starts_with_rbrace = trimmed_code.starts_with('}');
        let starts_with_rbracket = trimmed_code.starts_with(']');

        // Check for contract clause alignment (@wcet, @requires, @ensures, @invariant)
        let is_contract_clause = trimmed_code.starts_with("@wcet(")
            || trimmed_code.starts_with("@requires(")
            || trimmed_code.starts_with("@ensures(")
            || trimmed_code.starts_with("@invariant(");

        let line_depth = if starts_with_rbrace || starts_with_rbracket {
            depth.saturating_sub(1)
        } else if is_contract_clause {
            depth + 1
        } else {
            depth
        };

        let line_tokens = tokenize_line(trimmed_code);
        let formatted_code = format_tokens(&line_tokens);

        for _ in 0..line_depth * 4 {
            out.push(' ');
        }
        out.push_str(&formatted_code);

        if let Some(comment) = trimmed_comment {
            out.push(' ');
            out.push_str(comment);
        }

        out.push('\n');
        last_was_empty = false;

        // Update block depth for subsequent lines
        depth = depth
            .saturating_sub(close_braces)
            .saturating_sub(close_brackets)
            + open_braces
            + open_brackets;
    }

    // Ensure clean terminating newline
    if !out.ends_with('\n') {
        out.push('\n');
    }

    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_indentation_and_blocks() {
        let unformatted = r#"
fn compute($val) {
let $x = 10;
if ($val > 0) {
$x += 5;
} else {
$x -= 5;
}
return $x;
}
"#;
        let formatted = format_source(unformatted).expect("Formatting failed");
        let expected = "\
fn compute($val) {
    let $x = 10;
    if ($val > 0) {
        $x += 5;
    } else {
        $x -= 5;
    }
    return $x;
}
";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_format_operators_and_colons() {
        let unformatted = "let mut $x:i64=10+20*3; let $b=$x==70; let $gain:fixed<16>=1.5; $arr[0]:= -$x;";
        let formatted = format_source(unformatted).expect("Formatting failed");
        assert_eq!(
            formatted,
            "let mut $x: i64 = 10 + 20 * 3; let $b = $x == 70; let $gain: fixed<16> = 1.5; $arr[0] := -$x;\n"
        );
    }

    #[test]
    fn test_format_preserves_comments_and_strings() {
        let unformatted = r#"
// Top-level comment
let $msg = "hello   world   // not a comment";   // trailing comment


let $y = 42;
"#;
        let formatted = format_source(unformatted).expect("Formatting failed");
        let expected = "\
// Top-level comment
let $msg = \"hello   world   // not a comment\"; // trailing comment

let $y = 42;
";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_format_contract_clause_alignment() {
        let unformatted = r#"
@contract: @wcet(50us) @budget(200us);

fn safe_div($n, $d) -> i64
@requires($d != 0)
@ensures($result * $d <= $n)
{
return $n / $d;
}
"#;
        let formatted = format_source(unformatted).expect("Formatting failed");
        let expected = "\
@contract: @wcet(50us) @budget(200us);

fn safe_div($n, $d) -> i64
    @requires($d != 0)
    @ensures($result * $d <= $n)
{
    return $n / $d;
}
";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_format_syntax_error_detection() {
        let unclosed_string = "let $x = \"hello world;\n";
        assert!(format_source(unclosed_string).is_err());

        let unmatched_brace = "fn foo() { let $x = 10; ";
        assert!(format_source(unmatched_brace).is_err());
    }
}
