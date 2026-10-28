"""Regenerate original WavPack exponent and Speex final-packet vectors."""
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

def page(data, sequence, flags, granule):
    laces = bytes([255] * (len(data) // 255) + [len(data) % 255])
    raw = bytearray(b'OggS' + bytes([0, flags]) + struct.pack('<QII', granule, 4213, sequence)
                    + b'\0' * 4 + bytes([len(laces)]) + laces + data)
    crc = 0
    for byte in raw:
        crc ^= byte << 24
        for _ in range(8):
            crc = ((crc << 1) ^ (0x04c11db7 if crc & 0x80000000 else 0)) & 0xffffffff
    raw[22:26] = struct.pack('<I', crc)
    return raw

header = b'Speex   ' + b'public API probe'.ljust(20, b'\0') + struct.pack('<13i', 1, 80, 8000, 0, 4, 1, -1, 160, 0, 2, 0, 0, 0)
comment = struct.pack('<I', 4) + b'Aede' + struct.pack('<I', 0)
if len(sys.argv) != 3:
    raise SystemExit('Usage: generate-conformance.py /path/to/wavpack-normalization /path/to/speex-final-packet')
out = Path(__file__).parent
subprocess.run([sys.argv[1], str(out/'float32-norm142.wv')], check=True)
with tempfile.TemporaryDirectory() as directory:
    packet_file = Path(directory)/'packets'
    subprocess.run([sys.argv[2], str(packet_file)], check=True)
    raw = packet_file.read_bytes()
packets = []
at = 0
while at < len(raw):
    size = struct.unpack_from('<I', raw, at)[0]
    packets.append(raw[at + 4:at + 4 + size])
    at += 4 + size
assert at == len(raw) and len(packets) == 2
stream = page(header, 0, 2, 0) + page(comment, 1, 0, 0)
stream += page(packets[0], 2, 0, 280) + page(packets[1], 3, 4, 400)
(out/'speex-final-partial.spx').write_bytes(stream)
print(f'bytes={len(stream)} packets={list(map(len, packets))} nominal_frames_per_packet=2 actual_speech_frames=3')
