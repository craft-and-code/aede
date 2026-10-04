# Remote playback fixtures

`ack-window.flac` is a synthetic two-second, 44,100 Hz, mono, signed 16-bit
programme: 44,100 samples at `8000`, followed by 44,100 at `-8000`. It contains
no music, private metadata, padding or seek table. Its duration exceeds the
native PCM transport's one-second acknowledgement window. In the streaming
tests, receiving more than one second of its PCM before the final MD5 error
therefore requires an accepted client acknowledgement.

It was encoded and decoded with reference `flac 1.5.0`, passed `flac --test`,
and decoded byte-for-byte to the generated input. Python's independent PCM
MD5 is `d7b62ae6185cc7e73ad90ed8a9ec346d`, equal to STREAMINFO's MD5.
The tests alter only that digest when they need a decoded-content mismatch.
No external encoder is required to run them.

Regenerate from this directory with Python and the reference FLAC encoder:

```sh
python3 - <<'PY'
import struct
import subprocess

pcm = struct.pack('<h', 8000) * 44100 + struct.pack('<h', -8000) * 44100
subprocess.run([
    'flac', '--force', '--force-raw-format', '--endian=little', '--sign=signed',
    '--channels=1', '--bps=16', '--sample-rate=44100', '--no-padding',
    '--no-seektable', '-o', 'ack-window.flac', '-',
], input=pcm, check=True)
PY
```
