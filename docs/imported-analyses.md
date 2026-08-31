# What another tool found

Entirely optional, and it changes nothing if you never use it.

Aède reads the _structure_ of a file. It does not decode, so there are questions it cannot answer yet: is this FLAC a re-encoded MP3, was it upsampled, where does the spectrum stop, how loud is it really — and the decisive one, does the decoded audio still match the MD5 the encoder wrote into the file.

[FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) already does that pass. If you have run it, `aede import` puts the results into the catalog:

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

A folder is walked **recursively**, because reports are kept the way the albums they describe are: one folder per artist, one per album.

## The order does not matter

An analysis is filed under the **path** it describes, not under a catalog entry. So the two operations can be done either way round, which matters because analysing a folder and _then_ building the library from it is the natural order for someone who already owns the other tool.

- **Import first.** The records are stored and reported as `Waiting for a scan`. The scan that brings those files in makes them attach by themselves, and says so (`Analyses now attached`). `doctor` says how many are still waiting rather than letting them sit there unmentioned.
- **Scan first.** Files are matched by path, then by name and size for a library that has moved since — a name and a byte count together are very nearly unique.
- **Leave the report in the album folder.** A scan walks over it anyway: any `.json` announcing itself as a FlacCompagnon report is read and taken in, and the scan report says how many. Half a kilobyte is read from each `.json` met to recognise one, so nothing else in the library is parsed.

Matching never relies on the two paths being written the same way. Watched folders are stored canonical, so a report produced against a symbolic link — or against `/var` where macOS says `/private/var` — names the very same file by a string that will never compare equal; the name and the size bridge the two, and the record is then refiled under the path the catalog uses.

## When a scan does not make it go away

Attaching only ever happens two ways: the path matches exactly, or the **name and size together** match a file the catalog holds. Neither is guaranteed by the mere fact that a scan ran. A report exported against a library that has since moved, been renamed track by track, or was never under a scanned folder in the first place will sit waiting forever — re-running `aede scan` cannot fix what the paths themselves do not agree on.

`doctor` only ever says how many are stuck like that:

```
149 imported analyses waiting for the folders they name to be scanned
```

which answers "how many", not "which" — the one thing a count cannot show, and the one thing needed to tell "not scanned yet" apart from "will never match". `aede import --pending` names them, **grouped by folder**:

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

Grouped by folder because that is the unit you act on: a report covering a fourteen-track album is _one_ decision — scan that folder, or decide it is gone — and fourteen rows bury it. And the folder is written out **whole**, never cut to a column width: a path trimmed to fit loses its head, which is exactly the half that distinguishes a drive merely unplugged from a folder that was renamed.

Once a folder is confirmed to be dead weight, `--forget --pending` removes exactly what waits in it, leaving every analysis that did attach untouched:

```sh
aede import --forget --pending "/Volumes/OldDrive/Music"   # that folder only
aede import --forget --pending                             # everything waiting
```

Both `--pending` and `--forget --pending` accept folders, and `--source` narrows either to one tool. A folder given to a plain `--forget` is refused rather than silently ignored — on a command that deletes, a swallowed argument is the worst kind.

Being _about_ a file is not the same as _describing_ it: a record that matches by name and size is still checked against that file's modification date, and dropped if the file was written to since.

Imported analyses survive a scan — they are the one thing in the catalog that reading the files again cannot recompute.

`aede track` then shows a second panel, named after whoever measured it:

```
Analysed by flaccompagnon

  MD5              Match
  Real bit depth   16 bits
  Cutoff           22.1 kHz
  Dynamic range    9.3 dB
  True peak        0.28 dBTP
```

Three rules govern what happens to those numbers.

**They are never merged into Aède's own.** A verdict carries the method that produced it. Overwriting the bit depth read from the frames with one obtained by decoding would leave the catalog unable to say where the number came from — and unable to notice that the two disagree. Noticing is the point.

