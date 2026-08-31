# Building the library

## Tagging: use Picard

Aède **never writes to your files** — not tags, not names, not folders. That is a deliberate limit, and it leaves a real job undone: something has to put good metadata in there in the first place.

[MusicBrainz Picard](https://picard.musicbrainz.org/) is the tool for it, and the two compose rather than compete. Picard identifies your files against MusicBrainz and writes the tags; Aède reads them, never touches them, and builds the catalog. A library tagged with Picard already carries the MusicBrainz identifiers (`MUSICBRAINZ_*`), which is precisely what M1 will use to reach relations and credits without having to guess at a match — the hard and error-prone part of identification is then already done, by a tool built for it, under your eye.

It also makes the divergences M1 reports rare by construction: if your tags came from MusicBrainz, MusicBrainz will mostly agree with them, and the layer will be there to fill gaps rather than to argue.

If you would rather not run Picard, nothing breaks — Aède reads whatever the tags say and `aede doctor` tells you where they are thin.

## Folders never read

A music folder is rarely only music. `Audiobooks`, `Podcasts`, `_incoming`, a `Samples` folder for a DAW — none of it belongs in a music catalog, and reorganising the disk to suit the program is the wrong way round.

```sh
aede roots --exclude ~/Music/Audiobooks     # never read it again
aede roots                                  # shows what is watched and what is not
aede roots --exclude ~/Music/Audiobooks --remove
```

The exclusions live **in the catalog**, beside the watched roots, not in the options of one run. A plain `aede scan` re-reads every root, so an exclusion that had to be retyped would be forgotten precisely when it mattered. They also survive the rebuild a scan performs — the same rule that carries imported analyses across: **a scan may not destroy what it cannot recompute**, and an exclusion is typed, not read from any file. (The first version of this feature dropped them exactly there; the symptom was an exclusion that worked once and then vanished.)

Matching is on the canonical path, so a folder reached through a symbolic link is excluded too.

**And the change takes effect straight away.** Each of these three commands rescans on the spot, because each of them makes a scan necessary and there is no reason to hand that back to the person who just asked for the change. "Run `aede scan` to drop them" was an instruction the program could carry out itself — and one that is forgotten, leaving a catalog describing a library nobody has any more, with nothing on screen saying so.

`--no-scan` keeps the old behaviour, and it is not decoration: dropping four folders one after another would otherwise rescan four times, which on a slow drive is minutes. The message then names what is pending, so the state is never silent.

**`aede reset` deliberately stays out of this.** It destroys what a scan cannot rebuild, it is the only command that asks for confirmation, and rebuilding a catalog somebody has just chosen to throw away would answer a question they did not ask.

## Box sets, and where a release lives

A box set is almost always laid out with one folder per disc:

```
Nobuo Uematsu/1997 FINAL FANTASY VII [FLAC]/Disc 1/
Nobuo Uematsu/1997 FINAL FANTASY VII [FLAC]/Disc 2/
```

The folder is part of what identifies a release — it is what tells a CD rip from a vinyl rip of the same record by the same artist. But a **disc folder is a subdivision of a release, not another edition of it**, so `Disc 1`, `CD2`, `Disque 3` and the like are folded into their parent: the release lives where the album does. Without that, one soundtrack came back as two albums of the same name, each numbering its tracks from one, with nothing on screen saying which disc was which except the path.

The number then shows in the track column, as `1-01`, `2-07`, and only on albums that span more than one disc — a column of `1-` on every single-disc album in a library is noise. The column set does not change, which is the same rule that keeps `check` reporting in one shape.

Where the tags carry `discnumber` it is used; where a rip split the discs into folders and left the tag empty, the folder supplies it. The tag wins when both are there — it is what the person who made the file said.

The line under the tracks counts them too:

```
  4 discs · 85 tracks · 4:34:11 · 1.5 GB
```

It says how many discs are **there**, not how many the tags claim — a set missing its fourth disc reads as three, which is the question actually being asked of a box set. Like the column, it appears only past one disc.

## Compilations

A **compilation** is a release with no album artist: several artists share it, which is why it stays out of every discography. Nothing else in the program singles them out, so `albums` does:

```sh
aede albums --compilations       # only the ones several artists share
aede albums --no-compilations    # everything except those
```

Asking for both is refused — they are opposites, and an empty answer would look like a library with nothing in it.

## Reading the scan report

| Line                      | What it counts                                                                                                                          |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Files found               | Audio files seen while walking the folders, duplicates removed                                                                          |
| Read from disk            | Files whose tags were parsed: new ones, and those changed since the last scan                                                           |
| Reused from previous scan | Files identical in path, size and modification time; their tags came from the catalog, untouched on disk                                |
| Gone since last scan      | Files the catalog knew and that are no longer there; they leave the catalog                                                             |
| Analyses imported         | [FlacCompagnon reports](imported-analyses.md#what-another-tool-found) found in the folders and taken in; only shown when there were any |
| Analyses now attached     | Imported analyses that were waiting for a file and found it this time                                                                   |
| Elapsed                   | Wall-clock time of the whole scan, folder walk included                                                                                 |

`Files found` is always the sum of the two middle lines. A file that could not be read is listed underneath with the reason, and stays out of the catalog without stopping the scan.

## Starting over

`aede roots` weighs each watched folder, so the list answers "what is on this drive" and not merely "which drives":

```
Watched folders

  Folder                 Tracks  Duration       Size
  ─────────────────────  ──────  ────────  ─────────
  /Volumes/Music/FLAC     18 402   52 days     4.1 TB
  /Users/kcell/Music         746   2 days    112.4 GB
  (no longer watched)         92   6 h        8.1 GB
```

The last row appears only after `roots --remove --no-scan`: those files stay in the catalog until the next scan, and a table that hid them would make that promise unverifiable. Without `--no-scan` the removal rescans on the spot, and there is nothing left to show.

`aede reset` removes the catalog. It first says what it holds — tracks, albums, artists, watched folders, integrity verdicts, imported analyses — and what a rescan does not bring back:

```
$ aede reset

About to remove the catalog

  Tracks                  20 148
  Watched folders              2
  Integrity verdicts      20 148
  Imported analyses          312
  File                    9.4 MB
  a scan rebuilds the catalog; the watched folders and the integrity
  verdicts are lost and have to be redone
  the imported analyses go too, and have to be imported again
  Type "yes" to confirm:
```

`--yes` skips the question, for scripts and tests. Without a terminal to ask on, the command **refuses** rather than assuming an answer either way. Once done, it prints the `aede scan` that rebuilds what it removed, watched folders included — that line is the only trace left of them.

## Trying it without a library at hand

```sh
tools/demo-library.sh /tmp/demo-music   # requires ffmpeg
aede scan /tmp/demo-music
aede doctor
```

The demo library is deliberately damaged: untagged files, a duplicate, an album missing a track, an album with mixed formats. Enough for `doctor` to have something to bite on.
