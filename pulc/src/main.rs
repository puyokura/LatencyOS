//! `pulc` - PulseLang Host Compiler & Toolchain CLI for LatencyOS

use pulselang_core::{
    check, compile_pulse_to_binary, disassemble_px64_with_filename, preprocess_includes,
    CompileError, Compiler, Lexer, Token, MAX_TOKENS,
    RUNTIME_TINY, RUNTIME_CORE, RUNTIME_MATH, RUNTIME_FIX, RUNTIME_SYS, RUNTIME_NET, RUNTIME_VRAM, RUNTIME_GPU,
};
use std::process::Command;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "0.1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subcommand {
    Run {
        input: PathBuf,
        args: Vec<String>,
    },
    Compile {
        input: PathBuf,
        output: Option<PathBuf>,
    },
    Check {
        input: PathBuf,
    },
    Test {
        input: PathBuf,
        filter: Option<String>,
    },
    Disasm {
        input: PathBuf,
    },
    Help,
    Version,
}

#[derive(Debug, Clone)]
struct CliOptions {
    subcommand: Subcommand,
    json: bool,
    verbose: bool,
}

fn print_help() {
    println!(
        "\x1b[1mpulc\x1b[0m {} - PulseLang Compiler, Disassembler & VM Toolchain for LatencyOS

\x1b[1mUSAGE:\x1b[0m
    pulc <file.pul> [-o <out.bin>]
    pulc run <file.bin|file.pul> [args...]
    pulc compile <file.pul> [-o <out.bin>]
    pulc check <file.pul>
    pulc test <file.pul> [--filter <pattern>]
    pulc disasm <file.bin>
    pulc -d <file.bin>

\x1b[1mSUBCOMMANDS:\x1b[0m
    run <file> [args...]  Execute px64 binary (.bin) or source script (.pul) directly
    compile <file.pul>    Compile PulseLang source into px64 binary bytecode
    check <file.pul>      Validate syntax, types, linear ownership & WCET constraints
    test <file.pul>       Run annotated @test blocks from source script
    disasm <file.bin>     Disassemble px64 binary bytecode into assembly instructions

\x1b[1mFLAGS:\x1b[0m
    -o, --output <file>   Specify output binary file path (default: <input>.bin)
    -d, --disasm          Disassemble binary bytecode file
    --json                Emit JSON diagnostic and output format
    -v, --verbose         Enable verbose diagnostic logging
    -h, --help            Print help information
    -V, --version         Print version information

\x1b[1mEXIT CODES:\x1b[0m
    0   Success
    1   Compilation, syntax, linear ownership, WCET, or VM runtime error
    2   IO, file access, or command-line argument error
",
        VERSION
    );
}

fn print_version() {
    println!("pulc {} (PulseLang px64 toolchain for LatencyOS)", VERSION);
}

fn parse_cli_args<I>(args: I) -> Result<CliOptions, String>
where
    I: Iterator<Item = String>,
{
    let args_vec: Vec<String> = args.collect();
    if args_vec.is_empty() {
        return Ok(CliOptions {
            subcommand: Subcommand::Help,
            json: false,
            verbose: false,
        });
    }

    let mut json = false;
    let mut verbose = false;
    let mut output_opt: Option<PathBuf> = None;
    let mut disasm_flag = false;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args_vec.len() {
        let arg = &args_vec[i];
        match arg.as_str() {
            "-h" | "--help" => {
                return Ok(CliOptions {
                    subcommand: Subcommand::Help,
                    json,
                    verbose,
                });
            }
            "-V" | "--version" => {
                return Ok(CliOptions {
                    subcommand: Subcommand::Version,
                    json,
                    verbose,
                });
            }
            "--json" => {
                json = true;
            }
            "-v" | "--verbose" => {
                verbose = true;
            }
            "-d" | "--disasm" => {
                disasm_flag = true;
            }
            "-o" | "--output" => {
                if i + 1 >= args_vec.len() {
                    return Err("Option '-o / --output' requires an argument".to_string());
                }
                i += 1;
                output_opt = Some(PathBuf::from(&args_vec[i]));
            }
            "run" | "exec" => {
                if i + 1 >= args_vec.len() {
                    return Err("Missing input file for 'run' subcommand (expected .bin or .pul)".to_string());
                }
                let input = PathBuf::from(&args_vec[i + 1]);
                let run_args = args_vec[i + 2..].to_vec();
                return Ok(CliOptions {
                    subcommand: Subcommand::Run {
                        input,
                        args: run_args,
                    },
                    json,
                    verbose,
                });
            }
            "test" => {
                let mut input = None;
                let mut filter = None;
                let mut j = i + 1;
                while j < args_vec.len() {
                    if args_vec[j] == "--json" {
                        json = true;
                        j += 1;
                    } else if args_vec[j] == "-v" || args_vec[j] == "--verbose" {
                        verbose = true;
                        j += 1;
                    } else if args_vec[j] == "--filter" {
                        if j + 1 >= args_vec.len() {
                            return Err("Option '--filter' requires an argument".to_string());
                        }
                        filter = Some(args_vec[j + 1].clone());
                        j += 2;
                    } else if let Some(stripped) = args_vec[j].strip_prefix("--filter=") {
                        filter = Some(stripped.to_string());
                        j += 1;
                    } else if !args_vec[j].starts_with('-') && input.is_none() {
                        input = Some(PathBuf::from(&args_vec[j]));
                        j += 1;
                    } else {
                        return Err(format!("Unrecognized argument for 'test' subcommand: '{}'", args_vec[j]));
                    }
                }
                let input_path = input.ok_or_else(|| "Missing input source file for 'test' subcommand".to_string())?;
                return Ok(CliOptions {
                    subcommand: Subcommand::Test { input: input_path, filter },
                    json,
                    verbose,
                });
            }
            _ => {
                if let Some(stripped) = arg.strip_prefix("-o=") {
                    output_opt = Some(PathBuf::from(stripped));
                } else if arg.starts_with('-') {
                    return Err(format!("Unrecognized flag '{}'", arg));
                } else {
                    positional.push(arg.clone());
                }
            }
        }
        i += 1;
    }

    if positional.is_empty() {
        return Ok(CliOptions {
            subcommand: Subcommand::Help,
            json,
            verbose,
        });
    }

    let first = &positional[0];
    let subcommand = match first.as_str() {
        "compile" | "build" => {
            if positional.len() < 2 {
                return Err("Missing input source file for 'compile' subcommand".to_string());
            }
            let input = PathBuf::from(&positional[1]);
            let output = output_opt.or_else(|| {
                if positional.len() >= 3 {
                    Some(PathBuf::from(&positional[2]))
                } else {
                    None
                }
            });
            Subcommand::Compile { input, output }
        }
        "check" | "verify" => {
            if positional.len() < 2 {
                return Err("Missing input source file for 'check' subcommand".to_string());
            }
            Subcommand::Check {
                input: PathBuf::from(&positional[1]),
            }
        }
        "disasm" | "objdump" => {
            if positional.len() < 2 {
                return Err("Missing input binary file for 'disasm' subcommand".to_string());
            }
            Subcommand::Disasm {
                input: PathBuf::from(&positional[1]),
            }
        }
        _ => {
            let input = PathBuf::from(first);
            if disasm_flag || input.extension().and_then(|e| e.to_str()) == Some("bin") {
                Subcommand::Disasm { input }
            } else {
                let output = output_opt.or_else(|| {
                    if positional.len() >= 2 {
                        Some(PathBuf::from(&positional[1]))
                    } else {
                        None
                    }
                });
                Subcommand::Compile { input, output }
            }
        }
    };

    Ok(CliOptions {
        subcommand,
        json,
        verbose,
    })
}

fn derive_output_path(input: &Path, explicit_out: Option<PathBuf>) -> (PathBuf, bool) {
    if let Some(out) = explicit_out {
        let is_bin = out.extension().and_then(|e| e.to_str()) == Some("bin");
        return (out, is_bin);
    }
    let mut out = input.to_path_buf();
    #[cfg(windows)]
    out.set_extension("exe");
    #[cfg(not(windows))]
    out.set_extension("");
    (out, false)
}

fn generate_standalone_executable(
    bytecode: &[u8],
    imported_runtimes: u16,
    out_path: &Path,
) -> Result<(), (i32, String)> {
    let mut native_dispatch_arms = String::new();

    if (imported_runtimes & (RUNTIME_TINY | RUNTIME_CORE)) != 0 {
        native_dispatch_arms.push_str(r#"
            1 /* NATIVE_PRINT */ => {
                if (arg_val & ARG_TAG) != 0 {
                    let idx = (arg_val & 0xFF) as usize;
                    if idx < self.args.len() { print!("{}", self.args[idx]); }
                } else if (arg_val & STR_TAG) != 0 {
                    let raw = arg_val & !STR_TAG;
                    let offset = ((raw as u64) >> 32) as usize;
                    let len = (raw & 0xFFFF_FFFF) as usize;
                    if offset + len <= self.str_pool.len() {
                        if let Ok(s) = std::str::from_utf8(&self.str_pool[offset..offset + len]) {
                            print!("{}", s);
                        }
                    }
                } else {
                    print!("{}", arg_val);
                }
                0
            }
            2 /* NATIVE_PRINTLN */ => {
                if (arg_val & ARG_TAG) != 0 {
                    let idx = (arg_val & 0xFF) as usize;
                    if idx < self.args.len() { print!("{}", self.args[idx]); }
                    println!();
                } else if (arg_val & STR_TAG) != 0 {
                    let raw = arg_val & !STR_TAG;
                    let offset = ((raw as u64) >> 32) as usize;
                    let len = (raw & 0xFFFF_FFFF) as usize;
                    if offset + len <= self.str_pool.len() {
                        if let Ok(s) = std::str::from_utf8(&self.str_pool[offset..offset + len]) {
                            println!("{}", s);
                        } else { println!(); }
                    } else { println!(); }
                } else if arg_reg != 0 || arg_val != 0 {
                    println!("{}", arg_val);
                } else {
                    println!();
                }
                0
            }
        "#);
    }

    if (imported_runtimes & RUNTIME_CORE) != 0 {
        native_dispatch_arms.push_str(r#"
            8 /* NATIVE_SCRIPT_ARGC */ => self.args.len() as i64,
            9 /* NATIVE_SCRIPT_ARG */ => {
                if arg_val >= 0 && (arg_val as usize) < self.args.len() && (arg_val as usize) < 256 {
                    ARG_TAG | (arg_val & 0xFF)
                } else {
                    0
                }
            }
            10 /* NATIVE_TAG_OK */ => arg_val & !ERR_TAG,
            11 /* NATIVE_TAG_ERR */ => ERR_TAG | (arg_val & !ERR_TAG),
            12 /* NATIVE_IS_OK */ => if (arg_val & ERR_TAG) == 0 { 1 } else { 0 },
            13 /* NATIVE_IS_ERR */ => if (arg_val & ERR_TAG) != 0 { 1 } else { 0 },
            14 /* NATIVE_UNWRAP */ => {
                if (arg_val & ERR_TAG) != 0 {
                    eprintln!("PulseLang runtime error: unwrap failed on Err value");
                    std::process::exit(1);
                }
                arg_val
            }
            15 /* NATIVE_STREQ */ => {
                let s1 = arg_val;
                let s2 = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 0 };
                if s1 == s2 { 1 } else {
                    let b1 = self.get_str_bytes(s1);
                    let b2 = self.get_str_bytes(s2);
                    match (b1, b2) {
                        (Some(x), Some(y)) if x == y => 1,
                        _ => 0,
                    }
                }
            }
        "#);
    }

    if (imported_runtimes & RUNTIME_MATH) != 0 {
        native_dispatch_arms.push_str(r#"
            21 /* NATIVE_MATH_MIN */ => {
                let v2 = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 0 };
                arg_val.min(v2)
            }
            22 /* NATIVE_MATH_MAX */ => {
                let v2 = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 0 };
                arg_val.max(v2)
            }
            23 /* NATIVE_MATH_ABS */ => arg_val.abs(),
            24 /* NATIVE_MATH_CLAMP */ => {
                let v2 = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 0 };
                let v3 = if arg_reg > 1 { self.regs[(arg_reg - 2) as usize] } else { 0 };
                arg_val.clamp(v2, v3)
            }
            25 /* NATIVE_BIT_POPCNT */ => arg_val.count_ones() as i64,
            26 /* NATIVE_BIT_LZCNT */ => arg_val.leading_zeros() as i64,
            27 /* NATIVE_CRC32 */ => {
                let seed = (arg_val & 0xFFFF_FFFF) as u32;
                let v2 = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 0 };
                let bytes = v2.to_le_bytes();
                let mut crc = !seed;
                for b in bytes {
                    crc ^= b as u32;
                    for _ in 0..8 {
                        if (crc & 1) != 0 { crc = (crc >> 1) ^ 0xEDB88320; } else { crc >>= 1; }
                    }
                }
                (!crc) as i64
            }
        "#);
    }

    if (imported_runtimes & RUNTIME_FIX) != 0 {
        native_dispatch_arms.push_str(r#"
            30 /* NATIVE_FIX_TO_FIX */ => {
                let from_scale = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 16 };
                let to_scale = if arg_reg > 1 { self.regs[(arg_reg - 2) as usize] } else { 16 };
                if to_scale >= from_scale {
                    arg_val << (to_scale - from_scale)
                } else {
                    arg_val >> (from_scale - to_scale)
                }
            }
            31 /* NATIVE_FIX_TO_I64 */ => {
                let scale = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 16 };
                arg_val >> scale
            }
            32 /* NATIVE_FIX_MUL */ => {
                let v2 = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 0 };
                let scale = if arg_reg > 1 { self.regs[(arg_reg - 2) as usize] } else { 16 };
                ((arg_val as i128 * v2 as i128) >> scale) as i64
            }
            33 /* NATIVE_FIX_DIV */ => {
                let v2 = if arg_reg > 0 { self.regs[(arg_reg - 1) as usize] } else { 1 };
                let scale = if arg_reg > 1 { self.regs[(arg_reg - 2) as usize] } else { 16 };
                if v2 == 0 { 0 } else { (((arg_val as i128) << scale) / (v2 as i128)) as i64 }
            }
        "#);
    }

    if (imported_runtimes & RUNTIME_SYS) != 0 {
        native_dispatch_arms.push_str(r#"
            3 /* NATIVE_SYS_TSC */ => {
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0)
            }
            16 /* NATIVE_CORE_ID */ => 0,
            17 /* NATIVE_TSC_FREQ */ => 3_000_000_000,
            18 /* NATIVE_UPTIME_NS */ => {
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0)
            }
            19 /* NATIVE_BUSY_WAIT */ => {
                if arg_val > 0 {
                    let spins = (arg_val as usize).min(100_000);
                    for _ in 0..spins { std::hint::spin_loop(); }
                }
                0
            }
            20 /* NATIVE_RING_DEPTH */ => 0,
        "#);
    }

    if (imported_runtimes & RUNTIME_NET) != 0 {
        native_dispatch_arms.push_str(r#"
            4 /* NATIVE_NET_RTT */ => 100,
            5 /* NATIVE_NET_SET_RATE */ => 0,
            7 /* NATIVE_NET_SEND */ => 1,
        "#);
    }

    if (imported_runtimes & RUNTIME_VRAM) != 0 {
        native_dispatch_arms.push_str(r#"
            28 /* NATIVE_VRAM_READ */ => 0,
            29 /* NATIVE_VRAM_WRITE */ => 0,
        "#);
    }

    if (imported_runtimes & RUNTIME_GPU) != 0 {
        native_dispatch_arms.push_str(r#"
            6 /* NATIVE_GPU_CAPTURE */ => 0,
        "#);
    }

    native_dispatch_arms.push_str(r#"
            _ => {
                eprintln!("[RUNTIME_ERROR] Intrinsic was called but its runtime module was not imported via @import");
                std::process::exit(1);
            }
    "#);

    let mut bytecode_bytes_str = String::with_capacity(bytecode.len() * 5);
    for b in bytecode {
        use std::fmt::Write;
        let _ = write!(bytecode_bytes_str, "{},", b);
    }

    let stub_src = format!(
r#"// Auto-generated standalone binary stub for PulseLang px64
#![allow(dead_code, unused_variables, unused_assignments)]
use std::convert::TryInto;
const STR_TAG: i64 = 0x4000_0000_0000_0000;
const ARG_TAG: i64 = 0x2000_0000_0000_0000;
const ERR_TAG: i64 = 0x1000_0000_0000_0000;

static BYTECODE: &[u8] = &[{bytecode}];

struct StandaloneVM<'a> {{
    code: &'a [u8],
    str_pool: &'a [u8],
    const_pool: Vec<i64>,
    args: &'a [&'a str],
    regs: [i64; 20],
    spill_slots: [i64; 32],
    array_slots: [i64; 256],
    array_lens: [usize; 8],
    array_bases: [usize; 8],
    struct_insts: [[i64; 8]; 8],
    table_bases: [usize; 8],
    table_lens: [usize; 8],
    call_stack: [usize; 8],
    call_depth: usize,
    ip: usize,
}}

impl<'a> StandaloneVM<'a> {{
    fn new(bin: &'a [u8], args: &'a [&'a str]) -> Self {{
        let code_len = u16::from_be_bytes([bin[6], bin[7]]) as usize;
        let str_len = u16::from_be_bytes([bin[8], bin[9]]) as usize;
        let const_count = u16::from_be_bytes([bin[10], bin[11]]) as usize;

        let code_start = 16;
        let code_end = code_start + code_len;
        let str_end = code_end + str_len;

        let code = &bin[code_start..code_end];
        let str_pool = &bin[code_end..str_end];
        let mut const_pool = Vec::with_capacity(const_count);
        for i in 0..const_count {{
            let offset = str_end + i * 8;
            let val = i64::from_be_bytes(bin[offset..offset + 8].try_into().unwrap());
            const_pool.push(val);
        }}

        Self {{
            code,
            str_pool,
            const_pool,
            args,
            regs: [0; 20],
            spill_slots: [0; 32],
            array_slots: [0; 256],
            array_lens: [0; 8],
            array_bases: [0; 8],
            struct_insts: [[0; 8]; 8],
            table_bases: [0; 8],
            table_lens: [0; 8],
            call_stack: [0; 8],
            call_depth: 0,
            ip: 0,
        }}
    }}

    fn get_str_bytes(&self, val: i64) -> Option<&'a [u8]> {{
        if (val & STR_TAG) != 0 {{
            let raw = val & !STR_TAG;
            let offset = ((raw as u64) >> 32) as usize;
            let len = (raw & 0xFFFF_FFFF) as usize;
            if offset + len <= self.str_pool.len() {{
                Some(&self.str_pool[offset..offset + len])
            }} else {{
                None
            }}
        }} else if (val & ARG_TAG) != 0 {{
            let idx = (val & 0xFF) as usize;
            if idx < self.args.len() {{
                Some(self.args[idx].as_bytes())
            }} else {{
                None
            }}
        }} else {{
            None
        }}
    }}

    fn run(&mut self) {{
        while self.ip + 4 <= self.code.len() {{
            let op = self.code[self.ip];
            let rd = self.code[self.ip + 1] as usize;
            let rs1 = self.code[self.ip + 2] as usize;
            let rs2 = self.code[self.ip + 3] as usize;
            let imm16 = u16::from_be_bytes([self.code[self.ip + 2], self.code[self.ip + 3]]);
            self.ip += 4;

            match op {{
                0x00 /* NOP */ => {{}}
                0x01 /* MOV_IMM */ => {{ if rd < 20 {{ self.regs[rd] = imm16 as i64; }} }}
                0x02 /* MOV_REG */ => {{ if rd < 20 && rs1 < 20 {{ self.regs[rd] = self.regs[rs1]; }} }}
                0x03 /* MOVS */ => {{
                    let offset = rs1 as u64;
                    let len = rs2 as u64;
                    if rd < 20 {{ self.regs[rd] = STR_TAG | ((offset as i64) << 32) | (len as i64); }}
                }}
                0x04 /* ADD */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = self.regs[rs1].wrapping_add(self.regs[rs2]); }} }}
                0x05 /* SUB */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = self.regs[rs1].wrapping_sub(self.regs[rs2]); }} }}
                0x06 /* MUL */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = self.regs[rs1].wrapping_mul(self.regs[rs2]); }} }}
                0x07 /* DIV */ => {{
                    if rd < 20 && rs1 < 20 && rs2 < 20 {{
                        let d = self.regs[rs2];
                        self.regs[rd] = if d == 0 {{ 0 }} else {{ self.regs[rs1].wrapping_div(d) }};
                    }}
                }}
                0x08 /* MOD */ => {{
                    if rd < 20 && rs1 < 20 && rs2 < 20 {{
                        let d = self.regs[rs2];
                        self.regs[rd] = if d == 0 {{ 0 }} else {{ self.regs[rs1].wrapping_rem(d) }};
                    }}
                }}
                0x09 /* CMP_EQ */ => {{
                    if rd < 20 && rs1 < 20 && rs2 < 20 {{
                        let v1 = self.regs[rs1];
                        let v2 = self.regs[rs2];
                        let eq = if v1 == v2 {{ 1 }} else if ((v1 & STR_TAG) != 0 || (v1 & ARG_TAG) != 0) && ((v2 & STR_TAG) != 0 || (v2 & ARG_TAG) != 0) {{
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {{
                                (Some(b1), Some(b2)) => if b1 == b2 {{ 1 }} else {{ 0 }},
                                _ => 0,
                            }}
                        }} else {{ 0 }};
                        self.regs[rd] = eq;
                    }}
                }}
                0x0A /* CMP_NE */ => {{
                    if rd < 20 && rs1 < 20 && rs2 < 20 {{
                        let v1 = self.regs[rs1];
                        let v2 = self.regs[rs2];
                        let ne = if v1 == v2 {{ 0 }} else if ((v1 & STR_TAG) != 0 || (v1 & ARG_TAG) != 0) && ((v2 & STR_TAG) != 0 || (v2 & ARG_TAG) != 0) {{
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {{
                                (Some(b1), Some(b2)) => if b1 == b2 {{ 0 }} else {{ 1 }},
                                _ => 1,
                            }}
                        }} else {{ 1 }};
                        self.regs[rd] = ne;
                    }}
                }}
                0x0B /* CMP_LT */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = if self.regs[rs1] < self.regs[rs2] {{ 1 }} else {{ 0 }}; }} }}
                0x0C /* CMP_LE */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = if self.regs[rs1] <= self.regs[rs2] {{ 1 }} else {{ 0 }}; }} }}
                0x0D /* CMP_GT */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = if self.regs[rs1] > self.regs[rs2] {{ 1 }} else {{ 0 }}; }} }}
                0x0E /* CMP_GE */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = if self.regs[rs1] >= self.regs[rs2] {{ 1 }} else {{ 0 }}; }} }}
                0x0F /* JMP */ => {{ self.ip = imm16 as usize; }}
                0x10 /* JZ */ => {{ if rd < 20 && self.regs[rd] == 0 {{ self.ip = imm16 as usize; }} }}
                0x11 /* JNZ */ => {{ if rd < 20 && self.regs[rd] != 0 {{ self.ip = imm16 as usize; }} }}
                0x12 /* CALL_NAT */ => {{
                    let func_id = rs1 as u8;
                    let arg_reg = rs2;
                    let arg_val = if arg_reg < 20 {{ self.regs[arg_reg] }} else {{ 0 }};
                    let ret = match func_id {{
{native_dispatch_arms}
                    }};
                    if rd < 20 {{ self.regs[rd] = ret; }}
                }}
                0x16 /* HALT */ => break,
                0x17 /* LDC */ => {{
                    let idx = imm16 as usize;
                    if rd < 20 && idx < self.const_pool.len() {{
                        self.regs[rd] = self.const_pool[idx];
                    }}
                }}
                0x18 /* ADDI */ => {{ if rd < 20 && rs1 < 20 {{ self.regs[rd] = self.regs[rs1].wrapping_add(rs2 as i64); }} }}
                0x19 /* SUBI */ => {{ if rd < 20 && rs1 < 20 {{ self.regs[rd] = self.regs[rs1].wrapping_sub(rs2 as i64); }} }}
                0x1A /* AND */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = self.regs[rs1] & self.regs[rs2]; }} }}
                0x1B /* OR */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = self.regs[rs1] | self.regs[rs2]; }} }}
                0x1C /* XOR */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = self.regs[rs1] ^ self.regs[rs2]; }} }}
                0x1D /* SHL */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = self.regs[rs1].wrapping_shl((self.regs[rs2] & 63) as u32); }} }}
                0x1E /* SHR */ => {{ if rd < 20 && rs1 < 20 && rs2 < 20 {{ self.regs[rd] = ((self.regs[rs1] as u64) >> (self.regs[rs2] & 63)) as i64; }} }}
                0x1F /* SPILL_STORE */ => {{ if rs1 < 32 && rs2 < 20 {{ self.spill_slots[rs1] = self.regs[rs2]; }} }}
                0x20 /* SPILL_LOAD */ => {{ if rd < 20 && rs1 < 32 {{ self.regs[rd] = self.spill_slots[rs1]; }} }}
                0x21 /* ARR_DEF */ => {{
                    let arr_id = rd;
                    let len = imm16 as usize;
                    if arr_id < 8 {{
                        self.array_lens[arr_id] = len;
                        self.array_bases[arr_id] = arr_id * 32;
                    }}
                }}
                0x22 /* ASSERT */ => {{
                    if rd < 20 && self.regs[rd] == 0 {{
                        eprintln!("PulseLang assertion contract violation failed: condition evaluated to false");
                        std::process::exit(1);
                    }}
                }}
                0x23 /* CALL */ => {{
                    let target_pc = imm16 as usize;
                    if self.call_depth < 8 {{
                        self.call_stack[self.call_depth] = self.ip;
                        self.call_depth += 1;
                        self.ip = target_pc;
                    }} else {{
                        eprintln!("PulseLang call stack overflow: depth exceeded 8");
                        std::process::exit(1);
                    }}
                }}
                0x24 /* RET */ => {{
                    if self.call_depth > 0 {{
                        self.call_depth -= 1;
                        self.ip = self.call_stack[self.call_depth];
                    }} else {{
                        break;
                    }}
                }}
                0x25 /* STRUCT_DEF */ => {{}}
                0x26 /* STRUCT_LOAD */ => {{
                    let inst_id = rs1;
                    let field_idx = rs2;
                    if rd < 20 && inst_id < 8 && field_idx < 8 {{
                        self.regs[rd] = self.struct_insts[inst_id][field_idx];
                    }}
                }}
                0x27 /* STRUCT_STORE */ => {{
                    let inst_id = rd;
                    let field_idx = rs1;
                    if inst_id < 8 && field_idx < 8 && rs2 < 20 {{
                        self.struct_insts[inst_id][field_idx] = self.regs[rs2];
                    }}
                }}
                0x28 /* TBL_DEF */ => {{
                    let tbl_id = rd;
                    let base = rs1;
                    let len = rs2;
                    if tbl_id < 8 {{
                        self.table_bases[tbl_id] = base;
                        self.table_lens[tbl_id] = len;
                    }}
                }}
                0x29 /* TBL_LOAD */ => {{
                    let tbl_id = rs1;
                    let idx = if rs2 < 20 {{ self.regs[rs2] as usize }} else {{ 0 }};
                    if rd < 20 && tbl_id < 8 && idx < self.table_lens[tbl_id] {{
                        let c_idx = self.table_bases[tbl_id] + idx;
                        if c_idx < self.const_pool.len() {{
                            self.regs[rd] = self.const_pool[c_idx];
                        }}
                    }}
                }}
                0x2A /* STREQ */ => {{
                    if rd < 20 && rs1 < 20 && rs2 < 20 {{
                        let v1 = self.regs[rs1];
                        let v2 = self.regs[rs2];
                        let eq = if v1 == v2 {{ 1 }} else {{
                            match (self.get_str_bytes(v1), self.get_str_bytes(v2)) {{
                                (Some(b1), Some(b2)) if b1 == b2 => 1,
                                _ => 0,
                            }}
                        }};
                        self.regs[rd] = eq;
                    }}
                }}
                0x2B /* ARR_LOAD */ => {{
                    let arr_id = rs1;
                    let idx = if rs2 < 20 {{ self.regs[rs2] as usize }} else {{ 0 }};
                    if arr_id < 8 && idx < self.array_lens[arr_id] {{
                        let slot = self.array_bases[arr_id] + idx;
                        if slot < 256 && rd < 20 {{
                            self.regs[rd] = self.array_slots[slot];
                        }}
                    }}
                }}
                0x2C /* ARR_STORE */ => {{
                    let arr_id = rd;
                    let idx = if rs1 < 20 {{ self.regs[rs1] as usize }} else {{ 0 }};
                    if arr_id < 8 && idx < self.array_lens[arr_id] && rs2 < 20 {{
                        let slot = self.array_bases[arr_id] + idx;
                        if slot < 256 {{
                            self.array_slots[slot] = self.regs[rs2];
                        }}
                    }}
                }}
                0x2D /* DROP */ => {{}}
                _ => {{}}
            }}
        }}
    }}
}}