**They expire with the bytes they describe.** An analysis is bound to the size and modification date the file had when it was measured, the same test the incremental scan uses. Edit the file and the panel says `— stale: the file changed since` rather than answering confidently about music that is no longer there. Re-importing that same report is refused for the same reason.

**A disagreement is a finding, not something to arbitrate.** `doctor` reports an MD5 mismatch as an **error** even when `aede check` found the file intact, because the two look at different things:

```
error  audio does not match its MD5
       flaccompagnon decoded the audio and it does not match the file's own MD5,
       although the frame checksums are valid: the stream was re-encoded
```

The frame checksums prove the _container_ was not corrupted; the MD5 proves the _audio_ is the audio that was encoded. A file passes the first and fails the second when it was re-encoded by a tool that rewrote the frames but kept the old signature — exactly the case Aède cannot see before it decodes anything itself.

**And that is the only thing `doctor` says about an imported report.** The spectral verdicts — transcoded, upscaled, upsampled — are imported, stored, kept up to date, and reported nowhere. The distinction is not about which tool is better; it is about what kind of statement each verdict is. A failed MD5 is a _fact_: two methods compared a checksum and disagreed, and `aede check` can be pointed at the file to settle it. "Early roll-off at 33 kHz, possible transcoding" is an _inference_, hedged by the tool that made it — and rightly, since a 1988 analogue master genuinely holds nothing above 30 kHz, so a faithful 24/96 transfer of one looks exactly like an upsample. A report that turns another program's "possibly" into a warning of its own has stopped describing the library and started arguing about it.

What the inference was drawn _from_ stays on the file's page, attributed: the cutoff frequency, the real bit depth, the dynamic range, the peaks. Those are measurements, and a reader who knows their master can conclude what they like from them.

## Seeing what is held

`--pending` answers what failed to attach. For a long time nothing answered the other half, and the asymmetry was worse than it sounds: a report imported over an artist whose files are all clean produces no waiting line, no `doctor` entry and no message of any kind — every symptom of having done nothing at all. The only way to see otherwise was to open a track page and hope to land on a file the report covered.

```
$ aede import --list

Imported analyses

  Folder                                          Analyses  State                 Source
  /Users/…/Marilyn Manson/1994 Portrait of an…          21  21 attached           flaccompagnon
  /Users/…/Ozzy Osbourne/1988 No Rest for the…          12  10 attached, 2 stale  flaccompagnon
  /Volumes/OldDrive/…                                    4  4 waiting             flaccompagnon
  in all: 305 attached, 2 stale, 4 waiting
```

Three fates, not two. **Stale** — attached to a file whose bytes have changed since the report was written — is the one that shows up nowhere else, and it is the one that silently voids a verdict.

A store that can only show its failures cannot be trusted about its successes, which is the whole reason to look.

## What an album page says about it

The same asymmetry, one level up: both readings lived on the track page alone, so verifying an album — the unit anybody actually verifies — took one command per track. An album page now carries one line, and only when there is something to say:

```
Antichrist Superstar

  Marilyn Manson
  1996
  /Users/…/Marilyn Manson/1996 Antichrist Superstar [FLAC] [16B-44kHz]
  checked: 16 intact · flaccompagnon: 16 MD5 matches
```

Both methods are named because they do not prove the same thing — one read the container checksums, the other decoded the audio — and when they disagree, that is the most interesting fact on the page. The denominator appears only when a method did not cover the whole album (`9 of 12 intact`), because "12 of 12" on every page is a fraction nobody reads twice, and its absence is what makes the one page saying `9 of 12` visible.

## Where it is all stored

In the catalog, and nowhere else: `~/.local/share/aede/catalog.json` grows one more table, `analysis`, one row per path and per source. The report you imported is never referred to again — you can move it or throw it away. `aede export` includes the table, `aede import --forget` empties it, and `aede reset` warns about it before removing the catalog.

`--data <folder>` puts the catalog elsewhere, `$AEDE_HOME` does the same by environment. `aede roots` ends by naming the file it just read, so the answer to "where is all this kept" is on the screen that lists what is watched.
