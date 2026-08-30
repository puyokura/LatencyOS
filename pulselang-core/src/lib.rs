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
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src);
    let _tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src, &tokens);
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
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src.as_bytes());
    let _tok_count = lexer.tokenize(&mut tokens)?;

    let mut compiler = Compiler::new(src.as_bytes(), &tokens);
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
