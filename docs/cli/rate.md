# rate — Give an artist, album, or track a personal rating

rate KIND NAME --stars N stores a personal rating of 1 to 5. --remove clears it. Passing both actions or an out-of-range/noninteger rating is refused.

Ratings are scoped: rating:>=4 selects track ratings; album.rating:>=4 selects tracks whose album you rated. An album rating does not silently become individual track ratings. Read the result on album/artist/track and use query to select by that level.

Personal annotations are stored separately in user.json. They never retag or rename music. The kind can be artist, album, track, genre or label. Use the displayed catalog spelling and quote a multiword name; if the selection is ambiguous, refine it rather than guessing.

## Syntax and arguments

```text
aede rate <kind> <name>
```

Entity kind then its name; supply --stars or --remove.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--remove` | Undo or remove the personal value named by this command; the scope is described below. |
| `--stars N` | Rating from 1 to 5; use --remove to clear instead. Refused with --remove. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede rate album "Kind of Blue" --stars 5
aede rate album "Kind of Blue" --remove
aede query "album.rating:>=4"
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[query](query.md), [album](album.md).

Detailed existing guide: [annotating.md](../annotating.md).
