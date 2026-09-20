#!/usr/bin/env python3
"""Kconfig-lite configurator for fantuan-kernel (C1).

Reads config/Kconfig (plus config/apps/*.kconfig fragments), writes .config
at the repo root and, with --emit, the content-hashed build/config/features.rs
+ build/config/features.env. Python stdlib only; no curses.

  menu (default)  interactive toggle UI   --symbol NAME=Y|N  set symbol
  --text          print effective config  --profile minimal|rescue|net|
  --olddefconfig  fill missing defaults       tls|desktop|hypervisor|all
  --check         validate depends only   --emit  regenerate features.{rs,env}

A missing .config resolves to the `minimal` profile (SHELL only, plus the
declared BASH symbol), matching the build scripts' default; the rescue and
net profiles are explicit (`--profile rescue|net`, CONFIG_NET=y drives the
kconfig-net feature).
"""
import argparse, glob, hashlib, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCHEMA = os.path.join(ROOT, "config", "Kconfig")
APP_GLOB = os.path.join(ROOT, "config", "apps", "*.kconfig")
CONFIG_PATH = os.path.join(ROOT, ".config")
FEATURES_RS = os.path.join(ROOT, "build", "config", "features.rs")
FEATURES_ENV = os.path.join(ROOT, "build", "config", "features.env")

# minimal (C5): the Live kernel's boot set - kernel + boot + shell only. The
# rescue/diagnostic commands and boot repair are a profile (`rescue`), and the
# tools are non-default (app catalog at M14; kernel bridge until then).
NET = ["SHELL", "TOOLS", "NET", "NET_DRIVERS", "DEBUG_SELFTEST"]
TLS = NET + ["TLS"]
PROFILES = {
    "minimal": ["SHELL"],
    "rescue": ["SHELL", "RESCUE_REPAIR"],
    "net": NET,
    "tls": TLS,
    "desktop": NET + ["GRAPHICS", "DESKTOP"],
    "hypervisor": ["SHELL", "RESCUE_REPAIR", "VIRT"],
    "all": None,
}


def parse_kconfig(path):
    symbols, current, help_mode = {}, None, False
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            line = raw.strip()
            if help_mode:
                if line:
                    current["help"] += (" " if current["help"] else "") + line
                    continue
                help_mode = False
            if not line or line.startswith("#"):
                continue
            if line.startswith("config "):
                name = line.split(None, 1)[1].strip()
                current = {"prompt": "", "default": False, "depends": [], "help": ""}
                symbols[name] = current
            elif current is None:
                continue
            elif line.startswith("bool"):
                rest = line[4:].strip()
                if rest.startswith('"') and rest.endswith('"'):
                    current["prompt"] = rest[1:-1]
            elif line.startswith("prompt"):
                current["prompt"] = line[6:].strip().strip('"')
            elif line.startswith("default"):
                current["default"] = line.split(None, 1)[1].strip().lower() in ("y", "true", "1")
            elif line.startswith("depends on"):
                current["depends"].append(line[len("depends on"):].strip())
            elif line == "help":
                help_mode = True
    return symbols


def load_schema():
    symbols = parse_kconfig(SCHEMA)
    for path in sorted(glob.glob(APP_GLOB)):
        for name, sym in parse_kconfig(path).items():
            if name in symbols:
                print(f"warning: {path} redefines {name}; keeping the first", file=sys.stderr)
            else:
                symbols[name] = sym
    return symbols


def read_config(path):
    if not os.path.exists(path):
        return None
    values = {}
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            line = raw.strip()
            if line.startswith("CONFIG_") and "=" in line:
                name, _, value = line.partition("=")
                values[name[len("CONFIG_"):]] = value.strip().lower() in ("y", "true", "1")
            elif line.startswith("# CONFIG_") and line.endswith(" is not set"):
                values[line[len("# CONFIG_"):-len(" is not set")]] = False
    return values


def apply_profile(symbols, name):
    if name not in PROFILES:
        raise SystemExit(f"unknown profile: {name} (want {'|'.join(PROFILES)})")
    values = {n: s["default"] for n, s in symbols.items()}
    if PROFILES[name] is None:
        values = dict.fromkeys(values, True)
    else:
        for sym_name in PROFILES[name]:
            if sym_name not in symbols:
                raise SystemExit(f"profile {name}: unknown symbol {sym_name}")
            values[sym_name] = True
            for dep in symbols[sym_name]["depends"]:
                values[dep] = True
    return values


def validate(symbols, values):
    return [
        f"{name} depends on {dep} (=n)"
        for name, sym in symbols.items()
        if values.get(name, False)
        for dep in sym["depends"]
        if not values.get(dep, False)
    ]


def effective(args, symbols):
    if args.profile:
        values, profile = apply_profile(symbols, args.profile), args.profile
    else:
        loaded = read_config(CONFIG_PATH)
        if loaded is None:
            values, profile = apply_profile(symbols, "minimal"), "minimal"
        else:
            values = {n: loaded.get(n, symbols[n]["default"]) for n in symbols}
            profile = next(
                (
                    l.split(":", 1)[1].strip()
                    for l in open(CONFIG_PATH, encoding="utf-8")
                    if l.strip().startswith("# profile:")
                ),
                "custom",
            )
    for spec in args.symbol:
        name, sep, value = spec.partition("=")
        name, value = name.strip().upper(), value.strip().upper()
        if not sep or value not in ("Y", "N"):
            raise SystemExit(f"--symbol wants NAME=Y|N, got: {spec}")
        if name not in symbols:
            raise SystemExit(f"--symbol: unknown symbol {name}")
        values[name] = value == "Y"
    return values, profile


