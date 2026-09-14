# What another tool found

Entirely optional, and it changes nothing if you never use it.

Aède is a master of structure. It reads the physical "digital grooves" of your files—the tags, the frames, the containers. But it does not decode the audio itself, which means there are forensic questions it respectfully leaves unanswered: Is this "lossless" FLAC actually a re-encoded 128kbps MP3? Was it artificially upsampled? Where does its high-frequency spectrum truly stop? And critically, does the fully decoded audio still perfectly match the MD5 signature the original encoder stamped into the file?

[FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) performs exactly this kind of microscopic acoustic pass. If you have run it across your collection, `aede import` gracefully folds these external insights into your catalog:

```sh
aede import ~/Desktop/danzig-report.json
aede import ~/Desktop/reports/            # every .json underneath, at any depth
aede import --list                        # everything held, and what became of it
aede import --pending                     # which folders have no matching file yet
aede import --forget                      # remove them all
aede import --forget --source=flaccompagnon
aede import --forget --pending            # remove only what will never attach
aede import --forget --pending "/Volumes/OldDrive"   # …and only under that folder
```

A folder is walked **recursively**, because Aède understands that you keep your reports the same way you curate your albums: filed elegantly by artist and release.

## The order does not matter

An analysis is filed under the **path** it describes, not inherently tied to a catalog entry. Therefore, you can build your CDthèque in whatever order feels natural. Analysing a folder _before_ officially shelving it in the library is a perfectly valid archivist's workflow.

- **Import first.** The acoustic records are securely stored and reported as `Waiting for a scan`. The moment you scan the actual audio files into the sanctuary, the analyses attach themselves automatically, proudly declaring `Analyses now attached`. The `doctor` command keeps track of how many are still waiting, ensuring no analysis is forgotten in the dark.
- **Scan first.** Files are matched by path, then intelligently by name and size to accommodate a library that might have been moved. A precise filename paired with an exact byte count is nearly as unique as a fingerprint.
- **Leave the report in the album folder.** An Aède scan gracefully steps over your archival materials. Any `.json` file announcing itself as a FlacCompagnon report is read, digested, and reported in the scan summary. Only half a kilobyte is peeked at to recognise it; the rest of your meticulously saved non-audio files remain untouched and unparsed.

Matching is resilient. Watched folders are stored canonically, so a report produced against a symbolic link—or against `/var` where macOS says `/private/var`—still identifies the true file. The name and size bridge any path discrepancies, safely refiling the analysis under the master path your catalog trusts.

## When a scan does not make it go away

Attaching only ever happens two ways: the path matches exactly, or the **name and size together** match a file already preserved in the catalog.

A report exported against a library that has since been shifted to a new drive, heavily renamed, or was simply never under a watched folder, will sit waiting forever. Re-running `aede scan` cannot magically fix what the foundational paths disagree on.

`doctor` will alert you to these archival ghosts, but only with a count:

```
149 imported analyses waiting for the folders they name to be scanned
```

A count tells you "how many," not "which"—the one crucial detail needed to distinguish "I haven't scanned this yet" from "This hard drive died three years ago." `aede import --pending` answers this by naming them, **grouped logically by folder**:

```
$ aede import --pending

Waiting for a scan

  Folder                                                        Analyses  Source
  /Volumes/Musique externe/Bibliotheque/Danzig/1994 Danzig 4           2  flaccompagnon
  /Volumes/Musique externe/Bibliotheque/Ozzy Osbourne/1980 Bli…        4  flaccompagnon
  6 waiting analyses in all
  scan a folder to attach its analyses, or drop one that is gone for good:
  aede import --forget --pending <folder>
```

They are grouped by folder because that is the unit an archivist acts upon. A report covering a fourteen-track album represents _one_ curation decision—scan it, or discard it. Fourteen separate rows would only bury the truth. And the folder path is written out **whole**, never brutally truncated to fit a terminal width. A trimmed path loses its head, which is the very information you need to realize you are looking at an unplugged external drive rather than a renamed folder.

Once you confirm a folder is truly gone, `--forget --pending` elegantly purges only what is waiting, leaving every successfully attached analysis strictly untouched:

```sh
aede import --forget --pending "/Volumes/OldDrive/Music"   # that folder only
aede import --forget --pending                             # everything waiting
```

Both `--pending` and `--forget --pending` accept specific folders, and `--source` can narrow the focus to a single tool. A folder passed to a bare `--forget` is rightfully refused rather than silently ignored. When a command exists to delete data, a swallowed argument is the most dangerous kind of error.

Crucially, being _about_ a file is not the same as accurately _describing_ its current state. A record that perfectly matches by name and size is still rigorously checked against the file's modification date. If the audio was edited after the report was generated, the analysis is dropped. Imported analyses survive a scan, as they are the _only_ data in your CDthèque that reading the files cannot recompute on its own.

