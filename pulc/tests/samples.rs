use std::fs;
use std::path::Path;

#[test]
fn test_all_documented_sample_scripts() {
    let examples_dir = Path::new("../docs/examples");
    let target_dir = if examples_dir.exists() {
        examples_dir
    } else {
        Path::new("docs/examples")
    };

    assert!(target_dir.exists(), "Examples directory docs/examples must exist");

    let entries = fs::read_dir(target_dir).expect("Failed to read docs/examples directory");
    let mut tested_count = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("pul") {
            let filename = path.file_name().unwrap().to_string_lossy();
            println!("Testing documented sample script: {}", filename);

            let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));

            let stats = pulselang_core::check(&src).unwrap_or_else(|e| panic!("Check failed for {}: {:?}", filename, e));
            assert!(stats.total_binary_size > 16, "Binary size for {} must be > 16 bytes", filename);

            let bin = pulselang_core::compile(&src).unwrap_or_else(|e| panic!("Compilation failed for {}: {:?}", filename, e));
            assert!(bin.len() > 16);

            let mut out = String::new();
            let args = ["test_arg0", "test_arg1"];
            pulselang_core::run_binary_with_output(&bin, &args, &mut out)
                .unwrap_or_else(|e| panic!("Execution failed for {}: {:?}", filename, e));

            tested_count += 1;
        }
    }

    assert!(tested_count >= 10, "Expected at least 10 sample scripts tested, got {}", tested_count);
}
