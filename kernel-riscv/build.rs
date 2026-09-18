//! Link arguments for the RISC-V kernel: the linker script shipped next to
//! this crate, plus rebuild tracking.

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/link.ld");
    println!("cargo:rustc-link-arg=-T{dir}/link.ld");
}
