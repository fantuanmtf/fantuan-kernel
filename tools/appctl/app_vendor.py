#!/usr/bin/env python3
"""Vendoring operations (add/sync/upgrade/remove) for tools/appctl (C2)."""
import os
import shutil

import app_catalog
from app_manifest import parse
from app_util import tree_sha256

VENDOR_FILES = ("manifest.toml", "README.md", "patches", "src")


def _copy_app(source_dir, destination):
    source_dir = os.path.abspath(source_dir)
    if not os.path.isfile(os.path.join(source_dir, "manifest.toml")):
        raise ValueError(f"{source_dir}: no manifest.toml")
    os.makedirs(destination, exist_ok=True)
    for item in VENDOR_FILES:
        src = os.path.join(source_dir, item)
        dst = os.path.join(destination, item)
        if os.path.islink(src):
            raise ValueError(f"{src}: symlink in vendored tree")
        if os.path.isdir(src):
            shutil.copytree(src, dst, symlinks=False)
        elif os.path.isfile(src):
            shutil.copy2(src, dst)
    for required in ("patches", "src"):
        os.makedirs(os.path.join(destination, required), exist_ok=True)


def _entry(name, source, manifest, rev, sha):
    return {
        "name": name,
        "version": manifest["version"],
        "license": manifest["license"],
        "gpl": bool(manifest.get("gpl", False)),
        "source": source["name"],
        "rev": rev,
        "sha256": sha,
    }


def vendor(root, name, source, preserve_patches=None):
    os.makedirs(os.path.join(root, "build"), exist_ok=True)
    staged = app_catalog.stage(root, source, name, os.path.join(root, "build", "appctl-work"))
    manifest = parse(os.path.join(staged, "manifest.toml"))
    if manifest.get("name") != name:
        raise ValueError(f"{staged}: manifest name {manifest.get('name')!r} != {name!r}")
    staging = os.path.join(root, "build", "appctl-vendor", name)
    if os.path.isdir(staging):
        shutil.rmtree(staging)
    os.makedirs(staging, exist_ok=True)
    _copy_app(staged, staging)
    for rel, data in (preserve_patches or {}).items():
        target = os.path.join(staging, rel)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target, "wb") as handle:
            handle.write(data)
    sha = tree_sha256(staging)
    dest = os.path.join(root, "apps", name)
    if os.path.isdir(dest):
        shutil.rmtree(dest)
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    os.replace(staging, dest)
    return _entry(name, source, manifest, app_catalog.source_rev(root, source), sha)


def snapshot_patches(root, name):
    path = os.path.join(root, "apps", name, "manifest.toml")
    if not os.path.isfile(path):
        return {}
    try:
        manifest = parse(path)
    except (ValueError, OSError):
        return {}
    snapshot = {}
    for rel in manifest.get("patches", []):
        full = os.path.join(root, "apps", name, rel)
        if os.path.isfile(full):
            with open(full, "rb") as handle:
                snapshot[rel] = handle.read()
    return snapshot


def remove(root, name):
    dest = os.path.join(root, "apps", name)
    if os.path.isdir(dest):
        shutil.rmtree(dest)
    fragment = os.path.join(root, "config", "apps", f"{name}.kconfig")
    if os.path.isfile(fragment):
        os.remove(fragment)
