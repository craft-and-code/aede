#!/usr/bin/env python3
"""Pin FlacCompagnon Core to GitHub's latest published stable release.

This explicit online maintenance step precedes tools/check.sh, whose locked,
offline checks remain reproducible. No third-party Python packages are needed.
"""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
from urllib import error, request


REPOSITORY = "https://github.com/craft-and-code/FlacCompagnon"
LATEST_RELEASE = "https://api.github.com/repos/craft-and-code/FlacCompagnon/releases/latest"
MAX_RESPONSE_BYTES = 1024 * 1024
STABLE_TAG = re.compile(r"v?(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)")


class UpdateError(Exception):
    """The published release cannot be selected or pinned safely."""


def release_tag(payload):
    """Reject draft, prerelease and malformed release metadata."""
    if not isinstance(payload, dict):
        raise UpdateError("GitHub returned invalid release metadata")
    if payload.get("draft") is not False or payload.get("prerelease") is not False:
        raise UpdateError("GitHub's latest release is not a published stable release")
    if not isinstance(payload.get("published_at"), str) or not payload["published_at"]:
        raise UpdateError("GitHub's latest release has no publication date")
    tag = payload.get("tag_name")
    if not isinstance(tag, str) or not STABLE_TAG.fullmatch(tag):
        raise UpdateError("GitHub's latest release has no stable version tag")
    return tag


def latest_tag():
    """Query only the public release endpoint, never unpublished Git tags."""
    req = request.Request(LATEST_RELEASE, headers={
        "Accept": "application/vnd.github+json",
        "User-Agent": "Aede-FlacCompagnon-update",
    })
    try:
        with request.urlopen(req, timeout=30) as response:
            if response.status != 200:
                raise UpdateError(f"GitHub returned HTTP {response.status}")
            body = response.read(MAX_RESPONSE_BYTES + 1)
        if len(body) > MAX_RESPONSE_BYTES:
            raise UpdateError("GitHub's release response is too large")
        return release_tag(json.loads(body.decode("utf-8")))
    except (error.URLError, UnicodeError, json.JSONDecodeError) as exc:
        raise UpdateError(f"Cannot read the latest FlacCompagnon release: {exc}") from exc


def pin_manifest(text, tag):
    """Change only the shared FlacCompagnon tag, preserving other dependencies."""
    if not STABLE_TAG.fullmatch(tag):
        raise UpdateError("Invalid stable version tag")
    section = re.search(r"(?ms)^\[workspace\.dependencies\]\n(.*?)(?=^\[|\Z)", text)
    if section is None:
        raise UpdateError("Cargo.toml has no workspace dependency section")
    dependency = re.compile(
        r'(?m)^(flaccompagnon-core\s*=\s*\{\s*git\s*=\s*"'
        + re.escape(REPOSITORY)
        + r'"\s*,\s*tag\s*=\s*")([^"\n]+)("\s*\}[^\n]*)$'
    )
    body, count = dependency.subn(lambda match: match[1] + tag + match[3], section[1])
    if count != 1:
        raise UpdateError("Expected one shared FlacCompagnon Git/tag dependency in Cargo.toml")
    return text[:section.start(1)] + body + text[section.end(1):]


def update_dependency(root, tag):
    """Update the selected crate; restore the existing pin and lock on failure."""
    manifest, lock = root / "Cargo.toml", root / "Cargo.lock"
    original_manifest = manifest.read_bytes()
    original_lock = lock.read_bytes() if lock.exists() else None
    updated = pin_manifest(original_manifest.decode("utf-8"), tag).encode("utf-8")
    source = f'source = "git+{REPOSITORY}?tag={tag}#'.encode("utf-8")
    if updated == original_manifest and original_lock is not None and source in original_lock:
        return
    try:
        if updated != original_manifest:
            manifest.write_bytes(updated)
        subprocess.run(["cargo", "update", "-p", "flaccompagnon-core"], cwd=root, check=True)
    except (OSError, subprocess.CalledProcessError):
        # Cargo can change the lock before failing. Preserve even an uncommitted pin.
        manifest.write_bytes(original_manifest)
        if original_lock is None:
            lock.unlink(missing_ok=True)
        else:
            lock.write_bytes(original_lock)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        tag = latest_tag()
        print(f"FlacCompagnon Core: latest published stable release is {tag}", flush=True)
        update_dependency(root, tag)
        print(f"Pinned {tag}; Cargo.lock records the exact source revision.")
        return 0
    except (UpdateError, OSError, subprocess.CalledProcessError) as exc:
        print(f"FlacCompagnon update failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
