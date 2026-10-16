# Hybrid lossy WavPack fixture

`hybrid-lossy.wv` is a real 520-byte WavPack 5.9.0 encoding of 512 stereo frames at 44,100 Hz with signed 16-bit samples. It uses hybrid lossy mode without a correction file. The metadata regression distinguishes its stream properties from the existing lossless `track.wv` control.

Generate the source and encode it with the official WavPack command-line encoder:

```sh
python3 - <<'PY'
import struct
import wave

state = 319
samples = []
for _ in range(1024):
    state = (1664525 * state + 1013904223) & 0xffffffff
    samples.append((state >> 16) - 32768)

with wave.open('hybrid-source.wav', 'wb') as output:
    output.setparams((2, 2, 44100, 0, 'NONE', 'not compressed'))
    output.writeframes(struct.pack('<1024h', *samples))
PY
wavpack -q -m -b2 hybrid-source.wav -o hybrid-lossy.wv
wvunpack -ss hybrid-lossy.wv
ffmpeg -v error -i hybrid-lossy.wv -f s16le - | md5
```

The encoder stores the original PCM MD5 `ab8a3120a0fd6b3601edaec456cc159a`. Decoding the `.wv` alone yields `67656bdbfdc074350f452b13f563deed`, and `wvunpack -ss` identifies its modality as `hybrid lossy, dns`. These distinct checksums establish that this fixture really discards source information; a hybrid flag alone would not establish that for every possible signal.

WavPack's [official documentation](https://www.wavpack.com/wavpack_doc.html) describes the lossy `.wv` and optional `.wvc` correction file. Aède reads properties of the selected file and does not claim correction-file support from this fixture.
