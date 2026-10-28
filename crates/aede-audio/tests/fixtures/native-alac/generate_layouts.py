"""Original multichannel ALAC references with explicit Apple layout cookies."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).parent
PROFILES = [
    (3, "3.0", 113), (4, "4.0", 116), (5, "5.0(side)", 120),
    (6, "5.1(side)", 124), (7, "6.1", 142), (8, "7.1(wide-side)", 127),
]


def with_layout(encoded, channels, tag):
    data = bytearray(encoded)
    entry = data.index(b"alac", data.index(b"stsd") + 8) - 4
    config = data.index(b"alac", entry + 8) - 4
    assert int.from_bytes(data[config:config + 4], "big") == 36
    insert = config + 36
    extra = ((24).to_bytes(4, "big") + b"chan" + bytes(4)
             + ((tag << 16) | channels).to_bytes(4, "big") + bytes(8))

    def visit(start, end):
        offset = start
        while offset < end:
            size = int.from_bytes(data[offset:offset + 4], "big")
            kind = bytes(data[offset + 4:offset + 8])
            assert size >= 8 and offset + size <= end
            if offset < insert <= offset + size:
                data[offset:offset + 4] = (size + 24).to_bytes(4, "big")
                if kind == b"stsd":
                    for position in [entry, config]:
                        previous = int.from_bytes(data[position:position + 4], "big")
                        data[position:position + 4] = (previous + 24).to_bytes(4, "big")
            if kind in [b"moov", b"trak", b"mdia", b"minf", b"stbl"]:
                visit(offset + 8, offset + size)
            elif kind == b"stco":
                count = int.from_bytes(data[offset + 12:offset + 16], "big")
                for position in range(offset + 16, offset + 16 + 4 * count, 4):
                    previous = int.from_bytes(data[position:position + 4], "big")
                    assert previous >= insert
                    data[position:position + 4] = (previous + 24).to_bytes(4, "big")
            offset += size

    visit(0, len(data))
    data[insert:insert] = extra
    return data


with tempfile.TemporaryDirectory() as folder:
    source, encoded = [Path(folder) / name for name in ["source.pcm", "encoded.m4a"]]
    for channels, layout, tag in PROFILES:
        pcm = b"".join((((frame * 104729 + channel * 49999) % 65536) - 32768)
                       .to_bytes(2, "little", signed=True)
                       for frame in range(10001) for channel in range(channels))
        source.write_bytes(pcm)
        subprocess.run([
            "ffmpeg", "-v", "error", "-nostdin", "-y", "-f", "s16le",
            "-ar", "48000", "-ac", str(channels), "-channel_layout", layout,
            "-i", str(source), "-map_metadata", "-1", "-c:a", "alac",
            "-movflags", "+faststart", str(encoded),
        ], check=True)
        name = "surround16-layout" if channels == 6 else f"layout{channels}-16"
        output = ROOT / (name + ".m4a")
        output.write_bytes(with_layout(encoded.read_bytes(), channels, tag))
        reference = subprocess.check_output([
            "ffmpeg", "-v", "error", "-nostdin", "-i", str(output), "-f", "s16le", "-",
        ])
        # FFmpeg labels these surrounds as rear speakers. In its canonical
        # 6.1(back)/7.1(wide) order, Ls/Rs precede Cs or Lc/Rc. The source
        # layouts use side speakers, which follow Cs and Lc/Rc in mask order.
        order = {7: [0, 1, 2, 3, 5, 6, 4], 8: [0, 1, 2, 3, 6, 7, 4, 5]}
        order = order.get(channels, list(range(channels)))
        expected = b"".join(pcm[(frame * channels + channel) * 2:
                               (frame * channels + channel + 1) * 2]
                            for frame in range(10001) for channel in order)
        assert reference == expected, name
