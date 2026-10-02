# What another tool found

Entirely optional, and it changes nothing if you never use it.

Aède's native parsers read tags, frames, and containers. For questions about the decoded audio—possible transcoding, upsampling, spectral cutoff, loudness, and the FLAC audio MD5—`aede analyze` calls the [FlacCompagnon](https://github.com/craft-and-code/FlacCompagnon) Rust analysis library directly. The measurements stay attributed to FlacCompagnon in Aède's catalog.

```sh
aede analyze                            # analyze catalogued albums and store results in Aède
aede analyze ~/Music/Album --json       # also save Album.json in that album folder
aede analyze ~/Music --json --threads 4 # one report per album folder
```

An album's folder comes from the catalog, so a multi-disc release can use its shared album folder. Files without album tags are grouped by their containing folder. Audio and tags remain read-only. Report-writing options create separate JSON files.

### Re-running an analysis and keeping its results

Unchanged tracks with valid existing reports are reused. `--force` recomputes the selected audio and allows replacement of selected report destinations. Without report-writing options, measurements are still saved in Aède. `--json` saves in album folders; `--json-layout album|artist` enables saving and chooses album or parent artist folders. A malformed existing destination is refused unless deliberately replaced with `--force`. See [analyze](cli/analyze.md) for the complete behavior and options.

Both FlacCompagnon layouts are supported:

| Report location | Coverage | How Aède reads it |
| --- | --- | --- |
| `Artist/Artist.json` | Several albums below the artist folder | Scan the artist folder or an ancestor, or import the report explicitly |
| `Artist/Album/Album.json` | One album, including its disc subfolders | Scan the album folder or an ancestor, or import the report explicitly |

Attachment follows each audio file's path, size and modification date, independently of where the JSON is saved. A scan of an album alone does not search its parent artist folder for reports; use `aede import Artist/Artist.json` when that parent is outside the watched scan scope.

Overlapping reports produce one result per audio file and source. Among results that describe the same current file, the result with the most recent **report-file modification time** wins when dates are available, regardless of filenames or whether it came from an artist or album report. In-process measurements use their completion time. These dates are retained with subsecond precision in `conclusions.json`, so an old report cannot replace a newer stored measurement on a subsequent incremental or full scan. A stale measurement of an earlier version of the audio file cannot block a valid report of its current bytes. Equal or unknown dates keep the existing import-order rule; older stores remain readable.

Deleting a JSON **after it has been imported or analyzed** does not remove its measurements from Aède, even after a full scan. They stay in `conclusions.json` until explicitly forgotten with `aede import --forget`, or replaced by a newer result. Changing the audio file's size or modification date makes the old measurement stale; deleting its report does not. A report that Aède never read cannot be recovered after deletion.

If you already have a FlacCompagnon report from the desktop app or its standalone `flaccompagnon` command, `aede import` still folds it into your catalog:

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
- **Scan first.** Files are matched by path. A report carrying `file_md5` can then identify a moved or renamed file by its complete bytes, even when its modification date changed. Older reports without that field fall back to a unique name and size, checked against the modification date.
- **Leave the report in the album folder.** An Aède scan gracefully steps over your archival materials. Any `.json` file announcing itself as a FlacCompagnon report is read, digested, and reported in the scan summary. Only half a kilobyte is peeked at to recognise it; the rest of your meticulously saved non-audio files remain untouched and unparsed.

Watched folders are stored canonically. A report produced against a symbolic link—or against `/var` where macOS says `/private/var`—can still identify the true file. `file_md5` hashes the whole file, including tags and artwork; it differs from the FLAC audio MD5 status shown in the analysis. If two catalogued copies have identical bytes, Aède leaves the record waiting instead of choosing one album arbitrarily.

## When a scan does not make it go away

An analysis attaches when its path matches, or when one catalogued file has the same size and whole-file MD5. For older reports without `file_md5`, one unique name and size can still match if the modification date agrees.

A report stays waiting when no candidate matches, or when several identical copies make the destination ambiguous. Scanning a newly moved folder may resolve a waiting report; reports without a whole-file MD5 cannot recover from a rename if the old name no longer exists.

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

