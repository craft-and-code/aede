#!/usr/bin/env python3
"""Prepare integer probes and compare a digital capture without signal repair.

Prepare/compare never open an audio device. Run explicitly starts strict local
playback through the shared measurement runner; recording remains external.
Exact sample equality is evidence about the recorded boundary, not a hardware
certificate. No Python dependencies are required.
"""
from __future__ import annotations

import argparse
from array import array
from dataclasses import dataclass
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import sys

import gapless

MAX_BYTES = 64 * 1024 * 1024
MAX_FRAMES = 192000 * 90
RATES = (44100, 48000, 88200, 96000, 176400, 192000)
PCM_GUID = bytes.fromhex("0100000000001000800000aa00389b71")


@dataclass
class Pcm:
    rate: int
    channels: int
    container_bits: int
    valid_bits: int
    samples: array
    file_sha256: str

    @property
    def frames(self):
        return len(self.samples) // self.channels


def bounded_bytes(path, limit=MAX_BYTES):
    with Path(path).open("rb") as source:
        data = source.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f"file exceeds the {limit}-byte measurement budget")
    return data


def pcm_bytes(samples):
    # Canonical normalized signed 32-bit little-endian values, without loss.
    values = array("i", samples)
    if values.itemsize != 4:
        raise ValueError("this Python platform has no 32-bit signed int array")
    if sys.byteorder != "little":
        values.byteswap()
    return values.tobytes()


def read_pcm(path):
    data = bounded_bytes(path)
    if (len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"WAVE"
            or struct.unpack_from("<I", data, 4)[0] + 8 != len(data)):
        raise ValueError("use a complete little-endian RIFF/WAVE PCM capture")
    fmt = payload = None
    position = 12
    chunks = 0
    while position < len(data):
        chunks += 1
        if chunks > 4096 or position + 8 > len(data):
            raise ValueError("invalid or excessive WAV chunks")
        name, count = struct.unpack_from("<4sI", data, position)
        start, end = position + 8, position + 8 + count
        if end + (count % 2) > len(data):
            raise ValueError("truncated WAV chunk or missing padding")
        if name == b"fmt ":
            if fmt is not None or count not in (16, 18, 40):
                raise ValueError("duplicate or unsupported WAV format")
            fmt = data[start:end]
        elif name == b"data":
            if payload is not None:
                raise ValueError("duplicate WAV audio data")
            payload = memoryview(data)[start:end]
        elif name == b"slnt" or (name == b"LIST" and (count < 4 or data[start:start + 4] == b"wavl")):
            raise ValueError("waveform lists and silence chunks are not an unambiguous single programme")
        position = end + (count % 2)
    if fmt is None or payload is None:
        raise ValueError("missing WAV format or audio data")
    codec, channels, rate, byte_rate, alignment, bits = struct.unpack_from("<HHIIHH", fmt)
    if channels not in (1, 2) or not 8000 <= rate <= 192000 or bits not in (16, 24, 32):
        raise ValueError("use mono/stereo integer PCM on 16, 24 or 32 bits")
    valid = bits
    if codec == 1:
        if len(fmt) not in (16, 18) or (len(fmt) == 18 and fmt[16:18] != b"\0\0"):
            raise ValueError("unsupported PCM format extension")
    elif codec == 0xFFFE:
        if len(fmt) != 40 or struct.unpack_from("<H", fmt, 16)[0] != 22 or fmt[24:] != PCM_GUID:
            raise ValueError("use the integer PCM extensible subtype")
        valid, mask = struct.unpack_from("<HI", fmt, 18)
        if not 1 <= valid <= bits or mask not in ((0, 4) if channels == 1 else (3,)):
            raise ValueError("unknown valid precision or channel association")
    else:
        raise ValueError("floating-point/compressed capture is not an integer identity reference")
    width = bits // 8
    if alignment != channels * width or byte_rate != rate * alignment or len(payload) % alignment:
        raise ValueError("WAV block alignment or byte rate is inconsistent")
    frames = len(payload) // alignment
    if frames > MAX_FRAMES:
        raise ValueError("capture exceeds the frame measurement budget")
    samples = array("i")
    padding_mask = (1 << (bits - valid)) - 1
    for offset in range(0, len(payload), width):
        sample = int.from_bytes(payload[offset:offset + width], "little", signed=True)
        if sample & padding_mask:
            raise ValueError("nonzero extensible padding bits; no bits may be discarded")
        samples.append(sample << (32 - bits))
    return Pcm(rate, channels, bits, valid, samples, hashlib.sha256(data).hexdigest())


