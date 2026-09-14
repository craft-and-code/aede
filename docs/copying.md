# Copying: Taking Your CDthèque on the Road

`aede copy` is the single command in this entire suite that writes audio files, and it does so strictly **outside** the sanctuary of your library. A portable player, a micro-SD card, an external drive in your car — these are destinations for your music, not catalogs to be scanned.

```sh
aede copy /Volumes/Player                                   # the whole library
aede copy /Volumes/Player --query "loved rating:>=4"        # a selection
aede copy /Volumes/Card --collection wishlist --verify      # a saved query, read back
aede copy /Volumes/Player --dry-run                         # what it would do, writing nothing
```

**The selection uses the exact same grammar.** You don't have to learn a new filter system to export your music. Whatever `aede query` would have lovingly curated on your screen is exactly what `aede copy --query` packs for the trip. If you omit `--query` or `--collection`, your entire archive hits the road.

**Your folder structure is honored, never reinvented.** A track carefully filed in your vault at `~/Music/Ozzy Osbourne/1980 Blizzard of Ozz/01.flac` will arrive at exactly that path on the destination device. Aède respects your curation; inventing a new layout from tags on the fly is a completely different feature (auto-organizing), and one that violates the principle of keeping your files exactly where you placed them. Tracks living outside any watched folder have no inherent tree to preserve, so they are cleanly deposited at the root level alongside the structured ones.

## Packing the Liner Notes and Artwork

When you take an album off the shelf, the artwork comes with it. Aède lets you decide exactly how much of that visual history travels alongside the audio.

| `--extras`          | What comes                                                                       |
| ------------------- | -------------------------------------------------------------------------------- |
| `none`              | Audio only. Cover art embedded in the tags travels anyway: it is inside the file |
| `cover` _(default)_ | The one cover the catalog identified for the release                             |
| `images`            | Every image in the folder                                                        |
| `all`               | Everything beside the audio: logs, cue sheets, reports                           |

The default is `cover` rather than `images` for a very specific archivist reason: **a pristine rip folder often contains spectrograms, log files, and heavy high-res booklet scans saved as PNGs.** If Aède just blindly copied by file extension, it would drag all those heavy archival materials onto your portable device. Because the catalog already knows which specific image is the true "front cover" (assigned by rank during the scan), `cover` is a precise, deliberate choice, whereas `images` is just a blind scoop.

## Navigating Fragile Filesystems

FAT32 and exFAT — the archaic formats nearly every SD card and portable player relies on — outright reject characters like `? * : " < > |`, trailing dots, spaces, and legacy DOS device names. A rich music library is full of these: _Where Is My Mind?_, _Symphony No. 5: Allegro_. Left unchecked, a mass copy would violently fail on these files twenty minutes into the transfer.

Instead of guessing based on a fixed table of rules, Aède asks the destination exactly what it accepts by **writing a single, invisible probe file into it**. This empirical test succeeds exactly where blind inference fails (like on FUSE mounts, SMB shares, or quirky card readers). You can also use `--safe-names` and `--raw-names` to force the behavior manually.

Every adapted name is **listed, not just counted.** If a filename has to change to survive the journey, Aède tells you. A copy whose names quietly mutate is a copy you can never reliably compare against the original archive later. If two different tracks are forced into the same adapted filename (`Vol. 1: Live` and `Vol. 1? Live` both collapsing to `Vol. 1_ Live`), Aède intelligently counters them apart rather than letting one tragically overwrite the other.

## Ensuring It Arrives Intact

Peace of mind is paramount. File size is checked on every single transfer. This costs a mere metadata read but catches the real-world disasters: a cable pulled mid-transfer, or a disk silently running out of space. Each file is written under a temporary name and only moved into its final place upon completion. An interrupted run never leaves half a song masquerading as a whole one, and re-running the command simply skips what is already safely there.

`--verify` adds a full read-back and CRC-32 integrity comparison. We are honest about the limits here: the file is flushed to the device before being read, but modern operating systems might still serve that read from the kernel's RAM cache. This proves the bytes safely cleared the program and the filesystem logic, though not necessarily that they magnetized the physical platter. CRC-32 is perfect for catching accidental corruption during transit, not for cryptographically securing a border.

## What Aède Refuses to Do

**Write to a destination that does not exist.** Aède will never create the base directory for you. If you type `aede copy /Volumes/Player` but forgot to plug the player in, Aède stops. Otherwise, it would quietly create a "Player" folder on your internal hard drive and fill it until your computer crashed.

**Copy into a watched folder.** Dropping copies _inside_ your library's sanctuary means the next scan would read them all back in. Your catalog would double in size, and the `doctor` command would rightfully panic, reporting every single album as a duplicate.

**Start a copy that won't fit.** Space is checked before the very first byte moves, saving you from discovering a "disk full" error two hours into transferring your discography.

