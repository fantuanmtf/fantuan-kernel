//! Tracks and passes the user linker script (linked at 0x400000).

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/link.ld");
    println!("cargo:rustc-link-arg=-T{dir}/link.ld");
}
