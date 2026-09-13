use std::process::Command;
use std::path::Path;

#[test]
fn test_pure_pulselang_nano_editor() {
    let exe = env!("CARGO_BIN_EXE_pulc");

    let main_path = if Path::new("examples/apps/nano/main.pul").exists() {
        "examples/apps/nano/main.pul"
    } else {
        "../examples/apps/nano/main.pul"
    };

    // 1. Static check
    let output_check = Command::new(exe)
        .arg("check")
        .arg(main_path)
        .output()
        .expect("Failed to run pulc check nano main.pul");
    assert!(output_check.status.success());

    // 2. Run in non-interactive CI mode
    let output_run = Command::new(exe)
        .arg("run")
        .arg(main_path)
        .arg("ci")
        .output()
        .expect("Failed to run pulc run nano main.pul ci");
    assert!(output_run.status.success());
    let stdout = String::from_utf8_lossy(&output_run.stdout);
    assert!(stdout.contains("LatencyOS Nano | File: README.md"));
    assert!(stdout.contains("# LatencyOS Hard Real-Time Nano Editor"));
    assert!(stdout.contains("100% in pure PulseLang"));
    assert!(stdout.contains("[^S] Save  [^Q] Exit"));
}
