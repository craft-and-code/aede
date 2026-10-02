# collections — List the saved dynamic collections

collections lists saved dynamic queries and the track count each expression selects now. It is an overview of definitions, not a separate static-playlist store. An empty list means no definitions were saved, and the command shows how to create one.

Open a definition with collection NAME, edit it by supplying --query, or remove it with --remove on that singular command. The plural collections takes no name, pagination or export format options. It never changes a definition or copies audio.

## Syntax and arguments

```text
aede collections
```

No positional arguments.

## Options for this command

This command has no command-specific options.

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede collections
aede collection Road
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[collection](collection.md).

Detailed existing guide: [querying.md](../querying.md).
