# work — Show one composition and the recordings that realize it

work opens a composition identity and the recordings that realize it. It shows sourced composers, lyricists, writers and arrangers, keeping them distinct from performers of one recording. Explicit MusicBrainz part-of-work relationships allow parent/movement navigation.

A fetched source-only work is labeled as external evidence. A shared WORK title tag does not create a canonical shared identity. query work:PARENT_ID can select local recordings of identified parts; local work/movement text can also be queried without inferring relationships.

The Continue section provides copyable commands for adjacent entities. These graph pages have no CSV/JSON/M3U, paging or --output options in the current CLI; use query/export for machine-readable selections or graph data.

## Syntax and arguments

```text
aede work <title|MusicBrainz ID>
```

One title or the relevant MusicBrainz ID. Replace placeholder IDs with those displayed by Aède.

## Options for this command

This command has no command-specific options.

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede work MUSICBRAINZ_WORK_ID
aede query "work:MUSICBRAINZ_WORK_ID"
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[recording](recording.md), [query](query.md).

Detailed existing guide: [commands.md](../commands.md).
