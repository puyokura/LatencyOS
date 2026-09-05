//! # PulseLang Core
//!
//! PulseLang v2: AI-Native Temporal Reactive DSL, Compiler & Bytecode Toolchain for LatencyOS.
//! Designed for zero dynamic memory allocation in `no_std` environments, with high-level APIs
//! available when `alloc` or `std` is enabled.

#![no_std]

#[cfg(any(feature = "alloc", test))]
extern crate alloc;

#[cfg(any(feature = "std", test))]
extern crate std;

pub mod compiler;
pub mod disasm;
pub mod error;
pub mod isa;
pub mod lexer;
pub mod token;
pub mod vm;
#[cfg(any(feature = "alloc", test))]
pub mod include;
pub use compiler::{
    ArrayMeta, CompileStats, Compiler, ConstTableMeta, EnumDefMeta, FnMeta, HandleState,
    StructDefMeta, StructFieldMeta, StructInstMeta,
};
pub use disasm::{disassemble_px64, disassemble_px64_with_filename};
#[cfg(any(feature = "alloc", test))]
pub use disasm::{disasm, disasm_with_filename};
pub use error::CompileError;
pub use isa::*;
pub use lexer::Lexer;
pub use token::{get_line_and_col, Token, TokenKind};
pub use vm::{compute_crc32, run_binary, run_binary_with_output, NullWriter, PX64VM};
#[cfg(any(feature = "std", test))]
pub use vm::StdoutWriter;
#[cfg(any(feature = "alloc", test))]
pub use vm::{run_source, run_source_with_output};
#[cfg(any(feature = "alloc", test))]
pub use include::preprocess_includes;
/// Compile PulseLang source code into a binary px64 bytecode buffer (zero-heap `no_std` API).
///
/// Returns the number of bytes written to `out_buf`.
pub fn compile_pulse_to_binary(src: &[u8], out_buf: &mut [u8]) -> Result<usize, CompileError> {
    #[cfg(feature = "alloc")]
    {
        let mut tokens = alloc::vec![Token::empty(); MAX_TOKENS];
        compile_pulse_to_binary_with_tokens(src, &mut tokens, out_buf)
    }
    #[cfg(all(not(feature = "alloc"), test))]
    {
        std::thread_local! {
            static NO_STD_TOKENS: core::cell::RefCell<[Token; MAX_TOKENS]> = core::cell::RefCell::new([Token::empty(); MAX_TOKENS]);
        }
        NO_STD_TOKENS.with(|cell| {
            let mut tokens = cell.borrow_mut();
            compile_pulse_to_binary_with_tokens(src, &mut tokens[..], out_buf)
        })
    }
    #[cfg(all(not(feature = "alloc"), not(test)))]
    {
        static mut NO_STD_TOKENS: [Token; 512] = [Token::empty(); 512];
        let tokens = unsafe { &mut *(&raw mut NO_STD_TOKENS) };
        compile_pulse_to_binary_with_tokens(src, tokens, out_buf)
    }
}

/// Compile PulseLang source code into a binary px64 bytecode buffer using a caller-provided token slice.
pub fn compile_pulse_to_binary_with_tokens(
    src: &[u8],
    tokens: &mut [Token],
    out_buf: &mut [u8],
) -> Result<usize, CompileError> {
    let mut lexer = Lexer::new(src);
    let tok_count = lexer.tokenize(tokens)?;

    let mut compiler = Compiler::new(src, &tokens[..=tok_count]);
    let code_len = compiler.compile()?;

    let str_pool_len = compiler.str_pool_len;
    let const_pool_count = compiler.const_pool_len;
    let const_pool_bytes = const_pool_count * 8;

    let total_size = PX64_HEADER_SIZE + code_len + str_pool_len + const_pool_bytes;
    if total_size > out_buf.len() {
        return Err(CompileError::simple(
            "ERR_BINARY_BUFFER_OVERFLOW",
            "Output buffer too small for compiled px64 binary payload",
        ));
    }

    // Header (16 bytes)
    out_buf[0..4].copy_from_slice(&PX64_BIN_MAGIC); // b"PX64"
    out_buf[4..6].copy_from_slice(&PX64_BIN_VERSION.to_be_bytes()); // 3
    out_buf[6..8].copy_from_slice(&(code_len as u16).to_be_bytes());
    out_buf[8..10].copy_from_slice(&(str_pool_len as u16).to_be_bytes());
    out_buf[10..12].copy_from_slice(&(const_pool_count as u16).to_be_bytes());
    out_buf[12..14].copy_from_slice(&(PX64_NUM_REGISTERS as u16).to_be_bytes()); // 20
    out_buf[14..16].copy_from_slice(&[0u8, 0u8]); // Reserved

    // Payload: Code + String Pool + Constant Pool
    out_buf[PX64_HEADER_SIZE..PX64_HEADER_SIZE + code_len]
        .copy_from_slice(&compiler.code[..code_len]);
    out_buf[PX64_HEADER_SIZE + code_len..PX64_HEADER_SIZE + code_len + str_pool_len]
        .copy_from_slice(&compiler.str_pool[..str_pool_len]);

    let const_start = PX64_HEADER_SIZE + code_len + str_pool_len;
    for (i, &c) in compiler.const_pool[..const_pool_count].iter().enumerate() {
        out_buf[const_start + i * 8..const_start + (i + 1) * 8].copy_from_slice(&c.to_be_bytes());
    }

    Ok(total_size)
}

/// Compile PulseLang source code into a newly-allocated bytecode Vector (`alloc`/`std` API).
#[cfg(any(feature = "alloc", test))]
pub fn compile(src: &str) -> Result<alloc::vec::Vec<u8>, CompileError> {
    let max_size = PX64_HEADER_SIZE + MAX_BYTECODE_SIZE + MAX_STRING_POOL + MAX_CONST_POOL * 8;
    let mut buf = alloc::vec![0u8; max_size];
    let size = compile_pulse_to_binary(src.as_bytes(), &mut buf)?;
    buf.truncate(size);
    Ok(buf)
}

