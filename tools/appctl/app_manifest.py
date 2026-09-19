#!/usr/bin/env python3
"""Manifest parsing, validation and ABI discovery for tools/appctl (C2)."""
import os
import re

from app_util import REQUIRED_MANIFEST, SPDX_RE, load_toml

ABI_RE = re.compile(r"pub const ABI_VERSION: u64 = (\d+);")


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
    return errors


def abi_version(root):
    path = os.path.join(root, "kernel-core", "src", "syscall.rs")
    with open(path, encoding="utf-8") as handle:
        match = ABI_RE.search(handle.read())
    if not match:
        raise ValueError(f"{path}: ABI_VERSION not found")
    return int(match.group(1))
