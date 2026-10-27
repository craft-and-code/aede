"""Original ALAC fixtures, encoded by FFmpeg solely as a reference tool."""
from pathlib import Path
import subprocess, tempfile, wave

out = Path(__file__).parent
with tempfile.TemporaryDirectory() as temporary:
    for name, rate, channels, bits, frames in [
        ("mono16", 44100, 1, 16, 10001),
        ("stereo24", 96000, 2, 24, 48013),
        ("surround16", 48000, 6, 16, 513),
    ]:
        source = Path(temporary) / (name + ".wav")
        with wave.open(str(source), "wb") as file:
            file.setparams((channels, bits // 8, rate, 0, "NONE", "not compressed"))
            file.writeframes(b"".join(
                (((i * 104729 + c * 49999) % (1 << bits)) - (1 << (bits - 1))).to_bytes(bits // 8, "little", signed=True)
                for i in range(frames) for c in range(channels)
            ))
        subprocess.run([
            "ffmpeg", "-v", "error", "-nostdin", "-y", "-i", str(source),
            "-map_metadata", "-1", "-c:a", "alac", "-movflags", "+faststart", str(out / (name + ".m4a")),
        ], check=True)

# Equivalent media timing: change ticks from 1/96000 to 1/192000 without changing audio.
data = bytearray((out / "stereo24.m4a").read_bytes())
for kind in [b"mdhd", b"stts", b"elst"]:
    at = data.index(kind) + 4
    assert data[at] == 0  # Controlled fixture uses version-zero boxes.
    if kind == b"mdhd":
        fields = [at + 12, at + 16]  # media timescale and duration
    elif kind == b"stts":
        count = int.from_bytes(data[at + 4:at + 8], "big")
        fields = [at + 12 + 8 * i for i in range(count)]
    else:
        count = int.from_bytes(data[at + 4:at + 8], "big")
        fields = [at + 12 + 12 * i for i in range(count)]
    for field in fields:
        value = int.from_bytes(data[field:field + 4], "big", signed=(kind == b"elst"))
        if value >= 0:
            data[field:field + 4] = (2 * value).to_bytes(4, "big", signed=(kind == b"elst"))
(out / "stereo24-clock192k.m4a").write_bytes(data)

# A coarser (1/48000) media clock cannot express the odd final PCM frame exactly.
# Round only media ticks; keep every encoded packet, byte offset and PCM sample.
data = bytearray((out / "stereo24.m4a").read_bytes())
for kind in [b"mdhd", b"stts", b"elst"]:
    at = data.index(kind) + 4
    assert data[at] == 0
    if kind == b"mdhd":
        fields = [at + 12, at + 16]
    else:
        count = int.from_bytes(data[at + 4:at + 8], "big")
        stride = 8 if kind == b"stts" else 12
        fields = [at + 12 + stride * i for i in range(count)]
    for field in fields:
        value = int.from_bytes(data[field:field + 4], "big", signed=(kind == b"elst"))
        if value >= 0:
            data[field:field + 4] = ((value + 1) // 2).to_bytes(4, "big", signed=(kind == b"elst"))
(out / "stereo24-clock48k.m4a").write_bytes(data)
