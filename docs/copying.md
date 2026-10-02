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

Destination name restrictions depend on the filesystem and mount. Aède probes the destination and can adapt forbidden characters, trailing names and reserved device names. `--safe-names` forces adaptation; `--raw-names` preserves names verbatim and may fail on a restrictive destination. Those two options are incompatible.

Adapted names are listed. If adaptation causes collisions, deterministic counters distinguish outputs rather than letting one selected track overwrite another.

## Ensuring It Arrives Intact

A new file is written under a temporary name before publication. Plain copies already present at the expected nonzero size are skipped without a content comparison. Converted outputs already present and nonempty are also skipped. Size/existence is a resume heuristic, not proof of identity.

`--verify` rereads **newly written** plain files and compares CRC-32 with the source, after the applicable flush. Skipped existing files are not reread even with that flag. To refresh and check existing destinations deliberately, use `--replace --verify`.

The operating system may serve the read-back from cache; it is not proof of durable media contents or a substitute for a later independent read. CRC-32 detects accidental transfer corruption, not deliberate collision attacks. Copy verification is distinct from [container-integrity checking](integrity.md).

## What Aède Refuses to Do

- Create a missing base destination: an unplugged player must not become a new internal-disk folder. Aède creates needed subfolders only inside an existing destination.
- Use a destination inside, equal to, or above a watched root: both overlap directions are refused after path canonicalization, including a path reached through a symbolic link.
- Continue with an empty selection, conflicting selection/name options or invalid quality/format settings.
- Begin a transfer whose planned space requirement exceeds the available space; conversion sizes remain estimates, so the disk can still fill during work.

The final report counts written, already-present and failed files. Failures return an error after retaining completed work. Repeating a command can resume skipped work; `--replace` intentionally writes it again.

## Transcoding on the Way Out

```sh
aede copy /Volumes/Phone --compress opus --quality 128k
aede copy /Volumes/Phone --compress mp3 --quality V0 --query "loved"
```

Targets: `mp3`, `opus`, `aac` in M4A, `vorbis` in Ogg, `flac`, `wav`. `--quality` accepts MP3 `V0`–`V9`, Vorbis `q0`–`q10` or a bitrate such as `192k`, interpreted by the selected encoder. Lossless `flac`/`wav` refuse quality settings. A quality option without `--compress` is also refused.

Only sources identified as lossless are encoded. Already-lossy files are copied unchanged even if their codec differs from the requested target. MP3-to-Opus therefore remains a copied MP3; MP3-to-FLAC does not invent lossless quality.

FFmpeg must be installed for actual conversions. Conversion workers use an automatic parallel count by default, while a plain copy defaults to one worker; `--threads N` can override it. A conversion dry-run plans the work without encoding.

Metadata is carried where the destination container and FFmpeg support it. Embedded cover handling is supported by the selected MP3/AAC/FLAC paths, while WAV/Vorbis/Opus conversion has image limitations. WAV's metadata mapping is limited; do not assume every original credit/custom tag is preserved. Sidecar artwork remains selectable independently.

Before conversion, sizes are estimates. With `--verify`, Aède's own parsers check the newly encoded result's readable nonzero duration and compare it with source duration when available, using the implementation's tolerance. This catches incomplete output; it is not a full decoded sample comparison or proof of audible transparency.

### A Final Note on Data Philosophy

Converted destination files may receive tags copied by the encoder; library originals never do. An exported derivative and the source archive have different roles. Keep originals backed up separately.

For every option, argument and beginner example, use the [copy command reference](cli/copy.md). For a recovery bundle of catalog/personal data, use [backup](cli/backup.md), which does not contain the music.
