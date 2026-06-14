use std::env;
use std::fs;
use std::process::Command;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    // Be explicit: watching `user/` alone is not always enough for Cargo to
    // notice source edits and rebuild the embedded flat binary.
    println!("cargo:rerun-if-changed=user/src");
    println!("cargo:rerun-if-changed=user/Cargo.toml");
    println!("cargo:rerun-if-changed=user/user.ld");
    println!("cargo:rerun-if-changed=user/build.rs");

    // Find llvm-objcopy via rustc sysroot
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let sysroot_out = Command::new(&rustc)
        .arg("--print=sysroot")
        .output()
        .expect("failed to get rustc sysroot");
    let sysroot = String::from_utf8(sysroot_out.stdout).unwrap().trim().to_string();
    let host = env::var("HOST").unwrap();
    let llvm_objcopy = format!("{}/lib/rustlib/{}/bin/llvm-objcopy", sysroot, host);

    // Build user binary in a separate target dir to avoid lock conflicts
    let user_target_dir = format!("{}/user_target", out_dir);
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--manifest-path",
            &format!("{}/user/Cargo.toml", manifest_dir),
            "--target",
            "riscv64gc-unknown-none-elf",
            "--target-dir",
            &user_target_dir,
        ])
        .env("CARGO_ENCODED_RUSTFLAGS", "")
        .env_remove("RUSTFLAGS")
        .status()
        .expect("failed to build user binary");
    assert!(status.success(), "user binary build failed");

    // Convert ELF to flat binary
    let elf = format!(
        "{}/riscv64gc-unknown-none-elf/release/user",
        user_target_dir
    );
    let bin = format!("{}/user.bin", out_dir);
    let status = Command::new(&llvm_objcopy)
        .args(["-O", "binary", &elf, &bin])
        .status()
        .expect("failed to run llvm-objcopy");
    assert!(status.success(), "llvm-objcopy failed");

    let bin_bytes = fs::read(&bin).expect("failed to read user.bin");
    let checksum = fnv1a(&bin_bytes);

    println!("cargo:rustc-env=USER_BIN_PATH={}", bin);
    // Changing this env var forces the kernel crate to recompile when the
    // embedded user binary content changes, even though USER_BIN_PATH stays the same.
    println!("cargo:rustc-env=USER_BIN_CHECKSUM={:016x}", checksum);
    println!("cargo:rerun-if-changed={}", bin);
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
