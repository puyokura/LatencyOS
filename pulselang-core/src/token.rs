//! Token definitions and source location helpers for PulseLang

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
    AtPoolSize,
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

/// Calculate 1-indexed line and column from source byte position.
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
