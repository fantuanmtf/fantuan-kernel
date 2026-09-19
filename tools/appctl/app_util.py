#!/usr/bin/env python3
"""Shared helpers for tools/appctl (C2): hashing, TOML and small utilities."""
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib

REQUIRED_MANIFEST = ("name", "version", "license", "description", "abi_min", "build")
SPDX_RE = re.compile(r"[A-Za-z0-9.+()-]+(?: (?:AND|OR|WITH) [A-Za-z0-9.+()-]+)*")


def repo_root():
    return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def die(message, code=1):
    print(f"appctl: {message}", file=sys.stderr)
    sys.exit(code)


def git(*args, cwd=None):
    return subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True, check=False
    )


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def tree_files(root):
    files = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for name in sorted(filenames):
            full = os.path.join(dirpath, name)
            if os.path.islink(full):
                raise ValueError(f"symlink in vendored tree: {full}")
            if os.path.isfile(full):
                files.append(os.path.relpath(full, root).replace(os.sep, "/"))
    return sorted(files, key=os.fsencode)


def tree_sha256(root):
    digest = hashlib.sha256()
    for rel in tree_files(root):
        digest.update(f"{rel} {sha256_file(os.path.join(root, rel))}\n".encode())
    return digest.hexdigest()


def app_symbol(name):
    return "APP_" + re.sub(r"[^A-Za-z0-9]", "_", name).upper()


def load_toml(path):
    with open(path, "rb") as handle:
        return tomllib.load(handle)


def toml_string(value):
    return json.dumps(value)


def configured(root, symbol):
    path = os.path.join(root, ".config")
    if not os.path.exists(path):
        return False
    wanted = f"CONFIG_{symbol}"
    with open(path, encoding="utf-8") as handle:
        for raw in handle:
            line = raw.strip()
            if line.startswith(wanted + "="):
                return line.split("=", 1)[1].strip().lower() in ("y", "true", "1")
    return False
