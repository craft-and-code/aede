# help — Show the command index or the page for one command

help shows the command index when no name is supplied. help COMMAND and COMMAND --help open the same command-specific page, including command aliases. These pages are bundled with the executable and work offline.

Use --version (also -v/-V) to identify the build in a bug report. --no-color turns off terminal colors and is useful when copying output. Unknown options or missing values are validated before help/version, so a mistyped option can still fail instead of disappearing silently.

This website expands those concise terminal pages with examples and operating context. Follow the options reference for syntax and scope: many familiar switches such as --json and --limit apply only to specific commands.

## Syntax and arguments

```text
aede help [command]
```

Optional canonical command name or alias; exactly one help topic.

## Options for this command

This command has no command-specific options.

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede help
aede help fetch
aede fetch --help
aede --version
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md).

Detailed existing guide: [commands.md](../commands.md).
