# Are the files still intact?

The quietest enemy of any digital CDthèque is not accidental deletion; it is silent corruption. A bit flips on an old hard drive, a sector dies, a transfer drops over the network, and your pristine rip is permanently scarred.

`aede check` answers the one existential question your metadata tags cannot: has the audio been damaged since it was written? It reads no reference copy and decodes nothing — it meticulously verifies the checksums the containers themselves already carry.

| Container                 | What is verified                                              |
| ------------------------- | ------------------------------------------------------------- |
| FLAC                      | The CRC-16 of every frame and the CRC-8 of every frame header |
| Ogg (Vorbis, Opus, Speex) | The CRC-32 of every page                                      |
| MP3, MP4, WAV, AIFF       | Nothing: these formats carry no checksum                      |

This catches what actually happens to stored files in the real world — a flipped bit, a bad sector, a violently truncated copy. A truncated file is caught even though every frame it _still_ holds is mathematically valid, because the last frame is strictly required to end where the file itself ends.

There are four states of integrity in your vault, and the fourth is the one too often forgotten by other tools:

- **not verified** — no check has been run on this file yet; it remains an unknown.
- **nothing to check** — the container fundamentally carries no checksum, and no amount of re-running will magically change that.
- **intact** — every checksum matched perfectly.
- **damaged** — one did not, with the exact corrupted frame or page named.

This verdict is permanently stored per file and survives across your regular library scans. The heavy cost of reading the audio is paid exactly once. A second `aede check` has absolutely nothing to read — and crucially, it says so **while showing the verdicts all the same**. The question the command answers is "are my files intact?", not just "was there work to do right now":

```
$ aede check

Integrity

  Intact                     1304
  Damaged                       0
  No checksum in the file       0
  nothing to read: it all has a verdict
  aede check --full verifies them again
```

The table describes every file currently in scope, regardless of which historical run established the verdict; the line underneath describes **this specific run**. Mixing the two is a classic trap that makes "137 files to read" and "1304 intact" look like one confusing figure. Aède separates the effort from the truth. Naturally, a file that has been modified loses its verdict, since it is no longer the exact file that was verified. `doctor` reports any damage as a critical error and honestly states how many files have never been verified, rather than letting a partially checked library falsely masquerade as a perfectly healthy one.

## The Physical Toll: How long it takes, and how to start small

Verifying means **reading every single byte** of the files concerned. On a true archivist's library of 20,000 tracks — some 600 GB — that takes a few minutes on a blazing fast NVMe drive, but it can take well over an hour on a mechanical spinning disk or a NAS. The time is entirely spent on input/output moving the platters, not on computation, so throwing more CPU cores at it barely helps.

That is exactly why the check is strictly opt-in, and why it announces its intentions before starting the heavy lifting:

```
$ aede check
Verifying 20 148 files to read, 612.4 GB
  this reads every byte: minutes on an SSD, longer on a mechanical disk
  stopping it is safe — verdicts are saved every 250 files, so at most the batch in progress is lost
```

Two deliberate design choices make auditing a massive archive manageable:

**Start on a corner.** Running `aede check ~/Music/Deicide` restricts the deep read to a specific folder, or as many as you like. This is perfect for a quick first look, or for immediately re-verifying a suspicious external drive without dragging the rest of your sanctuary into the process.

**Completed batches are saved.** Verdicts are written to `conclusions.json` every 250 files and when the run completes. Stopping a locally running check with Ctrl-C loses at most the batch currently in progress; the next run skips files whose saved verdict is still current. This protects completed saves, not against storage failure: keep backups of the conclusions as well as the audio.

When a local Unix server is running for the same data directory, `check` automatically runs under that server. Ctrl-C or closing its CLI disconnects the display; the server keeps checking. Explicit `aede cancel` currently supports delegated `scan` and `fetch` only, not `check`. A graceful server shutdown waits for accepted commands, including this check. See [Operating the local server](operating.md).

### The Limits of the Container

What this check does **not** prove is that the audio itself is mathematically untouched by human hands — a stream that was lazily re-encoded by a bad tool would consistently pass a container check.

FLAC also stores a master MD5 of the _decoded_ audio, and verifying that requires a full decode; that ultimate verdict arrives natively with the playback engine at milestone M3, and Aède's database structure is already prepared for it. Until then, [taking in another tool's analysis](imported-analyses.md#what-another-tool-found) seamlessly fills the gap for the curator who already demands that level of microscopic acoustic scrutiny.
