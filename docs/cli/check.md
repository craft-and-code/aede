# check — Verify the checksums stored in audio containers

check reads the checksums already carried by supported containers: FLAC frame CRC and Ogg page CRC for Vorbis/Opus/Speex. It detects damaged frames/pages covered by those checks. MP3, MP4, WAV and AIFF do not provide this check’s supported checksum, so “no checksum” is not “verified healthy”.

By default unchanged files with stored verdicts are reused. --full rereads everything in scope, useful after a suspected disk problem. If nothing is waiting, the current report is printed. An optional folder narrows catalogued files; scan new music before checking it.

Verdicts are stored as conclusions and become stale when the source changes. This command is distinct from decoded FLAC MD5 verification or an audio-origin analysis performed through FlacCompagnon. It never repairs or overwrites a damaged file. Keep a separate backup of original music.

## Syntax and arguments

```text
aede check [folder…]
```

Zero or more folders containing catalogued audio. No argument covers the catalog.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--full` | Reread every selected file’s supported container checksum, including existing verdicts. |
| `--threads N` | Number of worker threads. A positive integer fixes the count; 0 or an omitted option uses the available processor parallelism, falling back to four workers if unavailable. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede check
aede check "$HOME/Music/Jazz" --full
aede check --threads 4
```

## Result and errors

Read the verdict report, including damaged/no-checksum/unreadable categories. The current check handler can finish with exit code 0 after reporting unreadable files or damaged verdicts: process success alone is not proof that every file is healthy. Invalid arguments and catalog/storage failures still return an error.

The complete supported stream must fit the current 2 GiB read limit: FLAC audio after metadata, or the entire Ogg file. Larger streams and files whose size changes during reading are reported as read errors without a new verdict. A partial read cannot produce an intact verdict. Existing verdicts are still reused unless `--full` is supplied.

A failed recheck clears that file's previous verdict and leaves it eligible for another attempt. Other conclusions and files not yet attempted are preserved. Earlier versions could retain a prefix-only verdict at the read limit; use `--full` on folders containing large files checked by those versions.

## Related reading

[scan](scan.md), [analyze](analyze.md), [copy](copy.md).

Detailed existing guide: [integrity.md](../integrity.md).