/// Validate syntax, type ownership, WCET constraints and calculate compilation statistics (`alloc`/`std` API).
#[cfg(any(feature = "alloc", test))]
pub fn check(src: &str) -> Result<CompileStats, CompileError> {
    let mut tokens = alloc::vec![Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src.as_bytes());
    let tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src.as_bytes(), &tokens[..=tok_count]);
    let _code_len = compiler.compile()?;
    Ok(compiler.stats())
}

/// Compiled unit test case with metadata and runnable bytecode payload.
#[cfg(any(feature = "alloc", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCaseCompiled {
    pub name: alloc::string::String,
    pub line: usize,
    pub budget_ns: Option<u64>,
    pub bytecode: alloc::vec::Vec<u8>,
}

/// Result of executing a unit test case in the px64 VM.
#[cfg(any(feature = "alloc", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestExecutionResult {
    pub passed: bool,
    pub error: Option<alloc::string::String>,
    pub steps: usize,
    pub elapsed_ns: u64,
    pub budget_ns: Option<u64>,
}

/// Compile all `@test` blocks in PulseLang source script into runnable unit test cases.
#[cfg(any(feature = "alloc", test))]
pub fn compile_pulse_tests(src: &[u8]) -> Result<alloc::vec::Vec<TestCaseCompiled>, CompileError> {
    let mut tokens = alloc::vec![Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src);
    let tok_count = lexer.tokenize(&mut tokens)?;

    // Pass 1: Find all @test blocks
    struct TestBlockInfo {
        name: alloc::string::String,
        line: usize,
        budget_ns: Option<u64>,
        body_start_tok: usize,
        body_end_tok: usize,
        test_tok_start: usize,
        test_tok_end: usize,
    }

    let mut test_blocks = alloc::vec::Vec::new();
    let mut i = 0;
    while i <= tok_count {
        if tokens[i].kind == TokenKind::AtTest {
            let test_tok_start = i;
            let line = tokens[i].line;
            i += 1;
            let mut name = alloc::string::String::new();
            if i <= tok_count && tokens[i].kind == TokenKind::StringLit {
                let tok = tokens[i];
                if tok.len >= 2 {
                    let s_bytes = &src[tok.start + 1..tok.start + tok.len - 1];
                    name = alloc::string::String::from_utf8_lossy(s_bytes).into_owned();
                }
                i += 1;
            }

            let mut budget_ns = None;
            if i <= tok_count && (tokens[i].kind == TokenKind::AtBudget || tokens[i].kind == TokenKind::Budget) {
                i += 1;
                if i <= tok_count && tokens[i].kind == TokenKind::LParen {
                    i += 1;
                    if i <= tok_count {
                        match tokens[i].kind {
                            TokenKind::TimeLiteral(ns) => budget_ns = Some(ns),
                            TokenKind::Number(n) if n >= 0 => budget_ns = Some(n as u64),
                            _ => {}
                        }
                        i += 1;
                    }
                    if i <= tok_count && tokens[i].kind == TokenKind::RParen {
                        i += 1;
                    }
                }
            }

            if i <= tok_count && tokens[i].kind == TokenKind::LBrace {
                let body_start_tok = i + 1;
                let mut depth = 1usize;
                i += 1;
                while i <= tok_count && depth > 0 {
                    match tokens[i].kind {
                        TokenKind::LBrace => depth += 1,
                        TokenKind::RBrace => depth -= 1,
                        TokenKind::Eof => break,
                        _ => {}
                    }
                    i += 1;
                }
                let body_end_tok = i - 1; // index of RBrace
                let test_tok_end = i;
                test_blocks.push(TestBlockInfo {
                    name,
                    line,
                    budget_ns,
                    body_start_tok,
                    body_end_tok,
                    test_tok_start,
                    test_tok_end,
                });
            }
        } else {
            i += 1;
        }
    }

    let mut compiled_tests = alloc::vec::Vec::new();

    for test in test_blocks {
        // Synthesize token stream: all tokens outside @test blocks, plus the statements inside this @test block, plus EOF
        let mut test_tokens = alloc::vec::Vec::new();
        let mut idx = 0;
        while idx <= tok_count {
            if idx == test.test_tok_start {
                // Insert this test's body tokens
                for b_idx in test.body_start_tok..test.body_end_tok {
                    test_tokens.push(tokens[b_idx]);
                }
                idx = test.test_tok_end;
            } else {
                test_tokens.push(tokens[idx]);
                idx += 1;
            }
        }
        if test_tokens.is_empty() || test_tokens.last().map(|t| t.kind) != Some(TokenKind::Eof) {
            test_tokens.push(Token {
                kind: TokenKind::Eof,
                start: src.len(),
                len: 0,
                line: test.line,
                col: 1,
            });
        }

        let mut compiler = Compiler::new(src, &test_tokens);
        let code_len = compiler.compile()?;

        let str_pool_len = compiler.str_pool_len;
        let const_pool_count = compiler.const_pool_len;
        let const_pool_bytes = const_pool_count * 8;
        let total_size = PX64_HEADER_SIZE + code_len + str_pool_len + const_pool_bytes;

        let mut bin = alloc::vec![0u8; total_size];
        bin[0..4].copy_from_slice(&PX64_BIN_MAGIC);
        bin[4..6].copy_from_slice(&PX64_BIN_VERSION.to_be_bytes());
        bin[6..8].copy_from_slice(&(code_len as u16).to_be_bytes());
        bin[8..10].copy_from_slice(&(str_pool_len as u16).to_be_bytes());
        bin[10..12].copy_from_slice(&(const_pool_count as u16).to_be_bytes());
        bin[12..14].copy_from_slice(&(PX64_NUM_REGISTERS as u16).to_be_bytes());
        bin[14..16].copy_from_slice(&[0u8, 0u8]);

        bin[PX64_HEADER_SIZE..PX64_HEADER_SIZE + code_len].copy_from_slice(&compiler.code[..code_len]);
        bin[PX64_HEADER_SIZE + code_len..PX64_HEADER_SIZE + code_len + str_pool_len]
            .copy_from_slice(&compiler.str_pool[..str_pool_len]);
        let const_start = PX64_HEADER_SIZE + code_len + str_pool_len;
        for (c_i, &c) in compiler.const_pool[..const_pool_count].iter().enumerate() {
            bin[const_start + c_i * 8..const_start + (c_i + 1) * 8].copy_from_slice(&c.to_be_bytes());
        }

        compiled_tests.push(TestCaseCompiled {
            name: test.name,
            line: test.line,
            budget_ns: test.budget_ns,
            bytecode: bin,
        });
    }

    Ok(compiled_tests)
}

