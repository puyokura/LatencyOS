use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LATENCYOS_RUNTIME_ZIP");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest_zip = out_dir.join("runtime.zip");

    if let Ok(src_zip) = env::var("LATENCYOS_RUNTIME_ZIP") {
        let src_path = PathBuf::from(src_zip);
        if src_path.exists() {
            println!("cargo:rerun-if-changed={}", src_path.display());
            fs::copy(&src_path, &dest_zip).expect("Failed to copy runtime.zip into OUT_DIR");
            return;
        }
    }

    if !dest_zip.exists() {
        // Create an empty dummy file so `include_bytes!` compiles in dev/check mode
        fs::write(&dest_zip, b"").expect("Failed to write dummy runtime.zip");
    }
}