fn main() {{
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let str_args: Vec<&str> = raw_args.iter().map(|s| s.as_str()).collect();
    let mut vm = StandaloneVM::new(BYTECODE, &str_args);
    vm.run();
}}
"#,
        bytecode = bytecode_bytes_str,
        native_dispatch_arms = native_dispatch_arms,
    );

    let stub_file = std::env::temp_dir().join(format!("pulc_standalone_{}.rs", std::process::id()));
    fs::write(&stub_file, stub_src).map_err(|e| (2, format!("Failed to create temporary stub file: {}", e)))?;

    let mut cmd = Command::new("rustc");
    cmd.arg(&stub_file)
        .arg("--edition")
        .arg("2021")
        .arg("-C")
        .arg("opt-level=3")
        .arg("-o")
        .arg(out_path);

    let status = cmd.status().map_err(|e| {
        let _ = fs::remove_file(&stub_file);
        (2, format!("Failed to invoke rustc: {}. Ensure rustc is installed in PATH", e))
    })?;

    let _ = fs::remove_file(&stub_file);

    if !status.success() {
        return Err((1, format!("rustc compilation failed with status: {:?}", status.code())));
    }

    Ok(())
}

fn read_and_preprocess(input_path: &Path, json: bool) -> Result<String, (i32, String)> {
    let raw_src = fs::read_to_string(input_path).map_err(|e| {
        (
            2,
            format!("Cannot read input file '{}': {}", input_path.display(), e),
        )
    })?;

    let parent_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
    let mut loader = |inc_rel_path: &str| -> Option<String> {
        let full_path = parent_dir.join(inc_rel_path);
        fs::read_to_string(&full_path).ok()
    };

    preprocess_includes(&raw_src, &mut loader).map_err(|err| {
        let err_msg = format_compile_error(&err, raw_src.as_bytes(), &input_path.display().to_string(), json);
        (1, err_msg)
    })
}

