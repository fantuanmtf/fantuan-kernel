//! Link arguments for the aarch64 kernel: the linker script shipped next to
//! this crate, plus the shared Kconfig-lite cfg emission. No C driver layer
//! in R9a (no storage/network on the direct-FDT path yet).

use std::env;

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../tools/kconfig_emit.rs"));

fn main() {
    kconfig_emit();
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/link.ld");
    println!("cargo:rustc-link-arg=-T{dir}/link.ld");
}
