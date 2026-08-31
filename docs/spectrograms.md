# Spectrograms

A spectrogram is the last arbiter when the provenance of a file is in doubt: a lossless container filled from an MP3 shows a wall at 16 kHz that no tag will ever mention.

```sh
aede spectrum                       # every track the catalog holds
aede spectrum ~/Music/Ozzy          # only what is under that folder
aede spectrum --dry-run             # say what it would draw, write nothing
aede spectrum --full                # redraw everything, even what is current
```

**With no folder it does the whole library, and on a large one that is long.** Each picture means decoding a whole track and running an FFT over it — seconds per track, so tens of thousands of tracks is hours, however many run at once. `--dry-run` says how many would be drawn before committing to it, and naming a folder is how the work is cut down to what is actually in question. There is no penalty for stopping half way: the run picks up where it left off, since what is already drawn is left alone.

One PNG per track, in a `spectrograms/` folder beside the music, **drawn with the same ffmpeg filter, size, gain and colour map as [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)** — deliberately and to the character. The two are used on the same library, and pictures that differed in scale or gain from one tool to the other would be unreadable _as a pair_, which is the whole reason to look at two. The folder name is the one thing that does not match: FlacCompagnon writes `spectres`, and a French word in an otherwise English program is a seam nobody would guess at. Matching a picture matters; matching a folder name does not.

Several run at once — drawing a spectrogram decodes the whole file and runs an FFT over it, and no two pictures share anything. `--threads` sets how many, and means what it means on `aede scan`.

Aède does not decode: it hands the file to ffmpeg, which must be installed (`brew install ffmpeg`, `apt install ffmpeg`). It is looked for once, before the first file, so a missing install is one sentence rather than one per track.

**A second run over an unchanged library draws nothing.** A picture is redrawn only when it is missing, or when the track's modification date has moved past the picture's — both read from the disk rather than from the catalog, because the question is whether this picture was drawn from the bytes that are there _now_.

A caption across the top says what the file claims to be — sample rate, depth or bitrate, channels, codec, Nyquist — so that a picture kept on its own still answers "at what sample rate?". It is drawn with ffmpeg's `drawtext`, which needs a font; where there is none the picture is drawn without it rather than not at all. The caption is built from values read out of the file, so it is restricted to a character set that cannot escape the filter expression: a file declaring a codec of `x'a,b` would otherwise inject into the command ffmpeg is handed.
