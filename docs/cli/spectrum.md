# spectrum — Create spectrograms beside the selected music

spectrum draws spectrogram images beside selected catalogued music through the FlacCompagnon engine. A spectrogram shows frequency content over time; a visual cutoff can inform an investigation but is not by itself proof that audio was transcoded.

Half-size images are the default; --size full uses the full drawing size. Existing pictures are skipped unless --full is supplied, including when only the size changed. --dry-run lists planned outputs without drawing, and --threads controls concurrent work.

The command creates sidecar images and needs write access to those folders; original music is read only. When tools/decoding fail, inspect the reported filename and verify format/tool availability before retrying. Use analyze for stored numerical measurements rather than interpreting an image as a checksum.

## Syntax and arguments

```text
aede spectrum [folder…]
```

Optional catalogued folders; no argument covers the catalog.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--full` | Redraw existing spectrograms, including when changing the requested size. |
| `--size VALUE` | With fetch --covers: 250, 500, 1200 (default) or original. With spectrum: half (default) or full. |
| `--threads N` | Number of worker threads. A positive integer fixes the count; 0 selects an automatic count. Plain copying defaults to one worker. |
| `--dry-run` | Preview the work without making downloads or creating outputs. It can still validate tools and inputs. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede spectrum "$HOME/Music/Jazz" --dry-run
aede spectrum "$HOME/Music/Jazz" --size full --full
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[analyze](analyze.md), [copy](copy.md).

Detailed existing guide: [spectrograms.md](../spectrograms.md).
