//! Link arguments for the RISC-V kernel: the linker script shipped next to
//! this crate, plus rebuild tracking.

use std::env;
use std::fs;

fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/link.ld");
    println!("cargo:rustc-link-arg=-T{dir}/link.ld");

    // Embed the user program (built by tools/build.sh before the kernel).
    let user = format!("{dir}/user_program.bin");
    println!("cargo:rerun-if-changed={user}");
    if !std::path::Path::new(&user).exists() {
        panic!(
            "kernel-riscv/user_program.bin is missing - build the user crate first (tools/build.sh or tools/run.sh)"
        );
    }
    let out = env::var("OUT_DIR").unwrap();
    let gen = format!("pub static USER_ELF: &[u8] = include_bytes!({user:?});\n");
    fs::write(format!("{out}/user_program.rs"), gen).unwrap();
}
