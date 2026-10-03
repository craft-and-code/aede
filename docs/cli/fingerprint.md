# fingerprint — Compute acoustic fingerprints without using the network

fingerprint decodes local audio to compute a Chromaprint acoustic fingerprint. It does not contact AcoustID. That separate lookup is fetch --identify, allowing a network retry without decoding the same audio again.

With no scope, the default focuses on files lacking title/artist identification, avoiding a full-library decode merely to confirm good tags. Names or folders select explicitly; --full includes already identified/fingerprinted files. --list shows stored fingerprints; --dry-run previews without decoding, but the required helper is still validated.

Inputs must resolve to regular local files before the helper runs; device files, pipes and protocol addresses are refused. Missing, subsecond or out-of-range catalog durations cannot produce a lookup. Human-readable paths and fingerprints display terminal controls literally; the stored and exported values keep their original text. A preview creates neither output files nor a data writer lock.

The helper can be fpcalc or an ffmpeg build supporting Chromaprint; a plain ffmpeg installation does not necessarily include that encoder. Duration is needed for a later AcoustID lookup. Results survive unchanged scans and become stale when files change. Errors identify failed files; successfully computed results remain stored. An acoustic fingerprint is an identification aid, not a byte-integrity checksum.

Listing stored fingerprints also takes no writer lock, so it remains available while a collection is running.

## Syntax and arguments

```text
aede fingerprint [folder…]
```

Optional catalogued folder or artist/album name restrictions.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--list` | List the individual stored records or decisions instead of the normal summary/operation. |
| `--full` | Fingerprint all selected files, including named/identified files and stored fingerprints. |
| `--dry-run` | Preview the work without making downloads or creating outputs. It can still validate tools and inputs. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede fingerprint "$HOME/Music/Jazz" --dry-run
aede fingerprint "$HOME/Music/Jazz"
aede fingerprint --list
aede fetch --identify "$HOME/Music/Jazz"
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[fetch](fetch.md), [check](check.md).

Detailed existing guide: [sources.md](../sources.md).