## Transcoding on the Way Out

A 64 GB micro-SD card cannot hold a sprawling FLAC CDthèque. `--compress` seamlessly encodes the audio as it leaves the library:

```sh
aede copy /Volumes/Phone --compress opus --quality 128k
aede copy /Volumes/Phone --compress mp3 --quality V0 --query "loved"
```

Supported targets: `mp3`, `opus`, `aac` (in an `.m4a`), `vorbis` (in an `.ogg`), `flac`, `wav`.
`--quality` takes `V0`…`V9` for MP3, `q0`…`q10` for Vorbis, or a strict bitrate like `192k`. Each encoder uses a sane, audiophile-approved default. If you type a value that doesn't make sense, Aède refuses it rather than quietly making a terrible sounding guess.

`--quality` is strictly refused on `flac` and `wav` because a lossless format keeps _every_ sample. Asking for `--compress wav --quality 128k` to get smaller files would ironically produce files eleven times larger than the bitrate you requested! The check happens instantly, stopping a mistake that would flood your portable device.

**Only lossless files are encoded.** Everything else is respectfully copied as-is. This single, elegant rule handles three massive headaches perfectly:

| Source          | `--compress mp3` asks for | What happens                                                                              |
| --------------- | ------------------------- | ----------------------------------------------------------------------------------------- |
| FLAC, WAV, ALAC | MP3                       | encoded                                                                                   |
| MP3             | MP3                       | copied — re-encoding loses quality to produce the same thing                              |
| MP3             | Opus                      | copied — a second lossy pass over a first one is audible, and the file was already small  |
| MP3             | FLAC                      | copied — the result would be _larger_ and no better: lossless in name, lossy in substance |

If your library is a mix of pristine FLACs and old MP3s, converting the whole thing for a phone results in the lossless half being carefully encoded, while the lossy half is copied untouched. It is exactly what you wanted, without ever having to script it yourself. The final report tells you exactly how many of each occurred, so a skipped encoding never looks like a missing file.

**Encoding uses every core; copying queues up.** When `--compress` is active, every file triggers an independent ffmpeg run. Doing this sequentially would leave your modern processor tragically idle. Conversely, a plain copy acts as a strict queue because writing to a single SD card with multiple threads just causes the write head to seek violently, drastically slowing down the transfer. `--threads` allows you to override this logic if you are copying to a blazing fast NVMe drive.

**FFmpeg is the engine, but it is an external tool.** Aède does not vendor or link heavy media libraries. If you build Aède without ffmpeg, the core program and every other command works flawlessly. `--compress` simply checks for it once before starting, and if it's missing, it tells you exactly how to get it:

```
$ aede copy /Volumes/Phone --compress mp3
Error: --compress needs ffmpeg, and it was not found.
  macOS          brew install ffmpeg
  Debian/Ubuntu  sudo apt install ffmpeg
```

**Your metadata survives the trip:** `-map_metadata` ensures your tags cross the threshold safely. For `mp3`, `aac`, and `flac`, the embedded cover art is successfully injected into the new container. However, `wav`, `vorbis`, and `opus` cannot accept embedded images via ffmpeg. That artwork is dropped by the encoder's limitations, not by Aède's lack of care.

Similarly, metadata travels cleanly everywhere except into `wav`, which relies on the archaic RIFF INFO chunk. That legacy format only understands a rigid handful of fields (title, artist, album, genre, date, track). Composer, publisher, and album artist simply cannot survive the journey into a WAV file. It is not perfect, but arriving on a portable player with an "Unknown Artist" and "Unknown Title" is a tragedy no archivist would accept. Aède does its absolute best with the formats it is handed.

Sizes shown before a conversion are honest **estimates**. An encoder's final file size isn't known until the audio is processed. Answering "I don't know" when you ask "Will this fit on my card?" is unhelpful, so Aède provides the best mathematical guess possible.

When transcoding, `--verify` cannot simply compare file checksums—the bytes are entirely different by design! Instead, it reads the resulting file back **using Aède's own internal parsers** to ensure it holds valid audio of the exact correct length. This catches the real-world failures: an encode cut violently short by a full disk. Asking ffmpeg if ffmpeg did a good job is not a real verification.

### A Final Note on Data Philosophy

Notice that Aède _does_ write metadata tags into these exported files. This is fundamentally different from modifying the tags inside your master library, which this project strictly refuses to do.

That refusal is the ultimate protection for your CDthèque. The modification date, the integrity hash, and the scan state of your original files all depend on them remaining completely untouched by Aède. But an exported file on an SD card? That file didn't exist a second ago. It is a derivative, a disposable copy meant for the road. Injecting tags there hurts nothing and helps everything. The distinction is highly deliberate, and your archive remains pristine.
