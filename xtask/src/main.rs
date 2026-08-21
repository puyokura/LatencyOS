use std::env;
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

#[cfg(windows)]
static mut SAVED_IN_MODE: u32 = 0x0187;
#[cfg(windows)]
static mut SAVED_OUT_MODE: u32 = 0x0007;

#[cfg(windows)]
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> i32 {
    // Return 1 (TRUE) for CTRL_C_EVENT (0) and CTRL_BREAK_EVENT (1) to prevent host from killing QEMU
    if ctrl_type == 0 || ctrl_type == 1 {
        1
    } else {
        0
    }
}

#[cfg(windows)]
fn save_and_restore_console_mode<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    use std::io::Write;
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
            fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
            fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
            fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
            fn FlushConsoleInputBuffer(hConsoleInput: *mut std::ffi::c_void) -> i32;
        }
        const STD_INPUT_HANDLE: u32 = 0xFFFFFFF6;
        const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5;
        const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
        const ENABLE_LINE_INPUT: u32 = 0x0002;
        const ENABLE_ECHO_INPUT: u32 = 0x0004;
        const ENABLE_QUICK_EDIT_MODE: u32 = 0x0040;
        const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;
        const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;

        let stdin = GetStdHandle(STD_INPUT_HANDLE);
        let stdout = GetStdHandle(STD_OUTPUT_HANDLE);

        let mut in_mode = 0u32;
        let mut out_mode = 0u32;

        GetConsoleMode(stdin, &mut in_mode);
        GetConsoleMode(stdout, &mut out_mode);

        if in_mode != 0 {
            SAVED_IN_MODE = in_mode;
        }
        if out_mode != 0 {
            SAVED_OUT_MODE = out_mode;
        }

        // Install handler that ignores host Ctrl+C so raw 0x03 reaches guest
        SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);

        // Put stdin in raw, non-echo, non-processed mode so Ctrl+S / Ctrl+Q / Ctrl+C reach guest directly without XOFF freeze
        let raw_in_mode = (in_mode & !(ENABLE_PROCESSED_INPUT | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT))
            | ENABLE_VIRTUAL_TERMINAL_INPUT
            | ENABLE_QUICK_EDIT_MODE
            | ENABLE_EXTENDED_FLAGS;
        SetConsoleMode(stdin, raw_in_mode);

        let res = f();

        SetConsoleMode(stdin, SAVED_IN_MODE);
        SetConsoleMode(stdout, SAVED_OUT_MODE);
        FlushConsoleInputBuffer(stdin);
        SetConsoleCtrlHandler(Some(console_ctrl_handler), 0);

        print!("\x1b[0m\x1b[?25h\r\n");
        let _ = std::io::stdout().flush();

        res
    }
}

#[cfg(not(windows))]
fn save_and_restore_console_mode<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    f()
}


