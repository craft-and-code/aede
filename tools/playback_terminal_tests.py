#!/usr/bin/env python3
"""Verify queued DoP controls with real terminals and an unavailable endpoint."""

import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

if os.name == "posix":
    import pty
    import termios

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name == "posix", "Unix pseudo-terminal regression")
class PlaybackTerminalTests(unittest.TestCase):
    def queued(self, keys, playlist=False):
        with tempfile.TemporaryDirectory(prefix="dop-terminal-") as folder:
            root = Path(folder)
            source = ROOT / "crates/aede-audio/tests/fixtures/tone.dsf"
            selection = source
            if playlist:
                selection = root / "selection.m3u"
                selection.write_text(f"{source}\n{source}\n")
            device = (
                "coreaudio:00000000-0000-0000-0000-000000000000-review-missing"
                if sys.platform == "darwin"
                else "alsa:hw:999999,0"
            )
            master, slave = pty.openpty()
            self.addCleanup(os.close, master)
            self.addCleanup(os.close, slave)
            original = termios.tcgetattr(slave)
            os.write(master, keys)
            with subprocess.Popen(
                [str(BINARY), "play", str(selection), "--dsd-output=dop",
                 f"--output-device={device}", "--data", str(root / "data")],
                stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                env={**os.environ, "AEDE_AUDIO_BACKEND": "native", "NO_COLOR": "1", "TERM": "xterm"},
            ) as child:
                try:
                    stdout, stderr = child.communicate(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.communicate()
                    self.fail("queued transport did not stop before device admission")
            if b"DoP output needs CoreAudio, WASAPI or the dop-alsa build feature" in stderr:
                self.skipTest("this CLI profile has no native DoP backend")
            self.assertEqual(child.returncode, 0, (stdout, stderr))
            self.assertEqual(stderr, b"")
            self.assertNotIn(b"Playing:", stdout)
            restored = termios.tcgetattr(slave)
            # macOS PENDIN records transient reprocessing of queued input.
            restored[3] &= ~getattr(termios, "PENDIN", 0)
            original[3] &= ~getattr(termios, "PENDIN", 0)
            self.assertEqual(restored, original)
            self.assertFalse((root / "data/user.json").exists())

    def test_queued_stop_never_opens_the_dop_device(self):
        for key in [b"q", b"s"]:
            with self.subTest(key=key):
                self.queued(key)

    def test_queued_pause_then_stop_never_opens_the_dop_device(self):
        self.queued(b" q")

    def test_queued_next_then_stop_does_not_open_either_visit(self):
        self.queued(b"nq", playlist=True)

    def test_queued_next_past_selection_end_does_not_reopen_the_last_track(self):
        self.queued(b"n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args, remaining = parser.parse_known_args()
    BINARY = args.binary.resolve()
    unittest.main(argv=[sys.argv[0], *remaining])
