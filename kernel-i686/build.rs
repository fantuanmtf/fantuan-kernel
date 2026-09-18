//! Passes the linker script and makes cargo relink when it changes (the
//! kernel is linked at PHYS_OFFSET + 16 MiB, matching the stage2 load).

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/link.ld");
    println!("cargo:rustc-link-arg=-T{dir}/link.ld");

    // The 48 ISR stubs are plain assembly; clang assembles them for the
    // custom target (same toolchain the riscv C drivers use).
    println!("cargo:rerun-if-changed={dir}/src/isr_stubs.S");
    println!("cargo:rerun-if-changed={dir}/src/context.S");
    println!("cargo:rerun-if-changed={dir}/src/user_entry.S");
    cc::Build::new()
        .compiler("clang")
        .archiver("llvm-ar")
        .flag("--target=i686-unknown-none-elf")
        .flag("-m32")
        .file(format!("{dir}/src/isr_stubs.S"))
        .file(format!("{dir}/src/context.S"))
        .file(format!("{dir}/src/user_entry.S"))
        .compile("isrobj");

    // The ring-3 test program is a flat binary (org 0x400000) assembled by
    // nasm, then embedded with include_bytes! in user.rs.
    println!("cargo:rerun-if-changed={dir}/src/user_stub.asm");
    let out = std::env::var("OUT_DIR").unwrap();
    let status = std::process::Command::new("nasm")
        .args(["-f", "bin", "-o"])
        .arg(format!("{out}/user_stub.bin"))
        .arg(format!("{dir}/src/user_stub.asm"))
        .status()
        .expect("nasm is required to build the i686 ring-3 stub");
    assert!(status.success(), "nasm failed on user_stub.asm");
}
