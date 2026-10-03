# Are the files still intact?

`aede check` verifies the checksums carried by supported audio containers. It reads the selected catalogued files without modifying them and without requiring a reference copy. This is a container-integrity check, not a full acoustic analysis.

| Container | What is verified |
| --- | --- |
| FLAC | CRC-16 of frames and CRC-8 of frame headers |
| Ogg (Vorbis, Opus, Speex) | CRC-32 of pages |
| MP3, MP4, WAV, AIFF and other unsupported checks | No checksum supported by this command |

An unsupported checksum result does not mean that the format can never contain a checksum, nor that the file has been proved healthy. The distinction is what this Aède implementation checks.

The report distinguishes:

- **Not verified:** no current verdict has been established.
- **No checksum:** this command has no supported container checksum to verify.
- **Intact:** the supported checks matched for the inspected file.
- **Damaged:** a supported check failed; inspect the reported frame/page and reason.

Unreadable files are reported separately and retain no successful verdict. The current handler can finish with process exit code 0 while reporting unreadable files or damaged verdicts. Read the report; exit status alone is not an integrity certificate. `doctor` also reports held damage and files still unverified.

Verification requires the complete stream. The current reader refuses more than 2 GiB of FLAC audio frames after metadata, or more than 2 GiB for an entire Ogg file. These files are reported as read errors, without a new verdict; a checked prefix never establishes that the whole file is intact. A file whose size changes while it is being read is also refused. A failed recheck removes that file's previous verdict, so the next ordinary check retries it; fingerprints and analyses are retained. Earlier versions could record a prefix-only verdict at the read limit. Existing verdicts remain reusable until explicitly rechecked: run `check --full` on folders containing large files verified by an earlier version.

## Reuse and current scope

Verdicts live in `conclusions.json` and attach to unchanged catalogued files. A regular scan preserves current conclusions; a changed file makes its old verdict ineligible. A normal check skips current verdicts but still prints the report for every file in scope. `--full` deliberately rereads them.

```sh
aede check
aede check "/path/to/music/album" --full
```

Folder restrictions select catalogued files under those folders. Scan new music first. An intact container checksum does not prove that the file was never re-encoded, that it matches a separate original, or that the drive will retain it indefinitely.

## The Physical Toll: How long it takes, and how to start small

Verification reads the selected files; storage throughput and file size dominate large-library time. No fixed duration is promised for a NAS or drive. Start with one album/folder and expand once you understand the report.

Completed batches are saved every 250 files and when the run completes. Stopping a locally running check with Ctrl-C loses at most the batch in progress; the next run can reuse saved current verdicts. This preserves completed work, not protection against storage failure. Back up conclusions and original music separately.

On Unix with a matching local server, check delegates to it. Closing the CLI or pressing its Ctrl-C disconnects display but does not stop the accepted check. Explicit `cancel` supports delegated scan/fetch only. Graceful server shutdown waits for accepted commands, including check. See [operating guidance](operating.md).

### The Limits of the Container

FLAC's audio MD5 describes decoded audio and needs a decode to compare. It is not verified by the current playback path or by this container-only check. `aede analyze` or imported [FlacCompagnon reports](imported-analyses.md#what-another-tool-found) can provide an attributed decoded-audio MD5 result.

A CRC pass and a decoded MD5 mismatch can coexist: they test different properties. A mismatch needs investigation; it does not by itself prove a particular editing history. Spectral source-quality inferences are also distinct from either checksum.

For command syntax, threads, stored verdicts and practical limits see [check](cli/check.md). For read-back of newly copied destination files see [copy](cli/copy.md); it has different verification semantics.