def write_config(symbols, values, profile):
    lines = [
        "# fantuan-kernel configuration (generated by tools/kconfig.py - do not edit)",
        f"# profile: {profile}",
    ]
    lines += [f"CONFIG_{n}={'y' if values.get(n, False) else 'n'}" for n in symbols]
    with open(CONFIG_PATH, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")


def emit(symbols, values, profile):
    canonical = "".join(f"{n}={'y' if values.get(n, False) else 'n'}\n" for n in symbols)
    digest = hashlib.sha256(canonical.encode()).hexdigest()
    header = f"profile: {profile}, config sha256: {digest}"
    os.makedirs(os.path.dirname(FEATURES_RS), exist_ok=True)
    rust = [
        "// Generated by tools/kconfig.py --emit from .config - do not edit.",
        f"// {header}",
    ] + [
        f"#[allow(dead_code)]\npub const CONFIG_{n}: bool = "
        f"{'true' if values.get(n, False) else 'false'};"
        for n in symbols
    ]
    with open(FEATURES_RS, "w", encoding="utf-8") as fh:
        fh.write("\n".join(rust) + "\n")
    env = [
        "# Generated by tools/kconfig.py --emit from .config - do not edit.",
        f"# {header}",
    ] + [f"config_{n.lower()}={'y' if values.get(n, False) else 'n'}" for n in symbols]
    with open(FEATURES_ENV, "w", encoding="utf-8") as fh:
        fh.write("\n".join(env) + "\n")
    return digest


def report(symbols, values, profile, what="config"):
    errors = validate(symbols, values)
    if not errors:
        if what == "emit":
            digest = emit(symbols, values, profile)
            print(f"wrote build/config/features.rs + features.env (sha256 {digest[:12]})")
        else:
            enabled = sum(1 for n in symbols if values.get(n, False))
            print(f"config: OK ({enabled}/{len(symbols)} symbols enabled, profile {profile})")
        return 0
    print("config: REJECTED")
    for err in errors:
        print(f"  {err}")
    return 1


def menu_toggle(symbols, values, name):
    if values.get(name, False):
        required = [o for o, s in symbols.items() if values.get(o, False) and name in s["depends"]]
        if required:
            print(f"{name} is required by {' '.join(required)}; disable those first")
        else:
            values[name] = False
    elif [d for d in symbols[name]["depends"] if not values.get(d, False)]:
        print(f"{name} depends on {' '.join(symbols[name]['depends'])}; enable those first")
    else:
        values[name] = True


def menu(symbols, values):
    names = list(symbols)
    print("fantuan-kernel configuration (Kconfig-lite)")
    while True:
        for i, name in enumerate(names, 1):
            sym, state = symbols[name], "y" if values.get(name, False) else "n"
            deps = f" [depends: {' '.join(sym['depends'])}]" if sym["depends"] else ""
            print(f"{i:3}) [{state}] {name:<16} {sym['prompt']}{deps}")
        try:
            cmd = input("toggle # | p <profile> | d defaults | s check | w write+exit | q quit: ")
        except EOFError:
            return 1
        cmd = cmd.strip()
        if cmd in ("q", "quit"):
            return 1
        if cmd in ("w", "write"):
            if report(symbols, values, "custom") == 0:
                write_config(symbols, values, "custom")
                print(f"wrote {CONFIG_PATH}")
                return 0
        elif cmd in ("d", "defaults"):
            values = {n: s["default"] for n, s in symbols.items()}
        elif cmd in ("s", "check"):
            report(symbols, values, "custom")
        elif cmd.startswith("p "):
            name = cmd.split(None, 1)[1].strip()
            if name not in PROFILES:
                print(f"unknown profile: {name}")
            else:
                values = apply_profile(symbols, name)
        elif cmd.isdigit() and 1 <= int(cmd) <= len(names):
            menu_toggle(symbols, values, names[int(cmd) - 1])
        else:
            print("unknown command")


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="tools/kconfig.py", description="Kconfig-lite configurator for fantuan-kernel"
    )
    parser.add_argument("command", nargs="?", default="menu", choices=["menu"])
    parser.add_argument("--text", action="store_true", help="print the effective configuration")
    parser.add_argument("--olddefconfig", action="store_true", help="fill missing symbols with defaults")
    parser.add_argument("--symbol", action="append", default=[], metavar="NAME=Y|N")
    parser.add_argument("--profile", choices=sorted(PROFILES))
    parser.add_argument("--check", action="store_true", help="validate depends; no write")
    parser.add_argument("--emit", action="store_true", help="write build/config/features.{rs,env}")
    return parser.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    symbols = load_schema()
    if not symbols:
        raise SystemExit(f"no symbols parsed from {SCHEMA}")
    interactive = not any(
        [args.text, args.olddefconfig, args.symbol, args.profile, args.check, args.emit]
    )
    if args.command == "menu" and interactive:
        loaded = read_config(CONFIG_PATH)
        if loaded is None:
            values = apply_profile(symbols, "minimal")
        else:
            values = {n: loaded.get(n, symbols[n]["default"]) for n in symbols}
        return menu(symbols, values)
    values, profile = effective(args, symbols)
    if args.olddefconfig:
        write_config(symbols, values, profile)
        print(f"wrote {CONFIG_PATH} (profile {profile})")
    if args.check or args.text:
        if args.text:
            for name, sym in symbols.items():
                deps = f" [depends: {', '.join(sym['depends'])}]" if sym["depends"] else ""
                state = "y" if values.get(name, False) else "n"
                print(f"CONFIG_{name:<16} {state}  {sym['prompt']}{deps}")
        return report(symbols, values, profile)
    if args.emit:
        return report(symbols, values, profile, "emit")
    if args.profile or args.symbol:
        if report(symbols, values, profile) == 0:
            write_config(symbols, values, profile)
            print(f"wrote {CONFIG_PATH} (profile {profile})")
            return 0
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