def write_pcm(path, samples, rate, bits, channels):
    width = bits // 8
    if bits not in (16, 24, 32) or channels not in (1, 2) or len(samples) % channels:
        raise ValueError("invalid integer probe format")
    payload = bytearray()
    for sample in samples:
        payload.extend(int(sample).to_bytes(width, "little", signed=True))
    alignment = width * channels
    fmt = struct.pack("<HHIIHH", 1, channels, rate, rate * alignment, alignment, bits)
    body = b"WAVEfmt " + struct.pack("<I", len(fmt)) + fmt
    body += b"data" + struct.pack("<I", len(payload)) + payload
    if len(payload) % 2:
        body += b"\0"
    with Path(path).open("xb") as target:
        target.write(b"RIFF" + struct.pack("<I", len(body)) + body)


def prepare(directory, rate=48000, bits=24, channels=2, seconds=3.0, flac=False, full_scale=False):
    if (rate not in RATES or bits not in (16, 24) or channels not in (1, 2)
            or not math.isfinite(seconds) or not 1 <= seconds <= 8):
        raise ValueError("use a supported rate, 16/24 bits, mono/stereo and 1..8 seconds per part")
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    lengths = [round(rate * seconds) + 137 + index * 211 for index in range(3)]
    samples = array("i")
    state = 0xA3DE715B
    # Quiet, nonperiodic channels exercise real low bits. Full-scale endpoints
    # are optional so ordinary probes do not unexpectedly emit full-scale noise.
    span = 1 << (bits - 6)
    for _ in range(sum(lengths) * channels):
        state = (1664525 * state + 1013904223) & 0xFFFFFFFF
        sample = ((state >> 13) % (2 * span)) - span
        samples.append(sample if sample else 1)
    if full_scale:
        patterns = [-(1 << (bits - 1)), (1 << (bits - 1)) - 1, -1, 1]
        for index, sample in enumerate(patterns):
            samples[index * channels] = sample
    write_pcm(directory / "whole.wav", samples, rate, bits, channels)
    entries = []
    files = ["whole.wav"]
    start = 0
    for index, frames in enumerate(lengths):
        name = f"{index + 1:02}.wav"
        write_pcm(directory / name, samples[start * channels:(start + frames) * channels], rate, bits, channels)
        files.append(name)
        if flac:
            name_flac = f"{index + 1:02}.flac"
            command = ["ffmpeg", "-nostdin", "-n", "-hide_banner", "-loglevel", "error",
                       "-i", str(directory / name), "-c:a", "flac", str(directory / name_flac)]
            subprocess.run(command, check=True)
            name = name_flac
            files.append(name)
        entries.append(name)
        start += frames
    (directory / "split.m3u").write_text("\n".join(entries) + "\n", encoding="utf-8")
    files.append("split.m3u")
    normalized = array("i", (sample << (32 - bits) for sample in samples))
    manifest = {"format": "aede-bit-perfect-probe", "version": 1, "rate": rate, "bits": bits,
                "channels": channels, "frames": sum(lengths), "lengths": lengths,
                "splits": [lengths[0], sum(lengths[:2])], "entries": entries,
                "full_scale": full_scale,
                "pcm_s32le_sha256": hashlib.sha256(pcm_bytes(normalized)).hexdigest(),
                "files": {name: hashlib.sha256(bounded_bytes(directory / name)).hexdigest() for name in files}}
    with (directory / "manifest.json").open("x", encoding="utf-8") as target:
        target.write(json.dumps(manifest, indent=2) + "\n")
    return manifest


