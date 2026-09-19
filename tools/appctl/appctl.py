#!/usr/bin/env python3
"""appctl - fantuan application catalog client (C2).

Vendors apps from the `fantuan-apps` catalog branch (or a local overlay /
`--from` path) into apps/<name>/, pins them in apps.lock, verifies hashes,
ABI and the GPL firewall, generates the CONFIG_APP_* Kconfig fragments and
emits the SBOM. Python stdlib only; run from the repository root.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import app_catalog
import app_lock
import app_menu
import app_sbom
import app_util
import app_vendor
import app_verify


def load_catalog(args, root):
    path = (
        args.catalog
        or os.environ.get("FANTUAN_APPS_CATALOG")
        or os.path.join(root, "apps-catalog.toml")
    )
    if not os.path.exists(path):
        app_util.die(f"catalog {path} not found")
    return app_catalog.Catalog(path)


def resolve_source(args, root, catalog, entry=None):
    if args.from_:
        return app_catalog.from_arg(args.from_, root)
    if entry is not None and entry["source"]:
        recorded = entry["source"]
        if recorded.startswith("path:"):
            return app_catalog.from_arg(recorded[len("path:"):], root)
        return catalog.resolve(root, recorded)
    return catalog.resolve(root)


def cmd_list(args, root, catalog):
    for source in catalog.sources():
        try:
            resolved = catalog.resolve(root, source["name"])
            names = app_catalog.list_apps(root, resolved)
            listing = ", ".join(names) if names else "(empty)"
            print(f"catalog ({source['name']}): {listing}")
        except (ValueError, OSError) as exc:
            print(f"catalog ({source['name']}): unavailable ({exc})")
    entries = app_lock.load(os.path.join(root, "apps.lock"))
    print("vendored:")
    if not entries:
        print("  (none)")
    for entry in entries:
        enabled = app_util.configured(root, app_util.app_symbol(entry["name"]))
        gpl = "true" if entry["gpl"] else "false"
        print(
            f"  {entry['name']} {entry['version']} {entry['license']} "
            f"gpl={gpl} enabled={'y' if enabled else 'n'} rev={entry['rev'][:12]}"
        )
    return 0


def cmd_add(args, root, catalog):
    source = resolve_source(args, root, catalog)
    entry = app_vendor.vendor(root, args.name, source)
    entries = app_lock.load(os.path.join(root, "apps.lock"))
    app_lock.upsert(entries, entry)
    app_lock.save(os.path.join(root, "apps.lock"), entries)
    print(
        f"added {entry['name']} {entry['version']} from {entry['source']} "
        f"rev {entry['rev'][:12]}"
    )
    return 0


def cmd_remove(args, root, catalog):
    entries = app_lock.load(os.path.join(root, "apps.lock"))
    if app_lock.find(entries, args.name) is None and not os.path.isdir(
        os.path.join(root, "apps", args.name)
    ):
        app_util.die(f"{args.name}: not vendored")
    app_vendor.remove(root, args.name)
    app_lock.drop(entries, args.name)
    app_lock.save(os.path.join(root, "apps.lock"), entries)
    print(f"removed {args.name} (tree + lock entry)")
    return 0


def sync_command(args, root, catalog, preserve):
    entries = app_lock.load(os.path.join(root, "apps.lock"))
    targets = [app_lock.find(entries, args.name)] if args.name else list(entries)
    if not targets:
        print("nothing to sync (apps.lock is empty)")
        return 0
    for entry in targets:
        name = entry["name"] if entry else args.name
        if entry is not None and not args.from_ and entry.get("source") == "upstream":
            print(f"{name}: pinned upstream tarball (not a catalog source; sync n/a)")
            continue
        source = resolve_source(args, root, catalog, entry)
        if (
            preserve
            and entry is not None
            and os.path.isdir(os.path.join(root, "apps", name))
            and app_catalog.source_rev(root, source) == entry["rev"]
        ):
            print(f"{name}: up to date (rev {entry['rev'][:12]})")
            continue
        patches = app_vendor.snapshot_patches(root, name) if preserve else None
        new = app_vendor.vendor(root, name, source, patches)
        app_lock.upsert(entries, new)
        old = f"{entry['rev'][:12]} -> " if entry else ""
        print(
            f"{'upgraded' if preserve else 'synced'} {name} {new['version']} "
            f"rev {old}{new['rev'][:12]}"
        )
    app_lock.save(os.path.join(root, "apps.lock"), entries)
    return 0


def cmd_verify(args, root, catalog):
    entries = app_lock.load(os.path.join(root, "apps.lock"))
    gpl_allow = catalog.gpl_allow()
    if args.name:
        target = app_lock.find(entries, args.name)
        errors = app_verify.check_app(root, args.name, target, args.apps_layer, gpl_allow)
    else:
        errors = []
        for entry in entries:
            errors += app_verify.check_app(
                root, entry["name"], entry, args.apps_layer, gpl_allow
            )
        errors += app_verify.orphans(root, {entry["name"] for entry in entries})
    if errors:
        print("verify: FAILED")
        for error in errors:
            print(f"  {error}")
        return 1
    layer = "apps layer" if args.apps_layer else "kernel/base layer"
    print(f"verify: OK ({len(entries)} app(s), {layer})")
    return 0


def cmd_menu(args, root, catalog):
    written, skipped, notes = app_menu.generate(root, catalog)
    if written:
        fragments = ", ".join(f"config/apps/{name}.kconfig" for name in written)
        print(f"menu: wrote {fragments}")
    else:
        print("menu: no fragments written")
    for name, reason in skipped:
        print(f"menu: skipped {name} ({reason})", file=sys.stderr)
    for name, note in notes:
        print(f"menu: {name}: unavailable ({note})", file=sys.stderr)
    return 0


def cmd_sbom(args, root):
    app_sbom.dump(root, args.output)
    return 0


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="tools/appctl/appctl.py", description=__doc__.splitlines()[0]
    )
    parser.add_argument("--root", help="integration tree (default: this repository)")
    parser.add_argument("--catalog", help="catalog file (default: <root>/apps-catalog.toml)")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="catalog apps plus vendored apps and CONFIG_APP_* state")

    p_add = sub.add_parser("add", help="vendor one app and pin it in apps.lock")
    p_add.add_argument("name")
    p_add.add_argument("--from", dest="from_", metavar="BRANCH|PATH")

    p_remove = sub.add_parser("remove", help="delete the vendored tree and lock entry")
    p_remove.add_argument("name")

    p_sync = sub.add_parser("sync", help="copy app trees from the catalog source")
    p_sync.add_argument("--from", dest="from_", metavar="BRANCH|PATH")
    p_sync.add_argument("--name")

    p_upgrade = sub.add_parser("upgrade", help="re-sync newer revisions, keep local patches")
    p_upgrade.add_argument("--from", dest="from_", metavar="BRANCH|PATH")
    p_upgrade.add_argument("--name")

    p_verify = sub.add_parser("verify", help="hashes, manifest, ABI and the GPL firewall")
    p_verify.add_argument("--name")
    p_verify.add_argument(
        "--apps-layer",
        action="store_true",
        help="allow GPL apps listed in the catalog (apps layer only)",
    )

    sub.add_parser("menu", help="generate config/apps/<name>.kconfig fragments")

    p_sbom = sub.add_parser("sbom", help="emit the JSON SBOM")
    p_sbom.add_argument("-o", "--output", metavar="FILE")
    return parser.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    root = os.path.abspath(
        args.root or os.environ.get("FANTUAN_ROOT") or app_util.repo_root()
    )
    try:
        if args.command == "list":
            return cmd_list(args, root, load_catalog(args, root))
        if args.command == "add":
            return cmd_add(args, root, load_catalog(args, root))
        if args.command == "remove":
            return cmd_remove(args, root, None)
        if args.command == "sync":
            return sync_command(args, root, load_catalog(args, root), preserve=False)
        if args.command == "upgrade":
            return sync_command(args, root, load_catalog(args, root), preserve=True)
        if args.command == "verify":
            return cmd_verify(args, root, load_catalog(args, root))
        if args.command == "menu":
            return cmd_menu(args, root, load_catalog(args, root))
        if args.command == "sbom":
            return cmd_sbom(args, root)
    except (ValueError, FileNotFoundError, OSError) as exc:
        app_util.die(str(exc))
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
