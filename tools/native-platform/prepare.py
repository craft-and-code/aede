#!/usr/bin/env python3
"""Create a disposable three-consumer test crate, never a product App build."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import urllib.request


def prepare(aede, fc, phemios, output):
    output.mkdir(parents=True, exist_ok=True)
    (output / "src").mkdir(exist_ok=True)
    (output / "tests").mkdir(exist_ok=True)
    (output / "src/lib.rs").write_text("//! Test-only platform qualification; no device or application entry point.\n")
    # JSON strings are also valid TOML basic strings for these controlled paths.
    quoted = lambda path: json.dumps(str(path.resolve()).replace("\\", "/"))
    manifest = f'''[package]
name = "native-audio-platform-acceptance"
version = "0.0.0"
edition = "2024"
rust-version = "1.90"
[workspace]
[dependencies]
aede-audio = {{ path = {quoted(aede / 'crates/aede-audio')}, features = ["ape-pcm16"] }}
aede-core = {{ path = {quoted(aede / 'crates/aede-core')}, default-features = false }}
flaccompagnon-core = {{ path = {quoted(fc / 'core')} }}
phemios-cd = {{ path = {quoted(phemios / 'crates/phemios-cd')} }}
md-5 = "0.10"
[patch."https://github.com/craft-and-code/aede"]
aede-audio = {{ path = {quoted(aede / 'crates/aede-audio')} }}
aede-dsp = {{ path = {quoted(aede / 'crates/aede-dsp')} }}
[patch."https://github.com/craft-and-code/FlacCompagnon"]
flaccompagnon-core = {{ path = {quoted(fc / 'core')} }}
flaccompagnon-spectrogram = {{ path = {quoted(fc / 'spectrogram')} }}
'''
    # A dependency's workspace patches do not propagate into this test crate.
    # Preserve Phémios's reviewed ioctl portability correction when present.
    cd_reader = phemios / 'vendor/cd-da-reader'
    if (cd_reader / 'Cargo.toml').is_file():
        manifest += f'\n[patch.crates-io]\ncd-da-reader = {{ path = {quoted(cd_reader)} }}\n'
    (output / "Cargo.toml").write_text(manifest)
    shutil.copyfile(Path(__file__).with_name("acceptance_tests.rs"), output / "tests/acceptance.rs")
    # Reuse the reviewed dependency versions; Cargo adds the CD reader and
    # removes unused workspace members when generating this test-only lock.
    if not (output / "Cargo.lock").exists():
        shutil.copyfile(aede / "Cargo.lock", output / "Cargo.lock")


def reference(output):
    # These bytes are a public reference recording, downloaded only by this
    # explicit qualification command, never redistributed as project audio.
    expected = "9b8e89b81a87001648d58dc9ef440a5b9b8c214a4df07bd22776da1ff6e32004"
    output.mkdir(parents=True, exist_ok=True)
    target = output / "sh3.ape"
    if not target.exists():
        with urllib.request.urlopen("https://samples.ffmpeg.org/monkeyaudio/sh3.ape", timeout=60) as response:
            data = response.read(2 * 1024 * 1024)
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError("historical APE reference hash differs; review before use")
        target.write_bytes(data)
    if hashlib.sha256(target.read_bytes()).hexdigest() != expected:
        raise ValueError("historical APE reference hash differs")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--aede", type=Path, required=True)
    parser.add_argument("--flaccompagnon", type=Path, required=True)
    parser.add_argument("--phemios", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--reference", action="store_true")
    args = parser.parse_args()
    prepare(args.aede, args.flaccompagnon, args.phemios, args.output)
    if args.reference:
        reference(args.output / "reference")
