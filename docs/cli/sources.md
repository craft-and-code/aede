# sources — Inspect, export, import, or remove externally sourced data

sources keeps a second layer of attributed claims beside local tags. The summary counts records attached to catalog objects and those still waiting. --list displays each source record, confidence and attachment. Approximate matches do not silently become local truth.

--export writes the stored source document; --import merges a document in the same format. --template creates blank keyed records, optionally narrowed by a positional name, for carefully authored manual data. --output is meaningful with export/template, not a normal screen summary. --source narrows operations where that source is used, such as list/forget. Summary and export currently retain all sources.

--forget removes source records in the selected scope and is a real loss of fetched evidence; it never deletes tags or music. The command normally takes no positional name: open album/artist/track for one entity, or use --template NAME when authoring. Use review for trusting a proposal, credit for a precise credit correction, and backup for a complete recovery bundle.

## Syntax and arguments

```text
aede sources
```

No positional arguments except an optional name with --template.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--forget` | Remove stored data/decisions in this command’s scope. It does not delete audio or retag files. |
| `--list` | List the individual stored records or decisions instead of the normal summary/operation. |
| `--source NAME` | Filter --list/--forget, or choose the source for --template. The ordinary summary and --export currently retain all sources. |
| `--export` | Export the command’s stored document. This is not the same as CSV/JSON presentation of a listing. |
| `--template` | Create an empty manual-source document for catalog entities, optionally narrowed by the positional name. |
| `--import FILE` | Merge a previously exported document from FILE; consult format-specific collision rules below. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede sources
aede sources --list --source musicbrainz
aede sources --export --output sources.json
aede sources --template "Miles Davis" --output manual-sources.json
aede sources --import manual-sources.json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[fetch](fetch.md), [review](review.md), [credit](credit.md), [backup](backup.md).

Detailed existing guide: [sources.md](../sources.md).