Being _about_ a file is not the same as accurately _describing_ its current state. Exact-path and legacy name-and-size matches still check the file's size and modification date. A moved file with a matching whole-file MD5 is byte-identical, so a changed modification date alone does not invalidate its analysis.

`aede track` then displays a secondary panel, explicitly attributed to the tool that measured it:

```
Analysed by flaccompagnon

  FLAC audio MD5       Match
  File MD5             0123456789abcdef0123456789abcdef
  File CRC32           89abcdef
  Real bit depth       16 bits
  Cutoff               22.1 kHz
  Dynamic range        9.3 dB
  Integrated loudness  -14.2 LUFS
  Loudness range       6.0 LU
  True peak            0.28 dBTP
  Clicks               18 (first at 2.500 s)
  Click 1              at 2.500 s, channel 1, duration 0.000159 s
```

The normal `aede track "<title>"` page shows measured loudness, dynamic range, phase, stereo balance, bit-depth evidence, source verdicts, discontinuity counts, every event location retained in the report, and available file hashes. Its `FLAC audio MD5` row is the audio signature check result; `File MD5` and `File CRC32` are hashes of the complete file. FlacCompagnon may retain fewer event locations than the total count. `--json` only changes the output format and includes the complete attributed file entry in each `analyses[].source_data`, including fields introduced later within the supported report format. The local HTTP `/api/v1/track` detail exposes the same data. Aède also keeps that entry in `conclusions.json` as `source_data`. Re-importing an older saved report restores only the fields that report actually contains; rerun `aede analyze` or import a newer report to obtain measurements that were previously discarded.

Three absolute rules govern how Aède handles these numbers.

**They are never merged into Aède's own findings.** A verdict carries the signature of the method that produced it. Silently overwriting the bit depth read from a FLAC frame with one obtained by spectral decoding would destroy provenance—leaving the catalog unable to say where the number came from, and blind to the fact that the two methods disagree. _Noticing the disagreement is the entire point._

**They expire when the file changes.** At its original path, an analysis is checked against the file's size and modification date. Edit the tags, and the panel steps back, stating `— stale: the file changed since`. After a move, a whole-file MD5 match proves that the bytes still agree; changing tags or artwork changes that digest. Since a refused stale record is never stored, `aede import` lists the **folders** it happened in immediately:

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

Frame checksums prove the _container_ survived the journey; the decoded MD5 compares the audio with the signature stored in the FLAC. A file can pass the first and fail the second, for example after a re-encode retaining an old signature. The mismatch identifies disagreement, not a uniquely proved editing history or comparison against a separate original copy.

**And that is the only thing `doctor` says about an imported report.** The spectral inferences—"transcoded," "upscaled," "upsampled"—are shown on the track page as FlacCompagnon's attributed verdicts, but they are _reported nowhere as Aède errors_. A failed MD5 is a mathematical _fact_. "Early roll-off at 33 kHz, possible transcoding" is an _inference_. A faithful 24/96 transfer of a 1988 analogue master genuinely holds nothing above 30 kHz; it will look exactly like an upsample to an algorithm. A report that turns another program's "possibly" into an Aède warning has stopped describing your library and started arguing with it. Aède remains an archivist, not an audio critic.

What the inference was drawn _from_ stays beside it on the file's page: the cutoff frequency, the real bit depth, the dynamic range. These are objective measurements. A curator who knows the history of their masters can draw their own conclusions.

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

Attributed analyses live in `conclusions.json`, separate from the rebuildable tag catalog. The original report is not required after successful ingestion: deleting it does not remove stored measurements. `import --forget` explicitly removes selected analyses. `reset` removes catalog.json and its watched roots but leaves conclusions, personal data and source stores intact.

`--data` selects the data folder for one command and `AEDE_HOME` selects it through the environment. Back up the persistent stores and original music separately. See [data safety](manual/catalog.md), [import](cli/import.md) and [backup](cli/backup.md).

## Reusing acoustic reports

Valid existing reports are reused according to file identity and dates as described above. The [analyze reference](cli/analyze.md) details `--force`, `--show-results`, report layout and replacement rules. The [import reference](cli/import.md) details report files/directories, pending/stale states and explicit removal.
