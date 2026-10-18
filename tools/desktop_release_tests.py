#!/usr/bin/env python3
"""Release helpers refuse drift and ambiguous outputs without installing anything."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), Path(__file__).with_name(name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


release = module("check-release")
packaging = module("package-desktop")


class Releases(unittest.TestCase):
    def fixture(self, root):
        for folder in ["desktop/src-tauri", "docs/releases"]:
            (root / folder).mkdir(parents=True, exist_ok=True)
        for path in ["Cargo.toml", "desktop/Cargo.toml"]:
            (root / path).write_text('[workspace]\nmembers = []\n[workspace.package]\nversion = "1.2.3"\n')
        for path in ["desktop/package.json", "desktop/package-lock.json", "desktop/src-tauri/tauri.conf.json"]:
            (root / path).write_text(json.dumps({"version": "1.2.3"}))
        (root / "docs/releases/v1.2.3.md").write_text("Release notes")

    def test_versions_and_tag_must_agree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            self.assertEqual(release.check(root, "v1.2.3"), "1.2.3")
            with self.assertRaises(ValueError):
                release.check(root, "v1.2.4")
            (root / "desktop/package.json").write_text('{"version":"1.2.4"}')
            with self.assertRaises(ValueError):
                release.check(root)

    def test_notes_are_required_before_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            (root / "docs/releases/v1.2.3.md").unlink()
            with self.assertRaises(ValueError):
                release.check(root)

    def test_app_packages_are_distinct_and_checksummed(self):
        import hashlib
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            bundle = root / "desktop/target/release/bundle/dmg"
            bundle.mkdir(parents=True)
            (bundle / "application.dmg").write_bytes(b"native test artifact")
            self.assertEqual(packaging.package(root, "macOS-AppleSilicon"), ["aede-App-1.2.3-macOS-AppleSilicon.dmg"])
            digest = hashlib.sha256(b"native test artifact").hexdigest()
            self.assertEqual((root / "dist-desktop/aede-App-1.2.3-macOS-AppleSilicon.dmg.sha256").read_text(), f"{digest}  aede-App-1.2.3-macOS-AppleSilicon.dmg\n")
            (bundle / "duplicate.dmg").write_bytes(b"ambiguous")
            with self.assertRaises(ValueError):
                packaging.package(root, "macOS-AppleSilicon")

    def test_unknown_platform_and_missing_installers_are_errors(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            for label in ["untrusted", "Windows-x64"]:
                with self.assertRaises(ValueError):
                    packaging.package(root, label)


if __name__ == "__main__":
    unittest.main()
