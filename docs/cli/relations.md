# relations — List local and sourced graph relationships with stable IDs

relations is the inventory of directed graph edges: local tag relationships and external/manual assertions. Each row keeps its stable relationship ID, kind, endpoints, provenance and trust; a list of people alone would lose this context.

An optional name matches either endpoint and relationship kind. --source narrows provenance such as musicbrainz; --tag narrows your personal edge labels. --json produces structured edges and --output requires that JSON form. Pagination applies to the resulting list.

Open one ID with relation, then annotate it without changing the edge. For a sourced-credit correction use credit --exclude rather than relation --remove: the latter removes your annotation only. Disappearing edges do not erase your personal annotations; doctor reports orphaned annotations, which can still be removed by ID.

## Syntax and arguments

```text
aede relations [name]
```

Optional name text; a relationship ID belongs to relation.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--output FILE / -o FILE` | Write structured edges with --json; refused for the ordinary human page. |
| `--source NAME` | Filter by the stored source/tool name. Use the names printed by the corresponding listing. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--tag LABEL[,LABEL]` | On notes/relations, filter a personal label; on relation, add comma-separated labels, or remove them with --remove. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede relations "Miles Davis"
aede relations --source musicbrainz --all
aede relations --tag dubious --json --output relations.json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[relation](relation.md), [credit](credit.md), [doctor](doctor.md).

Detailed existing guide: [commands.md](../commands.md).