fn run_compile(
    input_path: &Path,
    explicit_out: Option<PathBuf>,
    json: bool,
    verbose: bool,
) -> Result<(), (i32, String)> {
    let (out_path, is_bin) = derive_output_path(input_path, explicit_out);
    let src = read_and_preprocess(input_path, json)?;
    let mut tokens = [Token::empty(); MAX_TOKENS];
    let mut lexer = Lexer::new(src.as_bytes());
    let _tok_count = lexer.tokenize(&mut tokens).map_err(|err| {
        let err_msg = format_compile_error(&err, src.as_bytes(), &input_path.display().to_string(), json);
        (1, err_msg)
    })?;

    let mut compiler = Compiler::new(src.as_bytes(), &tokens);
    let _code_len = compiler.compile().map_err(|err| {
        let err_msg = format_compile_error(&err, src.as_bytes(), &input_path.display().to_string(), json);
        (1, err_msg)
    })?;

    let stats = compiler.stats();
    let mut out_bin = vec![0u8; stats.total_binary_size];
    let written = compile_pulse_to_binary(src.as_bytes(), &mut out_bin).map_err(|err| {
        let err_msg = format_compile_error(&err, src.as_bytes(), &input_path.display().to_string(), json);
        (1, err_msg)
    })?;

    if is_bin {
        fs::write(&out_path, &out_bin[..written]).map_err(|e| {
            (
                2,
                format!(
                    "Cannot write output binary '{}': {}",
                    out_path.display(),
                    e
                ),
            )
        })?;
    } else {
        generate_standalone_executable(&out_bin[..written], stats.imported_runtimes, &out_path)?;
    }
    if json {
        println!(
            r#"{{"success":true,"command":"compile","file":"{}","output":"{}","bytes":{},"instructions":{},"stats":{{"code_size":{},"instruction_count":{},"str_pool_len":{},"const_pool_len":{},"var_count":{},"function_count":{},"array_count":{},"struct_def_count":{},"struct_inst_count":{},"const_table_count":{},"total_binary_size":{}}}}}"#,
            escape_json(&input_path.display().to_string()),
            escape_json(&out_path.display().to_string()),
            written,
            stats.instruction_count,
            stats.code_size,
            stats.instruction_count,
            stats.str_pool_len,
            stats.const_pool_len,
            stats.var_count,
            stats.function_count,
            stats.array_count,
            stats.struct_def_count,
            stats.struct_inst_count,
            stats.const_table_count,
            stats.total_binary_size,
        );
    } else if verbose {
        println!(
            "\x1b[1;32m[pulc]\x1b[0m Compiled '{}' -> '{}'",
            input_path.display(),
            out_path.display()
        );
        println!(
            "  \x1b[1mBytecode:\x1b[0m {} bytes ({} instructions)",
            stats.code_size, stats.instruction_count
        );
        println!(
            "  \x1b[1mString Pool:\x1b[0m {} bytes | \x1b[1mConst Pool:\x1b[0m {} entries ({} bytes)",
            stats.str_pool_len,
            stats.const_pool_len,
            stats.const_pool_len * 8
        );
        println!(
            "  \x1b[1mVariables:\x1b[0m {} | \x1b[1mFunctions:\x1b[0m {} | \x1b[1mArrays:\x1b[0m {} | \x1b[1mStructs:\x1b[0m {} | \x1b[1mTables:\x1b[0m {}",
            stats.var_count,
            stats.function_count,
            stats.array_count,
            stats.struct_def_count,
            stats.const_table_count
        );
        println!("  \x1b[1mTotal Binary Size:\x1b[0m {} bytes", written);
    } else {
        println!(
            "[pulc] Compiled '{}' -> '{}' ({} bytes)",
            input_path.display(),
            out_path.display(),
            written
        );
    }

    Ok(())
}

