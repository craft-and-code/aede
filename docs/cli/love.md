# love — Mark an artist, album, or track as a favourite

love KIND NAME marks an entity as a favorite. --remove clears that flag while retaining ratings, notes and labels. Repeating the same flag is harmless.

Bare loved queries inherit an album/artist favorite to its tracks, while track.loved, album.loved and artist.loved ask for the exact level. This does not physically write a favorite to every track. favourites lists marked entities; query "loved" selects playable tracks.

Personal annotations are stored separately in user.json. They never retag or rename music. The kind can be artist, album, track, genre or label. Use the displayed catalog spelling and quote a multiword name; if the selection is ambiguous, refine it rather than guessing.

## Syntax and arguments

```text
aede love <kind> <name>
```

Entity kind followed by its catalog name.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--remove` | Undo or remove the personal value named by this command; the scope is described below. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede love album "Kind of Blue"
aede love artist "Miles Davis" --remove
aede query "loved played:0"
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[favourites](favourites.md), [query](query.md).

Detailed existing guide: [annotating.md](../annotating.md).
