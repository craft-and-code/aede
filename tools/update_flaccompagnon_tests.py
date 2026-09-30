"""Offline tests for published-release selection and dependency rollback."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch
from urllib import error

spec = importlib.util.spec_from_file_location(
    "update_flaccompagnon", Path(__file__).with_name("update-flaccompagnon.py")
)
update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update)

MANIFEST = '''[workspace.dependencies]
lofty = "0.25"
flaccompagnon-core = { git = "https://github.com/craft-and-code/FlacCompagnon", tag = "v0.9.5" }

[profile.release]
opt-level = 3
'''


def release(**overrides):
    return dict(tag_name="v0.9.6", draft=False, prerelease=False,
                published_at="2026-09-30T10:00:00Z", **overrides)


class ReleaseTests(unittest.TestCase):
    def test_only_published_stable_releases_are_accepted(self):
        self.assertEqual(update.release_tag(release()), "v0.9.6")
        for payload in [None, {}, {**release(), "draft": True},
                        {**release(), "prerelease": True},
                        {**release(), "published_at": None}]:
            with self.subTest(payload=payload), self.assertRaises(update.UpdateError):
                update.release_tag(payload)
        for tag in ["main", "v0.9.6-rc.1", "v0.9.6\n", 'v0.9.6"', "v01.2.3"]:
            with self.subTest(tag=tag), self.assertRaises(update.UpdateError):
                update.release_tag({**release(), "tag_name": tag})

    def test_lookup_uses_latest_release_endpoint_and_has_a_timeout(self):
        response = Mock(status=200)
        response.read.return_value = json.dumps(release()).encode()
        context = Mock()
        context.__enter__ = Mock(return_value=response)
        context.__exit__ = Mock(return_value=False)
        with patch.object(update.request, "urlopen", return_value=context) as fetch:
            self.assertEqual(update.latest_tag(), "v0.9.6")
        self.assertTrue(fetch.call_args.args[0].full_url.endswith("/releases/latest"))
        self.assertEqual(fetch.call_args.kwargs["timeout"], 30)

    def test_network_failure_stops_instead_of_silently_using_an_old_release(self):
        with patch.object(update.request, "urlopen", side_effect=error.URLError("offline")):
            with self.assertRaises(update.UpdateError):
                update.latest_tag()

    def test_malformed_response_is_rejected(self):
        response = Mock(status=200)
        response.read.return_value = b"not JSON"
        context = Mock()
        context.__enter__ = Mock(return_value=response)
        context.__exit__ = Mock(return_value=False)
        with patch.object(update.request, "urlopen", return_value=context):
            with self.assertRaises(update.UpdateError):
                update.latest_tag()


class PinTests(unittest.TestCase):
    def test_shared_pin_changes_without_changing_other_dependencies(self):
        self.assertEqual(update.pin_manifest(MANIFEST, "v0.9.6"),
                         MANIFEST.replace('tag = "v0.9.5"', 'tag = "v0.9.6"'))
        self.assertEqual(update.pin_manifest(MANIFEST, "v0.9.5"), MANIFEST)

    def test_unexpected_manifest_does_not_get_rewritten(self):
        for text in ["", MANIFEST.replace("flaccompagnon-core", "other-core"),
                     MANIFEST.replace(update.REPOSITORY, "https://example.invalid/repo")]:
            with self.subTest(text=text), self.assertRaises(update.UpdateError):
                update.pin_manifest(text, "v0.9.6")

    def test_only_core_is_updated_and_cargo_records_the_selected_revision(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "Cargo.toml").write_text(MANIFEST)
            with patch.object(update.subprocess, "run") as run:
                update.update_dependency(root, "v0.9.6")
            run.assert_called_once_with(["cargo", "update", "-p", "flaccompagnon-core"],
                                        cwd=root, check=True)
            self.assertIn('tag = "v0.9.6"', (root / "Cargo.toml").read_text())

    def test_already_locked_release_preserves_revision_and_other_dependencies(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "Cargo.toml").write_text(MANIFEST)
            lock = ('[[package]]\nname = "flaccompagnon-core"\n'
                    f'source = "git+{update.REPOSITORY}?tag=v0.9.5#' + "a" * 40 + '"\n').encode()
            (root / "Cargo.lock").write_bytes(lock)
            with patch.object(update.subprocess, "run") as run:
                update.update_dependency(root, "v0.9.5")
            run.assert_not_called()
            self.assertEqual((root / "Cargo.lock").read_bytes(), lock)
            self.assertEqual((root / "Cargo.toml").read_text(), MANIFEST)

    def test_failed_update_restores_uncommitted_manifest_and_lock(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            manifest = MANIFEST + "# local edit\n"
            (root / "Cargo.toml").write_text(manifest)
            (root / "Cargo.lock").write_bytes(b"original local lock")
            def fail(*args, **kwargs):
                (root / "Cargo.lock").write_bytes(b"partially updated lock")
                raise subprocess.CalledProcessError(1, args[0])
            with patch.object(update.subprocess, "run", side_effect=fail):
                with self.assertRaises(subprocess.CalledProcessError):
                    update.update_dependency(root, "v0.9.6")
            self.assertEqual((root / "Cargo.toml").read_text(), manifest)
            self.assertEqual((root / "Cargo.lock").read_bytes(), b"original local lock")

    def test_failed_first_update_removes_only_its_generated_lock(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "Cargo.toml").write_text(MANIFEST)
            def fail(*args, **kwargs):
                (root / "Cargo.lock").write_bytes(b"generated lock")
                raise subprocess.CalledProcessError(1, args[0])
            with patch.object(update.subprocess, "run", side_effect=fail):
                with self.assertRaises(subprocess.CalledProcessError):
                    update.update_dependency(root, "v0.9.6")
            self.assertFalse((root / "Cargo.lock").exists())
            self.assertEqual((root / "Cargo.toml").read_text(), MANIFEST)


if __name__ == "__main__":
    unittest.main()