fn run_check(input_path: &Path, json: bool, verbose: bool) -> Result<(), (i32, String)> {
    let src = read_and_preprocess(input_path, json)?;
    let stats = check(&src).map_err(|err| {
        let err_msg = format_compile_error(&err, src.as_bytes(), &input_path.display().to_string(), json);
        (1, err_msg)
    })?;

    if json {
        let mut breakdown_json = Vec::new();
        for i in 0..stats.wcet_breakdown_count {
            let item = &stats.wcet_breakdown[i];
            let name = String::from_utf8_lossy(&item.name[..item.name_len]);
            let decl_str = match item.declared_ns {
                Some(d) => format!("{}", d),
                None => "null".to_string(),
            };
            breakdown_json.push(format!(
                r#"{{"name":"{}","estimated_ns":{},"declared_ns":{}}}"#,
                escape_json(&name),
                item.estimated_ns,
                decl_str
            ));
        }

        let declared_wcet_str = match stats.declared_wcet_ns {
            Some(d) => format!("{}", d),
            None => "null".to_string(),
        };
        let declared_budget_str = match stats.declared_budget_ns {
            Some(b) => format!("{}", b),
            None => "null".to_string(),
        };

        println!(
            r#"{{"success":true,"command":"check","file":"{}","valid":true,"stats":{{"code_size":{},"instruction_count":{},"str_pool_len":{},"const_pool_len":{},"var_count":{},"function_count":{},"array_count":{},"struct_def_count":{},"struct_inst_count":{},"const_table_count":{},"total_binary_size":{}}},"wcet":{{"estimated_total_ns":{},"declared_wcet_ns":{},"declared_budget_ns":{},"breakdown":[{}]}}}}"#,
            escape_json(&input_path.display().to_string()),
            stats.code_size,
            stats.instruction_count,
            stats.str_pool_len,
            stats.const_pool_len,
            stats.var_count,
            stats.function_count,
            stats.array_count,
            stats.struct_def_count,
            stats.struct_inst_count,
            stats.const_table_count,
            stats.total_binary_size,
            stats.estimated_wcet_ns,
            declared_wcet_str,
            declared_budget_str,
            breakdown_json.join(",")
        );
    } else if verbose {
        println!(
            "\x1b[1;32m[pulc]\x1b[0m Syntax, linear type ownership, and WCET check PASSED for '{}'",
            input_path.display()
        );
        println!(
            "  \x1b[1mEstimated Bytecode:\x1b[0m {} bytes ({} instructions)",
            stats.code_size, stats.instruction_count
        );
        println!(
            "  \x1b[1mString Pool:\x1b[0m {} bytes | \x1b[1mConst Pool:\x1b[0m {} entries",
            stats.str_pool_len, stats.const_pool_len
        );
        println!(
            "  \x1b[1mFunctions:\x1b[0m {} | \x1b[1mArrays:\x1b[0m {} | \x1b[1mStructs:\x1b[0m {} | \x1b[1mTables:\x1b[0m {}",
            stats.function_count,
            stats.array_count,
            stats.struct_def_count,
            stats.const_table_count
        );
        if stats.wcet_breakdown_count > 0 {
            println!("  \x1b[1mFunction WCET Breakdown:\x1b[0m");
            for i in 0..stats.wcet_breakdown_count {
                let item = &stats.wcet_breakdown[i];
                let name = String::from_utf8_lossy(&item.name[..item.name_len]);
                if let Some(d) = item.declared_ns {
                    println!("    - {}() : {} ns (declared: {} ns)", name, item.estimated_ns, d);
                } else {
                    println!("    - {}() : {} ns", name, item.estimated_ns);
                }
            }
        }
        println!("  \x1b[1mSynthesized Total WCET:\x1b[0m {} ns", stats.estimated_wcet_ns);
    } else {
        println!(
            "[pulc] Check passed for '{}' ({} instructions, {} bytes total, est. WCET: {} ns)",
            input_path.display(),
            stats.instruction_count,
            stats.total_binary_size,
            stats.estimated_wcet_ns
        );
        for i in 0..stats.wcet_breakdown_count {
            let item = &stats.wcet_breakdown[i];
            let name = String::from_utf8_lossy(&item.name[..item.name_len]);
            if let Some(d) = item.declared_ns {
                println!("  [WCET] {}() : {} ns (contract: {} ns)", name, item.estimated_ns, d);
            } else {
                println!("  [WCET] {}() : {} ns", name, item.estimated_ns);
            }
        }
    }

    Ok(())
}

