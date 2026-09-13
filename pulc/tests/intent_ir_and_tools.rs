use std::process::Command;

#[test]
fn test_intent_ir_pipeline_and_tasks() {
    let src = r#"
        pipeline video_stream {
            stage capture {
                core 1
                budget 2ms
                zero_copy
                output #frame
            }
            stage encode {
                core 2
                budget 4500us
                hardware h264
                input #frame
                output #packet
            }
        }

        task telemetry_task {
            core 0
            budget 500us
            input $stats
        }

        fn compute_avg($a: i64) -> i64 {
            return $a;
        }
    "#;

    let dir = std::env::temp_dir();
    let file_path = dir.join("test_pipeline_stream.pul");
    std::fs::write(&file_path, src).unwrap();

    let exe = env!("CARGO_BIN_EXE_pulc");

    // 1. Test pulc ir --json
    let output_ir = Command::new(exe)
        .arg("ir")
        .arg(&file_path)
        .arg("--json")
        .output()
        .expect("Failed to run pulc ir");
    assert!(output_ir.status.success());
    let stdout_ir = String::from_utf8_lossy(&output_ir.stdout);
    assert!(stdout_ir.contains(r#""name":"video_stream""#));
    assert!(stdout_ir.contains(r#""name":"capture""#));
    assert!(stdout_ir.contains(r#""core":1"#));
    assert!(stdout_ir.contains(r#""zero_copy":true"#));
    assert!(stdout_ir.contains(r#""name":"encode""#));
    assert!(stdout_ir.contains(r#""core":2"#));
    assert!(stdout_ir.contains(r#""name":"telemetry_task""#));

    // 2. Test pulc inspect
    let output_inspect = Command::new(exe)
        .arg("inspect")
        .arg(&file_path)
        .arg("capture")
        .arg("--json")
        .output()
        .expect("Failed to run pulc inspect");
    assert!(output_inspect.status.success());
    let stdout_inspect = String::from_utf8_lossy(&output_inspect.stdout);
    assert!(stdout_inspect.contains(r#""symbol":"capture""#));
    assert!(stdout_inspect.contains(r#""core":1"#));
    assert!(stdout_inspect.contains(r#""zero_copy":true"#));

    // 3. Test pulc why
    let output_why = Command::new(exe)
        .arg("why")
        .arg(&file_path)
        .arg("capture")
        .arg("--json")
        .output()
        .expect("Failed to run pulc why");
    assert!(output_why.status.success());
    let stdout_why = String::from_utf8_lossy(&output_why.stdout);
    assert!(stdout_why.contains(r#""symbol":"capture""#));
    assert!(stdout_why.contains(r#""satisfied":true"#));

    // 4. Test pulc diff
    let src2 = r#"
        pipeline video_stream {
            stage capture {
                core 0
                budget 3ms
                zero_copy
                output #frame
            }
            stage encode {
                core 3
                budget 5000us
                hardware h264
                input #frame
                output #packet
            }
        }
    "#;
    let file_path2 = dir.join("test_pipeline_stream2.pul");
    std::fs::write(&file_path2, src2).unwrap();

    let output_diff = Command::new(exe)
        .arg("diff")
        .arg(&file_path)
        .arg(&file_path2)
        .output()
        .expect("Failed to run pulc diff");
    assert!(output_diff.status.success());
    let stdout_diff = String::from_utf8_lossy(&output_diff.stdout);
    assert!(stdout_diff.contains("Semantic Changes:"));
    assert!(stdout_diff.contains("video_stream::capture.core: 1 -> 0"));
    assert!(stdout_diff.contains("video_stream::encode.core: 2 -> 3"));

    let _ = std::fs::remove_file(&file_path);
    let _ = std::fs::remove_file(&file_path2);
}
