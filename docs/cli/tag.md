# tag — Attach or remove personal labels

tag KIND NAME LABEL[,LABEL] adds one or more personal labels. Quote the complete name so the final positional word is clearly the label list. These are personal categories, distinct from audio metadata tags such as album or title.

--remove with named labels removes only those. With no labels, --remove clears every personal label on that entity. Use a quoted name especially for a multiword title in that form. Ratings, notes and favorites remain.

query tag:LABEL targets track labels; album.tag:LABEL or artist.tag:LABEL selects those levels. Labels need no external taxonomy and can support your own curation.

Personal annotations are stored separately in user.json. They never retag or rename music. The kind can be artist, album, track, genre or label. Use the displayed catalog spelling and quote a multiword name; if the selection is ambiguous, refine it rather than guessing.

## Syntax and arguments

```text
aede tag <kind> <name> <label[,label…]>
```

Kind, quoted entity name, then comma-separated labels; labels optional only when removing all.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--remove` | Remove named personal labels, or every label if none is named. Does not clear other annotations. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede tag album "Kind of Blue" evening,jazz
aede tag album "Kind of Blue" evening --remove
aede tag album "Kind of Blue" --remove
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[query](query.md), [note](note.md).

Detailed existing guide: [annotating.md](../annotating.md).
