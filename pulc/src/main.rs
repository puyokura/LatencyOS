//! `pulc` - PulseLang Host Compiler & Toolchain CLI for LatencyOS

use pulselang_core::{
    check, compile_pulse_to_binary, disassemble_px64_with_filename, CompileError,
    Compiler, Lexer, Token, MAX_TOKENS,
};
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
    pulc disasm <file.bin>
    pulc -d <file.bin>

\x1b[1mSUBCOMMANDS:\x1b[0m
    run <file> [args...]  Execute px64 binary (.bin) or source script (.pul) directly
    compile <file.pul>    Compile PulseLang source into px64 binary bytecode
    check <file.pul>      Validate syntax, types, linear ownership & WCET constraints
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

fn derive_output_path(input: &Path, explicit_out: Option<PathBuf>) -> PathBuf {
    if let Some(out) = explicit_out {
        return out;
    }
    let mut out = input.to_path_buf();
    out.set_extension("bin");
    out
}

fn run_compile(
    input_path: &Path,
    explicit_out: Option<PathBuf>,
    json: bool,
    verbose: bool,
) -> Result<(), (i32, String)> {
    let out_path = derive_output_path(input_path, explicit_out);
    let src = fs::read_to_string(input_path).map_err(|e| {
        (
            2,
            format!(
                "Cannot read input file '{}': {}",
                input_path.display(),
                e
            ),
        )
    })?;

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
    let src = fs::read_to_string(input_path).map_err(|e| {
        (
            2,
            format!(
                "Cannot read input file '{}': {}",
                input_path.display(),
                e
            ),
        )
    })?;

    let stats = check(&src).map_err(|err| {
        let err_msg = format_compile_error(&err, src.as_bytes(), &input_path.display().to_string(), json);
        (1, err_msg)
    })?;

    if json {
        println!(
            r#"{{"success":true,"command":"check","file":"{}","valid":true,"stats":{{"code_size":{},"instruction_count":{},"str_pool_len":{},"const_pool_len":{},"var_count":{},"function_count":{},"array_count":{},"struct_def_count":{},"struct_inst_count":{},"const_table_count":{},"total_binary_size":{}}}}}"#,
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
    } else {
        println!(
            "[pulc] Check passed for '{}' ({} instructions, {} bytes total)",
            input_path.display(),
            stats.instruction_count,
            stats.total_binary_size
        );
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
        let src = core::str::from_utf8(&raw_bytes).map_err(|e| {
            (
                2,
                format!(
                    "File '{}' is not valid UTF-8 source code: {}",
                    input_path.display(),
                    e
                ),
            )
        })?;

        if verbose {
            eprintln!(
                "[pulc] Compiling and executing source: '{}' ({} bytes, {} args)",
                input_path.display(),
                src.len(),
                args.len()
            );
        }

        pulselang_core::run_source(src, &args_str_refs).map_err(|err| {
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

    let result = match options.subcommand {
        Subcommand::Help => {
            print_help();
            Ok(())
        }
        Subcommand::Version => {
            print_version();
            Ok(())
        }
        Subcommand::Run { input, args } => {
            run_exec(&input, &args, options.json, options.verbose)
        }
        Subcommand::Compile { input, output } => {
            run_compile(&input, output, options.json, options.verbose)
        }
        Subcommand::Check { input } => run_check(&input, options.json, options.verbose),
        Subcommand::Disasm { input } => run_disasm(&input, options.json),
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
        let derived = derive_output_path(input, None);
        assert_eq!(derived, PathBuf::from("path/to/my_script.bin"));

        let custom = derive_output_path(input, Some(PathBuf::from("custom.bin")));
        assert_eq!(custom, PathBuf::from("custom.bin"));
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
