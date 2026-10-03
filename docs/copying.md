# Copying: Taking Your CDthèque on the Road

`aede copy` writes a separate player/card/drive destination while leaving library originals untouched. It preserves the source tree relative to watched roots. For example, a file under a watched `Music` root at `Music/Artist/Album/01.flac` is copied under the destination's `Artist/Album/01.flac`; it is not automatically reorganized from tags.

```sh
aede copy /Volumes/Player --dry-run
aede copy /Volumes/Player --query "loved" --verify
aede copy /Volumes/Card --collection Road --verify
```

The destination base folder must already exist. `--query` uses the [query grammar](querying.md), and `--collection` reevaluates a saved query. Choose one; without either, the whole catalog is selected. Bare `loved` can inherit an album/artist favorite; a rating criterion must use the level where you wrote it, for example `album.rating:>=4`.

## Packing the Liner Notes and Artwork

| `--extras` | Content |
| --- | --- |
| `none` | Audio only; artwork already embedded inside an unconverted file remains inside it |
| `cover` (default) | The cover identified by the catalog for each selected release |
| `images` | Images beside the selected audio |
| `all` | Other companion files, such as logs, cue sheets and reports |

The default cover selection avoids automatically copying every large spectrogram or booklet image. The selected catalog cover is more precise than a generic image-extension filter.

## Navigating Fragile Filesystems

Destination name restrictions depend on the filesystem and mount. Aède probes the destination and can adapt forbidden characters, trailing names and reserved device names. `--safe-names` forces adaptation; `--raw-names` keeps original characters and may fail on a restrictive destination. Collision counters can still distinguish otherwise equivalent names. Those two options are incompatible.

Adapted names are listed. Files and folders share conservative matching keys that fold case, common composed/decomposed accents and punctuation. Deterministic counters distinguish collisions rather than merging source folders or selected tracks. A real transfer also tests the planned namespace in an isolated directory on the destination; aliases outside those matching rules or unsupported names are refused before changing real outputs.

`--dry-run` creates no probe or temporary output. Without an explicit name option it conservatively plans adapted names; pass `--safe-names` or `--raw-names` to use the same explicit policy for both preview and transfer.

## Ensuring It Arrives Intact

A new file is written in an exclusively created private temporary directory beside its destination, then published. Pre-existing temporary filenames cannot redirect an output. Symbolic links in destination files or subfolders are refused; choose an ordinary destination tree. Plain copies already present at the expected nonzero size are skipped without a content comparison. Converted outputs already present and nonempty are also skipped. Size/existence is a resume heuristic, not proof of identity.

Without `--replace`, a different-size plain output or an empty converted output is preserved and reported, rather than repaired silently. Publication of new files is atomic and refuses a destination created by another writer. This requires filesystem hard links; a real run checks that capability before transfers that need it. FAT/exFAT destinations usually lack it and require an intentional `--replace`, which permits replacing existing files as well as publishing new ones. An existing-only resume does not need this publication capability. These checks do not protect against a process concurrently replacing ancestor directories.

Audio sources and requested companions must be ordinary local files; stale catalog entries pointing to a FIFO or device are refused even in a preview. Encoding uses canonical local input paths, so a filename cannot be interpreted as an FFmpeg protocol. Direct conversion calls use isolated publication too and cannot truncate a source through a linked destination.

`--verify` rereads **newly written** plain files and compares their bytes with the source using bounded buffers, after the applicable flush. Skipped existing files are not reread with that flag. `--verify-existing` compares existing plain outputs with their source before skipping them. A mismatch preserves the existing destination and reports an error; `--replace --verify` explicitly refreshes it.

For converted outputs, `--verify-existing` produces a fresh temporary encode using the current recipe and compares its bytes with the existing output. A different encoder version or nondeterministic metadata can produce different bytes despite equivalent audio; a mismatch requires an explicit replacement. This mode needs FFmpeg and temporary space.

The operating system may serve the read-back from cache; it is not proof of durable media contents or a substitute for a later independent read. Copy verification is distinct from [container-integrity checking](integrity.md).

