# relation — Inspect, annotate, or label one graph relationship

relation opens the exact edge ID copied from relations and shows its endpoints and provenance. --text writes a personal note; --tag adds comma-separated labels. These annotations live in user.json and never alter the endpoints or the sourced assertion.

Source metadata and personal context are displayed literally, including terminal control characters. Storage and JSON graph exports preserve the original values.

--remove alone removes the whole personal annotation. --tag LABEL --remove removes only those labels. Combining a new note with a removal is refused. If the graph edge has disappeared, its retained annotation ID can still be used for removal, so personal data is not stranded.

An ID prefix matching several current edges is ambiguous and refused for removal as well as inspection. An ambiguous live selection is never treated as a disappeared edge.

Credit notes and labels remain attached when the same exact MusicBrainz artist changes from a source-only endpoint to a local catalog artist. This applies to listing, filtering, inspection, edits, removal, graph exports and orphan diagnostics; the graph IDs and saved annotation references are not rewritten. An exact owner/reference match takes precedence. Only a unique catalog-proven equivalent credit can be used as a fallback, with the same role, provenance and source relationship ID. Ambiguous historical annotations are retained and refused for modification rather than merged. A new credit annotation uses a known artist MBID only when that reference resolves uniquely to the selected local artist; otherwise it retains the selected reference. A uniquely resolved MBID remains usable if that artist later leaves the local tags. A legacy name-only reference without current identity evidence is not guessed from a homonym.

Previously saved ambiguous MBID-only references are retained rather than reassigned automatically: they no longer establish which local artist was selected. A waiting annotation can still be removed through its saved relationship ID.

This is the right place for “verify against booklet” or “interesting collaboration”. It is not a graph-editing command: use credit to exclude or correct a sourced credit and review to decide whether a proposed identity is trusted.

## Syntax and arguments

```text
aede relation <ID>
```

Exactly one stable relationship ID. Replace RELATION_ID.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--remove` | Remove the whole personal edge annotation, or only --tag labels when supplied; never delete the edge. |
| `--text TEXT` | Write the personal note text. Names with spaces are consumed until the next option. |
| `--tag LABEL[,LABEL]` | On notes/relations, filter a personal label; on relation, add comma-separated labels, or remove them with --remove. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede relation RELATION_ID
aede relation RELATION_ID --text "Verify against booklet" --tag dubious,booklet
aede relation RELATION_ID --tag dubious --remove
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[relations](relations.md), [credit](credit.md), [review](review.md).

Detailed existing guide: [commands.md](../commands.md).
