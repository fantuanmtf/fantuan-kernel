//! Kconfig-lite cfg emission (C1): the portable core gets the same
//! `kconfig_<lower>` cfgs as the kernels so shared code can be gated without
//! an arch dependency. Shared helper in tools/kconfig_emit.rs.

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../tools/kconfig_emit.rs"));

fn main() {
    kconfig_emit();
}
