# Are the files still intact?

`aede check` answers the one question the tags cannot: has the audio been damaged since it was written? It reads no reference copy and decodes nothing — it verifies the checksums the containers already carry.

| Container                 | What is verified                                              |
| ------------------------- | ------------------------------------------------------------- |
| FLAC                      | The CRC-16 of every frame and the CRC-8 of every frame header |
| Ogg (Vorbis, Opus, Speex) | The CRC-32 of every page                                      |
| MP3, MP4, WAV, AIFF       | Nothing: these formats carry no checksum                      |

That catches what actually happens to stored files — a flipped bit, a bad sector, a truncated copy. A truncated file is caught even though every frame it still holds is valid, because the last one has to end where the file does.

Four states, and the fourth is the one usually forgotten:

- **not verified** — no check has been run on this file yet;
- **nothing to check** — the container carries no checksum, and no amount of re-running will change that;
- **intact** — every checksum matched;
- **damaged** — one did not, with the frame or page named.

The verdict is stored per file and survives across scans, so the cost is paid once: a second `aede check` has nothing to read — and says so **while showing the verdicts all the same**, since the question the command answers is "are my files intact?", not "was there work to do":

```
$ aede check

Integrity

  Intact                     1304
  Damaged                       0
  No checksum in the file       0
  nothing to read: it all has a verdict
  aede check --full verifies them again
```

The table describes every file in scope, whatever run established each verdict; the line under it describes **this** run. Mixing the two is what makes "137 files to read" and "1304 intact" look like one figure. A file that changed loses its verdict, since it is no longer the file that was verified. `doctor` reports damage as an error and says how many files have never been verified rather than letting a library look healthy.

## How long it takes, and how to start small

Verifying means **reading every byte** of the files concerned. On a library of 20 000 tracks — some 600 GB — that is a few minutes on an NVMe drive, and it can be well over an hour on a mechanical disk or a NAS. The time is spent on input/output, not on computation, so more cores barely help.

That is why the check is opt-in, and why it announces itself before starting:

```
$ aede check
Verifying 20 148 files to read, 612.4 GB
  this reads every byte: minutes on an SSD, longer on a mechanical disk
  stopping it is safe — verified files are saved as the run goes
```

Two things make it manageable:

**Start on a corner.** `aede check ~/Music/Deicide` restricts the run to a folder, as many as you like. Useful for a first look, and for re-verifying a drive you suspect without touching the rest.

**Interrupting is safe.** Verdicts are written to the catalog every 250 files rather than at the end, so a `Ctrl-C` — or a laptop closing, or a drive going away — costs at most the batch in progress. Everything already verified is kept, and the next run picks up exactly where the last one stopped, since a file that has a verdict is no longer in the queue. A second full run therefore has nothing left to read.

What this does **not** prove is that the audio itself is untouched — a stream re-encoded consistently would pass. FLAC also stores an MD5 of the _decoded_ audio, and checking it means decoding; that verdict arrives with the playback engine at M3, and the stored shape already accommodates it. Until then, [taking in another tool's analysis](imported-analyses.md#what-another-tool-found) fills the gap for whoever already has one.
