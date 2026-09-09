# Spectrograms

A spectrogram is the last arbiter when the provenance of a file is in doubt: a lossless container filled from an MP3 shows a wall at 16 kHz that no tag will ever mention.

```sh
aede spectrum                       # every track the catalog holds
aede spectrum ~/Music/Ozzy          # only what is under that folder
aede spectrum --dry-run             # say what it would draw, write nothing
aede spectrum --full                # redraw everything, even what is current
aede spectrum --size full           # FlacCompagnon's own dimensions, for comparing the two
```

**With no folder it does the whole library, and on a large one that is long.** Each picture means decoding a whole track and running an FFT over it — seconds per track, so tens of thousands of tracks is hours, however many run at once. `--dry-run` says how many would be drawn before committing to it, and naming a folder is how the work is cut down to what is actually in question. There is no penalty for stopping half way: the run picks up where it left off, since what is already drawn is left alone.

One PNG per track, in a `spectrograms/` folder beside the music, **drawn with the same ffmpeg filter and colour map as [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)** — deliberately and to the character. The two are used on the same library, and pictures that differed in gain or colour map from one tool to the other would be unreadable _as a pair_, which is the whole reason to look at two.

The frame size is a reader's choice: `--size half` (the default) draws a picture at `900x470`, a quarter of the pixels of FlacCompagnon's own `1800x940` — a spectrogram is mostly noise, which a PNG cannot compress away, so the file on disk shrinks by roughly the same quarter. A library of a few thousand tracks stays in the megabytes rather than the gigabytes this way. `--size full` draws FlacCompagnon's own dimensions exactly, for putting the two side by side. Switching `--size` does not redraw what is already there — a picture is only ever redrawn when it is missing or out of date (or with `--full`), so changing the default size on an existing library needs `aede spectrum --full` to take effect everywhere.

Several run at once — drawing a spectrogram decodes the whole file and runs an FFT over it, and no two pictures share anything. `--threads` sets how many, and means what it means on `aede scan`.

Aède does not decode: it hands the file to ffmpeg, which must be installed (`brew install ffmpeg`, `apt install ffmpeg`). It is looked for once, before the first file, so a missing install is one sentence rather than one per track.

**A second run over an unchanged library draws nothing.** A picture is redrawn only when it is missing, or when the track's modification date has moved past the picture's — both read from the disk rather than from the catalog, because the question is whether this picture was drawn from the bytes that are there _now_.
