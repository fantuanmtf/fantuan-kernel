//! Passes the linker script and makes cargo relink when it changes (the
//! kernel is linked at PHYS_OFFSET + 16 MiB, matching the stage2 load).

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/link.ld");
    println!("cargo:rustc-link-arg=-T{dir}/link.ld");

    // The 48 ISR stubs are plain assembly; clang assembles them for the
    // custom target (same toolchain the riscv C drivers use).
    println!("cargo:rerun-if-changed={dir}/src/isr_stubs.S");
    cc::Build::new()
        .compiler("clang")
        .archiver("llvm-ar")
        .flag("--target=i686-unknown-none-elf")
        .flag("-m32")
        .file(format!("{dir}/src/isr_stubs.S"))
        .compile("isrobj");
}
