use std::env;
use std::path::PathBuf;
use std::process::Command;

fn find_nasm() -> PathBuf {
    // 1. Check PATH
    if let Ok(output) = Command::new("nasm").arg("-v").output() {
        if output.status.success() {
            return PathBuf::from("nasm");
        }
    }

    // 2. Check known Scoop / Windows installation paths
    if let Ok(user_profile) = env::var("USERPROFILE") {
        let scoop_shim = PathBuf::from(&user_profile).join("scoop").join("shims").join("nasm.exe");
        if scoop_shim.exists() {
            return scoop_shim;
        }
        let scoop_app = PathBuf::from(&user_profile).join("scoop").join("apps").join("NASM").join("current").join("nasm.exe");
        if scoop_app.exists() {
            return scoop_app;
        }
    }

    // 3. Fallback to default
    PathBuf::from("nasm")
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let boot_asm = manifest_dir.join("src").join("boot.asm");
    let boot_obj = out_dir.join("boot.o");
    let linker_script = manifest_dir.join("linker.ld");

    let nasm = find_nasm();

    let status = Command::new(&nasm)
        .arg("-f")
        .arg("elf64")
        .arg(&boot_asm)
        .arg("-o")
        .arg(&boot_obj)
        .status()
        .unwrap_or_else(|e| panic!("Failed to execute NASM at {:?}: {}", nasm, e));

    if !status.success() {
        panic!("NASM failed with status: {}", status);
    }

    println!("cargo:rustc-link-arg={}", boot_obj.display());
    println!("cargo:rustc-link-arg=-T{}", linker_script.display());
    println!("cargo:rerun-if-changed={}", boot_asm.display());
    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rerun-if-changed=build.rs");
}
