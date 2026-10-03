# export — Export the catalog as JSON or a table

export describes the whole catalog and takes no entity name/filter. The default is structural catalog JSON. --json makes that choice explicit. --csv instead produces one row per album; --tracks requires --csv and switches to one row per track.

--graph produces a versioned aede-graph JSON bundle joining catalog, conclusions, sources, personal information, decisions and materialized attributed edges. It is mutually exclusive with CSV and retains provenance. This is useful to inspect relationships outside Aède, but backup remains the intended restore format.

--output writes directly to a chosen file, and --separator affects CSV only. Exported data can contain absolute paths, prose or personal notes, so review it before sharing. To export a filtered selection instead, use albums/query/collection and their supported output formats.

## Syntax and arguments

```text
aede export
```

No positional arguments; choose output using options.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--csv` | Write a CSV table of this command’s result, suitable for a spreadsheet; mutually exclusive with other output formats. |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--graph` | Export catalog, conclusions/source evidence, review decisions, personal data and attributed graph edges together. Refused with CSV. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--separator ";" / --separator tab` | With --csv only: use comma by default, a one-character separator such as ;, or tab for tab-separated text. Quote ; in a shell. |
| `--tracks` | With export --csv only, write one row per track instead of one per album. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede export --output catalog.json
aede export --csv --tracks --output tracks.csv
aede export --graph --output graph.json
```

## Result and errors

Choose a report path outside the active stores. Exports refuse active data/lock paths, existing audio, symbolic links and non-regular targets before mutation, and replace ordinary report files atomically; see the shared [output rules](options.md).

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[backup](backup.md), [query](query.md), [relations](relations.md).

Detailed existing guide: [commands.md](../commands.md).
