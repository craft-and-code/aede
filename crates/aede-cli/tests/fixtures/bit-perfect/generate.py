"""Rebuild the independent integer source fixtures with reference libFLAC."""

import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


RATES = (44100, 48000, 88200, 96000, 176400, 192000)
FRAMES = 4233


def samples(bits, channels):
    minimum = -(1 << (bits - 1))
    maximum = (1 << (bits - 1)) - 1
    pattern = (0, minimum, maximum, -1, 1, -2, 2, -257, 256,
               -256, 255, 17, -19, -1023, 1024, minimum + 1, maximum - 1)
    for frame in range(FRAMES):
        for channel in range(channels):
            if frame < 16 or frame % 257 == 0:
                yield 0
            elif frame == 32:
                yield maximum if channel == 0 else 0
            elif frame == 33:
                yield minimum if channel == 1 else 0
            else:
                index = frame if channel == 0 else frame * 7 + 5
                yield pattern[index % len(pattern)]


def generate():
    version = subprocess.check_output(["flac", "--version"], text=True).strip()
    if version != "flac 1.5.0":
        raise SystemExit(f"Use the recorded reference encoder flac 1.5.0; found {version}")
    destination = Path(__file__).resolve().parent
    manifest = {"reference_encoder": version, "frames": FRAMES, "sources": []}
    with tempfile.TemporaryDirectory(prefix="aede_exact_fixtures_") as temporary:
        raw = Path(temporary) / "reference.raw"
        for bits in (16, 24):
            for channels in (1, 2):
                pcm = b"".join(value.to_bytes(bits // 8, "little", signed=True)
                               for value in samples(bits, channels))
                raw.write_bytes(pcm)
                for rate in RATES:
                    name = f"pcm-{bits}-{channels}-{rate}.flac"
                    output = destination / name
                    subprocess.run([
                        "flac", "--silent", "--force", "--force-raw-format",
                        "--endian=little", "--sign=signed", f"--channels={channels}",
                        f"--bps={bits}", f"--sample-rate={rate}", "--blocksize=4096",
                        "--no-padding", "--no-seektable", "--output-name", str(output),
                        str(raw),
                    ], check=True)
                    decoded = subprocess.check_output([
                        "flac", "--silent", "--decode", "--force-raw-format",
                        "--endian=little", "--sign=signed", "--stdout", str(output),
                    ])
                    if decoded != pcm:
                        raise SystemExit(f"Reference decoder changed source samples: {name}")
                    manifest["sources"].append({
                        "file": name, "bits": bits, "channels": channels, "rate": rate,
                        "pcm_sha256": hashlib.sha256(pcm).hexdigest(),
                        "pcm_md5": hashlib.md5(pcm).hexdigest(),
                        "file_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                    })
    (destination / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    generate()