fn run_disasm(input_path: &Path, json: bool) -> Result<(), (i32, String)> {
    let data = fs::read(input_path).map_err(|e| {
        (
            2,
            format!(
                "Cannot read binary file '{}': {}",
                input_path.display(),
                e
            ),
        )
    })?;

    let filename = input_path.display().to_string();
    let mut disasm_text = String::new();
    disassemble_px64_with_filename(&data, &filename, &mut disasm_text).map_err(|err| {
        let err_msg = format_compile_error(&err, &[], &filename, json);
        (1, err_msg)
    })?;

    if json {
        println!(
            r#"{{"success":true,"command":"disasm","file":"{}","disassembly":"{}"}}"#,
            escape_json(&filename),
            escape_json(&disasm_text),
        );
    } else {
        print!("{}", disasm_text);
    }

    Ok(())
}
fn run_exec(
    input_path: &Path,
    args: &[String],
    json: bool,
    verbose: bool,
) -> Result<(), (i32, String)> {
    let args_str_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let raw_bytes = fs::read(input_path).map_err(|e| {
        (
            2,
            format!(
                "Cannot read input file '{}': {}",
                input_path.display(),
                e
            ),
        )
    })?;

    let ext = input_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_bin_ext = ext.eq_ignore_ascii_case("bin") || ext.eq_ignore_ascii_case("px64");
    let is_binary = is_bin_ext || (raw_bytes.len() >= 4 && &raw_bytes[0..4] == b"PX64");

    if is_binary {
        if verbose {
            eprintln!(
                "[pulc] Executing px64 binary: '{}' ({} bytes, {} args)",
                input_path.display(),
                raw_bytes.len(),
                args.len()
            );
        }
        pulselang_core::run_binary(&raw_bytes, &args_str_refs).map_err(|err| {
            let formatted = format_compile_error(&err, &raw_bytes, &input_path.to_string_lossy(), json);
            (1, formatted)
        })?;
    } else {
        let src = read_and_preprocess(input_path, json)?;
        if verbose {
            eprintln!(
                "[pulc] JIT executing PulseLang script: '{}' ({} chars, {} args)",
                input_path.display(),
                src.len(),
                args.len()
            );
        }
        pulselang_core::run_source(&src, &args_str_refs).map_err(|err| {
            let formatted = format_compile_error(&err, src.as_bytes(), &input_path.to_string_lossy(), json);
            (1, formatted)
        })?;
    }

    Ok(())
}


