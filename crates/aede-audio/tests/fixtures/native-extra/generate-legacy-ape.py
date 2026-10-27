#!/usr/bin/env python3
"""Generate owned historical silence vectors; require two independent decoders."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import wave
import zlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("sdk", help="Monkey's Audio 13.43 console executable")
parser.add_argument("ffmpeg", help="Independent FFmpeg reference executable")
args = parser.parse_args()
out = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix="historical-ape-vectors-") as folder:
    folder = Path(folder)
    wav, encoded = folder / "silence.wav", folder / "silence.ape"
    with wave.open(str(wav), "wb") as target:
        target.setparams((2, 2, 44100, 0, "NONE", "not compressed"))
        target.writeframes(bytes(132300 * 4))
    subprocess.run([args.sdk, str(wav), str(encoded), "-c2000"], check=True)
    modern = encoded.read_bytes()
    d, h = struct.unpack_from("<II", modern, 8)
    first, second = struct.unpack_from("<II", modern, d + h)
    packet = modern[first:second]
    assert packet == bytes.fromhex("8c621c8d07000000000001002c"), "Review encoder changes"
    manifest = json.loads((out / "sha256.json").read_text())
    for version in [3950, 3970]:
        counts = [294912, 294912, 292176]
        packets = []
        for count in counts:
            crc = (zlib.crc32(bytes(count * 4)) >> 1) | 0x80000000
            packets.append(struct.pack("<I", crc) + packet[4:] + bytes(3))
        header = struct.pack("<4sHHHHIIIII", b"MAC ", version, 2000, 34, 2,
                             44100, 0, 0, len(counts), counts[-1])
        at, positions = 32 + 4 * len(counts), []
        for part in packets:
            positions.append(at)
            at += len(part)
        data = header + b"".join(struct.pack("<I", n) for n in positions) + b"".join(packets)
        source = folder / f"silence-{version}.ape"
        source.write_bytes(data)
        decoded, pcm = folder / f"{version}.wav", folder / f"{version}.s16le"
        subprocess.run([args.sdk, str(source), str(decoded), "-d"], check=True)
        subprocess.run([args.ffmpeg, "-v", "error", "-xerror", "-y", "-i", str(source),
                        "-f", "s16le", str(pcm)], check=True)
        with wave.open(str(decoded)) as target:
            actual = target.readframes(target.getnframes())
        assert actual == pcm.read_bytes() == bytes(882000 * 4)
        (out / source.name).write_bytes(data)
        manifest[source.name] = hashlib.sha256(data).hexdigest()
    (out / "sha256.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
