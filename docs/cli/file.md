# file — Inspect one audio file directly, without the catalog

file reads one existing audio path directly, without needing a scanned catalog. It displays container/codec, duration, technical fields and readable embedded tags. Use it to understand a file before scanning or troubleshoot a tag/parser mismatch.

This is inspection, not a full decoded-audio analysis or integrity proof. Supported tag-reading formats may differ from native playback decoding. An unreadable/corrupt/unsupported file produces a diagnostic rather than being repaired.

There are no CSV/JSON/--output options for this human-oriented page. It reads only and never retags. Use track after scanning for personal annotations, graph links and imported measurements.

## Syntax and arguments

```text
aede file <path>
```

One existing audio-file path; quote spaces.

## Options for this command

This command has no command-specific options.

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede file "/path/to/music/01.flac"
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md), [track](track.md), [check](check.md).

Detailed existing guide: [formats.md](../formats.md).
