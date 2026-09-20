#!/usr/bin/env python3
"""Manifest parsing, validation and ABI discovery for tools/appctl (C2)."""
import os
import re

from app_util import REQUIRED_MANIFEST, SPDX_RE, load_toml

ABI_RE = re.compile(r"pub const ABI_VERSION: u64 = (\d+);")

# `requires` gate for the menu (C3, docs/APPS.md): a manifest names the layers
# an app needs; an entry not in AVAILABLE_REQUIRES keeps CONFIG_APP_<NAME> at
# default n with an "unavailable" note. The layer lands at M14, so the set is
# empty until then; adding it flips bash to buildable.
KNOWN_REQUIRES = {
    "posix-libc": "M14 POSIX/libc layer",
    "kernel-net": "M11 kernel network stack (CONFIG_NET)",
}
AVAILABLE_REQUIRES = frozenset()


def unmet_requires(manifest):
    unmet = []
    for item in manifest.get("requires", []):
        if item not in AVAILABLE_REQUIRES:
            unmet.append(f"{item} ({KNOWN_REQUIRES.get(item, 'unknown layer')})")
    return unmet


def parse(path):
    manifest = load_toml(path)
    missing = [key for key in REQUIRED_MANIFEST if key not in manifest]
    if missing:
        raise ValueError(f"{path}: missing manifest fields: {', '.join(missing)}")
    return manifest


def validate(name, manifest):
    errors = []
    if manifest.get("name") != name:
        errors.append(f"manifest name {manifest.get('name')!r} != directory {name!r}")
    if not isinstance(manifest.get("version"), str) or not manifest["version"].strip():
        errors.append("version must be a non-empty string")
    if not isinstance(manifest.get("description"), str) or not manifest["description"].strip():
        errors.append("description must be a non-empty string")
    if not isinstance(manifest.get("build"), str) or not manifest["build"].strip():
        errors.append("build must be a non-empty string")
    license_id = manifest.get("license", "")
    if not isinstance(license_id, str) or not SPDX_RE.fullmatch(license_id):
        errors.append(f"license {license_id!r} is not an SPDX identifier/expression")
    abi_min = manifest.get("abi_min")
    if not isinstance(abi_min, int) or isinstance(abi_min, bool):
        errors.append("abi_min must be an integer")
    requires = manifest.get("requires", [])
    if not isinstance(requires, list) or any(
        not isinstance(item, str) or not item.strip() for item in requires
    ):
        errors.append("requires must be a list of non-empty strings")
    return errors


def abi_version(root):
    path = os.path.join(root, "kernel-core", "src", "syscall.rs")
    with open(path, encoding="utf-8") as handle:
        match = ABI_RE.search(handle.read())
    if not match:
        raise ValueError(f"{path}: ABI_VERSION not found")
    return int(match.group(1))
