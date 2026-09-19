#!/usr/bin/env python3
"""Integrity, ABI, licence and GPL-firewall checks for tools/appctl (C2)."""
import os

from app_manifest import abi_version, parse, validate
from app_util import tree_sha256


def check_app(root, name, entry, apps_layer=False, gpl_allow=()):
    appdir = os.path.join(root, "apps", name)
    if entry is None:
        return [f"{name}: not in apps.lock"]
    if not os.path.isdir(appdir):
        return [f"{name}: vendored tree apps/{name}/ is missing"]
    try:
        actual = tree_sha256(appdir)
    except ValueError as exc:
        return [f"{name}: {exc}"]
    errors = []
    locked = entry.get("sha256", "")
    if actual != locked:
        errors.append(
            f"{name}: sha256 mismatch (lock {str(locked)[:12]}, tree {actual[:12]})"
        )
    manifest_path = os.path.join(appdir, "manifest.toml")
    if not os.path.isfile(manifest_path):
        return errors + [f"{name}: manifest.toml missing"]
    try:
        manifest = parse(manifest_path)
    except (ValueError, OSError) as exc:
        return errors + [f"{name}: {exc}"]
    errors += [f"{name}: {err}" for err in validate(name, manifest)]
    try:
        ceiling = abi_version(root)
        abi_min = manifest.get("abi_min")
        if isinstance(abi_min, int) and not isinstance(abi_min, bool) and abi_min > ceiling:
            errors.append(f"{name}: abi_min {abi_min} > fantuan_abi ABI_VERSION {ceiling}")
    except (OSError, ValueError) as exc:
        errors.append(f"{name}: cannot read fantuan_abi ABI_VERSION: {exc}")
    if bool(manifest.get("gpl", False)):
        if not apps_layer:
            errors.append(f"{name}: gpl=true refused in the kernel/base layer")
        elif name not in tuple(gpl_allow):
            errors.append(f"{name}: gpl=true but not in the apps-layer allow list")
    return errors


def orphans(root, locked_names):
    errors = []
    apps_dir = os.path.join(root, "apps")
    if not os.path.isdir(apps_dir):
        return errors
    for name in sorted(os.listdir(apps_dir)):
        path = os.path.join(apps_dir, name)
        if os.path.isdir(path) and name not in locked_names and not name.startswith("."):
            errors.append(f"{name}: vendored tree without an apps.lock entry")
    return errors