## What Aède Refuses to Do

- Create a missing base destination: an unplugged player must not become a new internal-disk folder. Aède creates needed subfolders only inside an existing destination.
- Use a destination inside, equal to, or above a watched root: both overlap directions are refused after path canonicalization, including a path reached through a symbolic link.
- Continue with an empty selection, conflicting selection/name options or invalid quality/format settings.
- Begin a transfer whose planned space requirement exceeds the available space; conversion sizes remain estimates, so the disk can still fill during work.

The space estimate counts pending writes, excludes skipped files, and includes temporary encodes needed by `--verify-existing`. Failures while enumerating requested sidecars are reported and prevent the transfer; an unreadable folder is never treated as an empty folder.

The final report counts written, already-present and failed files. Failures return an error after retaining completed work. Repeating a command can resume skipped work; `--replace` intentionally writes it again.

## Transcoding on the Way Out

```sh
aede copy /Volumes/Phone --compress opus --quality 128k
aede copy /Volumes/Phone --compress mp3 --quality V0 --query "loved"
```

Targets: `mp3`, `opus`, `aac` in M4A, `vorbis` in Ogg, `flac`, `wav`. MP3 accepts `V0`–`V9`, Vorbis `q0`–`q10` (case-insensitive), and both accept supported bitrates such as `192k`; AAC and Opus accept bitrates only. Values are validated for the selected encoder. Lossless `flac`/`wav` refuse quality settings. WAV encoding preserves known integer depth or floating-point precision; unknown or unsupported precision is refused. Files already in the target audio format remain unchanged even when their extension is an alias. A quality option without `--compress` is also refused.

Only sources identified as lossless are encoded. Already-lossy files are copied unchanged even if their codec differs from the requested target. MP3-to-Opus therefore remains a copied MP3; MP3-to-FLAC does not invent lossless quality.

Floating-point PCM can be preserved in WAV. Conversion to FLAC is refused for floating-point sources, including during preview: FLAC stores integer samples and would quantize them.

Integer sources above 24 bits require FFmpeg's 32-bit FLAC encoding support; an incapable encoder is refused. Newly encoded lossless files are checked for preserved precision, sample rate and channel count before publication, even without `--verify`.

FFmpeg must be installed for actual conversions. Conversion workers use an automatic parallel count by default, while a plain copy defaults to one worker; `--threads N` can override it. A conversion dry-run plans the work without encoding.

Metadata is carried where the destination container and FFmpeg support it. Embedded cover handling is supported by the selected MP3/AAC/FLAC paths, while WAV/Vorbis/Opus conversion has image limitations. WAV's metadata mapping is limited; do not assume every original credit/custom tag is preserved. Sidecar artwork remains selectable independently.

Before conversion, sizes are estimates. With `--verify`, Aède's own parsers check the newly encoded result's readable nonzero duration and compare it with source duration when available, using the implementation's tolerance. This catches incomplete output; it is not a full decoded sample comparison or proof of audible transparency.

### A Final Note on Data Philosophy

Converted destination files may receive tags copied by the encoder; library originals never do. An exported derivative and the source archive have different roles. Keep originals backed up separately.

For every option, argument and beginner example, use the [copy command reference](cli/copy.md). For a recovery bundle of catalog/personal data, use [backup](cli/backup.md), which does not contain the music.

## Playlists that follow the exported files

`--playlists` adds an `aede-selection.m3u8` playlist in selection order, with paths relative to the destination. If a companion already reserves that name, a deterministic suffix keeps both distinct. With `--extras all`, copied UTF-8 M3U/M3U8 companions are remapped to the adapted filenames and converted extensions. References outside the selected audio, including remote URLs, are omitted and counted. Original playlists remain untouched. A differing existing destination playlist is preserved and reported unless `--replace` is supplied.

```sh
aede copy /Volumes/Player --collection Road --compress mp3 --quality V0 --playlists --verify
aede copy /Volumes/Player --collection Road --verify-existing
```
