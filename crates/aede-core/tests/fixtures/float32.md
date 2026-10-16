# Floating-point WavPack fixture

`float32.wv` is a real 568-byte WavPack 5.9.0 lossless encoding of 80 mono IEEE 32-bit floating-point samples at 8,000 Hz. Small nonzero samples exercise precision that an integer WAV/FLAC conversion cannot preserve. The selected file has no correction-file dependency.

Generate its source and encode it with the official WavPack encoder:

```sh
python3 - <<'PY'
import struct

values = [0.1, 1e-10, -1e-10, 0.25, -0.25] * 16
samples = b''.join(struct.pack('<f', value) for value in values)
fmt = struct.pack('<HHIIHH', 3, 1, 8000, 32000, 4, 32)
body = b'WAVE' + b'fmt ' + struct.pack('<I', len(fmt)) + fmt
body += b'data' + struct.pack('<I', len(samples)) + samples
with open('float32-source.wav', 'wb') as output:
    output.write(b'RIFF' + struct.pack('<I', len(body)) + body)
PY
wavpack -q -m -hh float32-source.wav -o float32.wv
wvunpack -ss float32.wv
ffmpeg -v error -i float32.wv -f f32le - | md5
```

The original and decoded floating-point PCM MD5 is `4990b10c7ac9d72de9999b20c4aafecb`. Forcing `pcm_s32le` WAV and decoding back to `f32le` instead yields `69ed056b5521d7562b973aaa47e77705`: the small nonzero samples become zero. These measurements establish sample changes for this fixture, not every floating-point signal.

WavPack's official `WavPack5FileFormat.pdf` identifies the first block containing audio as the source of the stream format; preceding blocks may contain metadata only. Its `wavpack.h` defines `FLOAT_DATA` (`0x80`) as IEEE 32-bit floating-point data. Aède examines at most 64 block headers without loading their bodies. Floating-point or unresolved representation leaves integer PCM precision unknown, so existing lossless-conversion checks refuse rather than quantize it while retaining the known codec and losslessness. This does not add floating-point WavPack conversion support.

The regression also reconstructs metadata-only prefixes before this real stream and the existing integer `track.wv` control. One prefix reuses the float fixture's real 56-byte final metadata block; the other prefixes use synthetic empty 32-byte non-audio headers. Official `wvunpack` and FFmpeg accept the one-block prefixed streams; decoded PCM hashes match the corresponding originals (`4990b10c7ac9d72de9999b20c4aafecb` for float, `b48e1dd0c193fdd40c05f288d60157a3` for integer). These reconstructed prefix files are decoder-acceptance controls, not ordinary encoder output. The integer control retains its known precision after one prefix block; the 65-block synthetic limit probe instead leaves its precision unknown conservatively. These cases do not add another stored audio fixture.
