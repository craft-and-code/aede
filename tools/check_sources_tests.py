"""Exercise the real source gate with inert command recorders, never App builds."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

EXCLUDED_APP = None

class SourceGateTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="source-gate-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "tools").mkdir()
        for name in ["check.sh", "check-sources.sh"]:
            shutil.copy2(Path(__file__).parent / name, self.root / "tools" / name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "commands.jsonl"
        (self.root / "target/doc").mkdir(parents=True)
        self.guard = self.root / "src-tauri/product-artifact"
        self.guard.parent.mkdir()
        self.guard.write_bytes(b"existing product artifact")
        recorder = "#!" + sys.executable + "\n" + r"""
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
with open(os.environ["SOURCE_GATE_LOG"], "a") as output:
    output.write(json.dumps([name, *sys.argv[1:]]) + "\n")
if os.environ.get("SOURCE_GATE_FAIL") and name == "cargo" and sys.argv[1] == "test":
    sys.exit(42)
"""
        for name in ["cargo", "npm", "npx", "node", "python3", "ffmpeg"]:
            path = self.bin / name
            path.write_text(recorder)
            path.chmod(0o755)
        self.env = {**os.environ, "PATH": str(self.bin) + ":/usr/bin:/bin", "SOURCE_GATE_LOG": str(self.log), "CARGO_TARGET_DIR": str(self.root / "target")}

    def run_gate(self, *args):
        return subprocess.run(["/bin/bash", str(self.root / "tools/check.sh"), *args], env=self.env, capture_output=True, text=True)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def test_source_mode_runs_checks_without_product_builds_or_preparation(self):
        result = self.run_gate("--sources-only")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        for verb in ["test", "clippy", "doc"]:
            self.assertTrue(any(row[:2] == ["cargo", verb] for row in calls), verb)
        for row in calls:
            self.assertNotIn(row[:2], [["cargo", "build"], ["cargo", "run"], ["cargo", "install"]])
            self.assertFalse(row[0] == "npm" and any("build" in item or item == "components:check" for item in row[1:]), row)
            self.assertFalse(any(item.endswith(("prepare-engine.mjs", "audio-source.mjs", "tauri.mjs")) for item in row[1:]), row)
            if EXCLUDED_APP and row[:2] in [["cargo", "test"], ["cargo", "clippy"], ["cargo", "doc"]] and "--workspace" in row:
                self.assertIn("--exclude", row)
                self.assertEqual(row[row.index("--exclude") + 1], EXCLUDED_APP)
        self.assertEqual(self.guard.read_bytes(), b"existing product artifact")

    def test_failed_library_check_cannot_report_a_successful_validation(self):
        self.env["SOURCE_GATE_FAIL"] = "1"
        result = self.run_gate("--sources-only")
        self.assertEqual(result.returncode, 42, result.stdout + result.stderr)
        self.assertNotIn("All source checks passed", result.stdout)
        self.assertFalse(any(row[:2] == ["cargo", "doc"] for row in self.calls()))

    def test_source_mode_rejects_combined_or_unknown_options_before_commands(self):
        for args in [("--sources-only", "--web"), ("--unknown",)]:
            result = self.run_gate(*args)
            self.assertEqual(result.returncode, 2)
        self.assertEqual(self.calls(), [])

if __name__ == "__main__":
    unittest.main()
