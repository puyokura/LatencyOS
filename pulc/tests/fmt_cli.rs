use std::fs;
use std::process::Command;

#[test]
fn test_fmt_cli_check_and_in_place() {
    let temp_dir = std::env::temp_dir().join(format!("pulc_fmt_test_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let test_file = temp_dir.join("test.pul");

    let messy_code = r#"
@contract: @wcet(50us) @budget(100us);
fn add($a:i64,$b:i64)->i64
@wcet(10ns)
@requires($a>=0&&$b>=0)
@ensures($result>=$a)
{
let mut $sum=$a+$b;
return $sum;
}
"#;
    fs::write(&test_file, messy_code).unwrap();

    let exe = env!("CARGO_BIN_EXE_pulc");

    // 1. --check should fail with exit code 1
    let check_status = Command::new(exe)
        .arg("fmt")
        .arg(&test_file)
        .arg("--check")
        .status()
        .expect("Failed to run pulc fmt --check");
    assert_eq!(check_status.code(), Some(1));

    // 2. In-place format should succeed with exit code 0
    let fmt_status = Command::new(exe)
        .arg("fmt")
        .arg(&test_file)
        .status()
        .expect("Failed to run pulc fmt");
    assert!(fmt_status.success());

    // 3. --check should now pass with exit code 0
    let check_pass_status = Command::new(exe)
        .arg("fmt")
        .arg(&test_file)
        .arg("--check")
        .status()
        .expect("Failed to run pulc fmt --check");
    assert!(check_pass_status.success());

    // 4. Verify content was formatted cleanly
    let formatted_content = fs::read_to_string(&test_file).unwrap();
    assert!(formatted_content.contains("fn add($a: i64, $b: i64) -> i64"));
    assert!(formatted_content.contains("    @wcet(10ns)"));
    assert!(formatted_content.contains("    @requires($a >= 0 && $b >= 0)"));
    assert!(formatted_content.contains("    let mut $sum = $a + $b;"));

    let _ = fs::remove_dir_all(&temp_dir);
}
