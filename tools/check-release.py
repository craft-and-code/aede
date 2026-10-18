#!/usr/bin/env python3
"""Refuse inconsistent CLI, desktop package and Tauri release versions before building."""
import json
import os
from pathlib import Path
import sys
import re

ROOT = Path(__file__).resolve().parent.parent

def workspace_version(path):
    text = path.read_text()
    section = re.search(r"(?ms)^\[workspace\.package\]\s*\n(.*?)(?=^\[|\Z)", text)
    match = re.search(r'^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$', section[1], re.M) if section else None
    if match is None:
        raise ValueError(f"Missing workspace version in {path}")
    return match[1]

def check(root=ROOT, tag=None):
    engine = workspace_version(root / "Cargo.toml")
    sources = {
        "desktop/Cargo.toml": workspace_version(root / "desktop/Cargo.toml"),
        "desktop/package.json": json.loads((root / "desktop/package.json").read_text())["version"],
        "desktop/package-lock.json": json.loads((root / "desktop/package-lock.json").read_text())["version"],
        "desktop/src-tauri/tauri.conf.json": json.loads((root / "desktop/src-tauri/tauri.conf.json").read_text())["version"],
    }
    for source, value in sources.items():
        if value != engine:
            raise ValueError(f"{source}: version {value} differs from CLI {engine}")
    if tag is not None and tag != f"v{engine}":
        raise ValueError(f"Release tag {tag} differs from v{engine}")
    if not (root / f"docs/releases/v{engine}.md").is_file():
        raise ValueError("Curated release notes are missing")
    return engine

if __name__ == "__main__":
    try:
        tag = os.environ.get("GITHUB_REF_NAME") if os.environ.get("GITHUB_REF_TYPE") == "tag" else None
        print(f"CLI/App release versions agree: {check(tag=tag)}")
    except (ValueError, OSError, KeyError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
