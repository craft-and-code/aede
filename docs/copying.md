# Copying to a player

`aede copy` is the one command that writes files, and it writes them **outside** the library. A player, an SD card, an external drive — somewhere that is not a catalog and will never be scanned.

```sh
aede copy /Volumes/Player                                   # the whole library
aede copy /Volumes/Player --query "loved rating:>=4"        # a selection
aede copy /Volumes/Card --collection wishlist --verify      # a saved query, read back
aede copy /Volumes/Player --dry-run                         # what it would do, writing nothing
```

**The selection is the grammar's**, not a set of filters of its own — the rule every listing already follows. Whatever `aede query` would have listed is what `aede copy --query` writes. With neither `--query` nor `--collection`, the whole library goes.

**The tree is kept relative to the watched folder that holds each file.** A track scanned under `~/Music` at `Ozzy Osbourne/1980 Blizzard of Ozz/01.flac` arrives at exactly that path under the destination. Inventing a layout from the tags would be a different feature — organising — and one this project has not decided it wants. A file sitting under no watched folder has no tree to keep, so it is reported rather than dropped at the top level among the ones that do.

## What travels beside the audio

| `--extras`          | What comes                                                                       |
| ------------------- | -------------------------------------------------------------------------------- |
| `none`              | Audio only. Cover art embedded in the tags travels anyway: it is inside the file |
| `cover` _(default)_ | The one cover the catalog identified for the release                             |
| `images`            | Every image in the folder                                                        |
| `all`               | Everything beside the audio: logs, cue sheets, reports                           |

The default is `cover` rather than `images` for a reason worth spelling out: **a rip folder's spectrograms and booklet scans are PNGs too.** Filtering on the extension copies exactly what you were trying to leave behind. The catalog already knows which file is the cover — the scan picked it by rank and stored it on the release — so `cover` is an exact answer where `images` can only be a guess.

## Names a player refuses

FAT32 and exFAT — which is what a card or a player almost always is — reject `? * : " < > |`, trailing dots and spaces, and the old DOS device names. A music library is full of them: `Where Is My Mind?`, `Symphony No. 5: Allegro`. Left alone, the copy fails on those files one at a time, twenty minutes into a run.

Aède asks the destination what it accepts by **writing one probe file into it**, rather than reading the filesystem's name and inferring. The empirical answer is right where the inference is wrong: a FUSE mount, an SMB share of a Windows folder or a card reader all report something no table lists. `--safe-names` and `--raw-names` force it either way.

Every adapted name is **listed, not counted** — a copy whose names quietly differ from the library is a copy nobody can compare against the original afterwards. Where two different names adapt to the same one (`Vol. 1: Live` and `Vol. 1? Live` both become `Vol. 1_ Live`), a counter keeps them apart rather than letting one overwrite the other.

## Getting it there intact

Size is checked on every file, always: it costs one metadata read and catches what actually goes wrong — a run interrupted mid-file, a disk that filled up. Each file is written under a temporary name and moved into place, so an interrupted run never leaves half a file wearing a whole one's name, and re-running skips what is already there at the right size.

`--verify` adds a full read-back and CRC-32 comparison. Two honest limits: the file is flushed to the device before being read back, but a read can still be served from the kernel's cache — this proves the bytes made it through the program and the filesystem, not that they reached the platter; and a CRC-32 detects accidental corruption, it is not meant to resist anyone deliberately producing a collision. Nothing here is a security boundary.

## What it refuses

**A destination that does not exist.** The folder is never created for you: `aede copy /Volumes/Player` with the player unplugged would otherwise create that folder on the internal disk and quietly fill it.

**A destination inside a watched folder.** The next scan would read the copies back in, the catalog would double, and `doctor` would report every album as its own duplicate.

**Not enough room**, checked before the first byte rather than discovered on the last album.

## Converting on the way out

A 64 GB card does not hold a FLAC library. `--compress` encodes as it copies:

```sh
aede copy /Volumes/Phone --compress opus --quality 128k
aede copy /Volumes/Phone --compress mp3 --quality V0 --query "loved"
```

Targets: `mp3`, `opus`, `aac` (in an `.m4a`), `vorbis` (in an `.ogg`), `flac`, `wav`. `--quality` takes `V0`…`V9` for MP3, `q0`…`q10` for Vorbis, or a bitrate like `192k`; each encoder has a sane default, and a value that parses as none of those is refused rather than quietly replaced.

`--quality` is refused on `flac` and `wav` rather than ignored, because there is nothing there to choose: a lossless format keeps every sample. `--compress wav --quality 128k` reads as a request for small files and would have produced files some eleven times larger than the number just typed — on the card this command exists to fill, that is the difference between fitting and not. The check happens before a single file is read, so being stopped costs nothing.

**Only lossless sources are encoded.** Everything else is copied exactly as it stands, and that one rule settles three cases at once:

| Source          | `--compress mp3` asks for | What happens                                                                              |
| --------------- | ------------------------- | ----------------------------------------------------------------------------------------- |
| FLAC, WAV, ALAC | MP3                       | encoded                                                                                   |
| MP3             | MP3                       | copied — re-encoding loses quality to produce the same thing                              |
| MP3             | Opus                      | copied — a second lossy pass over a first one is audible, and the file was already small  |
| MP3             | FLAC                      | copied — the result would be _larger_ and no better: lossless in name, lossy in substance |

So a mixed library converted for a phone comes out with its lossless half encoded and its lossy half untouched, which is what you wanted and never had to ask for. The report says how many of each, because a silent skip looks like lost files.

**Encoding runs several files at a time; a plain copy does not.** The two are different kinds of work and the default follows the work rather than the machine. With `--compress`, each file is an ffmpeg run that no other file waits on, so one at a time would leave most of the processor idle. A plain copy is a queue at a single device — one card, one stick, one slow drive — where several writers do not go faster but seek against each other, markedly so on cheap flash, and `--verify` reads back everything just written on the same device. `--threads` overrides it in either direction, for the person copying to an NVMe or encoding on a laptop they still want to use.

**ffmpeg does the encoding, and it is an external program — not a dependency.** Nothing is linked or vendored; a checkout without ffmpeg builds fine and every other command works. `--compress` looks for it once, before the first byte is written, and says how to install it if it is missing. This is how beets drives its `convert` plugin, and for the same reason.

```
$ aede copy /Volumes/Phone --compress mp3
Error: --compress needs ffmpeg, and it was not found.
  macOS          brew install ffmpeg
  Debian/Ubuntu  sudo apt install ffmpeg
```

**Metadata follows**: `-map_metadata` carries the tags across and the embedded cover is copied where the container holds one. Neither is perfect — no two tag formats hold quite the same fields — but arriving on a player with no artist and no title is not a trade anyone would accept.

Sizes shown before a conversion are **estimates**, and labelled as such: what an encoder produces is not known until it has produced it, and answering "unknown" to "will this fit on my card" would be answering the wrong question.

`--verify` cannot compare checksums here — the bytes differ by construction, which is the point. It instead reads the result back **with Aède's own parsers** and checks that it holds audio of the right length, which catches the failure that actually happens: an encode cut short by a full disk or a killed process. A verification that asked ffmpeg whether ffmpeg had done its job would not be one.

Note that writing tags into a _derived copy_ is not the same act as rewriting the tags of your library, which this project refuses to do. That refusal protects **your** files, whose modification date, integrity verdict and scan state all depend on not being touched; a file that did not exist a second ago has none of those. The distinction is deliberate, and recorded as such.
