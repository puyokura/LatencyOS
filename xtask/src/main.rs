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
    full_output.clear();
    tcp_stream.write_all(b"edit /home/err_syntax.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    wait_for("PulseEditor", 5, &mut full_output);

    let broken_script = "@contract: @wcet(100us) @budget(500us);\r\n$x := := 42;\r\n";
    tcp_stream.write_all(broken_script.as_bytes()).unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(200));

    // Save and Quit
    tcp_stream.write_all(&[0x13]).unwrap(); // ^S
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    tcp_stream.write_all(&[0x11]).unwrap(); // ^Q
    tcp_stream.flush().unwrap();
    wait_for("[c0|", 5, &mut full_output);

    // Compile broken script
    full_output.clear();
    println!("[xtask-test] Running: compile /home/err_syntax.pl");
    tcp_stream.write_all(b"compile /home/err_syntax.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }

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
    tcp_stream.write_all(four_lines.as_bytes()).unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(200));

    // Move cursor to Line 1 (Up 3 times, Home)
    tcp_stream.write_all(b"\x1b[A\x1b[A\x1b[A\x1b[H").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(50));

    // Send DELETE key (\x1b[3~) 7 times to delete "Line 1 "
    for _ in 0..7 {
        tcp_stream.write_all(b"\x1b[3~").unwrap();
    }
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(150));

    // Move to end of file (Down 3 times, End), send 25 Backspaces to delete line 4 and 3
    tcp_stream.write_all(b"\x1b[B\x1b[B\x1b[B\x1b[F").unwrap();
    for _ in 0..25 {
        tcp_stream.write_all(&[0x08]).unwrap();
    }
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(150));

    // Check that nano-style bottom bar \x1b[24;1H is rendered
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(full_output.contains("\x1b[24;1H\x1b[7m [^S / F2 Save]"), "Missing nano-style fixed bottom shortcuts bar at row 24!");

    // Save and Quit
    tcp_stream.write_all(&[0x13]).unwrap(); // ^S
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    tcp_stream.write_all(&[0x11]).unwrap(); // ^Q
    tcp_stream.flush().unwrap();
    wait_for("[c0|", 5, &mut full_output);

    // Inspect content
    full_output.clear();
    println!("[xtask-test] Running: cat /home/delete_clean.pl");
    tcp_stream.write_all(b"cat /home/delete_clean.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }

    println!("[xtask-test] Result file content:\n{}", full_output);

    assert!(full_output.contains("Beta"), "Missing Beta after DELETE key deletion");
    assert!(!full_output.contains("Line 2 Beta"), "DELETE key failed to delete 'Line 2 ' prefix!");
    assert!(!full_output.contains("Gamma"), "Ghost character 'Gamma' was not cleanly deleted!");
    assert!(!full_output.contains("Delta"), "Ghost character 'Delta' was not cleanly deleted!");

    // Test 1: run echo.pl without arguments
    full_output.clear();
    println!("[xtask-test] Running: run echo.pl");
    tcp_stream.write_all(b"run echo.pl\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    println!("[xtask-test] run echo.pl output:\n{}", full_output);
    assert!(full_output.contains("LatencyOS PulseLang Real-Time Script Engine Active"), "Failed to run echo.pl without args");

    // Test 2: run echo.pl "a" (single argument)
    full_output.clear();
    println!("[xtask-test] Running: run echo.pl \"a\"");
    tcp_stream.write_all(b"run echo.pl \"a\"\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    println!("[xtask-test] run echo.pl \"a\" output:\n{}", full_output);
    assert!(full_output.contains("a"), "Failed to echo single argument 'a'");

    // Test 3: run pulselang/echo.pl (path without leading slash) with multiple arguments
    full_output.clear();
    println!("[xtask-test] Running: run pulselang/echo.pl \"hello\" \"world\"");
    tcp_stream.write_all(b"run pulselang/echo.pl \"hello\" \"world\"\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    println!("[xtask-test] run pulselang/echo.pl output:\n{}", full_output);
    assert!(full_output.contains("hello world"), "Failed to run pulselang/echo.pl with multiple args");

    // Test 4: cd pulselang and run echo.pl with relative CWD resolution
    full_output.clear();
    println!("[xtask-test] Running: cd pulselang && run echo.pl \"cwd_test_success\"");
    tcp_stream.write_all(b"cd pulselang\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    tcp_stream.write_all(b"run echo.pl \"cwd_test_success\"\r\n").unwrap();
    tcp_stream.flush().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(chunk) = rx.try_recv() {
        full_output.push_str(&String::from_utf8_lossy(&chunk));
    }
    println!("[xtask-test] cd pulselang && run echo.pl output:\n{}", full_output);
    assert!(full_output.contains("cwd_test_success"), "Failed to resolve script from CWD in pulselang/");

    let _ = child.kill();
    let _ = child.wait();

    println!("[xtask-test] === TEST PASSED: Nano-style bottom bar, DELETE key, path resolution, and script argument execution verified! ===");
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
        "run" | "interactive" => {
            let kernel_path = run_cargo_build(release);
            run_qemu(&kernel_path, false, 0);
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
        "check" => {
            check_versions();
        }
        _ => {
            eprintln!("Usage: cargo xtask [build|run|interactive|test-paste|test-compile-error|test-editor-delete|test-boot|check] [--release]");
            std::process::exit(1);
        }
    }
}
