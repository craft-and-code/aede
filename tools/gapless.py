#!/usr/bin/env python3
"""Prepare, exercise and measure real-output joins without Python dependencies.

No device is opened by prepare/analyze. Run deliberately starts native playback;
recording remains an explicit external step. Synthetic evidence is labelled as
such and cannot be promoted to hardware acceptance by this tool.
"""
from __future__ import annotations

import argparse
from array import array
import hashlib
import json
import math
import multiprocessing
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import tempfile
import time
import wave

MAX_CAPTURE_BYTES = 64 * 1024 * 1024
MAX_DURATION_SECONDS = 90
MARKER_SECONDS = 127 / 1500


def write_wav(path, samples, rate, channels=2):
    pcm = array("h", (max(-32768, min(32767, round(x * 32767))) for x in samples))
    if sys.byteorder != "little":
        pcm.byteswap()
    with wave.open(str(path), "wb") as out:
        out.setnchannels(channels)
        out.setsampwidth(2)
        out.setframerate(rate)
        out.writeframes(pcm.tobytes())


def read_wav(path, channel=0):
    if Path(path).stat().st_size > MAX_CAPTURE_BYTES:
        raise ValueError("capture exceeds the 64 MiB measurement budget")
    with wave.open(str(path), "rb") as source:
        rate, channels = source.getframerate(), source.getnchannels()
        if source.getsampwidth() != 2 or source.getcomptype() != "NONE":
            raise ValueError("use uncompressed 16-bit PCM WAV (convert a copy with FFmpeg)")
        if not 8000 <= rate <= 192000 or not 1 <= channels <= 8 or not 0 <= channel < channels:
            raise ValueError("unsupported rate/channel or requested capture channel")
        if source.getnframes() > rate * MAX_DURATION_SECONDS:
            raise ValueError("capture exceeds the 90-second measurement budget")
        payload = source.readframes(source.getnframes())
        if len(payload) != source.getnframes() * channels * 2:
            raise ValueError("truncated WAV capture")
        pcm = array("h")
        pcm.frombytes(payload)
        if sys.byteorder != "little":
            pcm.byteswap()
        return rate, array("f", (x / 32767 for x in pcm[channel::channels]))


def marker_wave(index, rate):
    # A reproducible, distinct chip sequence makes an isolated correlation peak.
    state = index + 0xA3DE
    chips = []
    for _ in range(127):
        state = (state * 1664525 + 1013904223) & 0xFFFFFFFF
        chips.append(1.0 if state & 0x80000000 else -1.0)
    result = []
    count = round(MARKER_SECONDS * rate)
    for frame in range(count):
        chip = min(126, int(frame * 1500 / rate))
        fade = min(1.0, frame / (rate * .002), (count - 1 - frame) / (rate * .002))
        result.append(.065 * chips[chip] * (.5 - .5 * math.cos(math.pi * fade)))
    return result


