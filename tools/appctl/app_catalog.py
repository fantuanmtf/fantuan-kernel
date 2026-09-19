#!/usr/bin/env python3
"""Catalog sources (fantuan-apps branch / overlay path) for tools/appctl."""
import io
import os
import shutil
import subprocess
import tarfile

from app_util import git, load_toml


class Catalog:
    def __init__(self, path):
        self.path = os.path.abspath(path)
        self.base = os.path.dirname(self.path)
        self.data = load_toml(self.path)
        if "default" not in self.data or "source" not in self.data:
            raise ValueError(f"{path}: missing [default] or [[source]] entries")
        if "source" not in self.data["default"]:
            raise ValueError(f"{path}: [default] needs a source name")

    def source(self, name=None):
        wanted = name or self.data["default"]["source"]
        for source in self.data["source"]:
            if source.get("name") == wanted:
                if not source.get("enabled", True):
                    raise ValueError(f"{self.path}: source {wanted!r} is disabled")
                return dict(source)
        raise ValueError(f"{self.path}: unknown source {wanted!r}")

    def sources(self):
        return [dict(item) for item in self.data["source"] if item.get("enabled", True)]

    def gpl_allow(self):
        return list(self.data.get("licensing", {}).get("gpl_allow", []))

    def resolve(self, root, name=None):
        source = self.source(name)
        kind = source.get("kind")
        if kind == "path":
            path = source["path"]
            if not os.path.isabs(path):
                path = os.path.normpath(os.path.join(self.base, path))
            return {
                "name": source["name"],
                "kind": "path",
                "path": path,
                "apps": source.get("apps", "apps"),
            }
        if kind == "git":
            repo = source.get("repo", "self")
            if repo == "self":
                repo = root
            elif not os.path.isabs(repo):
                repo = os.path.normpath(os.path.join(self.base, repo))
            return {
                "name": source["name"],
                "kind": "git",
                "repo": repo,
                "rev": source.get("branch", "main"),
                "apps": source.get("apps", "apps"),
                "url": source.get("url", ""),
            }
        raise ValueError(f"{self.path}: source {source['name']!r} has kind {kind!r}")


def from_arg(arg, root):
    if os.path.isdir(arg):
        return {
            "name": "path:" + arg,
            "kind": "path",
            "path": os.path.abspath(arg),
            "apps": "apps",
        }
    result = git("rev-parse", "--verify", arg + "^{commit}", cwd=root)
    if result.returncode != 0:
        raise ValueError(f"--from {arg!r} is neither a directory nor a git revision")
    return {
        "name": arg,
        "kind": "git",
        "repo": root,
        "rev": result.stdout.strip(),
        "apps": "apps",
        "url": "",
    }


def source_rev(root, source):
    if source["kind"] == "path":
        result = git("rev-parse", "HEAD", cwd=source["path"])
        return result.stdout.strip() if result.returncode == 0 else "unversioned"
    result = git("rev-parse", "--verify", source["rev"] + "^{commit}", cwd=source["repo"])
    if result.returncode != 0:
        raise ValueError(
            f"git source {source['name']!r}: revision {source['rev']!r} not found"
        )
    return result.stdout.strip()


def _path_app_dir(source, name):
    candidates = [
        os.path.join(source["path"], source["apps"], name),
        os.path.join(source["path"], name),
        source["path"],
    ]
    for candidate in candidates:
        if os.path.isfile(os.path.join(candidate, "manifest.toml")):
            return candidate
    raise FileNotFoundError(
        f"{name!r} not found in path source {source['path']!r} "
        f"(want {source['apps']}/{name}/manifest.toml)"
    )


def list_apps(root, source):
    if source["kind"] == "path":
        base = os.path.join(source["path"], source["apps"])
        if not os.path.isdir(base):
            return []
        return sorted(
            name
            for name in os.listdir(base)
            if os.path.isfile(os.path.join(base, name, "manifest.toml"))
        )
    rev = source_rev(root, source)
    result = git("ls-tree", "--name-only", rev, source["apps"] + "/", cwd=source["repo"])
    if result.returncode != 0:
        raise ValueError(f"git source {source['name']!r}: {result.stderr.strip()}")
    prefix = source["apps"] + "/"
    return sorted(
        line[len(prefix):]
        for line in result.stdout.splitlines()
        if line.startswith(prefix) and "/" not in line[len(prefix):]
    )


def stage(root, source, name, workdir):
    if source["kind"] == "path":
        return _path_app_dir(source, name)
    rev = source_rev(root, source)
    archive = subprocess.run(
        ["git", "archive", rev, f"{source['apps']}/{name}"],
        cwd=source["repo"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if archive.returncode != 0:
        raise ValueError(
            f"{name!r} not found in git source {source['name']!r} at {rev[:12]}: "
            f"{archive.stderr.decode().strip()}"
        )
    if os.path.isdir(workdir):
        shutil.rmtree(workdir)
    os.makedirs(workdir)
    with tarfile.open(fileobj=io.BytesIO(archive.stdout), mode="r:") as tar:
        tar.extractall(workdir, filter="data")
    return os.path.join(workdir, source["apps"], name)
