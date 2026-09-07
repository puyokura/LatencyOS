use std::process::Command;

#[test]
fn test_cli_test_replay_mode() {
    let exe = env!("CARGO_BIN_EXE_pulc");
    let file = if std::path::Path::new("docs/examples/chaos_meter.pul").exists() {
        "docs/examples/chaos_meter.pul"
    } else {
        "../docs/examples/chaos_meter.pul"
    };

    // 1. Run pulc test with --replay on chaos_meter.pul
    let status = Command::new(exe)
        .arg("test")
        .arg(file)
        .arg("--replay")
        .status()
        .expect("Failed to run pulc test --replay");
    assert!(status.success());

    // 2. Run pulc test with custom --seed on chaos_meter.pul
    let status_seed = Command::new(exe)
        .arg("test")
        .arg(file)
        .arg("--seed")
        .arg("0xDEADBEEF")
        .status()
        .expect("Failed to run pulc test --seed");
    assert!(status_seed.success());

    // 3. Run pulc test with --replay --json and verify JSON schema
    let output = Command::new(exe)
        .arg("test")
        .arg(file)
        .arg("--replay")
        .arg("--seed=123456")
        .arg("--json")
        .output()
        .expect("Failed to run pulc test --replay --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""replay":true"#));
    assert!(stdout.contains(r#""seed":123456"#));
    assert!(stdout.contains(r#""passed":2"#));
}
