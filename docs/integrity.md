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

FLAC's audio MD5 describes decoded audio and needs a decode to compare. The container-only `aede check` command does not verify it. Playback now compares it during its normal decode, while `aede analyze` or imported [FlacCompagnon reports](imported-analyses.md#what-another-tool-found) can provide a separately attributed decoded-audio MD5 result.

A CRC pass and a decoded MD5 mismatch can coexist: they test different properties. A mismatch needs investigation; it does not by itself prove a particular editing history. Spectral source-quality inferences are also distinct from either checksum.

## FLAC audio MD5 during playback

`aede play` and native PCM playback verify a present FLAC STREAMINFO audio MD5
using the same progressive decode that supplies audio. The existing Symphonia
FLAC decoder computes it from original integer samples before conversion to
`f32`, downmix, gain, sample-rate conversion or other DSP. There is no extra
full-file preflight decode and no change to music or tags.

The checksum covers all decoded channels and samples; a zero checksum means
the value is unknown, as specified in [RFC 9639, section
8.2](https://www.rfc-editor.org/rfc/rfc9639.html#section-8.2). It can expose an
audio inconsistency even when container checks pass. MD5 is a consistency
check, not cryptographic proof of origin or authenticity; see [RFC
6151](https://www.rfc-editor.org/rfc/rfc6151.html).

The decoder's `flac_md5_status` distinguishes `Pending`, `Verified`,
`NoSignature` and `Mismatch`. Verification finishes only when decoding reaches EOF. Stopping
or skipping before EOF leaves a present checksum pending, without a successful whole-track
verdict. A zero STREAMINFO MD5 permits playback with `NoSignature`; it is not
reported as a verified digest. A mismatch is a decoding error, prevents natural
completion and discards any new full-track loudness result. The native PCM
route reports `decode_failed`; already delivered audio cannot be recalled.

Progressive seeking still decodes and discards the prefix before playing the
suffix. If that decoder subsequently reaches EOF, it has inspected the entire
track and can verify its MD5 even though the listening history remains
incomplete. Decoding verification and listening completion answer different
questions.

This runtime status does not write an integrity verdict or analysis to
`conclusions.json`. Existing Aède or FlacCompagnon conclusions do not bypass the
current decode's comparison. Other codecs retain their existing behavior.
Subsonic/OpenSubsonic streams original encoded bytes without this decode, so
decoded-audio verification there belongs to the client.

Native and Ogg FLAC sources use the same integer-sample validator. Source frame
counts and frame format declarations are checked; missing, repeated or reordered
native frames are errors even without a stored signature. Independent 32-bit
channels are supported, but the pinned codec cannot decode a correlated 32-bit
stereo frame's 33-bit side channel or the newer explicit 32-bit frame-header
encoding. Those sources are refused rather than reported as verified. Native
FLAC playback skips optional metadata bodies on the original opened descriptor and
exposes only STREAMINFO and encoded audio to the demuxer; large artwork is not
decoded or capped for playback. At most 65,536 metadata headers are accepted,
including zero-length blocks, to bound opening work. No source bytes are rewritten.

This is not a certificate for all container bytes. Optional metadata content,
trailing non-audio data and every Ogg page/link are outside the decoded MD5
verdict. Keep the separate container check when investigating structural errors.

For command syntax, threads, stored verdicts and practical limits see [check](cli/check.md). For read-back of newly copied destination files see [copy](cli/copy.md); it has different verification semantics.