def load_probe(path):
    path = Path(path).resolve()
    manifest = json.loads(bounded_bytes(path, 16384).decode("utf-8"))
    if (not isinstance(manifest, dict) or manifest.get("format") != "aede-bit-perfect-probe"
            or manifest.get("version") != 1):
        raise ValueError("unsupported integer probe manifest")
    source = read_pcm(path.parent / "whole.wav")
    for field, value in (("rate", source.rate), ("bits", source.valid_bits),
                         ("channels", source.channels), ("frames", source.frames)):
        if type(manifest.get(field)) is not int or manifest[field] != value:
            raise ValueError("probe format or frame count differs from the manifest")
    lengths = manifest.get("lengths")
    if (not isinstance(lengths, list) or len(lengths) != 3
            or any(type(value) is not int or value <= 0 for value in lengths)
            or sum(lengths) != source.frames
            or manifest.get("splits") != [lengths[0], sum(lengths[:2])]):
        raise ValueError("invalid probe occurrence boundaries")
    entries = manifest.get("entries")
    allowed = {"whole.wav", "split.m3u", "01.wav", "02.wav", "03.wav", "01.flac", "02.flac", "03.flac"}
    files = manifest.get("files")
    if (not isinstance(files, dict) or not {"whole.wav", "split.m3u", "01.wav", "02.wav", "03.wav"} <= files.keys()
            or not files.keys() <= allowed or not isinstance(entries, list) or len(entries) != 3
            or entries not in (["01.wav", "02.wav", "03.wav"], ["01.flac", "02.flac", "03.flac"])
            or any(name not in files for name in entries)):
        raise ValueError("invalid probe file inventory")
    for name, digest in files.items():
        actual = source.file_sha256 if name == "whole.wav" else hashlib.sha256(bounded_bytes(path.parent / name)).hexdigest()
        if actual != digest:
            raise ValueError(f"probe file changed after preparation: {name}")
    if (path.parent / "split.m3u").read_text(encoding="utf-8") != "\n".join(entries) + "\n":
        raise ValueError("playlist differs from the manifest occurrences")
    if hashlib.sha256(pcm_bytes(source.samples)).hexdigest() != manifest.get("pcm_s32le_sha256"):
        raise ValueError("canonical reference differs from its prepared digest")
    return manifest, source


