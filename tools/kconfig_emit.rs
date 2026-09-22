// Shared build-script helper (C1), included by every crate's build.rs via
// include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../tools/kconfig_emit.rs")).
//
// Reads the repo-root .config and emits cargo:rustc-cfg=kconfig_<lower> for
// every enabled symbol plus the matching check-cfg lines. A missing .config
// resolves to the `minimal` profile (C5): SHELL (plus the declared BASH
// symbol), with the other defaults from config/Kconfig. The symbol set
// mirrors tools/kconfig.py --profile minimal + the schema defaults.
//
// FEATURE_GATED lists the symbols whose code depends on an optional crate
// (`kernel-net`): their cfg is only emitted when the matching cargo feature
// `kconfig-<lower>` is active, so the cfg and the dependency edge can never
// disagree (a direct `cargo build` without the feature is a minimal kernel,
// no compile error). tools/build.sh / build-bios.sh pass the feature from
// .config; tools/smoke-config.sh proves the cargo tree edge.

#[allow(dead_code)]
fn kconfig_values() -> std::collections::BTreeMap<String, bool> {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    const DEFAULTS: &[(&str, bool)] = &[
        ("shell", true),
        ("bash", true),
        ("tools", false),
        ("net", false),
        ("net_drivers", false),
        ("tls", false),
        ("virt", false),
        ("graphics", false),
        ("desktop", false),
        ("rescue_repair", false),
        ("imager", false),
        ("debug_selftest", false),
        ("secure_wipe", false),
        ("smbios", false),
    ];

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir
        .parent()
        .expect("crate lives directly below the repo root");
    let config = root.join(".config");
    println!("cargo:rerun-if-changed={}", config.display());

    let text = fs::read_to_string(&config).unwrap_or_default();
    let mut values: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("CONFIG_") else {
            continue;
        };
        if let Some((name, value)) = rest.split_once('=') {
            let on = matches!(value.trim(), "y" | "Y" | "true" | "1");
            values.insert(name.trim().to_ascii_lowercase(), on);
        }
    }
    if values.is_empty() {
        for (name, on) in DEFAULTS {
            values.insert((*name).to_string(), *on);
        }
    }
    values
}

/// Read one Kconfig bool from the repo `.config` (missing symbols default to
/// `false`, matching the schema defaults for every optional subsystem).
#[allow(dead_code)]
fn kconfig_value(name: &str) -> bool {
    let values = kconfig_values();
    let key = name.to_ascii_lowercase();
    if values.is_empty() {
        return false;
    }
    values.get(&key).copied().unwrap_or(false)
}

fn kconfig_emit() {
    use std::env;

    // Symbols backed by an optional dependency behind `kconfig-<lower>`.
    const FEATURE_GATED: &[&str] = &["net"];
    let values = kconfig_values();

    for (name, on) in &values {
        println!("cargo:rustc-check-cfg=cfg(kconfig_{name})");
        let feature = format!("CARGO_FEATURE_KCONFIG_{}", name.to_ascii_uppercase());
        let feature_on = env::var(&feature).is_ok();
        if *on && (!FEATURE_GATED.contains(&name.as_str()) || feature_on) {
            println!("cargo:rustc-cfg=kconfig_{name}");
        }
    }
    for name in ["shell", "bash", "tools", "net", "net_drivers", "tls", "virt", "graphics",
        "desktop", "rescue_repair", "imager", "debug_selftest", "secure_wipe", "smbios"] {
        if !values.contains_key(name) {
            println!("cargo:rustc-check-cfg=cfg(kconfig_{name})");
        }
    }
}
