//! Passes the linker script and makes cargo relink when it changes (the
//! kernel is linked at PHYS_OFFSET + 16 MiB, matching the stage2 load).

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/link.ld");
    println!("cargo:rustc-link-arg=-T{dir}/link.ld");
}
