# recording — Show one recording, its local placements, and sourced work links

recording gathers all local track placements sharing the same MusicBrainz recording identity. It shows the recorded performance’s credits and works, with each assertion’s source/trust. This separates one performance appearing on several editions from different performances sharing a title.

Use an exact recording MBID when titles collide. A local recording lacking an MBID can be opened with local: followed by the complete file path, as in the example. It is not merged with another performance merely because the title matches.

The Continue section provides copyable commands for adjacent entities. These graph pages have no CSV/JSON/M3U, paging or --output options in the current CLI; use query/export for machine-readable selections or graph data.

## Syntax and arguments

```text
aede recording <title|MusicBrainz ID>
```

One title or the relevant MusicBrainz ID. Replace placeholder IDs with those displayed by Aède.

## Options for this command

This command has no command-specific options.

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede recording MUSICBRAINZ_RECORDING_ID
aede recording "local:/path/to/music/01.flac"
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[track](track.md), [work](work.md).

Detailed existing guide: [commands.md](../commands.md).
