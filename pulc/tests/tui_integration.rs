use std::process::Command;
use std::path::Path;

#[test]
fn test_tui_library_and_monitor_app() {
    let exe = env!("CARGO_BIN_EXE_pulc");

    // 1. Check std/tui.pul directly
    let tui_path = if Path::new("std/tui.pul").exists() {
        "std/tui.pul"
    } else {
        "../std/tui.pul"
    };
    let output_check = Command::new(exe)
        .arg("check")
        .arg(tui_path)
        .output()
        .expect("Failed to run pulc check std/tui.pul");
    assert!(output_check.status.success());

    // 2. Run examples/apps/monitor_tui.pul
    let app_path = if Path::new("examples/apps/monitor_tui.pul").exists() {
        "examples/apps/monitor_tui.pul"
    } else {
        "../examples/apps/monitor_tui.pul"
    };
    let output_run = Command::new(exe)
        .arg("run")
        .arg(app_path)
        .output()
        .expect("Failed to run pulc run monitor_tui.pul");
    assert!(output_run.status.success());
    let stdout = String::from_utf8_lossy(&output_run.stdout);
    assert!(stdout.contains("LatencyOS Pipeline Monitor"));
    assert!(stdout.contains("Core 0 (Control)"));
    assert!(stdout.contains("Columns"));
    assert!(stdout.contains("Rows"));
}
