# extract — Write artwork already embedded in audio files beside the music

extract writes artwork already embedded in supported audio tags beside the music. It is local-only and does not ask Cover Art Archive or Fanart.tv. Run it before fetch --covers when your files already contain a good front cover.

By default it extracts the selected front cover. --images also keeps other embedded images such as back/booklet/disc in artwork/. --dry-run lists what would be extracted without writing or creating the writer lock. Existing local images are not overwritten. Extraction groups the catalog by folder once and reads the first file carrying artwork in each folder; it preserves catalog order and does not reread every track to find that source.

The command needs write access to the album/sidecar directory but only reads audio. New image publication requires filesystem support for hard links; an unsupported filesystem refuses publication rather than risking replacement. JPEG and static PNG pixels are decoded before publication using the same validation as [fetch](fetch.md): complete container endings, 32 MiB input, 8192 pixels per axis and 16 million pixels. Undecodable images and animated PNG are refused before creating output. No external image helper is required. Files with no embedded artwork simply provide no image; this does not mean the album cannot receive downloaded art later. Rescan if you need the catalog’s artwork choice refreshed after extraction. artwork is an exact command alias.

Alias: `aede artwork`. Options and behavior are identical.

## Syntax and arguments

```text
aede extract [folder…]
```

Optional folders containing catalogued music.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--images` | Keep extra embedded/downloaded artwork such as back covers and booklets in artwork/. |
| `--dry-run` | Preview the work without making downloads or creating outputs. It can still validate tools and inputs. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede extract "$HOME/Music/Jazz" --dry-run
aede extract "$HOME/Music/Jazz" --images
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[fetch](fetch.md), [scan](scan.md).

Detailed existing guide: [library.md](../library.md).
