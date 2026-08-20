use std::env;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn get_workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    if manifest_dir.ends_with("xtask") {
        manifest_dir.parent().unwrap().to_path_buf()
    } else {
        manifest_dir
    }
}

fn get_augmented_path() -> String {
    let mut paths = Vec::new();

    if let Ok(user_profile) = env::var("USERPROFILE") {
        let profile = PathBuf::from(user_profile);
        paths.push(profile.join("scoop").join("apps").join("llvm").join("current").join("bin"));
        paths.push(profile.join("scoop").join("apps").join("rustup").join("current").join(".cargo").join("bin"));
        paths.push(profile.join("scoop").join("apps").join("QEMU").join("current"));
        paths.push(profile.join("scoop").join("shims"));
    }

    if let Ok(current_path) = env::var("PATH") {
        let current_entries: Vec<PathBuf> = env::split_paths(&current_path).collect();
        paths.extend(current_entries);
    }

    env::join_paths(paths.into_iter()).unwrap().to_string_lossy().to_string()
}

fn find_tool(tool_name: &str) -> PathBuf {
    let augmented_path = get_augmented_path();
    for path in env::split_paths(&augmented_path) {
        let candidate = path.join(tool_name);
        if candidate.exists() {
            return candidate;
        }
        let candidate_exe = path.join(format!("{}.exe", tool_name));
        if candidate_exe.exists() {
            return candidate_exe;
        }
    }
    PathBuf::from(tool_name)
}

fn run_cargo_build(release: bool) -> PathBuf {
    let root = get_workspace_root();
    let cargo = find_tool("cargo");

    println!("[xtask] Building LatencyOS kernel...");
    let mut cmd = Command::new(&cargo);
    cmd.current_dir(&root)
        .env("PATH", get_augmented_path())
        .arg("build")
        .arg("--package")
        .arg("kernel")
        .arg("--target")
        .arg("x86_64-unknown-none");

    if release {
        cmd.arg("--release");
    }

    let status = cmd.status().expect("Failed to execute cargo build");
    if !status.success() {
        eprintln!("[xtask] Build failed with status: {}", status);
        std::process::exit(1);
    }

    let profile_dir = if release { "release" } else { "debug" };
    root.join("target")
        .join("x86_64-unknown-none")
        .join(profile_dir)
        .join("kernel")
}

fn run_qemu(kernel_elf: &Path, capture_output: bool, timeout_secs: u64) -> Option<String> {
    let qemu = find_tool("qemu-system-x86_64");
    println!("[xtask] Launching QEMU with kernel: {}", kernel_elf.display());

    let mut cmd = Command::new(&qemu);
    cmd.arg("-kernel")
        .arg(kernel_elf)
        .arg("-cpu")
        .arg("max")
        .arg("-serial")
        .arg("stdio")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .arg("-no-shutdown")
        .arg("-m")
        .arg("256M")
        .arg("-smp")
        .arg("4")
        .arg("-netdev")
        .arg("user,id=net0")
        .arg("-device")
        .arg("e1000,netdev=net0")
        .env("PATH", get_augmented_path());

    if !capture_output {
        let status = cmd.status().expect("Failed to run QEMU");
        println!("[xtask] QEMU exited with status: {}", status);
        None
    } else {
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().expect("Failed to spawn QEMU process");
        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let reader = BufReader::new(stdout);

        let mut output_lines = Vec::new();
        let start_time = Instant::now();

        // Read lines in non-blocking fashion with timeout
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in reader.lines() {
                if let Ok(l) = line {
                    let _ = tx.send(l);
                } else {
                    break;
                }
            }
        });

        loop {
            if let Ok(line) = rx.recv_timeout(Duration::from_millis(100)) {
                println!("{}", line);
                let is_complete = line.contains("initialization complete");
                output_lines.push(line);
                if is_complete {
                    // Small delay to allow trailing output
                    std::thread::sleep(Duration::from_millis(300));
                    break;
                }
            }

            if start_time.elapsed() > Duration::from_secs(timeout_secs) {
                println!("[xtask] Timeout ({}s) reached waiting for QEMU output", timeout_secs);
                break;
            }
        }

        let _ = child.kill();
        let _ = child.wait();

        Some(output_lines.join("\n"))
    }
}

fn check_versions() {
    let tools = [
        ("rustup", "rustup --version"),
        ("rustc", "rustc --version"),
        ("cargo", "cargo --version"),
        ("clang", "clang --version"),
        ("nasm", "nasm -v"),
        ("qemu-system-x86_64", "qemu-system-x86_64 --version"),
    ];

    println!("=== Toolchain Versions ===");
    for (name, _) in tools {
        let exe = find_tool(name);
        let output = Command::new(&exe)
            .arg(if name == "nasm" { "-v" } else { "--version" })
            .env("PATH", get_augmented_path())
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let first_line = stdout.lines().next().unwrap_or("").trim();
                println!("{}: {} ({})", name, first_line, exe.display());
            }
            Err(e) => {
                println!("{}: NOT FOUND or ERROR: {} ({})", name, e, exe.display());
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("run");
    let release = args.iter().any(|arg| arg == "--release");

    match command {
        "build" => {
            let kernel_path = run_cargo_build(release);
            println!("[xtask] Build finished successfully: {}", kernel_path.display());
        }
        "run" => {
            let kernel_path = run_cargo_build(release);
            let output = run_qemu(&kernel_path, true, 90);
            if let Some(out) = output {
                if out.contains("LatencyOS Core0 booted") {
                    println!("\n[xtask] SUCCESS: LatencyOS Core 0 booted successfully!");
                } else {
                    eprintln!("\n[xtask] WARNING: Boot banner not detected in output.");
                }
            }
        }
        "interactive" => {
            let kernel_path = run_cargo_build(release);
            run_qemu(&kernel_path, false, 0);
        }
        "check" => {
            check_versions();
        }
        _ => {
            eprintln!("Usage: cargo xtask [build|run|interactive|check] [--release]");
            std::process::exit(1);
        }
    }
}
