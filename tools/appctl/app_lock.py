#!/usr/bin/env python3
"""apps.lock read/write for tools/appctl (C2)."""
import os

from app_util import load_toml, toml_string

HEADER = [
    "# apps.lock - vendored application trees pinned by tools/appctl (C2).",
    "# Schema: version = 1 and one [[app]] table per vendored app with the",
    "# fields name, version, license, gpl, source, rev and sha256 (tree hash).",
    "# Do not edit by hand; use `tools/appctl/appctl.py add|sync|remove`.",
]
FIELDS = ("name", "version", "license", "gpl", "source", "rev", "sha256")


def load(path):
    if not os.path.exists(path):
        return []
    entries = load_toml(path).get("app", [])
    for entry in entries:
        missing = [key for key in FIELDS if key not in entry]
        if missing:
            raise ValueError(f"{path}: lock entry missing {', '.join(missing)}")
    return entries


def find(entries, name):
    for entry in entries:
        if entry["name"] == name:
            return entry
    return None


def upsert(entries, entry):
    entries[:] = [item for item in entries if item["name"] != entry["name"]]
    entries.append(entry)
    entries.sort(key=lambda item: item["name"])
    return entries


def drop(entries, name):
    entries[:] = [item for item in entries if item["name"] != name]
    return entries


def save(path, entries):
    lines = list(HEADER) + ["", "version = 1"]
    for entry in sorted(entries, key=lambda item: item["name"]):
        lines += ["", "[[app]]"]
        for key in FIELDS:
            value = entry[key]
            if isinstance(value, bool):
                lines.append(f"{key} = {'true' if value else 'false'}")
            elif isinstance(value, str):
                lines.append(f"{key} = {toml_string(value)}")
            else:
                lines.append(f"{key} = {value}")
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines) + "\n")
