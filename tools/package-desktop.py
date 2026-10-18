#!/usr/bin/env python3
"""Collect native Tauri installers using separate App names and SHA-256 sidecars."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parent.parent

def package(root, label):
    if label not in {"macOS-AppleSilicon", "Linux-x86_64", "Windows-x64"}:
        raise ValueError("Unknown release platform")
    version = json.loads((root / "desktop/package.json").read_text())["version"]
    bundle = root / "desktop/target/release/bundle"
    output = root / "dist-desktop"
    output.mkdir(exist_ok=True)
    found = []
    for extension in (".dmg", ".AppImage", ".deb", ".rpm", ".msi", ".exe"):
        matches = sorted(path for path in bundle.rglob(f"*{extension}") if path.is_file())
        if len(matches) > 1:
            raise ValueError(f"Ambiguous desktop package for {extension}")
        for path in matches:
            name = f"aede-App-{version}-{label}{extension}"
            destination = output / name
            shutil.copy2(path, destination)
            digest = hashlib.sha256()
            with destination.open("rb") as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(block)
            checksum = digest.hexdigest()
            (output / f"{name}.sha256").write_text(f"{checksum}  {name}\n")
            found.append(name)
    if not found:
        raise ValueError("No native desktop installer was produced")
    return found

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--label", required=True)
    arguments = parser.parse_args()
    for name in package(ROOT, arguments.label):
        print(name)
