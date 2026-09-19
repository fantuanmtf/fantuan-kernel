#!/usr/bin/env python3
"""JSON SBOM export for tools/appctl (C2)."""
import json
import os

import app_lock


def build(root):
    entries = app_lock.load(os.path.join(root, "apps.lock"))
    apps = [
        {
            "name": entry["name"],
            "version": entry["version"],
            "license": entry["license"],
            "gpl": bool(entry["gpl"]),
            "source": entry["source"],
            "rev": entry["rev"],
            "sha256": entry["sha256"],
        }
        for entry in sorted(entries, key=lambda item: item["name"])
    ]
    return {"schema": "fantuan-apps-sbom/1", "apps": apps}


def dump(root, output=None):
    document = json.dumps(build(root), indent=2, sort_keys=True) + "\n"
    if output:
        with open(output, "w", encoding="utf-8") as handle:
            handle.write(document)
    else:
        print(document, end="")
    return document
