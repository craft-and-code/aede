#!/usr/bin/env python3
"""Independent synthetic failures for the real-output marker measurement."""
from __future__ import annotations

from array import array
from pathlib import Path
import tempfile
import unittest
import json
import math
import subprocess
import os
from unittest.mock import patch

import gapless


class GaplessMeasurementTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="aede-gapless-test-")
        self.root = Path(self.temp.name) / "probe"
        self.manifest = gapless.prepare(self.root, rate=8000, seconds=2)
        self.rate, self.source = gapless.read_wav(self.root / "whole.wav")

    def tearDown(self):
        self.temp.cleanup()

    def capture(self, samples, name="capture.wav"):
        path = self.root / name
        gapless.write_wav(path, samples, self.rate, channels=1)
        return path

    def analyze(self, capture):
        return gapless.analyze(self.root / "manifest.json", capture, "synthetic", window_ms=40)

    def test_split_files_are_exactly_the_whole_signal_without_boundary_padding(self):
        concatenated = array("f")
        for index in range(3):
            rate, samples = gapless.read_wav(self.root / f"{index + 1:02}.wav")
            self.assertEqual(rate, self.rate)
            concatenated.extend(samples)
        self.assertEqual(concatenated, self.source)
        self.assertTrue(all(split % 1024 for split in self.manifest["splits"]))

    def test_constant_latency_and_gain_do_not_create_a_join_gap(self):
        capture = self.capture(array("f", [0.0] * 143) + array("f", (value * .61 for value in self.source)))
        result = self.analyze(capture)
        self.assertTrue(result["all_joins_within_tolerance"])
        self.assertFalse(result["hardware_acceptance"])
        self.assertEqual(result["capture_kind"], "synthetic")
        for join in result["joins"]:
            self.assertAlmostEqual(join["delay_step_frames"], 0, delta=1)

    def test_an_inserted_gap_at_one_join_is_not_hidden_by_global_alignment(self):
        split = self.manifest["splits"][0]
        capture = self.capture(self.source[:split] + array("f", [0.0] * 80) + self.source[split:])
        result = self.analyze(capture)
        self.assertFalse(result["all_joins_within_tolerance"])
        self.assertAlmostEqual(result["joins"][0]["delay_step_ms"], 10, delta=.15)
        self.assertAlmostEqual(result["joins"][1]["delay_step_ms"], 0, delta=.15)

    def test_dropped_audio_is_reported_as_a_negative_delay_step(self):
        split = self.manifest["splits"][1]
        capture = self.capture(self.source[:split] + self.source[split + 64:])
        result = self.analyze(capture)
        self.assertFalse(result["all_joins_within_tolerance"])
        self.assertAlmostEqual(result["joins"][1]["delay_step_ms"], -8, delta=.15)

    def test_uncertainty_is_reserved_inside_the_limit_instead_of_expanding_it(self):
        split = self.manifest["splits"][0]
        capture = self.capture(self.source[:split] + array("f", [0.0] * 4) + self.source[split:])
        result = self.analyze(capture)
        self.assertAlmostEqual(result["joins"][0]["delay_step_ms"], .5, delta=.05)
        self.assertFalse(result["joins"][0]["within_tolerance"])

    def test_a_small_independent_clock_error_does_not_look_like_a_gap(self):
        # Independent interpolation oracle changes only capture clock by 300 ppm.
        ratio = 1.0003
        resampled = array("f")
        for frame in range(int(len(self.source) * ratio)):
            position = frame / ratio
            left = min(len(self.source) - 1, int(position))
            right = min(len(self.source) - 1, left + 1)
            resampled.append(self.source[left] + (self.source[right] - self.source[left]) * (position - left))
        result = self.analyze(self.capture(resampled))
        self.assertTrue(result["all_joins_within_tolerance"])
        self.assertAlmostEqual(result["clock_drift_ppm"], 300, delta=65)
        for join in result["joins"]:
            self.assertAlmostEqual(join["delay_step_ms"], 0, delta=.2)

    def test_missing_final_markers_refuse_a_successful_verdict(self):
        capture = self.capture(self.source[:self.manifest["splits"][1] + self.rate // 2])
        with self.assertRaisesRegex(ValueError, "marker|ends"):
            self.analyze(capture)

    def test_an_unrelated_capture_refuses_a_successful_verdict(self):
        capture = self.capture(array("f", [0.0]) * len(self.source))
        with self.assertRaisesRegex(ValueError, "correlation"):
            self.analyze(capture)

    def test_modified_reference_is_refused(self):
        capture = self.capture(self.source)
        gapless.write_wav(self.root / "whole.wav", [0.0] * len(self.source), self.rate, channels=1)
        with self.assertRaisesRegex(ValueError, "reference changed"):
            self.analyze(capture)

    def test_timing_noise_cannot_inflate_the_tolerance_into_a_success(self):
        capture = array("f", self.source)
        shifts = [0, 6, 1, 9, 0]
        for marker in self.manifest["markers"]:
            frame = marker["frame"]
            count = round(gapless.MARKER_SECONDS * self.rate)
            original = self.source[frame:frame + count]
            for index in range(frame, frame + count):
                capture[index] = .015 * math.sin(2 * math.pi * 437 * index / self.rate)
            target = frame + shifts[marker["index"] % len(shifts)]
            capture[target:target + count] = original
        with self.assertRaisesRegex(ValueError, "uncertainty.*inconclusive"):
            self.analyze(self.capture(capture))

    def test_malformed_manifest_shapes_refuse_a_measurement(self):
        capture = self.capture(self.source)
        for key, value in [("markers", None), ("splits", 1), ("lengths", [0]),
                           ("markers", [{"index": 0, "frame": 1, "track": 0}] * 15)]:
            manifest = dict(self.manifest)
            manifest[key] = value
            (self.root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                self.analyze(capture)

    def test_right_channel_uses_its_own_polarity_reference(self):
        result = gapless.analyze(self.root / "manifest.json", self.root / "whole.wav", "synthetic",
                                channel=1, reference_channel=1, window_ms=40)
        self.assertTrue(result["all_joins_within_tolerance"])
        self.assertEqual(result["reference_channel"], 1)

    def test_loaded_runner_stops_its_workers_and_keeps_separate_run_records(self):
        binary = self.root / "fake-aede"
        binary.write_bytes(b"deliberately not an audio executable")

        def fake_playback(command, **options):
            self.assertEqual(Path(command[command.index("--data") + 1]).parent, self.root.resolve())
            self.assertEqual(options["env"]["AEDE_AUDIO_BACKEND"], "native")
            self.assertFalse(any(key.startswith("AEDE_DELEGATED_") for key in options["env"]))
            self.assertEqual(options["stdin"], subprocess.DEVNULL)
            options["stdout"].write(b"Native output: 16000 consumed frames; queue 0/4000 frames; "
                                    b"0 missing frames in 0 callbacks; 0 host xruns\n")
            return subprocess.CompletedProcess(command, 0)

        with patch.dict(os.environ, {"AEDE_DELEGATED_CHILD": "1", "AEDE_DELEGATED_DATA_DIR": "/private-music"}), \
                patch("gapless.subprocess.run", side_effect=fake_playback):
            result = gapless.run(self.root, binary, workers=1, io=True, timeout=5)
        self.assertEqual(result["returncode"], 0)
        self.assertEqual(result["load_worker_exitcodes"], [0, 0])
        self.assertEqual(result["native_diagnostics"][0]["consumed_frames"], 16000)
        self.assertTrue(result["diagnostics_clean"])
        self.assertTrue((self.root / "split-load.run.json").is_file())
        with self.assertRaisesRegex(ValueError, "already exists"):
            gapless.run(self.root, binary, workers=1)


if __name__ == "__main__":
    unittest.main()
