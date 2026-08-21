use std::env;
use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RUNTIME_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/runtime.zip"));

fn get_runtime_cache_dir() -> PathBuf {
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        PathBuf::from(local_appdata).join("LatencyOS").join("runtime")
    } else if let Ok(temp) = env::var("TEMP") {
        PathBuf::from(temp).join("LatencyOS_runtime")
    } else {
        env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".latencyos_runtime")
    }
}

fn compute_payload_hash(data: &[u8]) -> u64 {
    // Simple FNV-1a 64-bit hash
    let mut hash = 0xcbf29ce484222325u64;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

#[cfg(windows)]
static mut SAVED_IN_MODE: u32 = 0x0187;
#[cfg(windows)]
static mut SAVED_OUT_MODE: u32 = 0x0007;

#[cfg(windows)]
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> i32 {
    if ctrl_type == 0 || ctrl_type == 1 {
        1 // Handle Ctrl+C / Ctrl+Break within guest OS
    } else {
        0
    }
}

struct ConsoleGuard {
    active: bool,
}

impl ConsoleGuard {
    fn new() -> Self {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
            use windows_sys::Win32::System::Console::{
                FlushConsoleInputBuffer, GetConsoleMode, GetStdHandle, SetConsoleCtrlHandler,
                SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
                ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
                STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
            };

            let stdin_handle = GetStdHandle(STD_INPUT_HANDLE);
            if stdin_handle != std::ptr::null_mut() && stdin_handle != INVALID_HANDLE_VALUE {
                let mut in_mode = 0;
                if GetConsoleMode(stdin_handle, &mut in_mode) != 0 {
                    SAVED_IN_MODE = in_mode;
                    let raw_in_mode = (in_mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
                        | ENABLE_VIRTUAL_TERMINAL_INPUT;
                    SetConsoleMode(stdin_handle, raw_in_mode);
                    FlushConsoleInputBuffer(stdin_handle);
                }
            }

            let stdout_handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if stdout_handle != std::ptr::null_mut() && stdout_handle != INVALID_HANDLE_VALUE {
                let mut out_mode = 0;
                if GetConsoleMode(stdout_handle, &mut out_mode) != 0 {
                    SAVED_OUT_MODE = out_mode;
                    SetConsoleMode(stdout_handle, out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }

            SetConsoleCtrlHandler(Some(console_ctrl_handler), 1);
        }
        Self { active: true }
    }
}

impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        if self.active {
            #[cfg(windows)]
            unsafe {
                use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
                use windows_sys::Win32::System::Console::{
                    GetStdHandle, SetConsoleCtrlHandler, SetConsoleMode, STD_INPUT_HANDLE,
                    STD_OUTPUT_HANDLE,
                };

                let stdin_handle = GetStdHandle(STD_INPUT_HANDLE);
                if stdin_handle != std::ptr::null_mut() && stdin_handle != INVALID_HANDLE_VALUE {
                    SetConsoleMode(stdin_handle, SAVED_IN_MODE);
                }

                let stdout_handle = GetStdHandle(STD_OUTPUT_HANDLE);
                if stdout_handle != std::ptr::null_mut() && stdout_handle != INVALID_HANDLE_VALUE {
                    SetConsoleMode(stdout_handle, SAVED_OUT_MODE);
                }

                SetConsoleCtrlHandler(Some(console_ctrl_handler), 0);
            }
        }
    }
}

fn unpack_runtime(target_dir: &Path) -> Result<(), String> {
    if RUNTIME_ZIP.is_empty() {
        return Err("Embedded runtime archive is empty. Build with 'cargo xtask bundle'.".into());
    }

    fs::create_dir_all(target_dir).map_err(|e| format!("Failed to create runtime dir {}: {}", target_dir.display(), e))?;

    let payload_hash = compute_payload_hash(RUNTIME_ZIP);
    let tag_file = target_dir.join("runtime.tag");
    let qemu_exe = target_dir.join("qemu-system-x86_64.exe");
    let kernel_bin = target_dir.join("kernel");

    if tag_file.exists() && qemu_exe.exists() && kernel_bin.exists() {
        if let Ok(content) = fs::read_to_string(&tag_file) {
            if content.trim() == format!("{:x}", payload_hash) {
                // Cached runtime is up to date
                return Ok(());
            }
        }
    }

    println!("[LatencyOS] Extracting self-contained portable runtime (first run only)...");

    let cursor = Cursor::new(RUNTIME_ZIP);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid zip archive: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Zip entry error: {}", e))?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| format!("Failed to create dir {}: {}", outpath.display(), e))?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| format!("Failed to create parent dir {}: {}", p.display(), e))?;
                }
            }
            let mut outfile = File::create(&outpath).map_err(|e| format!("Failed to create file {}: {}", outpath.display(), e))?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| format!("Failed to extract file {}: {}", outpath.display(), e))?;
        }
    }

    let _ = fs::write(&tag_file, format!("{:x}", payload_hash));
    Ok(())
}