`aede track` then displays a secondary panel, explicitly attributed to the tool that measured it:

```
Analysed by flaccompagnon

  MD5              Match
  Real bit depth   16 bits
  Cutoff           22.1 kHz
  Dynamic range    9.3 dB
  True peak        0.28 dBTP
```

Three absolute rules govern how Aède handles these numbers.

**They are never merged into Aède's own findings.** A verdict carries the signature of the method that produced it. Silently overwriting the bit depth read from a FLAC frame with one obtained by spectral decoding would destroy provenance—leaving the catalog unable to say where the number came from, and blind to the fact that the two methods disagree. _Noticing the disagreement is the entire point._

**They expire with the bytes they describe.** An analysis is permanently bound to the file's size and modification date at the exact moment it was measured. Edit the file's tags, and the panel respectfully steps back, stating `— stale: the file changed since`, rather than confidently lying about audio it can no longer guarantee. Importing a report against a changed file is refused for this exact reason. Since a refused stale record is never stored, `aede import` lists the **folders** it happened in immediately:

```
$ aede import ~/Desktop/ozzy-report.json

Import
  ...
  Changed since the report                                          2

Changed since the report

  Folder                                                  Analyses
  /Users/…/Ozzy Osbourne/1988 No Rest for the Living             2
  run FlacCompagnon again on the folders above
```

**A disagreement is a finding, not something to arbitrate.** `doctor` reports an MD5 mismatch as a critical **error** even if `aede check` found the file perfectly intact, because the two tools are interrogating different realities:

```
error  audio does not match its MD5
       flaccompagnon decoded the audio and it does not match the file's own MD5,
       although the frame checksums are valid: the stream was re-encoded
```

Frame checksums prove the _container_ survived the journey; the MD5 proves the _audio_ is mathematically identical to the source. A file passes the first and fails the second when it was re-encoded by a tool that rewrote the frames but lazily copied the old signature—an archival tragedy Aède cannot see until the audio is fully decoded.

**And that is the only thing `doctor` says about an imported report.** The spectral inferences—"transcoded," "upscaled," "upsampled"—are dutifully imported, stored, and kept up to date, but they are _reported nowhere as errors_. A failed MD5 is a mathematical _fact_. "Early roll-off at 33 kHz, possible transcoding" is an _inference_. A faithful 24/96 transfer of a 1988 analogue master genuinely holds nothing above 30 kHz; it will look exactly like an upsample to an algorithm. A report that turns another program's "possibly" into an Aède warning has stopped describing your library and started arguing with it. Aède remains an archivist, not an audio critic.

What the inference was drawn _from_ stays on the file's page: the cutoff frequency, the real bit depth, the dynamic range. These are objective measurements. A curator who knows the history of their masters can draw their own conclusions.

## Seeing what is held

`--pending` answers what failed to attach. But a store that only shows its failures cannot be fully trusted about its successes.

```
$ aede import --list

Imported analyses

  Folder                                          Analyses  State                 Source
  /Users/…/Marilyn Manson/1994 Portrait of an…          21  21 attached           flaccompagnon
  /Users/…/Ozzy Osbourne/1988 No Rest for the…          12  10 attached, 2 stale  flaccompagnon
  /Volumes/OldDrive/…                                    4  4 waiting             flaccompagnon
  in all: 305 attached, 2 stale, 4 waiting
```

Three fates, not two. **Stale**—attached to a file whose bytes have shifted since the report was written—silently voids a verdict. This complete list gives you the absolute truth of your external data.

## What an album page says about it

Verifying the integrity of an album is usually done at the album level, not track-by-track. Aède's album page now proudly carries a single line of summary, but only when there is something meaningful to say:

```
Antichrist Superstar

  Marilyn Manson
  1996
  /Users/…/Marilyn Manson/1996 Antichrist Superstar [FLAC] [16B-44kHz]
  checked: 16 intact · flaccompagnon: 16 MD5 matches
```

Both methods are named because they prove different truths. When they disagree, it is the most vital fact on the page. A denominator (`9 of 12 intact`) only appears when a method didn't cover the entire album, ensuring that when you do see a fraction, it demands your attention.

## Where it is all stored

In the vault, and nowhere else: `~/.local/share/aede/catalog.json` simply grows a new table, `analysis`, tracking one row per path and per source. The `.json` report you imported is never required again—you are free to archive it elsewhere or discard it. `aede export` faithfully includes this table, `aede import --forget` cleanses it, and `aede reset` politely warns you about it before dismantling the catalog.

`--data <folder>` lets you move the catalog to a custom location, and `$AEDE_HOME` does the same via environment variables. `aede roots` concludes by naming the exact catalog file it just consulted, ensuring the answer to "where is all this kept?" is always plainly visible.
