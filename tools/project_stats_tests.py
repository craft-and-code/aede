#!/usr/bin/env python3
"""Behaviour checks for source metrics and compiled Rust unit-test inventories."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parent / "project-stats.py"
SPEC = importlib.util.spec_from_file_location("aede_project_stats", SCRIPT)
stats = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stats)


class ProjectFixture:
    def __init__(self, root):
        self.root = Path(root)
        self.write("Cargo.toml", '[workspace]\nmembers = ["crates/core", "crates/client"]\n')
        self.write("Cargo.lock", "# Dependency identities\n")
        self.write("crates/core/Cargo.toml", '[package]\nname = "aede-core"\nversion = "0.1.0"\n')
        self.write("crates/client/Cargo.toml", '[package]\nname = "aede-client"\nversion = "0.1.0"\n')

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def artifact(self, crate, source, kind, target, test=True):
        return {
            "reason": "compiler-artifact", "profile": {"test": test},
            "target": {"kind": [kind], "src_path": str(self.root / f"crates/{crate}/{source}"), "name": target},
            "executable": str(self.root / f"target/{target}"),
        }


class SourceMetricTests(unittest.TestCase):
    def test_authored_sources_are_counted_by_crate_and_category_without_generated_files(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = ProjectFixture(directory)
            fixture.write("crates/core/src/lib.rs", '// Explanation\n\n#[cfg(test)]\n#[path = "signals.rs"]\nmod signals;\npub fn run() {}\n')
            fixture.write("crates/core/src/signals.rs", "// Test-only mathematical signal\npub fn signal() {}\n")
            fixture.write("crates/core/src/lib_tests.rs", "#[test]\nfn works() {}\n")
            fixture.write("crates/core/tests/integration.rs", "// Integration source\n")
            fixture.write("crates/core/examples/demo.rs", "fn main() {}")
            fixture.write("crates/core/schema.sql", "CREATE TABLE item (id TEXT);\n")
            fixture.write("crates/client/src/main.rs", "fn main() {}\r\n\r\n")
            fixture.write("tools/check.sh", "#!/bin/sh\nexit 0\n")
            fixture.write("tools/check_tests.py", "def test_contract():\n    pass\n")
            fixture.write("site/app.js", "export const version = 1;\n")
            for name in ("target/generated.rs", "crates/core/target/generated.rs", "tools/vendor/imported.py", "site/dist/app.js", "site/node_modules/package/index.js", "site/.cache/app.js", "crates/core/generated/table.rs"):
                fixture.write(name, "Unrelated generated source\n" * 100)
            fixture.write("crates/core/tests/fixtures/data.json", '{"not": "code"}\n')
            fixture.write("docs/manual.md", "# Manual\n")
            fixture.write("docs/fr/manual.md", "# Manuel\n")
            fixture.write("docs/library.md", "# Library topic without a manifest entry\n")
            fixture.write("docs/fr/library.md", "# Bibliothèque\n")
            fixture.write("docs/site-manual.json", json.dumps([{"slug": "manual/start", "section": "manual", "source": {"en": "docs/manual.md", "fr": "docs/fr/manual.md"}}]))
            with patch.object(stats, "git_revision", return_value=None):
                report = stats.collect(fixture.root)
            self.assertIsNone(report["unit_tests"])
            self.assertEqual({"files": 10, "physical_lines": 20, "nonblank_lines": 18}, report["source"]["total"])
            rows = {row["name"]: row for row in report["source"]["crates"]}
            self.assertEqual(7, rows["aede-core"]["production"]["physical_lines"])
            self.assertEqual(5, rows["aede-core"]["tests"]["physical_lines"])
            self.assertEqual(1, rows["aede-core"]["examples"]["physical_lines"])
            self.assertEqual(2, rows["aede-client"]["production"]["physical_lines"])
            self.assertEqual({"markdown_files": 4, "published_pages_by_language": {"en": 2, "fr": 2}}, report["source"]["documentation"])

    def test_linked_sources_and_directories_are_not_counted(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as outside:
            fixture = ProjectFixture(directory)
            fixture.write("crates/core/src/lib.rs", "pub fn run() {}\n")
            fixture.write("site/app.js", "export const version = 1;\n")
            external = Path(outside) / "outside.rs"
            external.write_text("External source\n" * 100, encoding="utf-8")
            try:
                (fixture.root / "crates/core/src/link.rs").symlink_to(external)
                (fixture.root / "site/external").symlink_to(outside, target_is_directory=True)
            except (OSError, NotImplementedError) as error:
                self.skipTest(f"This environment cannot create symlinks: {error}")
            report = stats.collect(fixture.root)
            self.assertEqual(2, report["source"]["total"]["files"])
            self.assertEqual(2, report["source"]["total"]["physical_lines"])

    def test_unreadable_or_binary_named_source_is_reported_instead_of_silently_omitted(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = ProjectFixture(directory)
            path = fixture.write("tools/broken.py", "normal source")
            path.write_bytes(b"binary\0source")
            with self.assertRaisesRegex(stats.StatsError, "NUL"):
                stats.collect(fixture.root)
            path.write_bytes(b"\xff")
            with self.assertRaisesRegex(stats.StatsError, "UTF-8"):
                stats.collect(fixture.root)
            path.unlink()
            rust = fixture.write("crates/core/src/lib.rs", "source")
            rust.write_bytes(b"\xff")
            with self.assertRaisesRegex(stats.StatsError, "UTF-8"):
                stats.collect(fixture.root)


class UnitInventoryTests(unittest.TestCase):
    def collected_inventory(self, fixture, *, unexpected_ignored=False):
        # This source deliberately contains more apparent #[test] functions than
        # the built harness: registered tests, not regex matches, define active TUs.
        fixture.write("crates/core/src/lib.rs", "#[cfg(never)]\nmod orphan { #[test] fn dead() {} }\n")
        fixture.write("crates/client/src/main.rs", "fn main() {}\n")
        artifacts = [
            fixture.artifact("core", "src/lib.rs", "lib", "core-unit"),
            fixture.artifact("client", "src/main.rs", "bin", "client-unit"),
            fixture.artifact("core", "tests/integration.rs", "test", "integration"),
            fixture.artifact("core", "examples/demo.rs", "example", "example"),
            fixture.artifact("core", "src/lib.rs", "lib", "production", test=False),
            {"reason": "build-finished", "success": True},
        ]
        calls = []

        def execute(command, root, **options):
            calls.append(command)
            if command[0] == "cargo" and command[1] == "test":
                return "\n".join(json.dumps(artifact) for artifact in artifacts)
            if command[0] == "rustc":
                return "rustc 1.89.0\nhost: x86_64-unknown-linux-gnu\n"
            if command == ["cargo", "--version"]:
                return "cargo 1.89.0\n"
            if Path(command[0]).name == "core-unit":
                # Rust 1.89 omits the summary in terse listing mode. Real
                # output from the project's DSP harness established this.
                if "terse" in command:
                    return "" if "--ignored" in command else "first: test\nsecond: test\nslow: test\nmeasurement: benchmark\n"
                if "--ignored" in command:
                    name = "unregistered" if unexpected_ignored else "slow"
                    return f"{name}: test\n\n1 test, 0 benchmarks\n"
                return "first: test\nsecond: test\nslow: test\nmeasurement: benchmark\n\n3 tests, 1 benchmark\n"
            if Path(command[0]).name == "client-unit":
                if "terse" in command:
                    return "" if "--ignored" in command else "first: test\nsecond: test\n"
                return "\n0 tests, 0 benchmarks\n" if "--ignored" in command else "first: test\nsecond: test\n\n2 tests, 0 benchmarks\n"
            self.fail(f"An integration/example or non-test command was invoked: {command}")

        with patch.object(stats, "run", side_effect=execute):
            inventory = stats.inventory(fixture.root)
        return inventory, calls

    def test_only_registered_nonignored_library_and_binary_tests_are_active_and_bodies_never_run(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = ProjectFixture(directory)
            inventory, calls = self.collected_inventory(fixture)
            self.assertEqual((5, 1, 4), (inventory["total"], inventory["ignored"], inventory["active"]))
            rows = {row["name"]: row for row in inventory["by_crate"]}
            self.assertEqual(2, rows["aede-core"]["active"])
            self.assertEqual(2, rows["aede-client"]["active"])
            self.assertFalse(inventory["provenance"]["executed_tests"])
            self.assertIn("--no-run", calls[0])
            self.assertIn("--offline", calls[0])
            for command in calls:
                if command[0] not in {"cargo", "rustc"}:
                    self.assertIn("--list", command)

    def test_ignored_inventory_must_be_a_subset_of_registered_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = ProjectFixture(directory)
            with self.assertRaisesRegex(stats.StatsError, "Ignored test names"):
                self.collected_inventory(fixture, unexpected_ignored=True)

    def test_cached_inventory_survives_documentation_changes_but_refuses_changed_rust_or_forged_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = ProjectFixture(directory)
            inventory, _ = self.collected_inventory(fixture)
            report_path = fixture.write("stats.json", json.dumps({"schema_version": 1, "unit_tests": inventory}))
            fixture.write("docs/manual.md", "# New documentation\n")
            self.assertEqual(4, stats.load_test_inventory(report_path, fixture.root)["active"])
            inventory["active"] += 1
            report_path.write_text(json.dumps({"schema_version": 1, "unit_tests": inventory}), encoding="utf-8")
            with self.assertRaisesRegex(stats.StatsError, "totals disagree"):
                stats.load_test_inventory(report_path, fixture.root)
            inventory["active"] -= 1
            report_path.write_text(json.dumps({"schema_version": 1, "unit_tests": inventory}), encoding="utf-8")
            fixture.write("crates/core/src/lib.rs", "pub fn changed() {}\n")
            with self.assertRaisesRegex(stats.StatsError, "stale"):
                stats.load_test_inventory(report_path, fixture.root)

    def test_truncated_or_duplicate_harness_lists_are_refused(self):
        for output in ("first: test\n", "first: test\n\n2 tests, 0 benchmarks\n", "same: test\nsame: test\n\n2 tests, 0 benchmarks\n"):
            with self.subTest(output=output), self.assertRaises(stats.StatsError):
                stats.listed_tests(output)

    def test_cached_inventory_requires_configuration_and_a_dated_provenance_for_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = ProjectFixture(directory)
            inventory, _ = self.collected_inventory(fixture)
            report_path = fixture.root / "stats.json"
            for missing in ("configuration", "platform", "collected_at"):
                copy = json.loads(json.dumps(inventory))
                if missing == "configuration":
                    del copy[missing]
                elif missing == "platform":
                    del copy["configuration"][missing]
                else:
                    del copy["provenance"][missing]
                report_path.write_text(json.dumps({"schema_version": 1, "unit_tests": copy}), encoding="utf-8")
                with self.subTest(missing=missing), self.assertRaises(stats.StatsError):
                    stats.load_test_inventory(report_path, fixture.root)

    def test_public_count_is_a_strict_rounded_lower_bound_even_at_exact_multiples(self):
        for active, bound in ((0, None), (1, 0), (10, 9), (99, 90), (100, 90), (1500, 1400), (1501, 1500)):
            with self.subTest(active=active):
                self.assertEqual(bound, stats.lower_bound(active))
                if bound is not None:
                    self.assertLess(bound, active)
        self.assertEqual("+ de 1 500 TU actifs", stats.public_labels(1501)["fr"])
        self.assertEqual("> 1,400 active unit tests", stats.public_labels(1500)["en"])
        for invalid in (-1, True, 1.5):
            with self.subTest(invalid=invalid), self.assertRaises(stats.StatsError):
                stats.lower_bound(invalid)

    def test_feature_options_require_an_actual_cargo_inventory(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "--no-default-features"], capture_output=True, text=True)
        self.assertEqual(2, result.returncode)
        self.assertIn("Cargo feature options require --tests", result.stderr)


if __name__ == "__main__":
    unittest.main()