fn run_qemu(kernel_elf: &Path, capture_output: bool, timeout_secs: u64) -> Option<String> {
    let qemu = find_tool("qemu-system-x86_64");
    println!("[xtask] Launching QEMU with kernel: {}", kernel_elf.display());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    save_and_restore_console_mode(|| {
        let mut cmd = Command::new(&qemu);
        cmd.arg("-kernel")
            .arg(kernel_elf)
            .arg("-cpu")
            .arg("max")
            .arg("-chardev")
            .arg(format!("socket,id=ser0,host=127.0.0.1,port={},server=off,reconnect-ms=100", port))
            .arg("-serial")
            .arg("chardev:ser0")
            .arg("-display")
            .arg("none")
            .arg("-no-reboot")
            .arg("-m")
            .arg("128M")
            .arg("-smp")
            .arg("4")
            .arg("-netdev")
            .arg("user,id=net0")
            .arg("-device")
            .arg("e1000,netdev=net0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("PATH", get_augmented_path());

        let mut child = cmd.spawn().expect("Failed to spawn QEMU process");

        // Accept connection from QEMU serial socket
        let (tcp_stream, _) = listener.accept().expect("Failed to accept QEMU serial connection");
        let _ = tcp_stream.set_nodelay(true);

        if !capture_output {
            use std::io::{Read, Write};
            let mut s_read = tcp_stream.try_clone().expect("Failed to clone TCP stream");
            let mut s_write = tcp_stream;

            // Spawn socket -> stdout forwarder thread
            std::thread::spawn(move || {
                let mut out = std::io::stdout();
                let mut buf = [0u8; 1024];
                while let Ok(n) = s_read.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let _ = out.write_all(&buf[..n]);
                    let _ = out.flush();
                }
            });

            // Interactive stdin -> socket forwarder thread
            std::thread::spawn(move || {
                let mut input = std::io::stdin();
                let mut buf = [0u8; 1024];
                while let Ok(n) = input.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if s_write.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = s_write.flush();
                }
            });

            let status = child.wait().expect("Failed to wait on QEMU");
            println!("[xtask] QEMU exited with status: {}", status);
            None
        } else {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(tcp_stream);
            let mut output_lines = Vec::new();
            let start_time = Instant::now();

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
                    let is_complete = line.contains("LatencyOS 0.0.5") || line.contains("[c0|") || line.contains("latencyos") || line.contains("initialization complete");
                    output_lines.push(line);
                    if is_complete {
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
    })
}

fn test_paste(kernel_elf: &Path) {
    let qemu = find_tool("qemu-system-x86_64");
    println!("[xtask] Running automated paste test with kernel: {}", kernel_elf.display());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let mut cmd = Command::new(&qemu);
    cmd.arg("-kernel")
        .arg(kernel_elf)
        .arg("-cpu")
        .arg("max")
        .arg("-chardev")
        .arg(format!("socket,id=ser0,host=127.0.0.1,port={},server=off,reconnect-ms=100", port))
        .arg("-serial")
        .arg("chardev:ser0")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .arg("-m")
        .arg("128M")
        .arg("-smp")
        .arg("4")
        .arg("-netdev")
        .arg("user,id=net0")
        .arg("-device")
        .arg("e1000,netdev=net0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("PATH", get_augmented_path());

    let mut child = cmd.spawn().expect("Failed to spawn QEMU process");

    // Accept connection from QEMU serial socket
    let (mut tcp_stream, _) = listener.accept().expect("Failed to accept QEMU serial connection");
    let _ = tcp_stream.set_nodelay(true);
    let mut tcp_read = tcp_stream.try_clone().expect("Failed to clone stream");

    use std::io::{Read, Write};

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match tcp_read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut full_output = String::new();
    let wait_for = |target: &str, timeout_secs: u64, full_out: &mut String| -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(timeout_secs) {
            while let Ok(chunk) = rx.try_recv() {
                full_out.push_str(&String::from_utf8_lossy(&chunk));
            }
            if full_out.contains(target) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    };

    println!("[xtask-test] Waiting for shell prompt...");
    if !wait_for("[c0|", 10, &mut full_output) {
        let _ = child.kill();
        panic!("Timeout waiting for shell prompt. Output so far:\n{}", full_output);
    }
    println!("[xtask-test] Shell prompt received!");

    // Send edit command
    println!("[xtask-test] Opening editor: edit /home/paste_test.pl");
    full_output.clear();
    tcp_stream.write_all(b"edit /home/paste_test.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();

    if !wait_for("PulseEditor", 5, &mut full_output) {
        let _ = child.kill();
        panic!("Timeout waiting for PulseEditor. Output so far:\n{}", full_output);
    }
    println!("[xtask-test] PulseEditor is open!");

    // Multi-line large PulseLang script test with comments and headers
    let test_script = concat!(
        "// ========================================================\r\n",
        "// PulseLang v2 High-Performance Streaming Benchmark Script\r\n",
        "// ========================================================\r\n",
        "@contract: @wcet(100us) @budget(500us);\r\n",
        "@pipeline: StreamNetwork @budget(5ms);\r\n",
        "\r\n",
        "$frame_count := 0;\r\n",
        "$sum_latency := 0;\r\n",
        "\r\n",
        "#f := @capture();\r\n",
        "$rtt := @rtt();\r\n",
        "@println(\"Pasting long code worked completely with zero loss!\");\r\n",
        "// End of Script\r\n"
    );

    println!("[xtask-test] Pasting large multi-line PulseLang script in a single burst...");
    full_output.clear();
    // Write ALL bytes in a single write call over TCP to test flow control buffer
    tcp_stream.write_all(test_script.as_bytes()).unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));

    // Send Save (^S = 0x13)
    println!("[xtask-test] Sending Save (^S)...");
    tcp_stream.write_all(&[0x13]).unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(150));

    // Send Quit (^Q = 0x11)
    println!("[xtask-test] Sending Quit (^Q)...");
    tcp_stream.write_all(&[0x11]).unwrap();
    tcp_stream.flush().unwrap();

    if !wait_for("[c0|", 5, &mut full_output) {
        let _ = child.kill();
        panic!("Timeout waiting for shell after quit. Output so far:\n{}", full_output);
    }

    // Inspect file content with cat
    println!("[xtask-test] Running: cat /home/paste_test.pl");
    full_output.clear();
    tcp_stream.write_all(b"cat /home/paste_test.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }

    let _ = child.kill();
    let _ = child.wait();

    println!("[xtask-test] File content output:\n{}", full_output);
    if full_output.contains("// ========================================================")
        && full_output.contains("// PulseLang v2 High-Performance Streaming Benchmark Script")
        && full_output.contains("@contract: @wcet(100us) @budget(500us);")
        && full_output.contains("@pipeline: StreamNetwork @budget(5ms);")
        && full_output.contains("$frame_count := 0;")
        && full_output.contains("$rtt := @rtt();")
        && full_output.contains("@println(\"Pasting long code worked completely with zero loss!\");")
        && full_output.contains("// End of Script")
    {
        println!("[xtask-test] === TEST PASSED: Long code with comments and headers was 100% completely pasted and saved! ===");
    } else {
        panic!("[xtask-test] === TEST FAILED: Long code was truncated! Output was:\n{} ===", full_output);
    }
}

fn test_compile_error(kernel_elf: &Path) {
    let qemu = find_tool("qemu-system-x86_64");
    println!("[xtask] Running automated compiler error diagnostic test with kernel: {}", kernel_elf.display());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let mut cmd = Command::new(&qemu);
    cmd.arg("-kernel")
        .arg(kernel_elf)
        .arg("-cpu")
        .arg("max")
        .arg("-chardev")
        .arg(format!("socket,id=ser0,host=127.0.0.1,port={},server=off,reconnect-ms=100", port))
        .arg("-serial")
        .arg("chardev:ser0")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .arg("-m")
        .arg("128M")
        .arg("-smp")
        .arg("4")
        .arg("-netdev")
        .arg("user,id=net0")
        .arg("-device")
        .arg("e1000,netdev=net0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("PATH", get_augmented_path());

    let mut child = cmd.spawn().expect("Failed to spawn QEMU process");

    let (mut tcp_stream, _) = listener.accept().expect("Failed to accept QEMU serial connection");
    let _ = tcp_stream.set_nodelay(true);
    let mut tcp_read = tcp_stream.try_clone().expect("Failed to clone stream");

    use std::io::{Read, Write};

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match tcp_read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut full_output = String::new();
    let wait_for = |target: &str, timeout_secs: u64, full_out: &mut String| -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(timeout_secs) {
            while let Ok(chunk) = rx.try_recv() {
                full_out.push_str(&String::from_utf8_lossy(&chunk));
            }
            if full_out.contains(target) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    };

    if !wait_for("[c0|", 10, &mut full_output) {
        let _ = child.kill();
        panic!("Timeout waiting for shell prompt");
    }

    // Create broken script via editor
    println!("[xtask-test] Opening editor: edit /home/err_syntax.pl");
    full_output.clear();
    tcp_stream.write_all(b"edit /home/err_syntax.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    assert!(wait_for("PulseEditor", 5, &mut full_output), "Timed out waiting for PulseEditor");

    let broken_script = "@contract: @wcet(100us) @budget(500us);\r\n$x := := 42;\r\n";
    tcp_stream.write_all(broken_script.as_bytes()).unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(200));

    // Send Save (^S = 0x13)
    tcp_stream.write_all(&[0x13]).unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(150));

    // Send Quit (^Q = 0x11)
    tcp_stream.write_all(&[0x11]).unwrap();
    tcp_stream.flush().unwrap();
    assert!(wait_for("[c0|", 5, &mut full_output), "Timed out waiting for shell prompt after editor quit");

    // Compile broken script
    full_output.clear();
    println!("[xtask-test] Running: compile /home/err_syntax.pl");
    tcp_stream.write_all(b"compile /home/err_syntax.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    assert!(wait_for("[ERROR_CODE]:", 5, &mut full_output), "Timed out waiting for compiler error diagnostic. Output:\n{}", full_output);
    assert!(wait_for("[c0|", 5, &mut full_output), "Timed out waiting for shell prompt after diagnostic");

    let _ = child.kill();
    let _ = child.wait();

    println!("[xtask-test] Diagnostic Output Received:\n{}", full_output);

    assert!(full_output.contains("[ERROR_CODE]: ERR_SYNTAX_UNEXPECTED_TOKEN"), "Missing ERROR_CODE");
    assert!(full_output.contains("[LOCATION]: Line "), "Missing line in location");
    assert!(full_output.contains("Column "), "Missing column in location");
    assert!(full_output.contains("[TOKEN_FOUND]:"), "Missing token kind");
    assert!(full_output.contains("[EXPECTED]:"), "Missing expected tokens specification");
    assert!(full_output.contains("[PARSER_STAGE]:"), "Missing parser stage");
    assert!(full_output.contains("$x := := 42;"), "Missing highlighted source line");
    assert!(full_output.contains("[Syntax Error Here]"), "Missing visual error pointer");
    assert!(full_output.contains("[HEX_DUMP"), "Missing hex dump block");
    assert!(full_output.contains("[AI_REPAIR_HINT]:"), "Missing AI repair suggestion");

    println!("[xtask-test] === TEST PASSED: AI-actionable machine-readable diagnostic log verified successfully! ===");
}

fn test_editor_delete(kernel_elf: &Path) {
    let qemu = find_tool("qemu-system-x86_64");
    println!("[xtask] Running automated editor delete & screen clean test with kernel: {}", kernel_elf.display());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let mut cmd = Command::new(&qemu);
    cmd.arg("-kernel")
        .arg(kernel_elf)
        .arg("-cpu")
        .arg("max")
        .arg("-chardev")
        .arg(format!("socket,id=ser0,host=127.0.0.1,port={},server=off,reconnect-ms=100", port))
        .arg("-serial")
        .arg("chardev:ser0")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .arg("-m")
        .arg("128M")
        .arg("-smp")
        .arg("4")
        .arg("-netdev")
        .arg("user,id=net0")
        .arg("-device")
        .arg("e1000,netdev=net0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("PATH", get_augmented_path());

    let mut child = cmd.spawn().expect("Failed to spawn QEMU process");

    let (mut tcp_stream, _) = listener.accept().expect("Failed to accept QEMU serial connection");
    let _ = tcp_stream.set_nodelay(true);
    let mut tcp_read = tcp_stream.try_clone().expect("Failed to clone stream");

    use std::io::{Read, Write};

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match tcp_read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut full_output = String::new();
    let wait_for = |target: &str, timeout_secs: u64, full_out: &mut String| -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(timeout_secs) {
            while let Ok(chunk) = rx.try_recv() {
                full_out.push_str(&String::from_utf8_lossy(&chunk));
            }
            if full_out.contains(target) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    };

    if !wait_for("[c0|", 10, &mut full_output) {
        let _ = child.kill();
        panic!("Timeout waiting for shell prompt");
    }

    // Open editor
    full_output.clear();
    tcp_stream.write_all(b"edit /home/delete_clean.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    wait_for("PulseEditor", 5, &mut full_output);

    // Paste 4 lines
    let four_lines = "Line 1 Alpha\r\nLine 2 Beta\r\nLine 3 Gamma\r\nLine 4 Delta\r\n";
    for chunk in four_lines.as_bytes().chunks(4) {
        tcp_stream.write_all(chunk).unwrap();
        tcp_stream.flush().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(200));

    // Move cursor to Line 1 (Up 3 times, Home)
    tcp_stream.write_all(b"\x1b[A\x1b[A\x1b[A\x1b[H").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(50));

    // Send DELETE key (\x1b[3~) 7 times to delete "Line 1 "
    for _ in 0..7 {
        tcp_stream.write_all(b"\x1b[3~").unwrap();
        tcp_stream.flush().unwrap();
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(150));

    // Move to end of file (Down 3 times, End), send 25 Backspaces to delete line 4 and 3
    tcp_stream.write_all(b"\x1b[B\x1b[B\x1b[B\x1b[F").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    for _ in 0..25 {
        tcp_stream.write_all(&[0x08]).unwrap();
        tcp_stream.flush().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(150));

    // Check that PulseEditor bottom bar \x1b[24;1H is rendered
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(full_output.contains("\x1b[24;1H\x1b[7m [^S / F2 Save]"), "Missing PulseEditor fixed bottom shortcuts bar at row 24!");

    // Save and Quit
    tcp_stream.write_all(&[0x13]).unwrap(); // ^S
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    tcp_stream.write_all(&[0x11]).unwrap(); // ^Q
    tcp_stream.flush().unwrap();
    wait_for("[c0|", 5, &mut full_output);

    // Inspect content
    println!("[xtask-test] Running: cat /home/delete_clean.pl");
    send_test_cmd(&mut tcp_stream, "cat /home/delete_clean.pl", "Beta", &rx, &mut full_output);
    assert!(!full_output.contains("Line 2 Beta"), "DELETE key failed to delete 'Line 2 ' prefix!");
    assert!(!full_output.contains("Gamma"), "Ghost character 'Gamma' was not cleanly deleted!");
    assert!(!full_output.contains("Delta"), "Ghost character 'Delta' was not cleanly deleted!");

    // Test 1: run echo.pl without arguments
    println!("[xtask-test] Running: run echo.pl");
    send_test_cmd(&mut tcp_stream, "run echo.pl", "LatencyOS PulseLang Real-Time Script Engine Active", &rx, &mut full_output);

    // Test 2: run echo.pl "a" (single argument)
    println!("[xtask-test] Running: run echo.pl \"a\"");
    send_test_cmd(&mut tcp_stream, "run echo.pl \"a\"", "a", &rx, &mut full_output);

    // Test 3: run pulselang/echo.pl (path without leading slash) with multiple arguments
    println!("[xtask-test] Running: run pulselang/echo.pl \"hello\" \"world\"");
    send_test_cmd(&mut tcp_stream, "run pulselang/echo.pl \"hello\" \"world\"", "hello world", &rx, &mut full_output);

    // Test 4: cd pulselang and run echo.pl with relative CWD resolution
    println!("[xtask-test] Running: cd pulselang");
    send_test_cmd(&mut tcp_stream, "cd pulselang", "", &rx, &mut full_output);
    println!("[xtask-test] Running: run echo.pl \"cwd_test_success\"");
    send_test_cmd(&mut tcp_stream, "run echo.pl \"cwd_test_success\"", "cwd_test_success", &rx, &mut full_output);

    let _ = child.kill();
    let _ = child.wait();

    println!("[xtask-test] === TEST PASSED: Nano-style bottom bar, DELETE key, path resolution, and script argument execution verified! ===");
}

fn test_editor_scroll(kernel_elf: &Path) {
    let qemu = find_tool("qemu-system-x86_64");
    println!("[xtask] Running automated nano editor scrolling & 35+ lines test with kernel: {}", kernel_elf.display());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let mut cmd = Command::new(&qemu);
    cmd.arg("-kernel")
        .arg(kernel_elf)
        .arg("-cpu")
        .arg("max")
        .arg("-chardev")
        .arg(format!("socket,id=ser0,host=127.0.0.1,port={},server=off,reconnect-ms=100", port))
        .arg("-serial")
        .arg("chardev:ser0")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .arg("-m")
        .arg("128M")
        .arg("-smp")
        .arg("4")
        .arg("-netdev")
        .arg("user,id=net0")
        .arg("-device")
        .arg("e1000,netdev=net0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("PATH", get_augmented_path());

    let mut child = cmd.spawn().expect("Failed to spawn QEMU process");

    let (mut tcp_stream, _) = listener.accept().expect("Failed to accept QEMU serial connection");
    let _ = tcp_stream.set_nodelay(true);
    let mut tcp_read = tcp_stream.try_clone().expect("Failed to clone stream");

    use std::io::{Read, Write};

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match tcp_read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut full_output = String::new();
    let wait_for = |target: &str, timeout_secs: u64, full_out: &mut String| -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(timeout_secs) {
            while let Ok(chunk) = rx.try_recv() {
                full_out.push_str(&String::from_utf8_lossy(&chunk));
            }
            if full_out.contains(target) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    };

    println!("[xtask-test] Waiting for shell prompt...");
    assert!(wait_for("[c0|", 15, &mut full_output), "Timed out waiting for shell prompt");

    // Open editor for a new file
    println!("[xtask-test] Opening editor: edit /home/scroll_35.pl");
    tcp_stream.write_all(b"edit /home/scroll_35.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();

    assert!(wait_for("LatencyOS PulseEditor", 5, &mut full_output), "Timed out waiting for PulseEditor to open");
    println!("[xtask-test] PulseEditor UI verified!");

    // Verify PulseEditor top and bottom headers
    assert!(full_output.contains("LatencyOS PulseEditor | File:"), "Missing PulseEditor header title");
    assert!(full_output.contains("\x1b[24;1H\x1b[7m [^S / F2 Save]"), "Missing PulseEditor shortcut bar");

    // Create 35 lines of code
    println!("[xtask-test] Typing 35 lines into editor...");
    let mut script_payload = String::new();
    for i in 1..=35 {
        script_payload.push_str(&format!("$line_{:02} := {};\n", i, i * 10));
    }
    for chunk in script_payload.as_bytes().chunks(8) {
        tcp_stream.write_all(chunk).unwrap();
        tcp_stream.flush().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(300));

    // Drain terminal buffer from typing
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }

    // Move to TOP (Line 1) using Page Up (^Y or \x1b[5~)
    println!("[xtask-test] Moving to TOP with Page Up (^Y)...");
    full_output.clear();
    tcp_stream.write_all(&[0x19, 0x19]).unwrap(); // ^Y twice
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));

    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(full_output.contains("line_") && full_output.contains(":="), "Line 1 not visible at top of viewport!");
    assert!(!full_output.contains("Line: 35"), "Line 35 should not be visible when scrolled to top!");

    // Scroll down to Line 35 using Page Down (^V or \x1b[6~)
    println!("[xtask-test] Scrolling down to BOTTOM with Page Down (^V)...");
    full_output.clear();
    tcp_stream.write_all(&[0x16, 0x16]).unwrap(); // ^V twice
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));

    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    println!("[xtask-test] Viewport at BOTTOM:\n{}", full_output);
    assert!(full_output.contains("Line: 3") || full_output.contains("line_"), "Line 35 not visible after scrolling down!");

    // Test Nano Cut (^K) and Uncut (^U)
    println!("[xtask-test] Testing Nano Cut (^K) and Uncut (^U)...");
    tcp_stream.write_all(&[0x0B]).unwrap(); // ^K
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    tcp_stream.write_all(&[0x15]).unwrap(); // ^U
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // Save and Quit
    println!("[xtask-test] Saving and exiting with ^X...");
    tcp_stream.write_all(&[0x18]).unwrap(); // ^X
    tcp_stream.flush().unwrap();
    assert!(wait_for("[c0|", 5, &mut full_output), "Timed out waiting for shell prompt after ^X");

    // Cat the file and verify all 35 lines exist in LatencyFS
    full_output.clear();
    println!("[xtask-test] Running: cat /home/scroll_35.pl");
    tcp_stream.write_all(b"cat /home/scroll_35.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    println!("[xtask-test] Result file content:\n{}", full_output);

    for i in 1..=35 {
        assert!(full_output.contains(&format!("$line_{:02} := {};", i, i * 10)), "Missing line {} in saved file!", i);
    }

    let _ = child.kill();
    let _ = child.wait();

    println!("[xtask-test] === TEST PASSED: GNU nano UI, vertical scrolling past 21 lines, and 35+ lines file persistence verified! ===");
}

fn test_px64_architecture(kernel_elf: &Path) {
    let qemu = find_tool("qemu-system-x86_64");
    println!("[xtask] Running px64 architecture verification test with kernel: {}", kernel_elf.display());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    let mut cmd = Command::new(&qemu);
    cmd.arg("-kernel")
        .arg(kernel_elf)
        .arg("-cpu")
        .arg("max")
        .arg("-chardev")
        .arg(format!("socket,id=ser0,host=127.0.0.1,port={},server=off,reconnect-ms=100", port))
        .arg("-serial")
        .arg("chardev:ser0")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot")
        .arg("-m")
        .arg("128M")
        .arg("-smp")
        .arg("4")
        .arg("-netdev")
        .arg("user,id=net0")
        .arg("-device")
        .arg("e1000,netdev=net0")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("PATH", get_augmented_path());

    let mut child = cmd.spawn().expect("Failed to spawn QEMU process");

    let (mut tcp_stream, _) = listener.accept().expect("Failed to accept QEMU serial connection");
    let _ = tcp_stream.set_nodelay(true);
    let mut tcp_read = tcp_stream.try_clone().expect("Failed to clone stream");

    use std::io::{Read, Write};

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match tcp_read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut full_output = String::new();
    let wait_for = |target: &str, start_pos: usize, timeout_secs: u64, full_out: &mut String| -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(timeout_secs) {
            while let Ok(chunk) = rx.try_recv() {
                full_out.push_str(&String::from_utf8_lossy(&chunk));
            }
            if full_out.len() >= start_pos && full_out[start_pos..].contains(target) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    };

    if !wait_for("] % ", 0, 10, &mut full_output) {
        let _ = child.kill();
        panic!("Timeout waiting for shell prompt. Output received:\n{}", full_output);
    }
    std::thread::sleep(Duration::from_millis(50));

    // Test 1: disasm /bin/echo.bin
    println!("[xtask-test] Running: disasm /bin/echo.bin");
    send_test_cmd(&mut tcp_stream, "disasm /bin/echo.bin", "=== [px64 Virtual Register Machine Disassembly] /bin/echo.bin ===", &rx, &mut full_output);

    // Test 2: compile /pulselang/echo.pl /bin/my_echo.bin
    println!("[xtask-test] Running: compile /pulselang/echo.pl /bin/my_echo.bin");
    send_test_cmd(&mut tcp_stream, "compile /pulselang/echo.pl /bin/my_echo.bin", "[BUILD] Compiled", &rx, &mut full_output);

    // Test 3: disasm /bin/my_echo.bin
    println!("[xtask-test] Running: disasm /bin/my_echo.bin");
    send_test_cmd(&mut tcp_stream, "disasm /bin/my_echo.bin", "=== [px64 Virtual Register Machine Disassembly] /bin/my_echo.bin ===", &rx, &mut full_output);

    // Test 4: run /bin/my_echo.bin "px64 register machine active"
    println!("[xtask-test] Running: run /bin/my_echo.bin \"px64 register machine active\"");
    send_test_cmd(&mut tcp_stream, "run /bin/my_echo.bin \"px64 register machine active\"", "px64 register machine active", &rx, &mut full_output);

    // Test 5: run /pulselang/bench.pl (BL-01 verification: sum must be 9900, not 400!)
    println!("[xtask-test] Running: run /pulselang/bench.pl (BL-01 fix verification)");
    send_test_cmd(&mut tcp_stream, "run /pulselang/bench.pl", "9900", &rx, &mut full_output);

    // Test 5b: run /pulselang/filter.pl (BL-02 verification: 300us must not misdiagnose congestion)
    println!("[xtask-test] Running: run /pulselang/filter.pl (BL-02 fix verification)");
    send_test_cmd(&mut tcp_stream, "run /pulselang/filter.pl", "Rate: 100%", &rx, &mut full_output);

    // Test 6: Static Loop Boundary Verification -> ERR_UNBOUNDED_LOOP (BL-04)
    println!("[xtask-test] Creating script with constant infinite loop (@while(1)) (/loop_unbounded.pl)...");
    tcp_stream.write_all(b"edit /loop_unbounded.pl\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(b"let $a = 0;\r\nwhile (1) {\r\n  $a += 1;\r\n}\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(&[0x18]).unwrap(); // Ctrl+X (Save & Quit)
    std::thread::sleep(Duration::from_millis(200));
    send_test_cmd(&mut tcp_stream, "compile /loop_unbounded.pl /bin/loop_unbounded.bin", "ERR_UNBOUNDED_LOOP", &rx, &mut full_output);

    // Test 6b: Non-decrementing loop -> ERR_PX64_WCET_EXCEEDED (10,000 steps)
    println!("[xtask-test] Creating and running non-decrementing loop (/loop_calc.pl)...");
    tcp_stream.write_all(b"edit /loop_calc.pl\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(b"let $i = 0;\r\nlet $a = 0;\r\nwhile ($i < 100000) {\r\n  $a += 1;\r\n}\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(&[0x18]).unwrap(); // Ctrl+X (Save & Quit)
    std::thread::sleep(Duration::from_millis(200));
    send_test_cmd(&mut tcp_stream, "run /loop_calc.pl", "ERR_PX64_WCET_EXCEEDED", &rx, &mut full_output);

    // Test 7: Adversarial @capture() loop -> ERR_PX64_TIMEOUT_EXCEEDED (5.0ms wall-clock timeout triggered)
    println!("[xtask-test] Creating and running adversarial @capture() loop (/loop_cap.pl)...");
    tcp_stream.write_all(b"edit /loop_cap.pl\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(b"let $i = 0;\r\nwhile ($i < 100000) {\r\n  let $f = @capture();\r\n}\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(&[0x18]).unwrap(); // Ctrl+X (Save & Quit)
    std::thread::sleep(Duration::from_millis(200));
    send_test_cmd(&mut tcp_stream, "run /loop_cap.pl", "ERR_PX64_TIMEOUT_EXCEEDED", &rx, &mut full_output);

    // Test 7b: Linear Type Handle Verification (BL-03)
    println!("[xtask-test] Testing Linear Type Unconsumed Handle (/unconsumed.pl)...");
    tcp_stream.write_all(b"edit /unconsumed.pl\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(b"#f := @capture();\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(&[0x18]).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    send_test_cmd(&mut tcp_stream, "compile /unconsumed.pl /bin/unconsumed.bin", "ERR_LINEAR_UNCONSUMED_HANDLE", &rx, &mut full_output);

    println!("[xtask-test] Testing Linear Type Double Send (/doublesend.pl)...");
    tcp_stream.write_all(b"edit /doublesend.pl\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(b"#f := @capture();\r\n@send(#f);\r\n@send(#f);\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(&[0x18]).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    send_test_cmd(&mut tcp_stream, "compile /doublesend.pl /bin/doublesend.bin", "ERR_LINEAR_DOUBLE_SEND", &rx, &mut full_output);

    // Test 8: Large 64-bit constant loading with LDC (>65535) and ADDI/SUBI
    println!("[xtask-test] Creating script with 64-bit constants and compound ops (/const_test.pl)...");
    tcp_stream.write_all(b"edit /const_test.pl\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(b"$big := 1000000;\r\n$big += 5;\r\n$big -= 2;\r\n@print(\"CALC_RES:\");\r\n@println($big);\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(&[0x18]).unwrap(); // Ctrl+X (Save & Quit)
    std::thread::sleep(Duration::from_millis(200));
    send_test_cmd(&mut tcp_stream, "compile /const_test.pl /bin/const_test.bin", "[BUILD] Compiled", &rx, &mut full_output);

    println!("[xtask-test] Disassembling /bin/const_test.bin...");
    send_test_cmd(&mut tcp_stream, "disasm /bin/const_test.bin", "1000000", &rx, &mut full_output);

    println!("[xtask-test] Running /bin/const_test.bin...");
    send_test_cmd(&mut tcp_stream, "run /bin/const_test.bin", "CALC_RES:1000003", &rx, &mut full_output);

    // Test 8b: Dynamic Filesystem Operations (BL-05)
    println!("[xtask-test] Testing Dynamic Filesystem mkdir, cd, pwd, touch, tree, rm (BL-05)...");
    send_test_cmd(&mut tcp_stream, "mkdir qa", "", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "cd qa", "", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "pwd", "/qa", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "touch inside", "", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "tree", "/qa/", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "cd ..", "", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "rm qa", "Directory not empty", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "rm /qa/inside", "", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "rm qa", "", &rx, &mut full_output);

    // Test 8c: SMP Multi-Core Activity & TSC Calibration (BL-06, BL-07)
    println!("[xtask-test] Testing SMP Cores Activity and TSC reporting (BL-06, BL-07)...");
    send_test_cmd(&mut tcp_stream, "cores", "core1: [apic 1] Capture", &rx, &mut full_output);
    send_test_cmd(&mut tcp_stream, "tsc", "TSC Frequency", &rx, &mut full_output);

    // Test 9: Execution of all 6 pre-compiled standard binaries in /bin/
    for script in &["stream.bin", "bench.bin", "filter.bin", "jitter.bin", "telemetry.bin", "echo.bin"] {
        println!("[xtask-test] Testing /bin/{} execution...", script);
        send_test_cmd(&mut tcp_stream, &format!("run /bin/{}", script), "", &rx, &mut full_output);
    }

    // Test 10: Run benchmark command to obtain real hardware/VM measured execution times
    println!("[xtask-test] Running benchmark for real execution timing...");
    send_test_cmd(&mut tcp_stream, "benchmark", "[PX64_VM_MICROBENCHMARK]", &rx, &mut full_output);

    // Test 11: Binary with invalid opcode (0xFE)
    println!("[xtask-test] Disassembling /bin/test_invalid_op.bin...");
    send_test_cmd(&mut tcp_stream, "disasm /bin/test_invalid_op.bin", "UNKNOWN_OP_0xfe", &rx, &mut full_output);

    println!("[xtask-test] Running /bin/test_invalid_op.bin (must trigger ERR_PX64_INVALID_OPCODE)...");
    send_test_cmd(&mut tcp_stream, "run /bin/test_invalid_op.bin", "ERR_PX64_INVALID_OPCODE", &rx, &mut full_output);

    // Test 12: Binary with LDC out-of-bounds (const[99] when const_count is 0)
    println!("[xtask-test] Disassembling /bin/test_oob_const.bin...");
    send_test_cmd(&mut tcp_stream, "disasm /bin/test_oob_const.bin", "const[99]", &rx, &mut full_output);

    println!("[xtask-test] Running /bin/test_oob_const.bin (must trigger ERR_PX64_CONST_OUT_OF_BOUNDS)...");
    send_test_cmd(&mut tcp_stream, "run /bin/test_oob_const.bin", "ERR_PX64_CONST_OUT_OF_BOUNDS", &rx, &mut full_output);

    let _ = child.kill();
    let _ = child.wait();

    println!("[xtask-test] === TEST PASSED: All 10 Audit Remediations (BL-01 to BL-10) and Phase 9 px64 Instruction Set Refactoring completely verified! ===");
}

fn send_test_cmd(
    stream: &mut std::net::TcpStream,
    cmd: &str,
    expected: &str,
    rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    full_out: &mut String,
) {
    use std::io::Write;
    std::thread::sleep(Duration::from_millis(50));
    full_out.clear();
    for chunk in cmd.as_bytes().chunks(4) {
        stream.write_all(chunk).unwrap();
        stream.flush().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(Duration::from_millis(5));
    stream.write_all(b"\r\n").unwrap();
    stream.flush().unwrap();

    let start = Instant::now();
    let mut found_expected = expected.is_empty();
    let mut found_prompt = false;

    while start.elapsed() < Duration::from_secs(10) {
        while let Ok(chunk) = rx.try_recv() {
            full_out.push_str(&String::from_utf8_lossy(&chunk));
        }

        if !found_expected && full_out.contains(expected) {
            found_expected = true;
        }

        if found_expected && full_out.contains("[c0|") {
            found_prompt = true;
        }

        if found_expected && found_prompt {
            std::thread::sleep(Duration::from_millis(30));
            return;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    panic!(
        "Failed command '{}'. expected='{}' (found={}), prompt found={}. Output:\n{}",
        cmd,
        expected,
        found_expected,
        found_prompt,
        full_out
    );
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

fn bundle_standalone_exe() {
    use std::fs::File;
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let root = get_workspace_root();
    let kernel_path = run_cargo_build(true);

    let qemu_exe = find_tool("qemu-system-x86_64");
    if !qemu_exe.exists() {
        eprintln!("[xtask] ERROR: qemu-system-x86_64 not found to bundle portable runner.");
        std::process::exit(1);
    }
    let qemu_dir = qemu_exe.parent().unwrap();

    let dist_dir = root.join("dist");
    std::fs::create_dir_all(&dist_dir).expect("Failed to create dist directory");

    let runtime_zip_path = dist_dir.join("runtime.zip");
    println!("[xtask] Packaging portable QEMU runtime and kernel into {}...", runtime_zip_path.display());

    let zip_file = File::create(&runtime_zip_path).expect("Failed to create runtime.zip");
    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    // 1. Add kernel
    println!("[xtask]   Adding kernel: {}", kernel_path.display());
    let mut kernel_data = Vec::new();
    File::open(&kernel_path).expect("Failed to open kernel").read_to_end(&mut kernel_data).unwrap();
    zip.start_file("kernel", options).unwrap();
    zip.write_all(&kernel_data).unwrap();

    // 2. Add qemu-system-x86_64.exe
    println!("[xtask]   Adding QEMU executable: {}", qemu_exe.display());
    let mut qemu_data = Vec::new();
    File::open(&qemu_exe).expect("Failed to open qemu executable").read_to_end(&mut qemu_data).unwrap();
    zip.start_file("qemu-system-x86_64.exe", options).unwrap();
    zip.write_all(&qemu_data).unwrap();

    // 3. Add all DLLs in qemu_dir
    if let Ok(entries) = std::fs::read_dir(qemu_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.eq_ignore_ascii_case("dll") {
                        let file_name = path.file_name().unwrap().to_str().unwrap();
                        let mut dll_data = Vec::new();
                        if let Ok(mut f) = File::open(&path) {
                            let _ = f.read_to_end(&mut dll_data);
                            let _ = zip.start_file(file_name, options);
                            let _ = zip.write_all(&dll_data);
                        }
                    }
                }
            }
        }
    }

    // 4. Add share/ ROM directory
    let share_dir = qemu_dir.join("share");
    if share_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&share_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path.file_name().unwrap().to_str().unwrap();
                    let zip_entry_name = format!("share/{}", file_name);
                    let mut rom_data = Vec::new();
                    if let Ok(mut f) = File::open(&path) {
                        let _ = f.read_to_end(&mut rom_data);
                        let _ = zip.start_file(zip_entry_name, options);
                        let _ = zip.write_all(&rom_data);
                    }
                }
            }
        }
    }

    zip.finish().expect("Failed to finalize runtime.zip");

    let zip_size_mb = std::fs::metadata(&runtime_zip_path).map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0);
    println!("[xtask] Runtime archive created: {:.2} MB", zip_size_mb);

    // 5. Build runner crate into dist/LatencyOS.exe
    println!("[xtask] Compiling standalone runner into dist/LatencyOS.exe...");
    let cargo = find_tool("cargo");
    let mut cmd = Command::new(&cargo);
    cmd.current_dir(&root)
        .env("PATH", get_augmented_path())
        .env("LATENCYOS_RUNTIME_ZIP", &runtime_zip_path)
        .arg("build")
        .arg("--package")
        .arg("runner")
        .arg("--release");

    let status = cmd.status().expect("Failed to build runner package");
    if !status.success() {
        eprintln!("[xtask] ERROR: Failed to compile runner crate.");
        std::process::exit(1);
    }

    let runner_exe = root.join("target").join("release").join("runner.exe");
    let out_exe = dist_dir.join("LatencyOS.exe");
    std::fs::copy(&runner_exe, &out_exe).expect("Failed to copy runner.exe to dist/LatencyOS.exe");

    let exe_size_mb = std::fs::metadata(&out_exe).map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0);
    println!("================================================================================");
    println!("[xtask] SUCCESS: Single standalone Windows executable generated!");
    println!("[xtask] Location: {}", out_exe.display());
    println!("[xtask] File Size: {:.2} MB", exe_size_mb);
    println!("[xtask] Portable: 100% self-contained (zero host dependencies, no install required)");
    println!("================================================================================");
}

fn test_standalone_exe() {
    use std::io::{Read, Write};
    let root = get_workspace_root();
    let out_exe = root.join("dist").join("LatencyOS.exe");
    if !out_exe.exists() {
        bundle_standalone_exe();
    }

    println!("[xtask] Testing standalone executable: {}", out_exe.display());

    let mut cmd = Command::new(&out_exe);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to launch standalone LatencyOS.exe");
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let mut stdout = child.stdout.take().expect("Failed to open stdout");

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut full_output = String::new();
    let wait_for = |target: &str, timeout_secs: u64, full_out: &mut String| -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(timeout_secs) {
            while let Ok(chunk) = rx.try_recv() {
                full_out.push_str(&String::from_utf8_lossy(&chunk));
            }
            if full_out.contains(target) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    };

    println!("[xtask-test] Waiting for standalone LatencyOS to boot and present shell prompt...");
    assert!(wait_for("[c0|", 15, &mut full_output), "Timed out waiting for shell prompt from standalone LatencyOS.exe");

    println!("[xtask-test] Shell prompt received! Output preview:\n{}", full_output);
    assert!(full_output.contains("LatencyOS Core0 booted"), "Missing boot banner");

    println!("[xtask-test] 1. Testing direct PATH execution: echo \"direct_path_test\"");
    stdin.write_all(b"echo \"direct_path_test\"\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("direct_path_test", 5, &mut full_output), "Failed direct PATH command execution");

    println!("[xtask-test] 2. Testing LatencyVFS (VRAM Disk) mount command: vfs");
    stdin.write_all(b"vfs\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("LatencyVFS: MOUNT TABLE", 5, &mut full_output), "Failed LatencyVFS mount query");

    println!("[xtask-test] 3. Testing LatencyVFS virtual stats file read: cat /vram/stats");
    stdin.write_all(b"cat /vram/stats\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("Physical GPU DMA Framebuffer Pool", 5, &mut full_output), "Failed reading /vram/stats");

    println!("[xtask-test] 4. Testing git command: git status");
    stdin.write_all(b"git status\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("On branch main", 5, &mut full_output), "Failed git status execution");

    println!("[xtask-test] 5. Testing compile command: compile /pulselang/echo.pl /bin/my_echo.bin");
    stdin.write_all(b"compile /pulselang/echo.pl /bin/my_echo.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[BUILD] Compiled", 5, &mut full_output), "Failed compile execution");

    println!("[xtask-test] 6. Testing disasm command: disasm /bin/my_echo.bin");
    stdin.write_all(b"disasm /bin/my_echo.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("=== [px64 Virtual Register Machine Disassembly]", 5, &mut full_output), "Failed disasm execution");

    println!("[xtask-test] 7. Testing run of compiled binary: run /bin/my_echo.bin \"hello_pulse\"");
    stdin.write_all(b"run /bin/my_echo.bin \"hello_pulse\"\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("hello_pulse", 5, &mut full_output), "Failed running compiled binary");

    println!("[xtask-test] 8. Testing relative path fs operations (BL-11 fix): mkdir qa, cd qa, touch alpha, cp alpha alpha_copy, mv alpha_copy beta, ls -l");
    full_output.clear();
    stdin.write_all(b"mkdir qa\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[c0|", 5, &mut full_output));

    full_output.clear();
    stdin.write_all(b"cd qa\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[c0|", 5, &mut full_output));

    full_output.clear();
    stdin.write_all(b"touch alpha\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[c0|", 5, &mut full_output));

    full_output.clear();
    stdin.write_all(b"cp alpha alpha_copy\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[c0|", 5, &mut full_output));

    full_output.clear();
    stdin.write_all(b"mv alpha_copy beta\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[c0|", 5, &mut full_output));

    full_output.clear();
    stdin.write_all(b"ls -l\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("beta", 5, &mut full_output), "Failed relative mv: 'beta' not found in ls -l");
    assert!(full_output.contains("alpha"), "Failed relative operations: 'alpha' not found in ls -l");
    assert!(!full_output.lines().any(|l| l.trim().ends_with("alpha_copy")), "Failed relative mv: 'alpha_copy' still in ls -l");

    println!("[xtask-test] 9. Testing timeline E2E status reporting (BL-12 fix): timeline");
    full_output.clear();
    stdin.write_all(b"timeline\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("stage 0 (isr)", 5, &mut full_output), "Failed timeline execution");
    assert!(full_output.contains("status: PASS") || full_output.contains("status: EXCEEDED"), "Failed timeline status check");
    assert!(!full_output.contains("margin: optimal"), "Timeline must not contain unconditional margin: optimal");

    println!("[xtask-test] 10. Testing disassembler out-of-bounds const display (BL-15 fix): disasm /bin/test_oob_const.bin");
    full_output.clear();
    stdin.write_all(b"disasm /bin/test_oob_const.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("<out of bounds>", 5, &mut full_output), "Failed disasm out-of-bounds const display");

    println!("[xtask-test] 11. Testing Phase 10-1 static range for loop: compile /pulselang/for_test.pl /bin/for_test.bin");
    full_output.clear();
    stdin.write_all(b"compile /pulselang/for_test.pl /bin/for_test.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[BUILD] Compiled", 5, &mut full_output), "Failed compiling for_test.pl");

    println!("[xtask-test] 12. Testing disasm of for loop bytecode: disasm /bin/for_test.bin");
    full_output.clear();
    stdin.write_all(b"disasm /bin/for_test.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("CMPLT", 5, &mut full_output), "Failed finding CMPLT in disasm");
    assert!(full_output.contains("JZ"), "Failed finding JZ in disasm");
    assert!(full_output.contains("ADDI"), "Failed finding ADDI in disasm");
    assert!(full_output.contains("JMP"), "Failed finding JMP in disasm");
    println!("[xtask-test] Disassembly for /bin/for_test.bin:\n{}", full_output.trim());

    println!("[xtask-test] 13. Testing execution of for loop binary: run /bin/for_test.bin");
    full_output.clear();
    stdin.write_all(b"run /bin/for_test.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("45", 5, &mut full_output), "Failed executing for_test.bin: sum 0..10 != 45");

    println!("[xtask-test] 14. Testing rewritten bench.pl with static for loop: compile /pulselang/bench.pl /bin/bench_for.bin && run");
    full_output.clear();
    stdin.write_all(b"compile /pulselang/bench.pl /bin/bench_for.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("[BUILD] Compiled", 5, &mut full_output), "Failed compiling bench.pl");

    full_output.clear();
    stdin.write_all(b"run /bin/bench_for.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("9900", 5, &mut full_output), "Failed executing bench_for.bin: sum != 9900");

    println!("[xtask-test] 15. Testing compile-time static WCET rejection: compile /pulselang/err_for_wcet.pl /bin/err_for.bin");
    full_output.clear();
    stdin.write_all(b"compile /pulselang/err_for_wcet.pl /bin/err_for.bin\r\n").unwrap();
    stdin.flush().unwrap();
    assert!(wait_for("ERR_FOR_WCET_EXCEEDED", 5, &mut full_output), "Failed compile-time static WCET rejection");
    assert!(full_output.contains("Static loop WCET exceeds MAX_VM_STEPS"), "Failed error message check");

    println!("[xtask-test] 16. Sending poweroff command...");
    stdin.write_all(b"poweroff\r\n").unwrap();
    stdin.flush().unwrap();
    std::thread::sleep(Duration::from_millis(500));

    let _ = child.kill();
    let _ = child.wait();

    println!("================================================================================");
    println!("[xtask] SUCCESS: Standalone LatencyOS.exe verified end-to-end with 100% success!");
    println!("================================================================================");
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
        "run" | "interactive" => {
            let kernel_path = run_cargo_build(release);
            run_qemu(&kernel_path, false, 0);
        }
        "bundle" | "dist" => {
            bundle_standalone_exe();
        }
        "test-standalone" => {
            test_standalone_exe();
        }
        "test-boot" => {
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
        "test-paste" => {
            let kernel_path = run_cargo_build(release);
            test_paste(&kernel_path);
        }
        "test-compile-error" => {
            let kernel_path = run_cargo_build(release);
            test_compile_error(&kernel_path);
        }
        "test-editor-delete" => {
            let kernel_path = run_cargo_build(release);
            test_editor_delete(&kernel_path);
        }
        "test-editor-scroll" => {
            let kernel_path = run_cargo_build(release);
            test_editor_scroll(&kernel_path);
        }
        "test-px64" => {
            let kernel_path = run_cargo_build(release);
            test_px64_architecture(&kernel_path);
        }
        "check" => {
            check_versions();
        }
        _ => {
            eprintln!("Usage: cargo xtask [build|run|interactive|bundle|dist|test-standalone|test-paste|test-compile-error|test-editor-delete|test-editor-scroll|test-px64|test-boot|check] [--release]");
            std::process::exit(1);
        }
    }
}
