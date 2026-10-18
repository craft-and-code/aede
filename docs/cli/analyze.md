# analyze — Analyze albums with FlacCompagnon and optionally save one JSON report per album

analyze runs [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)’s Rust analysis engine on catalogued albums. It reports acoustic measurements and stores attributed results for track details and playback loudness decisions. Results are saved in Aède even without report-writing options.

Unchanged tracks with valid reports are reused. --force measures selected albums again. --json creates report files in album folders; --json-layout artist instead saves in the parent artist folder. A pre-existing report that cannot be safely reused/replaced can require --force. Report creation requires write permission beside music; analysis never modifies audio tags.

--show-results prints track measurements. --threads controls album-analysis workers. Imported artist reports can cover several albums; scan the report’s folder or import it to attach results. Newer dated results win for overlapping files. Deleting a report does not erase measurements already imported; changing audio makes them stale. Some album analyses/report writes may succeed before another fails: the command retains completed results and returns an error summary.

Reused measurements keep their original dates, including in rewritten JSON reports and on later scans. Each Aède-written entry carries an additive exact `aede_result_at_ns` decimal string; reports without it use their file modification time. Human progress/results escape terminal control characters without changing retained measurements or JSON exports. See the [report contract](../imported-analyses.md#re-running-an-analysis-and-keeping-its-results).

Reusing a result preserves its complete source entry, including unknown nested measurements. Reports explicitly limited to a partial `analyses_run` selection are analyzed again to fulfill this command's complete measurement request. Older reports without coverage metadata retain the upstream complete-report convention; importing a partial report remains supported.

If a file selected for cache reuse changes or disappears while other files are decoded, the command reports that path and finishes with an error. Completed measurements remain stored in Aède, but the affected album report is not created or replaced with an incomplete subset. Scan the changed audio and retry. A newly decoded file is emitted once; an older cache that becomes compatible again during decoding cannot override its result or measurement date.

Before decoding, report-writing requests check all resolvable destinations and refuse the reserved catalog, conclusions, sources, personal, accounts and parental JSON names in this installation's data directory, even with `--force`. This includes parent-directory aliases and case variants. A report layout must not replace Aède's own stores; choose another layout or data directory if their paths collide. Ordinary report names in the same directory remain supported. An unavailable album folder does not prevent other albums from being analyzed; its failure is reported and the command ultimately fails while retaining completed work. Each report destination is checked again before publication, without recreating an unavailable album directory.

## Syntax and arguments

```text
aede analyze [folder…] [--json | --json-layout album|artist] [--force] [--show-results] [--threads N]
```

Zero or more catalogued album/artist folders. No argument considers catalogued albums.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--json / -j` | Save JSON report files beside the analyzed albums. It does not produce a machine-only stdout stream. |
| `--threads N` | Number of worker threads. A positive integer fixes the count; 0 selects an automatic count. Plain copying defaults to one worker. |
| `--force` | Reanalyze selected albums even when measurements are reusable; allow replacing selected analysis reports. |
| `--show-results` | Print per-track analysis measurements in addition to the progress summary. |
| `--json-layout album\|artist` | Save analysis JSON reports in each album folder (album) or the parent artist folder (artist). Enables saving even without --json. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede analyze "$HOME/Music/Jazz"
aede analyze "$HOME/Music/Jazz" --show-results
aede analyze "$HOME/Music/Jazz" --json-layout album --threads 2
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[import](import.md), [track](track.md), [play](play.md).

Detailed existing guide: [imported-analyses.md](../imported-analyses.md).
