//! PulseLang compilation and runtime error diagnostic types and formatters

use crate::token::TokenKind;

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
    pub fn is_runtime(&self) -> bool {
        self.code.starts_with("ERR_PX64_")
            || self.code.starts_with("ERR_BINARY_")
            || self.code.starts_with("ERR_VM_")
    }

    /// Format standard structured AI-actionable diagnostic output into a formatter/writer.
    pub fn format_diagnostic<W: core::fmt::Write>(
        &self,
        src: &[u8],
        filename: &str,
        mut w: W,
    ) -> core::fmt::Result {
        if self.is_runtime() {
            return self.format_runtime_diagnostic(filename, w);
        }

        writeln!(w, "==================== [PULSELANG COMPILE ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================")?;
        writeln!(w, "[ERROR_CODE]: {}", self.code)?;
        writeln!(w, "[MESSAGE]: {}", self.message)?;
        writeln!(w, "[FILE]: {}", filename)?;
        writeln!(w, "[LOCATION]: Line {}, Column {} (ByteOffset: {})", self.line, self.col, self.byte_offset)?;

        write!(w, "[TOKEN_FOUND]: Kind: {:?}, Value: \"", self.token_kind)?;
        if self.byte_offset + self.token_len <= src.len() && self.token_len > 0 {
            if let Ok(tok_str) = core::str::from_utf8(&src[self.byte_offset..self.byte_offset + self.token_len]) {
                write!(w, "{}", tok_str)?;
            }
        }
        writeln!(w, "\"")?;

        writeln!(w, "[EXPECTED]: {}", self.expected)?;
        writeln!(w, "[PARSER_STAGE]: {}", self.stage)?;
        writeln!(w, "[SOURCE_CONTEXT]:")?;

        format_source_context_lines(src, self.line, self.col, self.token_len, &mut w, false)?;

        let hex_start = (self.byte_offset.saturating_sub(16) / 16) * 16;
        let hex_end = core::cmp::min(hex_start + 32, src.len());
        writeln!(w, "[HEX_DUMP (offset 0x{:04x}..0x{:04x})]:", hex_start, hex_end)?;
        format_byte_hex_dump(src, hex_start, hex_end, &mut w)?;

        writeln!(w, "[AI_REPAIR_HINT]: {}", self.suggestion)?;
        writeln!(w, "=============================================================================================")?;
        Ok(())
    }

    /// Format ANSI colored diagnostic output for terminal CLI tools (e.g. `pulc`).
    pub fn format_diagnostic_ansi<W: core::fmt::Write>(
        &self,
        src: &[u8],
        filename: &str,
        mut w: W,
    ) -> core::fmt::Result {
        // ANSI escape codes
        let bold = "\x1b[1m";
        let red = "\x1b[1;31m";
        let green = "\x1b[1;32m";
        let yellow = "\x1b[1;33m";
        let cyan = "\x1b[1;36m";
        let reset = "\x1b[0m";

        if self.is_runtime() {
            writeln!(w, "{red}==================== [PULSELANG RUNTIME ERROR DIAGNOSTIC] ===================={reset}")?;
            writeln!(w, "{bold}[ERROR_CODE]:{reset} {red}{}{reset}", self.code)?;
            writeln!(w, "{bold}[MESSAGE]:{reset} {}", self.message)?;
            writeln!(w, "{bold}[FILE]:{reset} {}", filename)?;
            writeln!(w, "{bold}[AI_REPAIR_HINT]:{reset} {yellow}{}{reset}", self.suggestion)?;
            writeln!(w, "{red}=============================================================================={reset}")?;
            return Ok(());
        }

        writeln!(w, "{red}{bold}error[{}]{reset}: {}", self.code, self.message)?;
        writeln!(w, "  {cyan}-->{reset} {}:{}:{}", filename, self.line, self.col)?;
        writeln!(w, "   {cyan}|{reset}")?;

        format_source_context_lines(src, self.line, self.col, self.token_len, &mut w, true)?;

        writeln!(w, "   {cyan}|{reset}")?;
        writeln!(w, "   {cyan}={reset} {bold}expected:{reset} {}", self.expected)?;
        writeln!(w, "   {cyan}={reset} {bold}stage:{reset} {}", self.stage)?;
        writeln!(w, "   {green}={reset} {bold}{green}ai-repair-hint:{reset} {yellow}{}{reset}", self.suggestion)?;
        Ok(())
    }

    /// Format JSON diagnostic output for tooling and IDEs.
    pub fn format_json<W: core::fmt::Write>(&self, filename: &str, mut w: W) -> core::fmt::Result {
        write!(
            w,
            r#"{{"success":false,"error":{{"code":"{}","message":"{}","file":"{}","line":{},"col":{},"byte_offset":{},"token_kind":"{:?}","expected":"{}","stage":"{}","suggestion":"{}"}}}}"#,
            escape_json_str(self.code),
            escape_json_str(self.message),
            escape_json_str(filename),
            self.line,
            self.col,
            self.byte_offset,
            self.token_kind,
            escape_json_str(self.expected),
            escape_json_str(self.stage),
            escape_json_str(self.suggestion),
        )
    }

    fn format_runtime_diagnostic<W: core::fmt::Write>(
        &self,
        filename: &str,
        mut w: W,
    ) -> core::fmt::Result {
        writeln!(w, "==================== [PULSELANG RUNTIME ERROR DIAGNOSTIC (AI-ACTIONABLE)] ====================")?;
        writeln!(w, "[ERROR_CODE]: {}", self.code)?;
        writeln!(w, "[MESSAGE]: {}", self.message)?;
        writeln!(w, "[FILE]: {}", filename)?;
        writeln!(w, "[EXECUTION_DOMAIN]: px64 Real-Time Register Virtual Machine")?;

        match self.code {
            "ERR_PX64_TIMEOUT_EXCEEDED" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Wall-Clock Watchdog Deadline Violation")?;
                writeln!(w, "[TIMEOUT_LIMIT]: 5,000,000 ns (5.0 ms wall-clock)")?;
                writeln!(w, "[ROOT_CAUSE]: Script execution exceeded 5.0ms wall-clock threshold (infinite loop or long-running intrinsics)")?;
                writeln!(w, "[AI_REPAIR_HINT]: Bound while loops with finite counter or insert @within temporal deadline guards")?;
            }
            "ERR_PX64_WCET_EXCEEDED" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Instruction Step Limit Exceeded")?;
                writeln!(w, "[STEP_LIMIT]: 10,000 instruction steps (MAX_VM_STEPS)")?;
                writeln!(w, "[ROOT_CAUSE]: Pure arithmetic or branching loop executed without terminating within 10,000 steps")?;
                writeln!(w, "[AI_REPAIR_HINT]: Ensure loop condition decrements towards termination condition within 10,000 steps")?;
            }
            "ERR_BINARY_VERSION_MISMATCH" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Binary Version Incompatibility")?;
                writeln!(w, "[EXPECTED_VERSION]: PX64 Version 3")?;
                writeln!(w, "[ROOT_CAUSE]: Binary was compiled with an incompatible or outdated toolchain version")?;
                writeln!(w, "[AI_REPAIR_HINT]: Recompile source file with 'compile <src.pul> <dst.bin>'")?;
            }
            "ERR_BINARY_TRUNCATED" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Truncated Binary Payload")?;
                writeln!(w, "[ROOT_CAUSE]: Binary file payload is smaller than declared header code + string pool + const pool length")?;
                writeln!(w, "[AI_REPAIR_HINT]: Re-generate binary artifact or check file system storage integrity")?;
            }
            "ERR_PX64_CONST_OUT_OF_BOUNDS" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Constant Pool Access Violation")?;
                writeln!(w, "[ROOT_CAUSE]: Instruction attempted to load from an invalid 64-bit constant pool index")?;
                writeln!(w, "[AI_REPAIR_HINT]: Recompile source file or inspect binary with 'disasm <file.bin>'")?;
            }
            "ERR_PX64_ARRAY_OUT_OF_BOUNDS" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Fixed-Length Array Boundary Violation")?;
                writeln!(w, "[ROOT_CAUSE]: Array index expression evaluated to an index outside [0..N-1]")?;
                writeln!(w, "[AI_REPAIR_HINT]: Ensure array indexing expression is bounded with a static for loop (0..N) or bounds check")?;
            }
            "ERR_PX64_STRUCT_OUT_OF_BOUNDS" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Static Struct Field Access Violation")?;
                writeln!(w, "[ROOT_CAUSE]: Struct field offset or instance ID is out of bounds")?;
                writeln!(w, "[AI_REPAIR_HINT]: Verify struct field definitions and ensure instance ID is within [0..7]")?;
            }
            "ERR_PX64_TABLE_OUT_OF_BOUNDS" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Read-Only Const Table Boundary Violation")?;
                writeln!(w, "[ROOT_CAUSE]: Const table lookup index evaluated to an index outside [0..N-1]")?;
                writeln!(w, "[AI_REPAIR_HINT]: Bound table lookup index with a static range for loop (0..N) or check bounds")?;
            }
            "ERR_PX64_ASSERTION_FAILED" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Runtime Assertion Contract Failure")?;
                writeln!(w, "[ROOT_CAUSE]: @assert() condition evaluated to false (0)")?;
                writeln!(w, "[AI_REPAIR_HINT]: Check preceding computational pipeline and verify expected state invariants")?;
            }
            "ERR_PX64_STACK_OVERFLOW" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Static Call Stack Overflow Violation")?;
                writeln!(w, "[STACK_DEPTH_LIMIT]: 8 nested function call frames (MAX_CALL_DEPTH)")?;
                writeln!(w, "[ROOT_CAUSE]: Recursion or nested call depth exceeded static 8-frame call stack")?;
                writeln!(w, "[AI_REPAIR_HINT]: Eliminate recursive function calls or refactor into static bounded for-loops")?;
            }
            "ERR_PX64_UNWRAP_FAILED" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Tagged Result Unwrap Fault")?;
                writeln!(w, "[ROOT_CAUSE]: Attempted to unwrap an Err tagged value without checking @is_ok()")?;
                writeln!(w, "[AI_REPAIR_HINT]: Guard @unwrap($res) with 'if (@is_ok($res))' check")?;
            }
            "ERR_PX64_DIV_BY_ZERO" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Division or Modulo by Zero Fault")?;
                writeln!(w, "[ROOT_CAUSE]: Division (/) or modulo (%) operation attempted with a zero divisor")?;
                writeln!(w, "[AI_REPAIR_HINT]: Ensure divisor variable is non-zero before dividing or computing modulo")?;
            }
            "ERR_PX64_INVALID_OPCODE" => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Invalid Opcode Execution Fault")?;
                writeln!(w, "[ROOT_CAUSE]: Virtual machine encountered an unrecognized or unregistered instruction opcode")?;
                writeln!(w, "[AI_REPAIR_HINT]: Verify compiler code generator or inspect bytecode with 'disasm <file.bin>'")?;
            }
            _ => {
                writeln!(w, "[RUNTIME_FAULT_CATEGORY]: Virtual Machine Execution Fault")?;
                writeln!(w, "[ROOT_CAUSE]: Virtual machine execution fault or internal VM state corruption")?;
                writeln!(w, "[AI_REPAIR_HINT]: Recompile source file or inspect binary with 'disasm <file.bin>'")?;
            }
        }
        writeln!(w, "=============================================================================================")?;
        Ok(())
    }
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}: {} at line {}, col {}",
            self.code, self.message, self.line, self.col
        )
    }
}
impl From<core::fmt::Error> for CompileError {
    fn from(_: core::fmt::Error) -> Self {
        CompileError::simple("ERR_FMT_ERROR", "Formatting or buffer write error")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CompileError {}

fn format_source_context_lines<W: core::fmt::Write>(
    src: &[u8],
    error_line: usize,
    error_col: usize,
    tok_len: usize,
    w: &mut W,
    ansi: bool,
) -> core::fmt::Result {
    let mut cur_line = 1;
    let mut line_start = 0;
    let mut i = 0;

    let cyan = if ansi { "\x1b[1;36m" } else { "" };
    let red = if ansi { "\x1b[1;31m" } else { "" };
    let yellow = if ansi { "\x1b[1;33m" } else { "" };
    let reset = if ansi { "\x1b[0m" } else { "" };

    while i <= src.len() {
        if i == src.len() || src[i] == b'\n' {
            let line_end = i;
            if cur_line + 1 >= error_line && cur_line <= error_line + 1 {
                let line_bytes = &src[line_start..line_end];
                let line_str = core::str::from_utf8(line_bytes).unwrap_or("");
                if cur_line == error_line {
                    if ansi {
                        writeln!(w, "{cyan}{:4} |{reset} {}", cur_line, line_str)?;
                        write!(w, "     {cyan}|{reset} ")?;
                        let caret_col = error_col.saturating_sub(1);
                        for _ in 0..caret_col {
                            write!(w, " ")?;
                        }
                        let caret_len = core::cmp::max(tok_len, 1);
                        for _ in 0..caret_len {
                            write!(w, "{red}^{reset}")?;
                        }
                        writeln!(w, " {yellow}[Syntax Error Here]{reset}")?;
                    } else {
                        writeln!(w, "> Line {:3}: {}", cur_line, line_str)?;
                        write!(w, "         ")?;
                        let caret_col = error_col.saturating_sub(1);
                        for _ in 0..caret_col {
                            write!(w, " ")?;
                        }
                        let caret_len = core::cmp::max(tok_len, 1);
                        for _ in 0..caret_len {
                            write!(w, "^")?;
                        }
                        writeln!(w, " [Syntax Error Here]")?;
                    }
                } else if ansi {
                    writeln!(w, "{cyan}{:4} |{reset} {}", cur_line, line_str)?;
                } else {
                    writeln!(w, "  Line {:3}: {}", cur_line, line_str)?;
                }
            }
            cur_line += 1;
            line_start = i + 1;
        }
        i += 1;
    }
    Ok(())
}

fn format_byte_hex_dump<W: core::fmt::Write>(
    src: &[u8],
    start: usize,
    end: usize,
    w: &mut W,
) -> core::fmt::Result {
    let mut row_start = start;
    while row_start < end {
        let row_end = core::cmp::min(row_start + 16, src.len());
        write!(w, "  {:08x}: ", row_start)?;

        for j in 0..16 {
            if row_start + j < row_end {
                write!(w, "{:02x} ", src[row_start + j])?;
            } else {
                write!(w, "   ")?;
            }
        }
        write!(w, " |")?;
        for j in 0..16 {
            if row_start + j < row_end {
                let b = src[row_start + j];
                if (0x20..=0x7E).contains(&b) {
                    write!(w, "{}", b as char)?;
                } else {
                    write!(w, ".")?;
                }
            }
        }
        writeln!(w, "|")?;
        row_start += 16;
    }
    Ok(())
}

fn escape_json_str(s: &str) -> JsonEscaped<'_> {
    JsonEscaped(s)
}

struct JsonEscaped<'a>(&'a str);

impl<'a> core::fmt::Display for JsonEscaped<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for c in self.0.chars() {
            match c {
                '"' => write!(f, "\\\"")?,
                '\\' => write!(f, "\\\\")?,
                '\n' => write!(f, "\\n")?,
                '\r' => write!(f, "\\r")?,
                '\t' => write!(f, "\\t")?,
                c => write!(f, "{}", c)?,
            }
        }
        Ok(())
    }
}
