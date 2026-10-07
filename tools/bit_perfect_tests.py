#!/usr/bin/env python3
"""Independent capture mutations must never pass an integer identity verdict."""
from array import array
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import bit_perfect as bp


class IntegerCaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="aede-integer-capture-test-")
        self.root = Path(self.temp.name) / "probe"
        self.manifest = bp.prepare(self.root, rate=44100, bits=24, channels=2, seconds=1)
        self.source = bp.read_pcm(self.root / "whole.wav")
        self.path = self.root / "manifest.json"

    def tearDown(self):
        self.temp.cleanup()

    def capture(self, samples, bits=32, name="capture.wav", rate=None, channels=None):
        path = self.root / name
        bp.write_pcm(path, samples, rate or self.source.rate, bits, channels or self.source.channels)
        return path

    def compare(self, path, **kwargs):
        return bp.compare(self.path, path, "synthetic", **kwargs)

    def test_probe_parts_concatenate_to_the_whole_reference_with_real_low_bits(self):
        joined = array("i")
        for index in range(3):
            joined.extend(bp.read_pcm(self.root / f"{index + 1:02}.wav").samples)
        self.assertEqual(joined, self.source.samples)
        self.assertTrue(any((sample >> 8) & 255 for sample in joined))
        self.assertNotEqual(joined[::2], joined[1::2])
        self.assertTrue(all(split % 1024 for split in self.manifest["splits"]))

    def test_one_constant_latency_and_exact_widening_pass_without_hardware_claim(self):
        padding = array("i", [0] * 274)
        path = self.capture(padding + self.source.samples + padding)
        result = self.compare(path)
        self.assertTrue(result["comparison_passed"])
        self.assertEqual(result["offset_frames"], 137)
        self.assertEqual(result["capture_container_bits"], 32)
        self.assertEqual(result["reference_pcm_s32le_sha256"], result["compared_capture_pcm_s32le_sha256"])
        self.assertFalse(result["hardware_acceptance"])

    def test_16_bit_probe_does_not_repeat_its_alignment_prefix_within_a_normal_capture(self):
        root = self.root.parent / "probe16"
        bp.prepare(root, rate=44100, bits=16, channels=2, seconds=1)
        result = bp.compare(root / "manifest.json", root / "whole.wav", "synthetic")
        self.assertTrue(result["comparison_passed"])

    def test_a_single_changed_low_bit_is_found_even_in_a_wider_capture(self):
        samples = array("i", self.source.samples)
        index = (self.manifest["splits"][0] + 7) * 2 + 1
        samples[index] += 1
        result = self.compare(self.capture(samples))
        self.assertFalse(result["comparison_passed"])
        self.assertEqual(result["mismatched_samples"], 1)
        self.assertEqual(result["first_mismatch"]["channel"], 1)
        self.assertEqual(result["first_mismatch"]["frame"], index // 2)

    def test_gain_is_never_fitted_away(self):
        samples = array("i", (sample // 2 for sample in self.source.samples))
        result = self.compare(self.capture(samples), offset_frames=0)
        self.assertFalse(result["comparison_passed"])
        self.assertGreater(result["mismatched_samples"], self.source.frames)

    def test_inserted_and_dropped_frames_at_a_join_cannot_be_realigned(self):
        split = self.manifest["splits"][0] * 2
        for samples in (self.source.samples[:split] + array("i", [0, 0]) + self.source.samples[split:],
                        self.source.samples[:split] + self.source.samples[split + 2:]):
            with self.subTest(frames=len(samples) // 2):
                path = self.capture(samples, name=f"capture-{len(samples)}.wav")
                result = self.compare(path)
                self.assertFalse(result["comparison_passed"])
                self.assertEqual(result["first_mismatch"]["frame"], split // 2)

    def test_missing_tail_and_a_duplicated_last_frame_fail(self):
        missing = self.compare(self.capture(self.source.samples[:-2], name="short.wav"))
        self.assertFalse(missing["comparison_passed"])
        self.assertEqual(missing["missing_tail_frames"], 1)
        duplicate = self.compare(self.capture(self.source.samples + self.source.samples[-2:], name="duplicate.wav"))
        self.assertFalse(duplicate["comparison_passed"])
        self.assertEqual(duplicate["trailing_nonzero_frames"], 1)

    def test_a_substituted_silent_frame_is_detected_without_any_timing_change(self):
        samples = array("i", self.source.samples)
        samples[200:202] = array("i", [0, 0])
        result = self.compare(self.capture(samples))
        self.assertFalse(result["comparison_passed"])
        self.assertEqual(result["mismatched_samples"], 2)

    def test_swapped_channels_do_not_pass(self):
        samples = array("i", (value for left, right in zip(self.source.samples[::2], self.source.samples[1::2])
                               for value in (right, left)))
        result = self.compare(self.capture(samples), offset_frames=0)
        self.assertFalse(result["comparison_passed"])

    def test_a_different_rate_or_narrower_precision_is_refused(self):
        with self.assertRaisesRegex(ValueError, "rate/channel"):
            self.compare(self.capture(self.source.samples, rate=48000, name="rate.wav"))
        narrowed = array("i", (sample >> 16 for sample in self.source.samples))
        with self.assertRaisesRegex(ValueError, "less valid precision"):
            self.compare(self.capture(narrowed, bits=16, name="narrow.wav"))

    def test_ambiguous_prefix_cannot_choose_the_copy_that_passes(self):
        samples = self.source.samples + array("i", [0, 0]) + self.source.samples
        with self.assertRaisesRegex(ValueError, "ambiguous"):
            self.compare(self.capture(samples), search_frames=self.source.frames + 1)

    def test_corrupted_first_frame_is_reported_with_an_explicit_offset(self):
        samples = array("i", self.source.samples)
        samples[0] ^= 1
        path = self.capture(samples)
        with self.assertRaisesRegex(ValueError, "no exact prefix"):
            self.compare(path)
        result = self.compare(path, offset_frames=0)
        self.assertFalse(result["comparison_passed"])
        self.assertEqual(result["first_mismatch"]["frame"], 0)

    def test_nonzero_extensible_padding_is_refused_instead_of_discarded(self):
        path = self.capture(self.source.samples)
        original = path.read_bytes()
        fmt = struct.pack("<HHIIHHHHI", 0xFFFE, 2, 44100, 44100 * 8, 8, 32, 22, 24, 3) + bp.PCM_GUID
        body = b"WAVEfmt " + struct.pack("<I", 40) + fmt + original[36:]
        path.write_bytes(b"RIFF" + struct.pack("<I", len(body)) + body)
        self.assertTrue(self.compare(path)["comparison_passed"])
        data = bytearray(path.read_bytes())
        data[68] |= 1
        path.write_bytes(data)
        with self.assertRaisesRegex(ValueError, "padding bits"):
            self.compare(path)

    def test_a_changed_reference_or_playlist_is_refused(self):
        path = self.capture(self.source.samples)
        (self.root / "split.m3u").write_text("whole.wav\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "changed"):
            self.compare(path)

    def test_malformed_truncated_and_float_wav_are_refused(self):
        path = self.capture(self.source.samples)
        original = path.read_bytes()
        variants = [original[:-1], original + b"x"]
        floating = bytearray(original)
        struct.pack_into("<H", floating, 20, 3)
        variants.append(floating)
        bad_align = bytearray(original)
        struct.pack_into("<H", bad_align, 32, 1)
        variants.append(bad_align)
        for data in variants:
            path.write_bytes(data)
            with self.subTest(length=len(data)), self.assertRaises(ValueError):
                self.compare(path)

    def test_an_extra_waveform_list_cannot_hide_uncompared_programme(self):
        path = self.capture(self.source.samples)
        original = path.read_bytes()
        extra = b"wavl" + b"data" + struct.pack("<I", 8) + struct.pack("<ii", 256, -256)
        body = original[8:] + b"LIST" + struct.pack("<I", len(extra)) + extra
        path.write_bytes(b"RIFF" + struct.pack("<I", len(body)) + body)
        with self.assertRaisesRegex(ValueError, "waveform"):
            self.compare(path)

    def test_manifest_paths_and_invalid_budgets_do_not_expand_the_probe_scope(self):
        path = self.capture(self.source.samples)
        changed = dict(self.manifest)
        changed["files"] = {**self.manifest["files"], "../outside.wav": "0" * 64}
        self.path.write_text(json.dumps(changed), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "inventory"):
            self.compare(path)
        self.path.write_text(json.dumps(self.manifest), encoding="utf-8")
        for value in (-1, 44100 * 16, float("nan")):
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.compare(path, search_frames=value)

    def test_explicit_evidence_kind_cannot_promote_a_comparison_to_hardware_acceptance(self):
        for kind in ("synthetic", "digital-loopback", "digital-device"):
            result = bp.compare(self.path, self.root / "whole.wav", kind)
            self.assertTrue(result["comparison_passed"])
            self.assertFalse(result["hardware_acceptance"])
        with self.assertRaisesRegex(ValueError, "digital capture"):
            bp.compare(self.path, self.root / "whole.wav", "analog")

    def test_strict_runner_requests_the_explicit_policy_and_device(self):
        binary = self.root / "fake-aede"
        binary.write_bytes(b"not an audio executable")

        def fake_playback(command, **options):
            self.assertEqual(command[command.index("--playback") + 1], "bit-perfect")
            self.assertEqual(command[command.index("--output-device") + 1], "alsa:hw:CARD=0,DEV=0")
            self.assertEqual(options["env"]["AEDE_AUDIO_BACKEND"], "native")
            return subprocess.CompletedProcess(command, 0)

        with patch("gapless.subprocess.run", side_effect=fake_playback):
            result = bp.run(self.root, binary, "alsa:hw:CARD=0,DEV=0", timeout=5)
        self.assertEqual(result["returncode"], 0)
        self.assertEqual(result["playback_policy"], "bit-perfect")
        self.assertEqual(result["output_device"], "alsa:hw:CARD=0,DEV=0")


if __name__ == "__main__":
    unittest.main()