def fixed_offset(reference, capture, channels, search_frames):
    # Exact prefix matching, using KMP to keep adversarial/silent capture work
    # linear. It establishes one offset; it never repairs a later discrepancy.
    prefix = reference[:min(len(reference), 64 * channels)]
    if not prefix:
        raise ValueError("empty reference cannot establish an alignment")
    failure = [0] * len(prefix)
    matched = 0
    for index in range(1, len(prefix)):
        while matched and prefix[index] != prefix[matched]:
            matched = failure[matched - 1]
        if prefix[index] == prefix[matched]:
            matched += 1
        failure[index] = matched
    offsets = []
    matched = 0
    limit = min(len(capture), search_frames * channels + len(prefix))
    for index in range(limit):
        sample = capture[index]
        while matched and sample != prefix[matched]:
            matched = failure[matched - 1]
        if sample == prefix[matched]:
            matched += 1
        if matched == len(prefix):
            start = index + 1 - len(prefix)
            if start % channels == 0 and start // channels <= search_frames:
                offsets.append(start // channels)
                if len(offsets) > 1:
                    raise ValueError("ambiguous exact prefix; supply one explicit --offset-frames")
            matched = failure[matched - 1]
    if not offsets:
        raise ValueError("no exact prefix in the search budget; inspect capture or supply --offset-frames")
    return offsets[0]


def compare(manifest_path, capture_path, kind, offset_frames=None, search_frames=None):
    if kind not in ("synthetic", "digital-loopback", "digital-device"):
        raise ValueError("integer identity requires an explicitly classified digital capture")
    manifest, source = load_probe(manifest_path)
    capture = read_pcm(capture_path)
    if (capture.rate, capture.channels) != (source.rate, source.channels):
        raise ValueError("capture rate/channel association must equal the reference; no conversion is allowed")
    if capture.valid_bits < source.valid_bits:
        raise ValueError("capture has less valid precision than the source")
    if offset_frames is not None and search_frames is not None:
        raise ValueError("choose an explicit offset or a bounded prefix search")
    if offset_frames is None:
        search_frames = source.rate * 5 if search_frames is None else search_frames
        if type(search_frames) is not int or not 0 <= search_frames <= source.rate * 15:
            raise ValueError("prefix search must be between zero and fifteen seconds in frames")
        offset_frames = fixed_offset(source.samples, capture.samples, source.channels, search_frames)
    if type(offset_frames) is not int or not 0 <= offset_frames <= capture.frames:
        raise ValueError("capture offset must be an available nonnegative frame")
    start = offset_frames * source.channels
    count = min(len(source.samples), len(capture.samples) - start)
    mismatches = 0
    first = None
    for index in range(count):
        expected, actual = source.samples[index], capture.samples[start + index]
        if expected != actual:
            mismatches += 1
            if first is None:
                first = {"frame": index // source.channels, "channel": index % source.channels,
                         "expected_s32": expected, "capture_s32": actual}
    end = start + count
    leading = sum(any(capture.samples[index:index + source.channels])
                  for index in range(0, start, source.channels))
    trailing = sum(any(capture.samples[index:index + source.channels])
                   for index in range(end, len(capture.samples), source.channels))
    complete = count == len(source.samples)
    passed = complete and mismatches == 0 and leading == 0 and trailing == 0
    return {"version": 1, "capture_kind": kind, "hardware_acceptance": False,
            "comparison_passed": passed, "complete_programme": complete,
            "reference_frames": source.frames, "capture_frames": capture.frames,
            "compared_frames": count // source.channels,
            "missing_tail_frames": source.frames - count // source.channels,
            "offset_frames": offset_frames, "mismatched_samples": mismatches,
            "first_mismatch": first, "leading_nonzero_frames": leading,
            "trailing_nonzero_frames": trailing, "rate": source.rate, "channels": source.channels,
            "source_valid_bits": source.valid_bits, "capture_valid_bits": capture.valid_bits,
            "capture_container_bits": capture.container_bits,
            "reference_file_sha256": source.file_sha256,
            "capture_file_sha256": capture.file_sha256,
            "reference_pcm_s32le_sha256": manifest["pcm_s32le_sha256"],
            "compared_capture_pcm_s32le_sha256": hashlib.sha256(pcm_bytes(capture.samples[start:end])).hexdigest(),
            "scope": "Exact integer programme comparison at one fixed offset. Capture kind is supplied by the caller; route, driver and hardware acceptance require separate evidence."}


def run(directory, binary, device, whole=False, workers=0, io=False, timeout=30):
    load_probe(Path(directory) / "manifest.json")
    if not device:
        raise ValueError("strict probe playback requires an explicit --output-device")
    return gapless.run(directory, binary, whole=whole, workers=workers, io=io, timeout=timeout,
                       policy="bit-perfect", device=device)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare", help="write deterministic integer WAV/FLAC probes; no playback")
    prep.add_argument("directory", type=Path)
    prep.add_argument("--rate", type=int, choices=RATES, default=48000)
    prep.add_argument("--bits", type=int, choices=(16, 24), default=24)
    prep.add_argument("--channels", type=int, choices=(1, 2), default=2)
    prep.add_argument("--seconds", type=float, default=3)
    prep.add_argument("--flac", action="store_true")
    prep.add_argument("--full-scale", action="store_true", help="include signed full-scale endpoints; set a suitable external listening level before run")
    analysis = commands.add_parser("compare", help="compare every integer sample in a separately recorded digital WAV")
    analysis.add_argument("manifest", type=Path)
    analysis.add_argument("capture", type=Path)
    analysis.add_argument("--kind", choices=("synthetic", "digital-loopback", "digital-device"), required=True)
    alignment = analysis.add_mutually_exclusive_group()
    alignment.add_argument("--offset-frames", type=int)
    alignment.add_argument("--search-frames", type=int)
    analysis.add_argument("--output", type=Path, required=True)
    exercise = commands.add_parser("run", help="STARTS bit-perfect audio playback; capture must already be recording")
    exercise.add_argument("directory", type=Path)
    exercise.add_argument("--binary", type=Path, default=Path("target/release/aede"))
    exercise.add_argument("--output-device", required=True)
    exercise.add_argument("--whole", action="store_true")
    exercise.add_argument("--cpu-workers", type=int, default=0)
    exercise.add_argument("--io-load", action="store_true")
    exercise.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            result = prepare(args.directory, args.rate, args.bits, args.channels, args.seconds, args.flac, args.full_scale)
            print(f"Prepared {result['frames']} integer frames; no audio played.")
            return 0
        if args.command == "compare":
            if args.output.exists():
                raise ValueError("comparison output already exists")
            result = compare(args.manifest, args.capture, args.kind, args.offset_frames, args.search_frames)
            with args.output.open("x", encoding="utf-8") as target:
                target.write(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result, indent=2))
            return 0 if result["comparison_passed"] else 1
        result = run(args.directory, args.binary, args.output_device, args.whole, args.cpu_workers, args.io_load, args.timeout)
        print(json.dumps(result, indent=2))
        return 0 if result.get("returncode") == 0 else 1
    except (OSError, ValueError, TypeError, KeyError, OverflowError, subprocess.SubprocessError) as error:
        print(f"Bit-perfect measurement refused: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
