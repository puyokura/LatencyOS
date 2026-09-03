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

pub use compiler::{
    ArrayMeta, CompileStats, Compiler, ConstTableMeta, FnMeta, HandleState, StructDefMeta,
    StructFieldMeta, StructInstMeta,
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

/// Compile PulseLang source code into a binary px64 bytecode buffer (zero-heap `no_std` API).
///
/// Returns the number of bytes written to `out_buf`.
pub fn compile_pulse_to_binary(src: &[u8], out_buf: &mut [u8]) -> Result<usize, CompileError> {
    #[cfg(feature = "alloc")]
    {
        let mut tokens = alloc::vec![Token::empty(); MAX_TOKENS];
        compile_pulse_to_binary_with_tokens(src, &mut tokens, out_buf)
    }
    #[cfg(not(feature = "alloc"))]
    {
        static mut NO_STD_TOKENS: [Token; 512] = [Token::empty(); 512];
        let tokens = unsafe { &mut NO_STD_TOKENS };
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
        // 14 variables should trigger ERR_MAX_VARS_EXCEEDED
        let src = r#"
            let $v1 = 1; let $v2 = 2; let $v3 = 3; let $v4 = 4;
            let $v5 = 5; let $v6 = 6; let $v7 = 7; let $v8 = 8;
            let $v9 = 9; let $v10 = 10; let $v11 = 11; let $v12 = 12;
            let $v13 = 13; let $v14 = 14;
        "#;
        let err = compile(src).unwrap_err();
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
    fn test_error_json_formatting() {
        let src = "let $x = 10;\n$x := 20;\n";
        let mut buf = [0u8; 1024];
        let err = compile_pulse_to_binary(src.as_bytes(), &mut buf).unwrap_err();
        let mut json_out = alloc::string::String::new();
        err.format_json("script.pul", &mut json_out).unwrap();
        assert!(json_out.contains("\"success\":false"));
        assert!(json_out.contains("ERR_MUTABILITY_VIOLATION"));
    }
}
