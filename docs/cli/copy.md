# copy — Copy a selection to a player, card, or other destination

copy prepares a separate destination, preserving the source folder tree. Without --query or --collection it selects the whole library. The destination must already exist and have no overlap with any watched root (neither inside it, equal to it nor above it); excluded subfolders inside a watched root are still covered by that guard. Aède never creates the base destination, so an unplugged drive cannot silently become a new folder on the internal disk.

Start with --dry-run. Existing plain copies with the expected nonzero size are skipped without CRC reread, even with --verify. Existing nonempty converted outputs are also skipped without full validation. --replace deliberately writes and verifies them again when --verify is supplied; size/existence alone does not prove identity. --extras cover copies the selected cover only, images all images, all all sidecars, and none only audio. Automatic probing adapts names rejected by the destination; --safe-names or --raw-names override that choice.

Conversion needs ffmpeg. Only lossless sources are encoded, while already-lossy audio is copied as it stands. --quality is refused without --compress or for lossless targets. --verify rereads ordinary copies and validates converted outputs; it cannot prove that lossy output equals the source. Errors list failed files and return failure after completed copies are kept. Insufficient space and empty selections are reported. Sources and their tags are never rewritten.

## Syntax and arguments

```text
aede copy <destination>
```

Exactly one destination folder. Selection belongs in --query or --collection, not extra positional names.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--threads N` | Number of worker threads. A positive integer fixes the count; 0 selects an automatic count. Plain copying defaults to one worker. |
| `--replace` | Write existing destination files again; this does not change watched roots. |
| `--query EXPRESSION` | Pass a relational query expression. Its projection depends on the command: save it, select tracks or retain matching albums/artists. |
| `--extras none\|cover\|images\|all` | Choose sidecar content to copy: none, cover (default), images or all. Embedded artwork is already inside copied audio. |
| `--dry-run` | Preview the work without making downloads or creating outputs. It can still validate tools and inputs. |
| `--verify` | Read back destination files after copying. For plain copies compare checksums; for conversion verify the encoded output and duration. |
| `--safe-names` | Adapt filenames to destination restrictions, even if the automatic destination probe would keep them. |
| `--raw-names` | Preserve names verbatim instead of adapting them. Refused with --safe-names; unsupported names may fail. |
| `--collection NAME` | Use a previously saved collection as the copy selection. Refused with --query. |
| `--compress FORMAT` | Use ffmpeg to encode lossless sources as mp3, opus, aac, vorbis, flac or wav on the destination. Already-lossy sources are copied unchanged. |
| `--quality SETTING` | With lossy --compress only: MP3 V0–V9, Vorbis q0–q10 or a bitrate such as 192k. Not applicable to flac/wav. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede copy "/Volumes/Player" --query "loved" --dry-run
aede copy "/Volumes/Player" --collection Road --verify
aede copy "/Volumes/Player" --query "genre:jazz" --compress mp3 --quality V0 --verify
```

## Result and errors

The plan lists tracks, sidecars, bytes and destination; adaptation of names is listed separately. --dry-run returns after this plan without writing audio. A real run reports files written, already present and failed, plus failed paths and reasons. Per-file failures return an error after completed files remain in place; rerunning can resume. A destination missing/overlapping a root, empty selection, invalid conversion setting, missing encoder or insufficient available space is refused. Skipped files were not CRC-verified; the newly encoded-file check is a readable nonzero duration with source-duration tolerance when available, not complete decoded equality. Sources remain unchanged.

## Related reading

[query](query.md), [collection](collection.md), [check](check.md).

Detailed existing guide: [copying.md](../copying.md).
