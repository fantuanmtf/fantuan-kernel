//! Link arguments for the RISC-V kernel: the linker script shipped next to
//! this crate, plus rebuild tracking.

use std::env;
use std::fs;

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../tools/kconfig_emit.rs"));

fn main() {
    kconfig_emit();
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

    // C driver layer: only blk.c + the virtio-mmio transport on this arch
    // (no port I/O). Compiled with clang for the bare-metal riscv target.
    let c = format!("{dir}/../drivers/c");
    println!("cargo:rerun-if-changed={c}/blk.c");
    println!("cargo:rerun-if-changed={c}/virtio_mmio.c");
    println!("cargo:rerun-if-changed={c}/include/virtio_mmio.h");
    println!("cargo:rerun-if-changed={c}/include/driver.h");
    println!("cargo:rerun-if-changed={c}/include/rust_core.h");
    cc::Build::new()
        .compiler("clang")
        .archiver("llvm-ar")
        .flag("--target=riscv64-unknown-none-elf")
        .flag("-march=rv64gc")
        .flag("-mabi=lp64d")
        .flag("-mcmodel=medany")
        .flag("-ffreestanding")
        .flag("-fno-builtin")
        .flag("-fno-stack-protector")
        .include(format!("{c}/include"))
        .file(format!("{c}/blk.c"))
        .file(format!("{c}/virtio_mmio.c"))
        .compile("drivobj");
}