def prepare(directory, rate=48000, tracks=3, seconds=3.0, flac=False):
    if not 8000 <= rate <= 96000 or not 2 <= tracks <= 8 or not 2 <= seconds <= 8:
        raise ValueError("use rate 8000..96000, tracks 2..8 and 2..8 seconds per track")
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    lengths = [round(seconds * rate) + 137 + index * 211 for index in range(tracks)]
    total = sum(lengths)
    # Quiet carrier crosses every split unchanged. Markers stay below -20 dBFS.
    mono = array("f", (.015 * math.sin(2 * math.pi * 437 * frame / rate) for frame in range(total)))
    markers, splits = [], []
    start = 0
    for track, length in enumerate(lengths):
        for fraction in (.10, .30, .50, .70, .90):
            position = start + round(length * fraction)
            index = len(markers)
            template = marker_wave(index, rate)
            for offset, value in enumerate(template):
                mono[position + offset] += value
            markers.append({"index": index, "frame": position, "track": track})
        start += length
        if track + 1 < tracks:
            splits.append(start)
    stereo = array("f", (value for sample in mono for value in (sample, -sample)))
    write_wav(directory / "whole.wav", stereo, rate)
    entries = []
    start = 0
    for index, length in enumerate(lengths):
        name = f"{index + 1:02}.wav"
        write_wav(directory / name, stereo[start * 2:(start + length) * 2], rate)
        if flac:
            converted = f"{index + 1:02}.flac"
            subprocess.run(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-i",
                            str(directory / name), "-c:a", "flac", str(directory / converted)], check=True)
            name = converted
        entries.append(name)
        start += length
    (directory / "split.m3u").write_text("\n".join(entries) + "\n", encoding="utf-8")
    manifest = {"version": 1, "rate": rate, "channels": 2, "frames": total,
                "lengths": lengths, "splits": splits, "markers": markers,
                "whole_sha256": hashlib.sha256((directory / "whole.wav").read_bytes()).hexdigest()}
    (directory / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def correlation(signal, template, start, stride=1):
    count = len(template)
    if start < 0 or start + (count - 1) * stride >= len(signal):
        return -1.0
    total = cross = energy = 0.0
    for index, value in enumerate(template):
        sample = signal[start + index * stride]
        total += sample
        cross += sample * value
        energy += sample * sample
    variance = energy - total * total / count
    template_energy = sum(value * value for value in template)
    return cross / math.sqrt(variance * template_energy) if variance > 1e-15 else -1.0


def centered(values):
    mean = sum(values) / len(values)
    return array("f", (value - mean for value in values))


def locate(signal, template, rate, low, high):
    stride = max(1, rate // 3000)
    coarse = centered(template[::stride])
    low = max(0, round(low))
    high = min(len(signal) - len(template), round(high))
    if high < low:
        raise ValueError("capture ends before an expected marker")
    best = max(range(low, high + 1, stride),
               key=lambda start: correlation(signal, coarse, start, stride))
    fine = centered(template)
    position = max(range(max(low, best - stride), min(high, best + stride) + 1),
                   key=lambda start: correlation(signal, fine, start))
    return position, correlation(signal, fine, position)


def analyze(manifest_path, capture_path, kind, channel=0, search_seconds=5.0,
            window_ms=250.0, max_gap_ms=.5, reference_channel=0):
    if kind not in ("analog", "digital-loopback", "synthetic"):
        raise ValueError("declare the capture kind explicitly")
    if not .1 <= search_seconds <= 15 or not 5 <= window_ms <= 1000 or not 0 <= max_gap_ms <= 20:
        raise ValueError("invalid marker search window or gap tolerance")
    path = Path(manifest_path)
    if path.stat().st_size > 16384:
        raise ValueError("manifest exceeds the measurement budget")
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or manifest.get("version") != 1:
        raise ValueError("unsupported measurement manifest")
    source_rate, reference = read_wav(path.parent / "whole.wav", reference_channel)
    source_path = path.parent / "whole.wav"
    if hashlib.sha256(source_path.read_bytes()).hexdigest() != manifest.get("whole_sha256"):
        raise ValueError("whole reference changed after preparation")
    rate, capture = read_wav(capture_path, channel)
    markers = manifest.get("markers")
    splits = manifest.get("splits")
    lengths = manifest.get("lengths")
    if (source_rate != manifest.get("rate") or len(reference) != manifest.get("frames")
            or not isinstance(markers, list) or not 10 <= len(markers) <= 40
            or not isinstance(splits, list) or not 1 <= len(splits) <= 7
            or not isinstance(lengths, list) or len(lengths) != len(splits) + 1
            or not all(type(length) is int and length > 0 for length in lengths)
            or sum(lengths) != len(reference)):
        raise ValueError("invalid source or marker manifest")
    if (not all(isinstance(marker, dict) and all(type(marker.get(key)) is int
                                               for key in ("index", "frame", "track")) for marker in markers)
            or not all(type(split) is int for split in splits)
            or [marker["index"] for marker in markers] != list(range(len(markers)))
            or [marker["frame"] for marker in markers] != sorted(set(marker["frame"] for marker in markers))
            or [marker["track"] for marker in markers] != sorted(marker["track"] for marker in markers)
            or splits != [sum(lengths[:index + 1]) for index in range(len(lengths) - 1)]
            or not all(sum(marker["track"] == track for marker in markers) == 5
                       for track in range(len(lengths)))):
        raise ValueError("invalid marker ordering or split positions")
    observations = []
    latency = 0
    for marker in markers:
        index, frame, track = marker["index"], marker["frame"], marker["track"]
        count = round(MARKER_SECONDS * source_rate)
        track_start = sum(lengths[:track])
        if (not 0 <= frame <= len(reference) - count or not 0 <= index < 40
                or not 0 <= track < len(lengths)
                or not track_start <= frame <= track_start + lengths[track] - count):
            raise ValueError("invalid marker position or track")
        # Reference includes the carrier and actual 16-bit source quantization.
        segment = reference[frame:frame + count]
        template = array("f")
        for output_frame in range(round(len(segment) * rate / source_rate)):
            position = output_frame * source_rate / rate
            left = min(len(segment) - 1, int(position))
            right = min(len(segment) - 1, left + 1)
            template.append(segment[left] + (segment[right] - segment[left]) * (position - left))
        expected = frame * rate / source_rate
        if not observations:
            observed, confidence = locate(capture, template, rate, 0, search_seconds * rate)
            latency = observed - expected
        else:
            center = expected + latency
            radius = window_ms * rate / 1000
            observed, confidence = locate(capture, template, rate, center - radius, center + radius)
        if confidence < .70:
            raise ValueError(f"marker {index} correlation {confidence:.3f} is too low; no measurement verdict")
        observations.append({"index": index, "track": track, "source_frame": frame,
                             "expected_capture_frame": expected, "capture_frame": observed,
                             "correlation": confidence})
    # Estimate clock drift only WITHIN tracks. A fit across a join could absorb a gap.
    slopes = [(right["capture_frame"] - left["capture_frame"]) /
              (right["expected_capture_frame"] - left["expected_capture_frame"])
              for left, right in zip(observations, observations[1:]) if left["track"] == right["track"]]
    slope = statistics.median(slopes)
    if abs(slope - 1) > .002:
        raise ValueError("clock drift exceeds 2000 ppm; inspect capture rate and route")
    for marker in observations:
        marker["delay_frames"] = marker["capture_frame"] - marker["expected_capture_frame"] * slope
    local_noise = [abs(right["delay_frames"] - left["delay_frames"])
                   for left, right in zip(observations, observations[1:]) if left["track"] == right["track"]]
    uncertainty = max(1.0, 3 * statistics.median(local_noise))
    resolution = max(1.0, max_gap_ms * rate / 1000)
    if uncertainty > resolution:
        raise ValueError(f"marker timing uncertainty {uncertainty:.1f} frames exceeds "
                         f"the {resolution:.1f}-frame resolution; measurement is inconclusive")
    joins = []
    for index, split in enumerate(manifest["splits"]):
        before = [x["delay_frames"] for x in observations if x["track"] == index][-2:]
        after = [x["delay_frames"] for x in observations if x["track"] == index + 1][:2]
        if len(before) != 2 or len(after) != 2:
            raise ValueError("missing marker pairs around a join")
        delta = statistics.median(after) - statistics.median(before)
        joins.append({"source_frame": split, "delay_step_frames": delta,
                      "delay_step_ms": 1000 * delta / rate,
                      "within_tolerance": abs(delta) + uncertainty <= resolution})
    result = {"capture_kind": kind, "hardware_acceptance": False,
              "capture_rate": rate, "capture_channel": channel, "reference_channel": reference_channel,
              "clock_drift_ppm": (slope - 1) * 1e6,
              "uncertainty_frames": uncertainty, "gap_tolerance_ms": max_gap_ms,
              "effective_gap_tolerance_frames": resolution,
              "joins": joins, "markers": observations,
              "all_joins_within_tolerance": all(x["within_tolerance"] for x in joins),
              "scope": "Marker delay continuity only; inspect logs and whole-vs-split waveforms before acceptance."}
    return result


def cpu_load(stop, deadline):
    block = b"aede-gapless-load" * 65536
    while not stop.is_set() and time.monotonic() < deadline:
        hashlib.sha256(block).digest()


def io_load(stop, deadline, directory):
    # Two 16 MiB files bound disk use; flush requests writeback, not cold disk I/O.
    block = b"aede-gapless-io\0" * 65536
    paths = [Path(directory) / f"load-{index}.bin" for index in range(2)]
    while not stop.is_set() and time.monotonic() < deadline:
        for path in paths:
            with path.open("wb") as out:
                for _ in range(16):
                    out.write(block)
                out.flush()
                os.fsync(out.fileno())
            with path.open("rb") as source:
                while source.read(len(block)):
                    if stop.is_set() or time.monotonic() >= deadline:
                        return


def run(directory, binary, whole=False, workers=0, io=False, timeout=30, bass=0, treble=0,
        policy=None, device=None):
    if not 0 <= workers <= 8 or not 5 <= timeout <= 90 or not -12 <= bass <= 12 or not -12 <= treble <= 12:
        raise ValueError("use 0..8 CPU workers, timeout 5..90 seconds and tone -12..12 dB")
    if policy not in (None, "without-effects", "bit-perfect", "dsp"):
        raise ValueError("unknown playback policy")
    if policy in ("without-effects", "bit-perfect") and (bass or treble):
        raise ValueError("the selected playback policy requires flat tone")
    if policy == "bit-perfect" and not device:
        raise ValueError("strict measurement requires an explicit output device")
    directory, binary = Path(directory).resolve(), Path(binary).resolve()
    if not binary.is_file() or not (directory / "manifest.json").is_file():
        raise ValueError("a prepared directory and built Aède binary are required")
    source = directory / ("whole.wav" if whole else "split.m3u")
    label = f"{'whole' if whole else 'split'}-{'load' if workers or io else 'idle'}"
    log, record = directory / f"{label}.log", directory / f"{label}.run.json"
    if log.exists() or record.exists():
        raise ValueError("this run label already exists; prepare another directory for repeated trials")
    ctx = multiprocessing.get_context("spawn")
    stop = ctx.Event()
    processes = []
    started = time.monotonic()
    deadline = started + timeout + 2
    command = [str(binary), "--data", str(directory / f"data-{label}"), "--no-color", "play",
               str(source), "--normalize", "off", "--bass", str(bass), "--treble", str(treble)]
    if policy is not None:
        command += ["--playback", policy]
    if device is not None:
        command += ["--output-device", device]
    # A delegated CLI child's forced directory outranks --data. Do not inherit it
    # (or delegated terminal flags) into a measurement's isolated history store.
    environment = {key: value for key, value in os.environ.items() if not key.startswith("AEDE_DELEGATED_")}
    environment["AEDE_AUDIO_BACKEND"] = "native"
    outcome = {"version": 1, "platform": platform.system(), "architecture": platform.machine(),
               "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "cpu_workers": workers, "io_load": io, "timeout_seconds": timeout,
               "selection": "whole" if whole else "split", "bass_db": bass, "treble_db": treble,
               "playback_policy": policy or ("dsp" if bass or treble else "without-effects"),
               "output_device": device,
               "backend": "native", "physical_capture": "external; not established by this runner"}
    load_files = tempfile.TemporaryDirectory(prefix="aede-gapless-load-")
    try:
        for _ in range(workers):
            process = ctx.Process(target=cpu_load, args=(stop, deadline))
            process.start()
            processes.append(process)
        if io:
            process = ctx.Process(target=io_load, args=(stop, deadline, load_files.name))
            process.start()
            processes.append(process)
        with log.open("xb") as output:
            try:
                result = subprocess.run(command, env=environment, stdin=subprocess.DEVNULL,
                                        stdout=output, stderr=subprocess.STDOUT, timeout=timeout)
                outcome["returncode"] = result.returncode
            except subprocess.TimeoutExpired:
                outcome.update(returncode=None, timed_out=True)
    finally:
        stop.set()
        for process in processes:
            process.join(timeout=3)
            if process.is_alive():
                process.terminate()
                process.join()
        outcome["load_worker_exitcodes"] = [process.exitcode for process in processes]
        load_files.cleanup()
        outcome["elapsed_seconds"] = time.monotonic() - started
        diagnostics = re.findall(r"Native output: (\d+) consumed frames; queue (\d+)/(\d+) frames; "
                                 r"(\d+) missing frames in (\d+) callbacks; (\d+) host xruns",
                                 log.read_text(encoding="utf-8", errors="replace") if log.exists() else "")
        outcome["native_diagnostics"] = [dict(zip(("consumed_frames", "queued_frames", "capacity_frames",
                                                   "missing_frames", "shortage_callbacks", "host_xruns"),
                                                  map(int, match))) for match in diagnostics]
        outcome["diagnostics_clean"] = bool(diagnostics) and all(
            row["missing_frames"] == 0 and row["host_xruns"] == 0 for row in outcome["native_diagnostics"])
        with record.open("x", encoding="utf-8") as output:
            output.write(json.dumps(outcome, indent=2) + "\n")
    return outcome


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare", help="write quiet synthetic WAV/FLAC fixtures; no audio playback")
    prep.add_argument("directory", type=Path)
    prep.add_argument("--rate", type=int, default=48000)
    prep.add_argument("--tracks", type=int, default=3)
    prep.add_argument("--seconds", type=float, default=3)
    prep.add_argument("--flac", action="store_true", help="encode split files with installed FFmpeg")
    analysis = commands.add_parser("analyze", help="measure marker delay steps in an existing capture")
    analysis.add_argument("manifest", type=Path)
    analysis.add_argument("capture", type=Path)
    analysis.add_argument("--kind", choices=("analog", "digital-loopback", "synthetic"), required=True)
    analysis.add_argument("--channel", type=int, default=0)
    analysis.add_argument("--reference-channel", type=int, choices=(0, 1), default=0)
    analysis.add_argument("--search-seconds", type=float, default=5)
    analysis.add_argument("--window-ms", type=float, default=250)
    analysis.add_argument("--max-gap-ms", type=float, default=.5)
    analysis.add_argument("--output", type=Path, required=True)
    exercise = commands.add_parser("run", help="STARTS native audio playback; capture must already be recording")
    exercise.add_argument("directory", type=Path)
    exercise.add_argument("--binary", type=Path, default=Path("target/release/aede"))
    exercise.add_argument("--whole", action="store_true")
    exercise.add_argument("--cpu-workers", type=int, default=0)
    exercise.add_argument("--io-load", action="store_true")
    exercise.add_argument("--timeout", type=float, default=30)
    exercise.add_argument("--bass", type=float, default=0)
    exercise.add_argument("--treble", type=float, default=0)
    exercise.add_argument("--playback", choices=("without-effects", "bit-perfect", "dsp"))
    exercise.add_argument("--output-device", help="explicit host-qualified native device ID")
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            result = prepare(args.directory, args.rate, args.tracks, args.seconds, args.flac)
            print(f"Prepared {result['frames']} frames in {args.directory}; no audio played.")
            return 0
        if args.command == "analyze":
            if args.output.exists():
                raise ValueError("analysis output already exists")
            result = analyze(args.manifest, args.capture, args.kind, args.channel,
                             args.search_seconds, args.window_ms, args.max_gap_ms, args.reference_channel)
            with args.output.open("x", encoding="utf-8") as output:
                output.write(json.dumps(result, indent=2) + "\n")
            print(json.dumps({key: result[key] for key in ("capture_kind", "clock_drift_ppm", "joins",
                                                          "all_joins_within_tolerance")}, indent=2))
            return 0 if result["all_joins_within_tolerance"] else 1
        result = run(args.directory, args.binary, args.whole, args.cpu_workers, args.io_load,
                     args.timeout, args.bass, args.treble, args.playback, args.output_device)
        print(json.dumps(result, indent=2))
        return 0 if result.get("returncode") == 0 else 1
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, wave.Error) as error:
        print(f"Gapless measurement refused: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
