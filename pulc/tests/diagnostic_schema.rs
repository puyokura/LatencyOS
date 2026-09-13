use std::process::Command;

#[test]
fn test_pulc_json_schema_compliance() {
    let exe = env!("CARGO_BIN_EXE_pulc");
    let dir = std::env::temp_dir();
    let file_path = dir.join("test_schema_invalid.pul");
    std::fs::write(&file_path, "$x := := 42;\n").unwrap();

    let output = Command::new(exe)
        .arg("check")
        .arg(&file_path)
        .arg("--json")
        .output()
        .expect("Failed to run pulc check --json");

    let _ = std::fs::remove_file(&file_path);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Parse JSON
    assert!(stderr.contains(r#""$schema":"https://latencyos.org/schema/pulselang-diagnostic-v1.json""#));
    assert!(stderr.contains(r#""version":"1.0""#));
    assert!(stderr.contains(r#""success":false"#));
    assert!(stderr.contains(r#""diagnostics":["#));
    assert!(stderr.contains(r#""category":"syntax""#));
    assert!(stderr.contains(r#""repairability":"#));
    assert!(stderr.contains(r#""ai_repair_hint":"#));
    assert!(stderr.contains(r#""repairs":["#));
}