#[cfg(any(feature = "alloc", test))]
pub fn run_test_case(test: &TestCaseCompiled) -> TestExecutionResult {
    let bin = &test.bytecode;
    if bin.len() < PX64_HEADER_SIZE || bin[0..4] != PX64_BIN_MAGIC {
        return TestExecutionResult {
            passed: false,
            error: Some(alloc::string::String::from("Invalid px64 binary payload")),
            steps: 0,
            elapsed_ns: 0,
            budget_ns: test.budget_ns,
        };
    }

    let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
    let str_pool_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;
    let const_count = u16::from_be_bytes([bin[10], bin[11]]) as usize;
    let const_bytes = const_count * 8;

    if bin.len() < PX64_HEADER_SIZE + code_len + str_pool_len + const_bytes {
        return TestExecutionResult {
            passed: false,
            error: Some(alloc::string::String::from("Truncated px64 binary payload")),
            steps: 0,
            elapsed_ns: 0,
            budget_ns: test.budget_ns,
        };
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

    let mut vm = PX64VM::new(code, str_pool, &const_pool[..count], &[]);
    let mut null_writer = NullWriter;

    #[cfg(feature = "std")]
    let start_time = std::time::Instant::now();

    let exec_res = vm.run_with_output(&mut null_writer);

    #[cfg(feature = "std")]
    let elapsed_ns = start_time.elapsed().as_nanos() as u64;
    #[cfg(not(feature = "std"))]
    let elapsed_ns = (vm.steps as u64).saturating_mul(15);

    let steps = vm.steps;

    match exec_res {
        Ok(()) => {
            if let Some(budget) = test.budget_ns {
                if elapsed_ns > budget {
                    return TestExecutionResult {
                        passed: false,
                        error: Some(alloc::format!(
                            "Test exceeded budget: elapsed {}ns > budget {}ns",
                            elapsed_ns, budget
                        )),
                        steps,
                        elapsed_ns,
                        budget_ns: test.budget_ns,
                    };
                }
            }
            TestExecutionResult {
                passed: true,
                error: None,
                steps,
                elapsed_ns,
                budget_ns: test.budget_ns,
            }
        }
        Err(err) => TestExecutionResult {
            passed: false,
            error: Some(alloc::format!("{}: {}", err.code, err.message)),
            steps,
            elapsed_ns,
            budget_ns: test.budget_ns,
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    extern crate std;

    #[test]
    fn test_compile_arithmetic() {
        let src = "let $x = 10 + 20 * 3;\n";
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Compilation failed");
        assert!(size >= PX64_HEADER_SIZE);
        assert_eq!(&buf[0..4], &PX64_BIN_MAGIC);
    }

    #[test]
    fn test_compile_contracts_and_directives() {
        let src = r#"
            @contract: latency <= 50us;
            let $a = 100;
            @assert($a == 100);
        "#;
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Contract compilation failed");
        assert!(size > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_compile_for_loop_and_bounds() {
        let src = r#"
            let mut $sum = 0;
            for $i in 0..10 {
                $sum += $i;
            }
        "#;
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("For loop compilation failed");
        assert!(size > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_compile_unbounded_while_rejected() {
        let src = r#"
            @while(1) {
                let $x = 1;
            }
        "#;
        let mut buf = [0u8; 1024];
        let err = compile_pulse_to_binary(src.as_bytes(), &mut buf).unwrap_err();
        assert_eq!(err.code, "ERR_UNBOUNDED_LOOP");
    }

    #[test]
    fn test_linear_type_ownership_success() {
        let src = r#"
            #f0 := @capture();
            @send(#f0);
        "#;
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Linear type valid flow");
        assert!(size > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_linear_type_unconsumed_rejected() {
        let src = r#"
            #f0 := @capture();
        "#;
        let mut buf = [0u8; 1024];
        let err = compile_pulse_to_binary(src.as_bytes(), &mut buf).unwrap_err();
        assert_eq!(err.code, "ERR_LINEAR_UNCONSUMED_HANDLE");
    }

    #[test]
    fn test_linear_type_double_send_rejected() {
        let src = r#"
            #f0 := @capture();
            @send(#f0);
            @send(#f0);
        "#;
        let mut buf = [0u8; 1024];
        let err = compile_pulse_to_binary(src.as_bytes(), &mut buf).unwrap_err();
        assert_eq!(err.code, "ERR_LINEAR_DOUBLE_SEND");
    }

    #[test]
    fn test_linear_type_overwrite_rejected() {
        let src = r#"
            #f0 := @capture();
            #f0 := @capture();
            @send(#f0);
        "#;
        let mut buf = [0u8; 1024];
        let err = compile_pulse_to_binary(src.as_bytes(), &mut buf).unwrap_err();
        assert_eq!(err.code, "ERR_LINEAR_OVERWRITE");
    }

    #[test]
    fn test_struct_definition_and_access() {
        let src = r#"
            struct Point {
                x: i64,
                y: i64,
            };
            let $pt: Point;
            $pt.x := 42;
            $pt.y := 84;
            let $px = $pt.x;
        "#;
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Struct compilation failed");
        assert!(size > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_const_table_lookup() {
        let src = r#"
            const LUT: [i64; 4] = [10, 20, 30, 40];
            let $val = LUT[2];
        "#;
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Const table compilation failed");
        assert!(size > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_match_pattern() {
        let src = r#"
            let $res = @ok(123);
            match $res {
                Ok($val) => {
                    let $x = $val;
                },
                Err($code) => {
                    let $err = $code;
                },
            };
        "#;
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Match compilation failed");
        assert!(size > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_disassembly_output() {
        let src = "let $x = 115;\n";
        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Compilation failed");

        let mut disasm_text = alloc::string::String::new();
        disassemble_px64_with_filename(&buf[..size], "test.bin", &mut disasm_text)
            .expect("Disassembly failed");

        assert!(disasm_text.contains("PX64"));
        assert!(disasm_text.contains("MOV"));
        assert!(disasm_text.contains("115"));
    }

    #[test]
    fn test_check_api() {
        let src = r#"
            let mut $acc = 0;
            for $i in 0..5 {
                $acc += $i;
            }
        "#;
        let stats = check(src).expect("Check failed");
        assert!(stats.code_size > 0);
        assert!(stats.instruction_count > 0);
    }

    #[test]
    fn test_compile_function_decl_and_call() {
        let src = r#"
            fn sum($a, $b) {
                return $a + $b;
            }
            let $res = sum(10, 20);
        "#;
        let bin = compile(src).expect("Function compile failed");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_compile_bitwise_and_shifts() {
        let src = r#"
            let $a = 0xFF & 0x0F;
            let $b = 1 << 4;
            let $c = $b >> 2;
            let $d = $a ^ $c;
            let $e = $a | $d;
        "#;
        let bin = compile(src).expect("Bitwise compile failed");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_compile_within_block() {
        let src = r#"
            @within(500us) {
                let $x = 123;
            } !drop;
        "#;
        let bin = compile(src).expect("Within block compile failed");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_compile_array_allocation_and_indexing() {
        let src = r#"
            let $arr: [i64; 4];
            $arr[0] := 100;
            $arr[1] := 200;
            let $elem = $arr[0];
        "#;
        let bin = compile(src).expect("Array compile failed");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_matrix_multiplication_3x3() {
        let src = r#"
            @contract: @wcet(50us) @budget(100us);
            let $a = [
                1, 2, 3,
                4, 5, 6,
                7, 8, 9
            ];
            let $b = [
                9, 8, 7,
                6, 5, 4,
                3, 2, 1
            ];
            let mut $c: [i64; 9];
            for $i in 0..3 {
                for $j in 0..3 {
                    let mut $sum = 0;
                    for $k in 0..3 {
                        let $a_index = ($i * 3) + $k;
                        let $b_index = ($k * 3) + $j;
                        $sum += $a[$a_index] * $b[$b_index];
                    }
                    let $c_index = ($i * 3) + $j;
                    $c[$c_index] := $sum;
                }
            }
            @println($c[0]);
            @println($c[1]);
            @println($c[2]);
            @println($c[3]);
            @println($c[4]);
            @println($c[5]);
            @println($c[6]);
            @println($c[7]);
            @println($c[8]);
        "#;
        let bin = compile(src).expect("Matrix multiplication compile failed");
        let mut out = alloc::string::String::new();
        run_binary_with_output(&bin, &[], &mut out).expect("Matrix multiplication run failed");
        let lines: alloc::vec::Vec<&str> = out.trim().lines().collect();
        assert_eq!(lines, &["30", "24", "18", "84", "69", "54", "138", "114", "90"]);
    }

    #[test]
    fn test_matrix_identity_multiplication() {
        let src = r#"
            let $a = [1, 2, 3, 4, 5, 6, 7, 8, 9];
            let $ident = [1, 0, 0, 0, 1, 0, 0, 0, 1];
            let mut $res: [i64; 9];
            for $i in 0..3 {
                for $j in 0..3 {
                    let mut $sum = 0;
                    for $k in 0..3 {
                        let $a_idx = ($i * 3) + $k;
                        let $b_idx = ($k * 3) + $j;
                        $sum += $a[$a_idx] * $ident[$b_idx];
                    }
                    let $out_idx = ($i * 3) + $j;
                    $res[$out_idx] := $sum;
                }
            }
            for $idx in 0..9 {
                @assert($res[$idx] == $a[$idx]);
            }
        "#;
        let bin = compile(src).expect("Identity matrix compile failed");
        run_binary(&bin, &[]).expect("Identity matrix run failed");
    }

    #[test]
    fn test_matrix_zero_multiplication() {
        let src = r#"
            let $a = [1, 2, 3, 4, 5, 6, 7, 8, 9];
            let $zero = [0, 0, 0, 0, 0, 0, 0, 0, 0];
            let mut $res: [i64; 9];
            for $i in 0..3 {
                for $j in 0..3 {
                    let mut $sum = 0;
                    for $k in 0..3 {
                        let $a_idx = ($i * 3) + $k;
                        let $b_idx = ($k * 3) + $j;
                        $sum += $a[$a_idx] * $zero[$b_idx];
                    }
                    let $out_idx = ($i * 3) + $j;
                    $res[$out_idx] := $sum;
                }
            }
            for $idx in 0..9 {
                @assert($res[$idx] == 0);
            }
        "#;
        let bin = compile(src).expect("Zero matrix compile failed");
        run_binary(&bin, &[]).expect("Zero matrix run failed");
    }

    #[test]
    fn test_array_bounds_checking() {
        let src_valid = r#"
            let $arr = [10, 20, 30, 40, 50, 60, 70, 80, 90];
            let $v0 = $arr[0];
            let $v8 = $arr[8];
        "#;
        let bin = compile(src_valid).expect("Valid array access compile failed");
        run_binary(&bin, &[]).expect("Valid array access run failed");

        let src_oob = r#"
            let $arr = [10, 20, 30, 40, 50, 60, 70, 80, 90];
            let $bad = $arr[9];
        "#;
        let bin_oob = compile(src_oob).expect("OOB compile success (runtime check)");
        let err = run_binary(&bin_oob, &[]).unwrap_err();
        assert_eq!(err.code, "ERR_PX64_ARRAY_OUT_OF_BOUNDS");
    }

    #[test]
    fn test_variable_limit_maintained() {
        // 46 distinct variables (13 GPRs + 32 spill slots + 1) should trigger ERR_MAX_VARS_EXCEEDED
        let mut src = alloc::string::String::new();
        for i in 1..=46 {
            src.push_str(&alloc::format!("let $v{} = {}; ", i, i));
        }
        let err = compile(&src).unwrap_err();
        assert_eq!(err.code, "ERR_MAX_VARS_EXCEEDED");
    }

    #[test]
    fn test_array_limit_maintained() {
        // 9 arrays should trigger ERR_MAX_ARRAYS_EXCEEDED
        let src = r#"
            let $a1: [i64; 2]; let $a2: [i64; 2]; let $a3: [i64; 2];
            let $a4: [i64; 2]; let $a5: [i64; 2]; let $a6: [i64; 2];
            let $a7: [i64; 2]; let $a8: [i64; 2]; let $a9: [i64; 2];
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_MAX_ARRAYS_EXCEEDED");

        // Array with > 256 elements should trigger ERR_ARRAY_CAPACITY_EXCEEDED
        let src_cap = r#"
            let $big: [i64; 257];
        "#;
        let err_cap = compile(src_cap).unwrap_err();
        assert_eq!(err_cap.code, "ERR_ARRAY_CAPACITY_EXCEEDED");
    }
    #[test]
    fn test_combinator_matrix_multiplication_v32() {
        let src = r#"
            @contract: @wcet(25us) @budget(50us);
            fn mul($x, $y) -> $ret {
                return $x * $y;
            }
            let $a = [
                1, 2, 3,
                4, 5, 6,
                7, 8, 9
            ];
            let $b = [
                9, 8, 7,
                6, 5, 4,
                3, 2, 1
            ];
            let mut $c: [i64; 9];
            for $i in 0..3 {
                let $row_i = @row($a, $i, 3);
                for $j in 0..3 {
                    let $col_j = @col($b, $j, 3);
                    let $dot = @zip_with($row_i, $col_j, mul) |> @sum();
                    $c[($i * 3) + $j] := $dot;
                }
            }
            @println($c[0]);
            @println($c[1]);
            @println($c[2]);
            @println($c[3]);
            @println($c[4]);
            @println($c[5]);
            @println($c[6]);
            @println($c[7]);
            @println($c[8]);
        "#;
        let bin = compile(src).expect("Combinator matrix multiplication compile failed");
        let mut out = alloc::string::String::new();
        run_binary_with_output(&bin, &[], &mut out).expect("Combinator matrix multiplication run failed");
        let lines: alloc::vec::Vec<&str> = out.trim().lines().collect();
        assert_eq!(lines, &["30", "24", "18", "84", "69", "54", "138", "114", "90"]);
    }
    #[test]
    fn test_compile_time_div_by_zero() {
        let src = "let $x = 10 / 0;\n";
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_DIV_BY_ZERO");
    }

    #[test]
    fn test_runtime_div_by_zero() {
        let src = r#"
            let $a = 10;
            let $b = 0;
            let $c = $a / $b;
        "#;
        let bin = compile(src).expect("Compilation should succeed for variable division");
        let err = run_binary(&bin, &[]).unwrap_err();
        assert_eq!(err.code, "ERR_PX64_DIV_BY_ZERO");
    }

    #[test]
    fn test_runtime_mod_by_zero() {
        let src = r#"
            let $a = 10;
            let $b = 0;
            let $c = $a % $b;
        "#;
        let bin = compile(src).expect("Compilation should succeed for variable modulo");
        let err = run_binary(&bin, &[]).unwrap_err();
        assert_eq!(err.code, "ERR_PX64_DIV_BY_ZERO");
    }
    #[test]
    fn test_else_if_chain_syntax() {
        let src = r#"
            let $val = 3;
            let mut $res = 0;
            if ($val == 1) {
                $res := 10;
            } else if ($val == 2) {
                $res := 20;
            } else if ($val == 3) {
                $res := 30;
            } else {
                $res := 99;
            }
            @assert($res == 30);
        "#;
        let bin = compile(src).expect("Else-if chain compilation failed");
        run_binary(&bin, &[]).expect("Else-if chain execution failed");
    }
    #[test]
    fn test_else_if_fizzbuzz_multi_branch() {
        let src = r#"
            @contract: @wcet(10us) @budget(100us);
            for $i in 1..16 {
                if (($i % 15) == 0) {
                    @println("FizzBuzz");
                } else if (($i % 3) == 0) {
                    @println("Fizz");
                } else if (($i % 5) == 0) {
                    @println("Buzz");
                } else {
                    @println($i);
                }
            }
        "#;
        let bin = compile(src).expect("FizzBuzz else-if chain compile failed");
        let mut out = alloc::string::String::new();
        run_binary_with_output(&bin, &[], &mut out).expect("FizzBuzz run failed");
        let lines: alloc::vec::Vec<&str> = out.trim().lines().collect();
        assert_eq!(lines[0], "1");
        assert_eq!(lines[2], "Fizz");
        assert_eq!(lines[4], "Buzz");
        assert_eq!(lines[14], "FizzBuzz");
    }
    #[test]
    fn test_register_spill_over_13_variables() {
        let src = r#"
            let $v1 = 1;  let $v2 = 2;  let $v3 = 3;  let $v4 = 4;
            let $v5 = 5;  let $v6 = 6;  let $v7 = 7;  let $v8 = 8;
            let $v9 = 9;  let $v10 = 10; let $v11 = 11; let $v12 = 12;
            let $v13 = 13; let $v14 = 14; let $v15 = 15; let $v16 = 16;
            let $v17 = 17; let $v18 = 18;
            let $sum = $v1 + $v2 + $v3 + $v4 + $v5 + $v6 + $v7 + $v8 + $v9 + $v10 + $v11 + $v12 + $v13 + $v14 + $v15 + $v16 + $v17 + $v18;
            @assert($sum == 171);
        "#;
        let bin = compile(src).expect("Compilation with 18 spilled variables failed");
        run_binary(&bin, &[]).expect("Execution with 18 spilled variables failed");
    }
    #[test]
    fn test_spill_variable_mutations() {
        let src = r#"
            let $v1 = 1;  let $v2 = 2;  let $v3 = 3;  let $v4 = 4;
            let $v5 = 5;  let $v6 = 6;  let $v7 = 7;  let $v8 = 8;
            let $v9 = 9;  let $v10 = 10; let $v11 = 11; let $v12 = 12;
            let $v13 = 13;
            let mut $v14 = 100;
            let mut $v15 = 200;
            $v14 += 50;
            $v15 -= 25;
            let $total = $v14 + $v15;
            @assert($total == 325);
        "#;
        let bin = compile(src).expect("Mutable spill variables compile failed");
        run_binary(&bin, &[]).expect("Mutable spill variables run failed");
    }
    #[test]
    fn test_static_include_expansion() {
        let src = r#"
            @include "helpers.pul";
            let $res = add_ten(50);
            @assert($res == 60);
        "#;
        let mut loader = |path: &str| -> Option<alloc::string::String> {
            if path == "helpers.pul" {
                Some(alloc::string::String::from("fn add_ten($x) -> $r { $r := $x + 10; return $r; }\n"))
            } else {
                None
            }
        };
        let expanded = preprocess_includes(src, &mut loader).expect("Include preprocessing failed");
        let bin = compile(&expanded).expect("Compilation of included code failed");
        run_binary(&bin, &[]).expect("Execution failed");
    }

    #[test]
    fn test_circular_include_detected() {
        let src = r#"@include "a.pul";"#;
        let mut loader = |path: &str| -> Option<alloc::string::String> {
            if path == "a.pul" {
                Some(alloc::string::String::from("@include \"b.pul\";\n"))
            } else if path == "b.pul" {
                Some(alloc::string::String::from("@include \"a.pul\";\n"))
            } else {
                None
            }
        };
        let err = preprocess_includes(src, &mut loader).unwrap_err();
        assert_eq!(err.code, "ERR_CIRCULAR_INCLUDE");
    }
    #[test]
    fn test_fixed_point_q_format_arithmetic() {
        let src = r#"
            // Q16.16 fixed-point arithmetic test
            let $a = @to_fix(3, 16);  // 3 * 65536 = 196608
            let $b = @to_fix(4, 16);  // 4 * 65536 = 262144
            let $prod = @fix_mul($a, $b, 16); // 12 in Q16.16 = 786432
            let $quot = @fix_div($prod, $a, 16); // 4 in Q16.16 = 262144
            let $int_res = @to_i64($quot, 16); // 4
            @assert($int_res == 4);
            @assert($prod == 786432);
        "#;
        let bin = compile(src).expect("Fixed point compilation failed");
        run_binary(&bin, &[]).expect("Fixed point execution failed");
    }
    #[test]
    fn test_fixed_point_sine_lut_interpolation() {
        let src = r#"
            // Sine LUT in Q16.16: sin(0)=0, sin(pi/6)=0.5, sin(pi/2)=1.0
            // 0 -> 0, 0.5 -> 32768, 1.0 -> 65536
            const SINE_Q16: [i64; 3] = [0, 32768, 65536];
            let $sin_0 = SINE_Q16[0];
            let $sin_half = SINE_Q16[1];
            let $sin_one = SINE_Q16[2];

            // Amplitude scaling: 10 * sin(pi/2) = 10
            let $amp = @to_fix(10, 16);
            let $scaled = @fix_mul($amp, $sin_one, 16);
            let $res_int = @to_i64($scaled, 16);
            @assert($res_int == 10);

            // Half scaling: 10 * sin(pi/6) = 5
            let $scaled_half = @fix_mul($amp, $sin_half, 16);
            let $res_half = @to_i64($scaled_half, 16);
            @assert($res_half == 5);
        "#;
        let bin = compile(src).expect("Sine LUT fixed point compile failed");
        run_binary(&bin, &[]).expect("Sine LUT fixed point run failed");
    }
    #[test]
    fn test_declarative_pool_size_override() {
        let src = r#"
            @pool_size(elements: 512, arrays: 16);
            let $big: [i64; 300];
            $big[299] := 42;
            @assert($big[299] == 42);
        "#;
        let bin = compile(src).expect("Compile with enlarged pool_size failed");
        run_binary(&bin, &[]).expect("Run with enlarged pool_size failed");
    }
    #[test]
    fn test_combinator_with_builtin_function() {
        let src = r#"
            let $a = [10, 50, 30];
            let $b = [20, 40, 60];
            // Use built-in @min in @zip_with
            let $min_sum = @zip_with($a, $b, @min) |> @sum();
            // min(10,20)=10, min(50,40)=40, min(30,60)=30 -> sum = 80
            @assert($min_sum == 80);
        "#;
        let bin = compile(src).expect("Combinator with builtin min compile failed");
        run_binary(&bin, &[]).expect("Execution with builtin min failed");
    }
    #[test]
    fn test_error_json_formatting() {
        let src = "let $x = 10;\n$x := 20;\n";
        let mut buf = [0u8; 1024];
        let err = compile_pulse_to_binary(src.as_bytes(), &mut buf).unwrap_err();
        let mut json_out = alloc::string::String::new();
        err.format_json("script.pul", &mut json_out).unwrap();
        assert!(json_out.contains("\"success\":false"));
        assert!(json_out.contains("ERR_MUTABILITY_VIOLATION"));
    }

    #[test]
    fn test_ensures_contract_success_and_failure() {
        let src_valid = r#"
            fn abs_val($x) -> i64
            @ensures($result >= 0)
            {
                if ($x < 0) {
                    return -$x;
                }
                return $x;
            }
            let $a = abs_val(-42);
            @assert($a == 42);
            let $b = abs_val(10);
            @assert($b == 10);
        "#;
        let mut out = alloc::string::String::new();
        run_source_with_output(src_valid, &[], &mut out).expect("Valid @ensures failed");

        let src_violating = r#"
            fn bad_inc($x) -> i64
            @ensures($result > $x)
            {
                return $x; // Does not increase, violates @ensures
            }
            let $res = bad_inc(5);
        "#;
        let mut out2 = alloc::string::String::new();
        let err = run_source_with_output(src_violating, &[], &mut out2).unwrap_err();
        assert_eq!(err.code, "ERR_PX64_ASSERTION_FAILED");
    }

    #[test]
    fn test_test_block_compilation_and_runner() {
        let src = r#"
            fn add($x, $y) -> i64
            @requires($x >= 0)
            @ensures($result >= $x)
            {
                return $x + $y;
            }

            @test "add two numbers" @budget(50us) {
                let $res = add(10, 20);
                @assert($res == 30);
            }

            @test "add with zero" {
                let $res = add(5, 0);
                @assert($res == 5);
            }

            @test "failing test" {
                let $res = add(1, 1);
                @assert($res == 999);
            }
        "#;

        // 1. Normal compilation strips @test blocks completely
        let bin = compile(src).expect("Normal compilation failed");
        assert!(bin.len() > PX64_HEADER_SIZE);

        // 2. Unit test compilation discovers all 3 tests
        let tests = compile_pulse_tests(src.as_bytes()).expect("Test compilation failed");
        assert_eq!(tests.len(), 3);
        assert_eq!(tests[0].name, "add two numbers");
        assert_eq!(tests[0].budget_ns, Some(50_000));
        assert_eq!(tests[1].name, "add with zero");
        assert_eq!(tests[1].budget_ns, None);
        assert_eq!(tests[2].name, "failing test");

        // 3. Execution of unit tests
        let res0 = run_test_case(&tests[0]);
        assert!(res0.passed, "Test 0 should pass: {:?}", res0.error);
        assert!(res0.steps > 0);

        let res1 = run_test_case(&tests[1]);
        assert!(res1.passed, "Test 1 should pass: {:?}", res1.error);

        let res2 = run_test_case(&tests[2]);
        assert!(!res2.passed, "Test 2 should fail assertion");
        assert!(res2.error.as_ref().unwrap().contains("ERR_PX64_ASSERTION_FAILED"));
    }

    #[test]
    fn test_enum_definition_and_exhaustive_match() {
        let src = r#"
            enum State {
                Idle,
                Running,
                Failed,
            }

            let $state = State::Running;
            let mut $code = 0;

            match $state {
                State::Idle => {
                    $code = 10;
                },
                State::Running => {
                    $code = 20;
                },
                State::Failed => {
                    $code = 30;
                },
            };
            @assert($code == 20);
        "#;

        let mut buf = [0u8; 1024];
        let size = compile_pulse_to_binary(src.as_bytes(), &mut buf).expect("Enum match compilation failed");
        assert!(size > PX64_HEADER_SIZE);

        let mut out = alloc::string::String::new();
        run_source_with_output(src, &[], &mut out).expect("Execution failed");
    }

    #[test]
    fn test_enum_with_type_annotation() {
        let src = r#"
            enum Mode {
                Low,
                High,
            }
            let $m: Mode = Mode::High;
            let mut $res = 0;
            match $m {
                Mode::Low => { $res = 1; },
                Mode::High => { $res = 2; },
            };
            @assert($res == 2);
        "#;
        let mut out = alloc::string::String::new();
        run_source_with_output(src, &[], &mut out).expect("Execution failed");
    }

    #[test]
    fn test_enum_match_with_wildcard() {
        let src = r#"
            enum Color {
                Red,
                Green,
                Blue,
            }
            let $c = Color::Red;
            let mut $val = 0;
            match $c {
                Color::Red => { $val = 100; },
                _ => { $val = 0; },
            };
            @assert($val == 100);
        "#;
        let mut out = alloc::string::String::new();
        run_source_with_output(src, &[], &mut out).expect("Execution failed");
    }

    #[test]
    fn test_enum_missing_variant_rejected() {
        let src = r#"
            enum State {
                Idle,
                Running,
                Failed,
            }
            let $s = State::Idle;
            match $s {
                State::Idle => {},
                State::Running => {},
            };
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_NON_EXHAUSTIVE_MATCH");
        assert_eq!(err.stage, "Pattern Matching -> Exhaustiveness Check");
    }

    #[test]
    fn test_enum_unreachable_pattern_rejected() {
        let src = r#"
            enum State {
                Idle,
                Running,
            }
            let $s = State::Idle;
            match $s {
                State::Idle => {},
                State::Idle => {},
                State::Running => {},
            };
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_UNREACHABLE_PATTERN");
        assert!(err.suggestion.contains("Remove duplicate or unreachable pattern arm"));
    }

    #[test]
    fn test_enum_unknown_variant_rejected() {
        let src = r#"
            enum State {
                Idle,
                Running,
            }
            let $s = State::Unknown;
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_UNKNOWN_ENUM_VARIANT");
        assert!(err.suggestion.contains("Valid variants"));
    }

    #[test]
    fn test_duplicate_enum_name_rejected() {
        let src = r#"
            enum State { A, B }
            enum State { C, D }
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_DUPLICATE_ENUM_NAME");
    }

    #[test]
    fn test_duplicate_enum_variant_rejected() {
        let src = r#"
            enum State {
                Idle,
                Idle,
            }
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_DUPLICATE_ENUM_VARIANT");
    }

    #[test]
    fn test_enum_type_mismatch_rejected() {
        let src = r#"
            enum State { Idle, Running }
            enum Action { Start, Stop }
            let $s = State::Idle;
            match $s {
                Action::Start => {},
                _ => {},
            };
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_ENUM_TYPE_MISMATCH");
    }

    #[test]
    fn test_issue_2_multiple_requires_and_ensures() {
        let src = r#"
            fn make_character($code, $index) -> i64
            @requires($code >= 0)
            @requires($index >= 0)
            @ensures($result >= 0)
            {
                return $code;
            }
            let $c = make_character(65, 0);
            @assert($c == 65);
        "#;
        let bin = compile(src).expect("Compilation of Issue #2 snippet failed");
        assert!(bin.len() > PX64_HEADER_SIZE);

        let mut out = alloc::string::String::new();
        run_source_with_output(src, &[], &mut out).expect("Execution of Issue #2 snippet failed");
    }

    #[test]
    fn test_struct_literal_rejected_with_actionable_hint() {
        let src = r#"
            struct Point { x: i64, y: i64 }
            let $p = Point { x: 10, y: 20 };
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_STRUCT_LITERAL_UNSUPPORTED");
        assert!(err.suggestion.contains("Declare with 'let mut $var: Type;'"));
    }

    #[test]
    fn test_fixed_point_arithmetic_and_scale_matching() {
        let src = r#"
            let $gain: fixed<16> = 1.5;
            let $base: fixed<16> = 2.0;
            let $product = $gain * $base;
            let $sum = $gain + $base;
        "#;
        let bin = compile(src).expect("Compilation of fixed-point arithmetic failed");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_fixed_point_scale_mismatch_rejected() {
        let src = r#"
            let $a: fixed<16> = 1.5;
            let $b: fixed<8> = 2.0;
            let $bad = $a + $b;
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_FIXED_SCALE_MISMATCH");
    }
    #[test]
    fn test_function_wcet_contract_passed_and_stats() {
        let src = r#"
            @contract: @wcet(100us) @budget(500us);
            fn compute($a, $b) -> i64
                @wcet(50ns)
            {
                return $a + $b;
            }
            let $res = compute(10, 20);
        "#;
        let mut tokens = alloc::vec![Token::empty(); MAX_TOKENS];
        let mut lexer = Lexer::new(src.as_bytes());
        let tok_count = lexer.tokenize(&mut tokens).expect("Tokenize failed");
        let mut compiler = Compiler::new(src.as_bytes(), &tokens[..=tok_count]);
        compiler.compile().expect("Compilation should succeed");
        let stats = compiler.stats();
        assert!(stats.estimated_wcet_ns > 0);
        assert_eq!(stats.declared_wcet_ns, Some(100_000));
        assert_eq!(stats.declared_budget_ns, Some(500_000));
        assert!(stats.wcet_breakdown_count >= 1);
        let fn_item = &stats.wcet_breakdown[0];
        let name = core::str::from_utf8(&fn_item.name[..fn_item.name_len]).unwrap();
        assert_eq!(name, "compute");
        assert_eq!(fn_item.declared_ns, Some(50));
        assert!(fn_item.estimated_ns <= 50);
    }

    #[test]
    fn test_function_wcet_contract_mismatch_rejected() {
        let src = r#"
            fn heavy_calculation($a, $b) -> i64
                @wcet(1ns)
            {
                let $x = $a * 2;
                let $y = $b * 3;
                let $z = $x + $y;
                return $z * 4;
            }
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_WCET_CONTRACT_MISMATCH");
    }

    #[test]
    fn test_script_level_wcet_contract_mismatch_rejected() {
        let src = r#"
            @contract: @wcet(1ns) @budget(500us);
            let $x = 10;
            let $y = 20;
            let $z = $x + $y;
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_WCET_CONTRACT_MISMATCH");
    }
    #[test]
    fn test_typestate_branch_match_valid() {
        let src = r#"
            let $cond = 1;
            #f := @capture();
            if ($cond == 1) {
                @send(#f);
            } else {
                @send(#f);
            }
        "#;
        let bin = compile(src).expect("Both branches consuming handle must pass");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_typestate_branch_mismatch_rejected() {
        let src = r#"
            let $cond = 1;
            #f := @capture();
            if ($cond == 1) {
                @send(#f);
            } else {
                let $x = 10;
            }
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_TYPESTATE_MISMATCH");
    }

    #[test]
    fn test_typestate_loop_confinement_valid() {
        let src = r#"
            for $i in 0..5 {
                #f := @capture();
                @send(#f);
            }
        "#;
        let bin = compile(src).expect("Loop with balanced capture and send must pass");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_typestate_loop_confinement_leak_rejected() {
        let src = r#"
            for $i in 0..5 {
                #f := @capture();
            }
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_TYPESTATE_MISMATCH");
    }

    #[test]
    fn test_typestate_within_drop_valid() {
        let src = r#"
            @within(500us) {
                #f := @capture();
            } !drop;
        "#;
        let bin = compile(src).expect("Captured handle dropped via !drop must pass");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }
    #[test]
    fn test_for_loop_invariant_valid() {
        let src = r#"
            let mut $sum = 0;
            for $i in 0..5 @invariant($sum >= 0) {
                $sum += 10;
            }
            @assert($sum == 50);
        "#;
        let bin = compile(src).expect("Loop with satisfied invariant must compile and pass");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_while_loop_invariant_valid() {
        let src = r#"
            let mut $i = 0;
            while ($i < 3) @invariant($i >= 0) {
                $i += 1;
            }
            @assert($i == 3);
        "#;
        let bin = compile(src).expect("While loop with valid invariant must pass");
        assert!(bin.len() > PX64_HEADER_SIZE);
    }

    #[test]
    fn test_loop_invariant_violation_fails_runtime_assert() {
        let src = r#"
            let mut $val = 10;
            for $i in 0..3 @invariant($val > 5) {
                $val -= 3;
            }
        "#;
        let bin = compile(src).expect("Compilation succeeds, fails at runtime");
        let mut vm = PX64VM::new(&bin[PX64_HEADER_SIZE..], &[], &[], &[]);
        let err = vm.run().unwrap_err();
        assert_eq!(err.code, "ERR_PX64_ASSERTION_FAILED");
    }

    #[test]
    fn test_loop_invariant_syntax_error() {
        let src = r#"
            let mut $x = 0;
            for $i in 0..3 @invariant {
                $x += 1;
            }
        "#;
        let err = compile(src).unwrap_err();
        assert_eq!(err.code, "ERR_INVARIANT_SYNTAX");
    }
}