fn format_compile_error(
    err: &CompileError,
    src: &[u8],
    filename: &str,
    json: bool,
) -> String {
    let mut out = String::new();
    if json {
        let _ = err.format_json(filename, &mut out);
    } else {
        let _ = err.format_diagnostic_ansi(src, filename, &mut out);
    }
    out
}
fn run_test(input_path: &Path, filter: Option<&str>, json: bool, _verbose: bool) -> Result<(), (i32, String)> {
    let preprocessed = read_and_preprocess(input_path, json)?;
    let tests = pulselang_core::compile_pulse_tests(preprocessed.as_bytes())
        .map_err(|e| (1, format_compile_error(&e, preprocessed.as_bytes(), &input_path.to_string_lossy(), json)))?;

    let filtered_tests: Vec<_> = if let Some(f) = filter {
        tests.into_iter().filter(|t| t.name.to_lowercase().contains(&f.to_lowercase())).collect()
    } else {
        tests
    };
    let mut passed = 0;
    let mut failed = 0;
    let mut budget_violations = 0;
    let mut total_elapsed_ns: u64 = 0;

    if json {
        let mut results = Vec::new();
        for t in &filtered_tests {
            let res = pulselang_core::run_test_case(t);
            total_elapsed_ns = total_elapsed_ns.saturating_add(res.elapsed_ns);

            let status = if res.passed {
                if let Some(b) = res.budget_ns {
                    if res.elapsed_ns > b {
                        budget_violations += 1;
                        "budget_exceeded"
                    } else {
                        passed += 1;
                        "pass"
                    }
                } else {
                    passed += 1;
                    "pass"
                }
            } else {
                failed += 1;
                "fail"
            };

            let err_field = match &res.error {
                Some(e) => format!(r#","error":"{}""#, escape_json(e)),
                None => String::new(),
            };
            let budget_field = match res.budget_ns {
                Some(b) => format!(r#","budget_ns":{}"#, b),
                None => r#","budget_ns":null"#.to_string(),
            };

            results.push(format!(
                r#"{{"name":"{}","status":"{}","line":{},"elapsed_ns":{},"steps":{}{}{}}}"#,
                escape_json(&t.name),
                status,
                t.line,
                res.elapsed_ns,
                res.steps,
                budget_field,
                err_field
            ));
        }
        println!(
            r#"{{"file":"{}","total":{},"passed":{},"failed":{},"budget_violations":{},"elapsed_ns":{},"tests":[{}]}}"#,
            escape_json(&input_path.to_string_lossy()),
            filtered_tests.len(),
            passed,
            failed,
            budget_violations,
            total_elapsed_ns,
            results.join(",")
        );
    } else {
        println!("[pulc test] Running {} tests from '{}'...", filtered_tests.len(), input_path.display());
        for t in &filtered_tests {
            let res = pulselang_core::run_test_case(t);
            total_elapsed_ns = total_elapsed_ns.saturating_add(res.elapsed_ns);

            let budget_str = match res.budget_ns {
                Some(b) => format!(", budget: {} ns", b),
                None => String::new(),
            };

            if res.passed {
                if let Some(b) = res.budget_ns {
                    if res.elapsed_ns > b {
                        budget_violations += 1;
                        println!(
                            "  test \"{}\" ... BUDGET_EXCEEDED: elapsed {} ns > budget {} ns (steps: {})",
                            t.name, res.elapsed_ns, b, res.steps
                        );
                    } else {
                        passed += 1;
                        println!(
                            "  test \"{}\" ... PASS (elapsed: {} ns, steps: {}{})",
                            t.name, res.elapsed_ns, res.steps, budget_str
                        );
                    }
                } else {
                    passed += 1;
                    println!(
                        "  test \"{}\" ... PASS (elapsed: {} ns, steps: {})",
                        t.name, res.elapsed_ns, res.steps
                    );
                }
            } else {
                failed += 1;
                let err_msg = res.error.as_deref().unwrap_or("Assertion failed");
                println!(
                    "  test \"{}\" ... FAIL: {} (elapsed: {} ns, steps: {}{})",
                    t.name, err_msg, res.elapsed_ns, res.steps, budget_str
                );
            }
        }
        println!("--------------------------------------------------------------------------------");
        println!(
            "Test result: {} passed, {} failed, {} budget violations in {} ns",
            passed, failed, budget_violations, total_elapsed_ns
        );
    }

    if failed > 0 || budget_violations > 0 {
        Err((1, format!("{} tests failed", failed + budget_violations)))
    } else {
        Ok(())
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn main() -> ExitCode {
    let raw_args = env::args().skip(1);
    let options = match parse_cli_args(raw_args) {
        Ok(opts) => opts,
        Err(err_msg) => {
            eprintln!("\x1b[1;31merror\x1b[0m: {}", err_msg);
            eprintln!("Try 'pulc --help' for more information.");
            return ExitCode::from(2);
        }
    };

    let result = match &options.subcommand {
        Subcommand::Help => {
            print_help();
            Ok(())
        }
        Subcommand::Version => {
            print_version();
            Ok(())
        }
        Subcommand::Run { input, args } => {
            run_exec(input, args, options.json, options.verbose)
        }
        Subcommand::Compile { input, output } => {
            run_compile(input, output.clone(), options.json, options.verbose)
        }
        Subcommand::Test { input, filter } => {
            run_test(input, filter.as_deref(), options.json, options.verbose)
        }
        Subcommand::Check { input } => run_check(input, options.json, options.verbose),
        Subcommand::Disasm { input } => run_disasm(input, options.json),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, err_msg)) => {
            if options.json {
                if !err_msg.starts_with('{') {
                    eprintln!(
                        r#"{{"success":false,"error":{{"code":"ERR_CLI","message":"{}"}}}}"#,
                        escape_json(&err_msg)
                    );
                } else {
                    eprintln!("{}", err_msg);
                }
            } else {
                eprintln!("{}", err_msg);
            }
            ExitCode::from(code as u8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_default_compile() {
        let args = vec!["script.pul".to_string()];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert_eq!(
            opts.subcommand,
            Subcommand::Compile {
                input: PathBuf::from("script.pul"),
                output: None,
            }
        );
        assert!(!opts.json);
    }

    #[test]
    fn test_cli_parse_compile_with_output() {
        let args = vec![
            "compile".to_string(),
            "test.pul".to_string(),
            "-o".to_string(),
            "out.bin".to_string(),
            "--json".to_string(),
        ];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert_eq!(
            opts.subcommand,
            Subcommand::Compile {
                input: PathBuf::from("test.pul"),
                output: Some(PathBuf::from("out.bin")),
            }
        );
        assert!(opts.json);
    }

    #[test]
    fn test_cli_parse_check() {
        let args = vec!["check".to_string(), "foo.pul".to_string(), "-v".to_string()];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert_eq!(
            opts.subcommand,
            Subcommand::Check {
                input: PathBuf::from("foo.pul"),
            }
        );
        assert!(opts.verbose);
    }

    #[test]
    fn test_cli_parse_disasm() {
        let args = vec!["disasm".to_string(), "program.bin".to_string()];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert_eq!(
            opts.subcommand,
            Subcommand::Disasm {
                input: PathBuf::from("program.bin"),
            }
        );
    }

    #[test]
    fn test_cli_parse_disasm_flag() {
        let args = vec!["-d".to_string(), "program.bin".to_string()];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert_eq!(
            opts.subcommand,
            Subcommand::Disasm {
                input: PathBuf::from("program.bin"),
            }
        );
    }

    #[test]
    fn test_derive_output_path() {
        let input = Path::new("path/to/my_script.pul");
        let (derived, is_bin) = derive_output_path(input, None);
        assert!(!is_bin);
        #[cfg(windows)]
        assert_eq!(derived, PathBuf::from("path/to/my_script.exe"));

        let (custom_bin, is_custom_bin) = derive_output_path(input, Some(PathBuf::from("custom.bin")));
        assert!(is_custom_bin);
        assert_eq!(custom_bin, PathBuf::from("custom.bin"));
    }

    #[test]
    fn test_cli_parse_run_pul() {
        let args = vec!["run".to_string(), "script.pul".to_string()];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert_eq!(
            opts.subcommand,
            Subcommand::Run {
                input: PathBuf::from("script.pul"),
                args: Vec::new(),
            }
        );
    }

    #[test]
    fn test_cli_parse_run_bin_with_args() {
        let args = vec![
            "run".to_string(),
            "app.bin".to_string(),
            "arg1".to_string(),
            "42".to_string(),
            "--flag".to_string(),
        ];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert_eq!(
            opts.subcommand,
            Subcommand::Run {
                input: PathBuf::from("app.bin"),
                args: vec!["arg1".to_string(), "42".to_string(), "--flag".to_string()],
            }
        );
    }

    #[test]
    fn test_cli_parse_run_verbose_flag_before() {
        let args = vec![
            "-v".to_string(),
            "run".to_string(),
            "test.pul".to_string(),
            "x".to_string(),
        ];
        let opts = parse_cli_args(args.into_iter()).unwrap();
        assert!(opts.verbose);
        assert_eq!(
            opts.subcommand,
            Subcommand::Run {
                input: PathBuf::from("test.pul"),
                args: vec!["x".to_string()],
            }
        );
    }

    #[test]
    fn test_run_exec_pul_file() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_run_exec.pul");
        fs::write(&file_path, "let $x = 10; let $y = 20; @assert($x + $y == 30);").unwrap();
        let res = run_exec(&file_path, &[], false, false);
        let _ = fs::remove_file(&file_path);
        assert!(res.is_ok());
    }

    #[test]
    fn test_run_exec_bin_file() {
        let dir = std::env::temp_dir();
        let pul_path = dir.join("test_run_bin.pul");
        let bin_path = dir.join("test_run_bin.bin");
        let src = "@println(\"Hello from compiled binary!\");";
        fs::write(&pul_path, src).unwrap();

        // Compile to bin
        let comp_res = run_compile(&pul_path, Some(bin_path.clone()), false, false);
        assert!(comp_res.is_ok());

        // Run binary
        let run_res = run_exec(&bin_path, &[], false, false);
        let _ = fs::remove_file(&pul_path);
        let _ = fs::remove_file(&bin_path);
        assert!(run_res.is_ok());
    }

    #[test]
    fn test_run_exec_with_arguments() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_run_args.pul");
        let src = r#"
            let $c = @argc();
            @assert($c == 2);
            let $a0 = @arg(0);
            let $a1 = @arg(1);
            @assert($a0 == "arg_one");
            @assert($a1 == "arg_two");
        "#;
        fs::write(&file_path, src).unwrap();
        let args = vec!["arg_one".to_string(), "arg_two".to_string()];
        let res = run_exec(&file_path, &args, false, false);
        let _ = fs::remove_file(&file_path);
        assert!(res.is_ok());
    }

    #[test]
    fn test_run_exec_runtime_error() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("test_run_err.pul");
        fs::write(&file_path, "let $x = 10; @assert($x == 999);").unwrap();
        let res = run_exec(&file_path, &[], false, false);
        let _ = fs::remove_file(&file_path);
        assert!(res.is_err());
        let (code, err_msg) = res.unwrap_err();
        assert_eq!(code, 1);
        assert!(err_msg.contains("ERR_PX64_ASSERTION_FAILED"));
    }

    #[test]
    fn test_run_exec_file_not_found() {
        let non_existent = PathBuf::from("this_file_does_not_exist_12345.pul");
        let res = run_exec(&non_existent, &[], false, false);
        assert!(res.is_err());
        let (code, _err_msg) = res.unwrap_err();
        assert_eq!(code, 2);
    }
}
