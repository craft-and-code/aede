# playlist — Write portable playlists in album and artist folders

playlist writes one portable .m3u in each selected album folder. Paths resolve relative to the playlist’s directory, so moving the album with its playlist preserves those links. A multi-disc release gets one playlist at the common album folder, with discs in order.

By default #EXTINF lines carry titles and duration. --simple leaves paths only. --artists also writes a chronological discography playlist when albums share a real artist folder; it does not invent an artist folder or clutter a flat music root. --dry-run previews exactly those files without writing.

This command creates/updates playlist sidecars, not audio copies. Folder permissions matter. To export a temporary selection somewhere else, use query --m3u --output or collection --m3u instead. To play the result, pass its M3U/M3U8 path to play.

## Syntax and arguments

```text
aede playlist [folder…]
```

Optional catalogued music folders.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--dry-run` | Preview the work without making downloads or creating outputs. It can still validate tools and inputs. |
| `--simple` | Write plain playlist paths without #EXTINF duration/title lines. |
| `--artists` | Also create one playlist per shared artist folder when that folder can be identified safely. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede playlist "$HOME/Music/Jazz" --dry-run
aede playlist "$HOME/Music/Jazz" --artists
```

## Result and errors

The summary distinguishes playlists to write, written, already up to date and failed. --dry-run lists at most the first 20 planned paths and counts the rest without writing; unchanged playlists retain their timestamps. No selected catalog album returns an explanatory message and succeeds. Per-playlist filesystem errors are listed and counted, but the current handler still returns success after those failures: inspect Failed rather than relying on exit code alone. Playlist output is UTF-8 text with a .m3u filename; the command copies no audio.

## Related reading

[play](play.md), [query](query.md), [collection](collection.md).

Detailed existing guide: [playlists.md](../playlists.md).
