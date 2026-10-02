# release-group — Show the album identity shared by every local edition

release-group opens the album identity shared across local editions. It lists the release/edition placements and links to their exact album pages, rather than treating two pressings as two unrelated album identities.

A MusicBrainz release-group ID identifies the common album; a release ID identifies one exact edition. Use those distinct IDs when navigating same-titled releases. The page only describes held graph data and never downloads or rewrites a tag.

The Continue section provides copyable commands for adjacent entities. These graph pages have no CSV/JSON/M3U, paging or --output options in the current CLI; use query/export for machine-readable selections or graph data.

## Syntax and arguments

```text
aede release-group <title|MusicBrainz ID>
```

One title or the relevant MusicBrainz ID. Replace placeholder IDs with those displayed by Aède.

## Options for this command

This command has no command-specific options.

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede release-group MUSICBRAINZ_RELEASE_GROUP_ID
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[album](album.md), [albums](albums.md).

Detailed existing guide: [commands.md](../commands.md).