fn main() {
    let _guard = ConsoleGuard::new();

    let target_dir = get_runtime_cache_dir();

    if let Err(err) = unpack_runtime(&target_dir) {
        eprintln!("[LatencyOS Error] {}", err);
        std::process::exit(1);
    }

    let qemu_exe = target_dir.join("qemu-system-x86_64.exe");
    let kernel_bin = target_dir.join("kernel");
    let share_dir = target_dir.join("share");

    if !qemu_exe.exists() {
        eprintln!("[LatencyOS Error] qemu-system-x86_64.exe not found at {}", qemu_exe.display());
        std::process::exit(1);
    }
    if !kernel_bin.exists() {
        eprintln!("[LatencyOS Error] Kernel binary not found at {}", kernel_bin.display());
        std::process::exit(1);
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral local port");
    let port = listener.local_addr().expect("Failed to get port").port();

    let mut smp_val = "4".to_string();
    let mut mem_val = "128M".to_string();
    let mut extra_args = Vec::new();

    let mut args_iter = env::args().skip(1);
    while let Some(arg) = args_iter.next() {
        if arg == "-smp" || arg == "--smp" || arg == "--cores" {
            if let Some(val) = args_iter.next() {
                smp_val = val;
            }
        } else if arg == "-m" || arg == "--mem" {
            if let Some(val) = args_iter.next() {
                mem_val = val;
            }
        } else {
            extra_args.push(arg);
        }
    }

    let mut cmd = Command::new(&qemu_exe);
    cmd.current_dir(&target_dir)
        .arg("-kernel")
        .arg(&kernel_bin)
        .arg("-cpu")
        .arg("max")
        .arg("-smp")
        .arg(&smp_val)
        .arg("-m")
        .arg(&mem_val)
        .arg("-netdev")
        .arg("user,id=net0")
        .arg("-device")
        .arg("e1000,netdev=net0")
        .arg("-chardev")
        .arg(format!("socket,id=ser0,host=127.0.0.1,port={},server=off,reconnect-ms=100", port))
        .arg("-serial")
        .arg("chardev:ser0")
        .arg("-display")
        .arg("none")
        .arg("-no-reboot");

    if share_dir.exists() {
        cmd.arg("-L").arg(&share_dir);
    }

    // Forward additional CLI flags if specified
    for arg in extra_args {
        cmd.arg(arg);
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[LatencyOS Error] Failed to launch QEMU: {}", e);
            std::process::exit(1);
        }
    };

    let (tcp_stream, _) = match listener.accept() {
        Ok(conn) => conn,
        Err(e) => {
            let _ = child.kill();
            eprintln!("[LatencyOS Error] Failed to connect to QEMU serial socket: {}", e);
            std::process::exit(1);
        }
    };
    let _ = tcp_stream.set_nodelay(true);

    use std::io::{Read, Write};
    let mut s_read = tcp_stream.try_clone().expect("Failed to clone TCP stream");
    let mut s_write = tcp_stream;

    // Socket -> Stdout forwarder thread
    let out_thread = std::thread::spawn(move || {
        let mut out = std::io::stdout();
        let mut buf = [0u8; 1024];
        while let Ok(n) = s_read.read(&mut buf) {
            if n == 0 {
                break;
            }
            if out.write_all(&buf[..n]).is_err() {
                break;
            }
            let _ = out.flush();
        }
    });

    // Stdin -> Socket forwarder thread
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

    let status = child.wait().expect("Failed to wait on QEMU process");
    let _ = out_thread.join();
    std::process::exit(status.code().unwrap_or(0));
}
