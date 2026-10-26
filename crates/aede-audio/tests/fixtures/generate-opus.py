"""Generate original Opus fixtures. FFmpeg/libopus is a reference encoder only."""
from pathlib import Path
import math
import struct
import subprocess
import tempfile
import wave

root = Path(__file__).parent
with tempfile.TemporaryDirectory() as directory:
    for name, channels, frames in [
        ("opus-mono", 1, 12037), ("opus-stereo", 2, 48013),
        ("opus-short", 1, 121), ("opus-surround", 6, 2401),
    ]:
        source = Path(directory) / (name + ".wav")
        with wave.open(str(source), "wb") as output:
            output.setparams((channels, 2, 48000, 0, "NONE", "not compressed"))
            output.writeframes(b"".join(
                struct.pack("<h", round(6500 * math.sin(2 * math.pi * (1000 + 1700 * channel) * i / 48000)))
                for i in range(frames) for channel in range(channels)
            ))
        subprocess.run([
            "ffmpeg", "-v", "error", "-nostdin", "-y", "-i", str(source),
            "-map_metadata", "-1", "-c:a", "libopus", "-b:a", "128k", str(root / (name + ".opus")),
        ], check=True)
