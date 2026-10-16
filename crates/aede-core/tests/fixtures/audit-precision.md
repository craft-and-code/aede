# FLAC precision boundary fixtures

These four real Xiph FLAC 1.5.0 encodings use 24-bit signed stereo PCM at 8,000 Hz and 64-sample blocks. They contain significant original low bits that are not established by encoded wasted-bit counters alone:

| Fixture | Frames | Bytes | Significant low-bit case | Source and decoded s24le PCM MD5 |
| --- | ---: | ---: | --- | --- |
| `audit-mid-side-lsb24.flac` | 1 | 355 | Opposed channels with mid/side rounding | `ce991ee0b4bdd204dbea3ed0e8320ab5` |
| `audit-constant-lsb24.flac` | 1 | 229 | One independent channel is constant one | `86a3faa4f6fcee1385e0defe13e69b04` |
| `audit-dc-then-padded24.flac` | 2 | 372 | Nonzero constant frame before genuinely padded samples | `180758843fd603a2eef6f1d3e67706ba` |
| `audit-padded-then-dc24.flac` | 2 | 372 | The same frame types in reverse order | `ecca68769abd8228cda1e65ac89d8c0d` |

Before the correction, Aède reports 23 effective bits for the mid/side case and 16 for the other three, although their original PCM includes odd sample values. All four pass `flac --test` and Aède's existing frame-CRC check. FFmpeg decoding to `s24le` reproduces the source PCM hashes above. The ordinary existing `audit-padded24.flac` remains the positive control: its effective depth is still 16.

Generate the sources using only Python's standard library, then encode with Xiph FLAC 1.5.0:

```sh
python3 - <<'PY'
import wave


def random_words():
    state = 9181
    while True:
        state = (1664525 * state + 1013904223) & 0xffffffff
        yield state


def write(name, pairs):
    raw = b''.join(value.to_bytes(3, 'little', signed=True)
                   for pair in pairs for value in pair)
    with wave.open(name + '.wav', 'wb') as output:
        output.setparams((2, 3, 8000, len(pairs), 'NONE', 'not compressed'))
        output.writeframes(raw)


words = random_words()
mid_side = []
for _ in range(64):
    left = (next(words) >> 9) - (1 << 22)
    right = -left + 4 * ((next(words) >> 24) - 128)
    mid_side.append((left, right))
write('audit-mid-side-lsb24', mid_side)
words = random_words()
write('audit-constant-lsb24',
      [(1, ((next(words) >> 16) - 32768) << 8) for _ in range(64)])
words = random_words()
padded = [(((next(words) >> 16) - 32768) << 8,
           ((next(words) >> 16) - 32768) << 8) for _ in range(64)]
dc = [(1, 1)] * 64
write('audit-dc-then-padded24', dc + padded)
write('audit-padded-then-dc24', padded + dc)
PY
flac -s -f -0 -m -b64 --no-padding --no-seektable audit-mid-side-lsb24.wav
flac -s -f -0 --no-mid-side -b64 --no-padding --no-seektable audit-constant-lsb24.wav
flac -s -f -0 -m -b64 --no-padding --no-seektable audit-dc-then-padded24.wav
flac -s -f -0 -m -b64 --no-padding --no-seektable audit-padded-then-dc24.wav
flac -t audit-mid-side-lsb24.flac audit-constant-lsb24.flac audit-dc-then-padded24.flac audit-padded-then-dc24.flac
ffmpeg -v error -i audit-mid-side-lsb24.flac -f s24le - | md5
```

Only the four tiny FLACs are stored. No user audio is used or modified. The last command can be repeated for each fixture.

The interpretation follows [RFC 9639 section 4.2](https://www.rfc-editor.org/rfc/rfc9639.html#section-4.2) and [section 9.2.2](https://www.rfc-editor.org/rfc/rfc9639.html#section-9.2.2): wasted bits are restored before stereo decorrelation, and mid/side reconstruction restores the side channel's parity bit. Equal minimum wasted-bit counters do not prove the original channels' parity sample by sample. A nonzero constant subframe can also carry meaningful low bits even when its counter is zero.

The public FLAC audit API therefore keeps the structural candidate but confirms it with the existing pinned integer decoder, one already-read frame at a time, with STREAMINFO-bounded output below 2 MiB. All nonzero frames can constrain that candidate, including whole constant frames before or after it; true digital zero is neutral. A malformed, contradictory or unsupported decoder input withdraws the padding claim. This is a conservative precision check for the examined portion, not a decoded-integrity, provenance or full-file claim when audit limits truncate the walk. It does not change FlacCompagnon analyses or playback. The separate CRC reader still performs no decoding.
