"""Remux an original ALAC ramp into a CAF speaker-hint refusal reference."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).parent

with tempfile.TemporaryDirectory() as folder:
    remuxed = Path(folder) / "layout4.caf"
    subprocess.run([
        "ffmpeg", "-v", "error", "-nostdin", "-y", "-i",
        str(ROOT / "layout4-16.m4a"), "-map", "0:a:0", "-c:a", "copy",
        str(remuxed),
    ], check=True)
    source = remuxed.read_bytes()

assert source[:8] == b"caff\x00\x01\x00\x00"
output = bytearray(source[:8])
offset = 8
while offset < len(source):
    kind = source[offset:offset + 4]
    size = int.from_bytes(source[offset + 4:offset + 12], "big", signed=True)
    assert size >= 0 and offset + 12 + size <= len(source)
    payload = source[offset + 12:offset + 12 + size]
    if kind == b"chan":
        # CAF describes the canonical decoded 4.0 order: L R C Cs.
        assert payload == ((115 << 16) | 4).to_bytes(4, "big") + bytes(8)
    if kind == b"kuki":
        # Apple's documented modern cookie is the contiguous 48-byte payload
        # within the legacy frma/alac wrappers emitted by this fixture muxer.
        assert payload[:12] == bytes.fromhex("0000000c66726d61616c6163")
        assert payload[16:24] == b"alac" + bytes(4)
        payload = payload[24:]
        assert len(payload) == 48
        assert payload[9] == 4
        assert payload[36:40] == ((116 << 16) | 4).to_bytes(4, "big")
    output.extend(kind + len(payload).to_bytes(8, "big", signed=True) + payload)
    offset += 12 + size

target = ROOT / "layout4-16-hint-conflict.caf"
target.write_bytes(output)
reference = subprocess.check_output([
    "ffmpeg", "-v", "error", "-nostdin", "-i", str(target), "-f", "s16le", "-",
])
expected = b"".join((((frame * 104729 + channel * 49999) % 65536) - 32768)
                    .to_bytes(2, "little", signed=True)
                    for frame in range(10001) for channel in range(4))
assert reference == expected
