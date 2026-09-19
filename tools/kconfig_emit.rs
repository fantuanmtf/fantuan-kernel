// Shared build-script helper (C1), included by every crate's build.rs via
// include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../tools/kconfig_emit.rs")).
//
// Reads the repo-root .config and emits cargo:rustc-cfg=kconfig_<lower> for
// every enabled symbol plus the matching check-cfg lines. A missing .config
// means the `net` profile - the historical default until C4 flips it to
// minimal. The symbol set mirrors tools/kconfig.py --profile net.

fn kconfig_emit() {
    use std::collections::BTreeMap;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    const DEFAULTS: &[(&str, bool)] = &[
        ("shell", true),
        ("bash", true),
        ("tools", true),
        ("net", true),
        ("net_drivers", true),
        ("tls", false),
        ("virt", false),
        ("graphics", false),
        ("desktop", false),
        ("rescue_repair", true),
        ("debug_selftest", true),
        ("secure_wipe", false),
    ];

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir
        .parent()
        .expect("crate lives directly below the repo root");
    let config = root.join(".config");
    println!("cargo:rerun-if-changed={}", config.display());

    let text = fs::read_to_string(&config).unwrap_or_default();
    let mut values: BTreeMap<String, bool> = BTreeMap::new();
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

    for (name, on) in &values {
        println!("cargo:rustc-check-cfg=cfg(kconfig_{name})");
        if *on {
            println!("cargo:rustc-cfg=kconfig_{name}");
        }
    }
    for (name, _) in DEFAULTS {
        if !values.contains_key(*name) {
            println!("cargo:rustc-check-cfg=cfg(kconfig_{name})");
        }
    }
}
