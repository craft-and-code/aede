"""Original RFC 7845 family-one references; FFmpeg is a fixture encoder only."""
from pathlib import Path
import math
import struct
import subprocess
import tempfile

root = Path(__file__).parent
layouts = ["mono", "stereo", "3.0", "quad", "5.0", "5.1", "6.1", "7.1"]
with tempfile.TemporaryDirectory() as folder:
    source = Path(folder) / "source.pcm"
    for channels, layout in enumerate(layouts, 1):
        pcm = b"".join(struct.pack("<h", round(6500 * math.sin(
            2 * math.pi * (80 if channels >= 6 and c == 3 else 700 + 500 * c) * i / 48000
        ))) for i in range(12037) for c in range(channels))
        source.write_bytes(pcm)
        subprocess.run([
            "ffmpeg", "-v", "error", "-nostdin", "-y", "-f", "s16le",
            "-ar", "48000", "-ac", str(channels), "-channel_layout", layout,
            "-i", str(source), "-map_metadata", "-1", "-c:a", "libopus",
            "-mapping_family", "1", "-b:a", str(64000 * channels),
            str(root / f"opus-family1-{channels}.opus"),
        ], check=True)
