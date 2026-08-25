# Aède

A local music library, written in Rust.

An _aède_ (Greek ἀοιδός, _aoidos_) was the poet-singer of archaic Greece: he held the whole repertoire in memory and performed it. Keeping and playing, in one word — which is exactly what this program is for.

This repository is **milestone M0.5**: read folders, turn them into a catalog of linked entities, answer questions about it, and keep what you think of it. No audio playback and no network access yet — that is deliberate, see the roadmap at the end.

## Getting started

```sh
cargo build --release
./target/release/aede scan ~/Music
./target/release/aede stats
```

Rust 1.89 or later. The build downloads one dependency, `lofty`; everything after the first build works offline.

## Commands

| Command                                                   | What it does                                                                                                                 |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `aede scan [folder…]`                                     | Scan the watched folders; any folder given is added to them                                                                  |
| `aede roots`                                              | List the watched folders (`--remove <folder>` to drop one)                                                                   |
| `aede stats`                                              | Tracks, albums, formats, quality, decades, completeness                                                                      |
| `aede doctor`                                             | Missing tags, duplicates, incomplete albums, mixed formats                                                                   |
| `aede check [folder…]`                                    | Verify the checksums the files carry (`--full` re-verifies everything)                                                       |
| `aede artists` / `albums` / `genres` / `labels` / `years` | Listings (`artists --role producer`, `albums --compilations`)                                                                |
| `aede artist "<name>"`                                    | Discography, collaborations, roles (`--with <other>` lists the tracks two artists share)                                     |
| `aede album "<title>"`                                    | Tracks, durations, formats, credits                                                                                          |
| `aede track "<title>"`                                    | Every track carrying this title: album, credits, technical details, tags                                                     |
| `aede genre <name>`                                       | What is in a genre: albums and the artists audible on them                                                                   |
| `aede label <name>`                                       | A label's catalogue and its artists                                                                                          |
| `aede search <text>`                                      | Search across the whole catalog (`--comments` looks in the comment tag)                                                      |
| `aede file <path>`                                        | Inspect a single file, outside the catalog                                                                                   |
| `aede export`                                             | Export the catalog as JSON, or as CSV with `--csv`                                                                           |
| `aede copy <destination>`                                 | Copy a selection to a player, a card or a drive, keeping its folder tree                                                     |
| `aede import <report…>`                                   | Take in a FlacCompagnon report (`--pending` lists the folders whose analyses match no file yet, `--forget` removes analyses) |
| `aede reset`                                              | Remove the catalog, after confirmation (`--yes` skips it)                                                                    |
| `aede love\|rate\|note\|tag <kind> <name>`                | What you think of it: a favourite, 1–5 stars, a note, free labels (`tag` takes a comma-separated list)                       |
| `aede favourites` / `notes` / `history`                   | What you wrote, and what you played                                                                                          |
| `aede played <track>`                                     | Record a listen, until playback records its own                                                                              |
| `aede query <expression>`                                 | Every track an expression matches, as a selection                                                                            |
| `aede collection <name>`                                  | Save a query under a name, run it, or drop it                                                                                |
| `aede collections`                                        | The saved queries, and how much each holds now                                                                               |
| `aede help`                                               | Every command and every option, which is the contract                                                                        |

`--json` produces machine-readable output wherever `--csv` does — the same rows, typed — plus `stats`, `doctor`, `search` and `track`, which have shapes of their own. `aede help` lists every option.

The catalog lives in `$AEDE_HOME`, or `~/.local/share/aede/catalog.json`.

### Getting the data out

Three formats, because they answer three different questions.

**JSON** (`aede export`) is the faithful dump: ten linked tables, one per table of the model. It is what rebuilds a catalog or feeds another program.

**CSV** (`aede export --csv`) is for a spreadsheet, and a spreadsheet cannot hold a graph. It writes **one row per album** — artist, title, year, track and disc counts, duration, size, formats, sample rates, bit depths, label, catalogue number, genres, integrity, folder — which is the view from above: sort by size to find what to re-rip, filter on `lossless` to see what is left to replace. `--tracks` switches to one row per track when the album is too coarse.

Its values are **raw**: `duration_ms` and `size_bytes`, not `4:20` and `31.2 MB`. A column that cannot be added up is a column that cannot be used.

`--separator=;` for Excel in a French or German locale, `--separator=tab` for a TSV. Fields are quoted per RFC 4180, so a title carrying a comma or a quotation mark does not shift every column that follows.

**M3U** (`--m3u`) is not an export of the catalog but of a **selection**: whatever is on screen becomes a playlist.

```sh
aede album "To Hell With God" --m3u --output=deicide.m3u8
aede search coltrane --m3u --output=coltrane.m3u8
mpv --playlist=deicide.m3u8
```

Paths are absolute, so the playlist works wherever it is saved; `#EXTINF` carries the duration and the title, so a player shows them without opening every file. Without `--output` it goes to standard output, which a shell supporting process substitution can hand straight to a player — `mpv --playlist=<(aede artist "Ozzy Osbourne" --m3u)`.

### Where each option applies

Three groups, and an option that a command cannot honour is **refused**, never ignored. So is an **argument**: `aede artists ozzy` used to list every artist and drop the word, which looks like an answer. It now says what to type instead.

`export` describes the **catalog**: `--csv` gives one row per album, `--tracks` one row per track. It takes no argument.

The **listings** — `albums`, `artists`, `genres`, `labels`, `years` — turn into a table of exactly what they show, filters included. This is how several albums land in one file:

```sh
aede albums --csv --artist="Deicide" --output=deicide.csv
aede albums --csv --year=1990
aede albums --csv --compilations --output=compilations.csv
aede artists --csv --limit=100 --output=artists.csv
```

`album`, `artist`, `track` and `search` describe a **selection**: `--csv` and `--m3u` both apply to it, as a table of tracks or as a playlist. For an artist, that means the tracks they are audible on; for a search, the track hits and not the artists or albums found.

```sh
aede album "To Hell With God" --csv --output=album.csv
aede artist "Deicide" --csv | sort -t, -k9 -n     # sorted by size
```

`aede album` takes **one** title — the words are joined so a title can be typed without quotes — and says which command lists several when given more.

The same holds for an option whose value is a **name**: `--artist`, `--album`, `--with`, `--genre` and `--label` take the words that follow, up to the next option. `--limit`, `--year`, `--output` and the rest take exactly one word, because a number or a path is one word.

```sh
aede artist Ozzy --with Zakk Wylde        # no quotes needed anywhere
aede artist Ozzy --with "Zakk Wylde"      # the same thing
aede track So What --artist Miles Davis --limit 1
```

Put the positional before the option: `aede track --artist Miles Davis So What` gives the whole tail to `--artist`, and the command then says it was given no title — rather than answering a question you did not ask.

`--output <file>`, or `-o`, writes wherever these produce text, and states where it went instead of filling the terminal.

```
$ aede stats

Library

  Tracks                        20
  Albums                         6
    of which compilations        1
  Artists                        8
  Total duration              38 s
  Size on disk              1.3 MB

Quality

                       Count      Size
  ───────────────────  ─────  ────────  ────────────────────
  Lossless (CD)           11  399.3 kB  ████████████████████
  Hi-res                   4  664.3 kB  ███████·············
  Lossy (>= 256 kbps)      3  248.3 kB  █████···············
```

### Writing things down

One note per thing — a track, an album, an artist, a label, a genre — and it is
kept **exactly as typed**, blank lines and all. A note is not a field to be
tidied: no wrapping, no trimming, no reflowing.

```sh
aede note album "Kind of Blue" --text "the 1997 remaster is the one"
aede note album "Kind of Blue" --file notes/kind-of-blue.md
vim /tmp/note.md && aede note artist "Miles Davis" --file /tmp/note.md
somecommand | aede note artist "Miles Davis" --file - --append
aede note artist "Miles Davis"            # reads it back
aede note artist "Miles Davis" --remove
aede note album "Legion" --from album:"Once Upon the Cross"
```

`--file` is what makes a note a _written_ thing rather than a command-line
argument: write it in a real editor, pipe it in with `-`. `--append` adds to
what is there, separated by a blank line, because two thoughts a month apart
are not one paragraph.

It gets a section of its own on every page, below the marks:

```
Yours

  ★★★★★   ♥   vinyl

Notes

  # Kind of Blue

  The 1997 remaster is the one: the first three sides
  run fast on the original pressings.

  written 3 days ago
```

**Markdown is the intended format**, and deliberately not handled here. Aède
stores the bytes it was given and prints them unchanged; rendering headings and
emphasis is the front end's job at M2. Two things follow for whoever writes that
front end: the text is **untrusted user input**, so it must be escaped before it
reaches any HTML, and the storage must never start "helpfully" rewriting it —
the day Aède reformats a note is the day the note stops being the user's.

### Saving a question

A query worth typing twice is worth a name.

```sh
aede collection wishlist --query "loved played:0"
aede collection wishlist                 # what it holds now
aede collection wishlist --m3u           # …as a playlist
aede collections                         # every saved query, and its size
aede collection wishlist --remove
```

It keeps the **question**, not the answer, which is the whole difference
between a smart collection and a playlist: it answers with what the library
holds now. And since running one produces a selection, `--m3u`, `--csv` and
`--json` apply to it with nothing written for the purpose.

An expression that does not parse is refused **when it is saved**, not the next
time somebody opens it: a collection that only fails when you reach for it is a
trap left for later.

### Backing up what cannot be rebuilt

```sh
aede notes --export -o backup.json
aede notes --import backup.json
```

Lose the catalog and a scan rebuilds it in a minute. Lose this and it is gone,
so it is the one file worth a backup — and the export is the file itself:
readable, greppable, repairable by hand.

**Import merges, it never replaces.** Someone restoring half a backup wants
their two halves, and an import that emptied what was already there would be
the one operation in this program able to lose everything at once. Where both
sides know a thing, the one written **last** wins, and the one that lost is
counted out loud rather than dropped in silence. Play counters take the larger
of the two, since a count is a total and neither side ever counted the other's
listens. Importing the same backup twice changes nothing.

### Asking a question

Options compose by AND and by nothing else. That ceiling is what a grammar
lifts: there is no `--genre metal OR --genre jazz`, no "everything except this
label", no "between 1990 and 1999" — and all three come free with one parser.

```sh
aede query "genre:metal year:1990..1999 -label:earache"
aede query "(artist:ozzy OR artist:dio) album.rating:>=4"
aede query "loved played:0" --m3u          # what I love and have never played
aede query "lossless:false size:>50000000" # big, and not lossless
```

Fields: `title`, `artist`, `album`, `albumartist`, `genre`, `label`, `comment`,
`path`, `codec`, `year`, `duration`, `size`, `bitrate`, `samplerate`,
`lossless`, `compilation`, `played`, and what you wrote — `rating`, `loved`,
`tag`, `note`.

**And who did what**, which is what a graph is for: `composer`, `lyricist`,
`producer`, `engineer`, `performer`, `conductor`, `remixer`, `featured`,
`mainartist`, and `performing` for anyone audible on it — the class that
counts a guest verse and not the words behind it. `artist:` matches any credit
in any role, which is why
`artist:ozzy artist:"zakk wylde"` already means "both are on it"; the role
fields ask the finer question.

```sh
aede query "composer:rhoads mainartist:ozzy"   # Ozzy singing what Randy wrote
aede query "producer:\"rick rubin\" year:1990.."
```

A value naming nothing in the library — a genre that does not exist, an artist
nobody ever heard of — is an **error**, not an empty result: the two are
different questions and deserve different answers.

A flag reads either way round: `lossless:false` and `-lossless` ask the same
thing, and accepting only one would make the other a silent trap.

Those last four also read `album.rating`, `artist.loved` and so on, because
**where** an opinion was written is part of what it says: five stars on the
artist is not five stars on the track, and a field that folded the two together
could never say which was meant.

Ranges are inclusive and either end may be left open (`year:1990..`,
`duration:..3:30`); comparisons are `>`, `>=`, `<`, `<=`; `field:=value` asks
for an exact match where a bare value asks for a substring; a length may be
typed `3:45` or in seconds; `-` or `NOT` negates; juxtaposition means AND.

A track with nothing to compare is **absent** from a numeric answer rather than
counted as zero — otherwise every untagged file would file itself under "before
1970".

What the command produces is a **selection**, so `--csv`, `--json` and `--m3u`
apply to it exactly as they do to an album page. That is why the grammar
evaluates over tracks: a saved query is a smart collection, and a smart
collection that is already a selection is already playable.

### Browsing by facet

The model is a graph of entities carrying roles towards one another, and every entity deserves a page. `artist`, `album` and `track` had one; `genre` and `label` now do too.

```sh
aede genre metal            # the albums, and who is audible on them
aede label "Blue Note"      # the catalogue, and its artists
```

A name matching nothing exact widens to the names containing it — `aede genre metal` reaches Black Metal and Doom Metal — and says it did. What the page gathers is a **selection**, so `--csv` and `--m3u` apply: `aede genre jazz --m3u` is a playlist of every jazz track.

The listings take the same facets as filters:

```sh
aede albums --genre metal --year 1994
aede albums --label "Blue Note"
```

**Roles read both ways.** `--role` means two things, depending on what it is attached to — and both are useful:

```sh
aede artists --role producer                    # who produces, in my library
aede artist Ozzy --role performer               # what Ozzy sang on
aede artist Ozzy --role performer --m3u         # …as a playlist
aede artists --role composer --csv --output=composers.csv
```

On the **listing**, it answers "who does this here". On one person's **page**, it answers "what did they do in that role" — one step finer than the performing/writing split the page already shows. That a role can be read in both directions is the whole reason credits store one rather than a bare artist column.

It needs a person, so `aede album "<title>" --role performer` is refused: a role with nobody attached asks nothing. There, `--artist` is the filter.

A role is typed the way it is **shown**: `--role "album artist"` as well as `--role album`, in either case, with or without quotes. What a screen displays must be what the parser accepts, or the program contradicts itself — as it did, denying a role and listing it among the artist's credits in the same message.

Three different answers, because they are three different situations: a word that names no role at all lists the ones that do; a real role this library happens not to hold says so; and a role the _person_ does not hold names the ones they do. `aede stats` shows the whole vocabulary **this** library holds, with counts — so a role that returns nothing can be told apart from a bug: your files simply never carried that tag.

```
Roles

  Role      Artists  Credits
  ────────  ───────  ───────
  composer       48      412
  producer       11       87
```

`main` and `album` are left out: every track and every release carries them by construction, so they say nothing. At M1, a role coming from MusicBrainz will work here without a line of code, because the list is read from the credits rather than fixed.

### Comments

The `comment` tag is the one field _you_ write: where a rip came from, which pressing this is, what still needs replacing. It is read from every format and it is searchable, but only when asked:

```sh
aede search --comments "vinyl rip"
aede search --comments "to replace" --m3u --output=todo.m3u8
aede track "So What" --comment "2009 remaster"
aede albums --comment "vinyl"
```

Off by default on `search`, because a comment is free prose: a common word in one would bury the album that actually bears the name. Comment hits are shown in **their own section** and marked `found_in: comment` in the JSON — a hit says by which route it was found, the same rule that keeps an imported analysis in its own panel.

### Compilations

A **compilation** is a release with no album artist: several artists share it, which is why it stays out of every discography. Nothing else in the program singles them out, so `albums` does:

```sh
aede albums --compilations       # only the ones several artists share
aede albums --no-compilations    # everything except those
```

Asking for both is refused — they are opposites, and an empty answer would look like a library with nothing in it.

### Paging through a result

Every listing shows **50 rows** by default and says which ones they are:

```
  1–50 of 312 albums — --offset=50 for the next page, --all for every row
```

Three options, one window, the same everywhere:

```sh
aede albums                        # the first 50
aede albums --limit 50 --offset 50 # the next 50
aede albums --all                  # every row, however many
aede albums --all --csv -o all.csv # and into a file
```

`--offset` is what a front end needs: the order of every listing is deterministic, so page two is genuinely the rows after page one. `--all` says "everything" by name rather than by an encoding to remember — `--limit=0` is refused, since it would show nothing, and so is `--limit abc`, which used to fall back on the default and answer a question nobody asked.

Nothing is printed when everything fit, so the line keeps meaning something. A window past the end says so rather than showing an empty screen that reads as an empty library.

`-o` is short for `--output`.

### Trying it without a library at hand

```sh
tools/demo-library.sh /tmp/demo-music   # requires ffmpeg
aede scan /tmp/demo-music
aede doctor
```

The demo library is deliberately damaged: untagged files, a duplicate, an album missing a track, an album with mixed formats. Enough for `doctor` to have something to bite on.

### Reading the scan report

| Line                      | What it counts                                                                                                      |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Files found               | Audio files seen while walking the folders, duplicates removed                                                      |
| Read from disk            | Files whose tags were parsed: new ones, and those changed since the last scan                                       |
| Reused from previous scan | Files identical in path, size and modification time; their tags came from the catalog, untouched on disk            |
| Gone since last scan      | Files the catalog knew and that are no longer there; they leave the catalog                                         |
| Analyses imported         | [FlacCompagnon reports](#what-another-tool-found) found in the folders and taken in; only shown when there were any |
| Analyses now attached     | Imported analyses that were waiting for a file and found it this time                                               |
| Elapsed                   | Wall-clock time of the whole scan, folder walk included                                                             |

`Files found` is always the sum of the two middle lines. A file that could not be read is listed underneath with the reason, and stays out of the catalog without stopping the scan.

### Starting over

`aede roots` weighs each watched folder, so the list answers "what is on this drive" and not merely "which drives":

```
Watched folders

  Folder                 Tracks  Duration       Size
  ─────────────────────  ──────  ────────  ─────────
  /Volumes/Music/FLAC     18 402   52 days     4.1 TB
  /Users/kcell/Music         746   2 days    112.4 GB
  (no longer watched)         92   6 h        8.1 GB
```

The last row appears only after `roots --remove`: those files stay in the catalog until the next scan, and a table that hid them would make that promise unverifiable.

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

## Are the files still intact?

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

### How long it takes, and how to start small

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

What this does **not** prove is that the audio itself is untouched — a stream re-encoded consistently would pass. FLAC also stores an MD5 of the _decoded_ audio, and checking it means decoding; that verdict arrives with the playback engine at M3, and the stored shape already accommodates it. Until then, [taking in another tool's analysis](#what-another-tool-found) fills the gap for whoever already has one.

## Copying to a player

`aede copy` is the one command that writes files, and it writes them **outside** the library. A player, an SD card, an external drive — somewhere that is not a catalog and will never be scanned.

```sh
aede copy /Volumes/Player                                   # the whole library
aede copy /Volumes/Player --query "loved rating:>=4"        # a selection
aede copy /Volumes/Card --collection wishlist --verify      # a saved query, read back
aede copy /Volumes/Player --dry-run                         # what it would do, writing nothing
```

**The selection is the grammar's**, not a set of filters of its own — the rule every listing already follows. Whatever `aede query` would have listed is what `aede copy --query` writes. With neither `--query` nor `--collection`, the whole library goes.

**The tree is kept relative to the watched folder that holds each file.** A track scanned under `~/Music` at `Ozzy Osbourne/1980 Blizzard of Ozz/01.flac` arrives at exactly that path under the destination. Inventing a layout from the tags would be a different feature — organising — and one this project has not decided it wants. A file sitting under no watched folder has no tree to keep, so it is reported rather than dropped at the top level among the ones that do.

### What travels beside the audio

| `--extras`          | What comes                                                                       |
| ------------------- | -------------------------------------------------------------------------------- |
| `none`              | Audio only. Cover art embedded in the tags travels anyway: it is inside the file |
| `cover` _(default)_ | The one cover the catalog identified for the release                             |
| `images`            | Every image in the folder                                                        |
| `all`               | Everything beside the audio: logs, cue sheets, reports                           |

The default is `cover` rather than `images` for a reason worth spelling out: **a rip folder's spectrograms and booklet scans are PNGs too.** Filtering on the extension copies exactly what you were trying to leave behind. The catalog already knows which file is the cover — the scan picked it by rank and stored it on the release — so `cover` is an exact answer where `images` can only be a guess.

### Names a player refuses

FAT32 and exFAT — which is what a card or a player almost always is — reject `? * : " < > |`, trailing dots and spaces, and the old DOS device names. A music library is full of them: `Where Is My Mind?`, `Symphony No. 5: Allegro`. Left alone, the copy fails on those files one at a time, twenty minutes into a run.

Aède asks the destination what it accepts by **writing one probe file into it**, rather than reading the filesystem's name and inferring. The empirical answer is right where the inference is wrong: a FUSE mount, an SMB share of a Windows folder or a card reader all report something no table lists. `--safe-names` and `--raw-names` force it either way.

Every adapted name is **listed, not counted** — a copy whose names quietly differ from the library is a copy nobody can compare against the original afterwards. Where two different names adapt to the same one (`Vol. 1: Live` and `Vol. 1? Live` both become `Vol. 1_ Live`), a counter keeps them apart rather than letting one overwrite the other.

### Getting it there intact

Size is checked on every file, always: it costs one metadata read and catches what actually goes wrong — a run interrupted mid-file, a disk that filled up. Each file is written under a temporary name and moved into place, so an interrupted run never leaves half a file wearing a whole one's name, and re-running skips what is already there at the right size.

`--verify` adds a full read-back and CRC-32 comparison. Two honest limits: the file is flushed to the device before being read back, but a read can still be served from the kernel's cache — this proves the bytes made it through the program and the filesystem, not that they reached the platter; and a CRC-32 detects accidental corruption, it is not meant to resist anyone deliberately producing a collision. Nothing here is a security boundary.

### What it refuses

**A destination that does not exist.** The folder is never created for you: `aede copy /Volumes/Player` with the player unplugged would otherwise create that folder on the internal disk and quietly fill it.

**A destination inside a watched folder.** The next scan would read the copies back in, the catalog would double, and `doctor` would report every album as its own duplicate.

**Not enough room**, checked before the first byte rather than discovered on the last album.

### Converting on the way out — not yet

Transcoding to fill a small card (`--compress mp3` and friends) is the second half of this feature and is not implemented yet. It will drive **ffmpeg** as an external program, the way beets does, rather than taking a codec library as a dependency — which means it will be optional, detected at run time, and refused with a clear message when ffmpeg is not installed. Two decisions are already settled: a source that is already lossy is copied as it stands rather than re-encoded a second time, and metadata follows the audio.

Note that writing tags into a _derived copy_ is not the same act as rewriting the tags of your library, which this project refuses to do. The distinction is deliberate, and recorded as such.

## What another tool found

Entirely optional, and it changes nothing if you never use it.

Aède reads the _structure_ of a file. It does not decode, so there are questions it cannot answer yet: is this FLAC a re-encoded MP3, was it upsampled, where does the spectrum stop, how loud is it really — and the decisive one, does the decoded audio still match the MD5 the encoder wrote into the file.

[FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) already does that pass. If you have run it, `aede import` puts the results into the catalog:

```sh
aede import ~/Desktop/danzig-report.json
aede import ~/Desktop/reports/            # every .json underneath, at any depth
aede import --pending                     # which folders have no matching file yet
aede import --forget                      # remove them all
aede import --forget --source=flaccompagnon
aede import --forget --pending            # remove only what will never attach
aede import --forget --pending "/Volumes/OldDrive"   # …and only under that folder
```

A folder is walked **recursively**, because reports are kept the way the albums they describe are: one folder per artist, one per album.

### The order does not matter

An analysis is filed under the **path** it describes, not under a catalog entry. So the two operations can be done either way round, which matters because analysing a folder and _then_ building the library from it is the natural order for someone who already owns the other tool.

- **Import first.** The records are stored and reported as `Waiting for a scan`. The scan that brings those files in makes them attach by themselves, and says so (`Analyses now attached`). `doctor` says how many are still waiting rather than letting them sit there unmentioned.
- **Scan first.** Files are matched by path, then by name and size for a library that has moved since — a name and a byte count together are very nearly unique.
- **Leave the report in the album folder.** A scan walks over it anyway: any `.json` announcing itself as a FlacCompagnon report is read and taken in, and the scan report says how many. Half a kilobyte is read from each `.json` met to recognise one, so nothing else in the library is parsed.

Matching never relies on the two paths being written the same way. Watched folders are stored canonical, so a report produced against a symbolic link — or against `/var` where macOS says `/private/var` — names the very same file by a string that will never compare equal; the name and the size bridge the two, and the record is then refiled under the path the catalog uses.

### When a scan does not make it go away

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
  Verdict          Clean — full-band content to ~22.1 kHz, no lossy signature.
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

The frame checksums prove the _container_ was not corrupted; the MD5 proves the _audio_ is the audio that was encoded. A file passes the first and fails the second when it was re-encoded by a tool that rewrote the frames but kept the old signature — exactly the case Aède cannot see before it decodes anything itself. A lossy ancestry — transcoded, upscaled, upsampled — is reported as a warning, with the reason and the source named.

### Where it is all stored

In the catalog, and nowhere else: `~/.local/share/aede/catalog.json` grows one more table, `analysis`, one row per path and per source. The report you imported is never referred to again — you can move it or throw it away. `aede export` includes the table, `aede import --forget` empties it, and `aede reset` warns about it before removing the catalog.

`--data <folder>` puts the catalog elsewhere, `$AEDE_HOME` does the same by environment. `aede roots` ends by naming the file it just read, so the answer to "where is all this kept" is on the screen that lists what is watched.

## Supported formats

| Container   | Codecs                    | Tags                          | Duration from                  |
| ----------- | ------------------------- | ----------------------------- | ------------------------------ |
| FLAC        | FLAC                      | Vorbis Comment, leading ID3v2 | STREAMINFO                     |
| MP3         | MPEG 1/2/2.5 layers I–III | ID3v2.2/2.3/2.4, ID3v1        | Xing / VBRI / constant bitrate |
| MP4         | ALAC, AAC                 | iTunes atoms, freeform `----` | `mvhd`                         |
| Ogg         | Vorbis, Opus              | Vorbis Comment                | Granule position               |
| WAV         | PCM                       | `LIST/INFO`, `id3 ` chunk     | `fmt ` + `data`                |
| AIFF / AIFC | PCM                       | `NAME`/`AUTH`, `ID3 ` chunk   | `COMM`                         |

Extensions: `.flac` `.mp3` `.m4a` `.m4b` `.mp4` `.alac` `.ogg` `.oga` `.opus` `.wav` `.wave` `.aif` `.aiff` `.aifc`

The formats below are read through [`lofty`](https://crates.io/crates/lofty), which takes over whenever the signature matches none of the parsers above:

| Container      | Codecs           | Tags           | Duration from      |
| -------------- | ---------------- | -------------- | ------------------ |
| AAC            | AAC              | ID3v2, ID3v1   | ADTS frame headers |
| WavPack        | WavPack          | APEv2, ID3v1   | Block headers      |
| Monkey's Audio | APE              | APEv2, ID3v1   | Descriptor         |
| Musepack       | Musepack SV7/SV8 | APEv2, ID3v1   | Stream header      |
| Speex          | Speex            | Vorbis Comment | Granule position   |

Extensions: `.aac` `.ape` `.wv` `.mpc` `.mp+` `.mpp` `.spx`

The fallback is only ever reached last, so it can never take a format away from a parser above.

The parsers are written by hand from the specifications and validated against real files produced by ffmpeg and cross-checked with ffprobe (`crates/aede-core/tests/real_files.rs`). The awkward cases are covered: UTF-16 with a BOM, ID3v2.3 unsynchronisation, numeric genres like `(17)Rock`, the LAME encoder delay (needed later for gapless playback), the ALAC magic cookie for the real bit depth, the Opus pre-skip.

No `unwrap` and no direct indexing anywhere in the parsers: a truncated file yields an error or a partial result, never a panic. A test checks this by truncating a real file to a quarter, a third and a half of its size.

## Dependencies

One: [`lofty`](https://crates.io/crates/lofty), which reads the formats listed above as its own. A dependency is a requirement here, not a dogma — it has to do something we could not do as well ourselves, be maintained and widely used, and bring a dependency tree small enough to read. `lofty` earns its place by covering a long tail of formats that would each take days to parse and would be exercised by a handful of files.

What it does not replace: the encoder delay and padding of the LAME tag, the ALAC magic cookie and the Opus pre-skip are not exposed by any general-purpose library, and milestone M3 needs them for gapless playback. That is why the main formats keep their own parsers.

## Architecture

```
crates/
  aede-core/        library, no terminal I/O
    src/tags/         one parser per format -> RawTags
      foreign.rs        the formats handed to lofty
    src/audit/        what a file contains, as opposed to what it claims
      integrity.rs      the checksums the containers carry
    src/model/        the graph: entities, credits, relations
      mod.rs            the entities, and the catalog that holds them
      query.rs          reading the graph (&self throughout)
      builder.rs        scanned files -> entities, deterministically
      relations.rs      the links inferred from credits and track lists
    src/scan.rs       directory walk, parallel reads, incremental
    src/store.rs      JSON persistence, atomic writes
    src/stats.rs      statistics
    src/doctor.rs     diagnostics
    src/analysis.rs   analyses imported from another tool
    src/text.rs       name normalization (how entities are matched)
    src/json.rs       minimal JSON reader and writer
    src/clock.rs      the one unit of time the catalog stores
    schema.sql        the SQLite schema targeted by milestone M1
  aede-cli/         the `aede` binary
    src/commands/     one module per group of commands
tools/              development scripts
CLAUDE.md           project conventions and invariants
```

### The decision that shapes everything

The model is not "an album belongs to an artist" but **entities carrying roles towards one another**. Two tables do all the work:

- `credit` — _who does what, on what_: performer, composer, conductor, producer, engineer…
- `relation` — _typed links between entities_, with a weight and a source.

This is what will let you click a drummer and see their forty appearances. A flat schema would have to be thrown away the day MusicBrainz arrives.

The graph is already usable at M0, with no network access at all: `aede artist "Queen"` shows "played with David Bowie", inferred purely from the fact that both are credited on the same track. `aede artist "Queen" --with "David Bowie"` then lists those tracks. The relation itself stores only a count; the tracks behind it are recomputed from the `credit` table on demand, so a corrected tag never leaves a stale list behind.

### A few choices worth knowing about

**The catalog is a JSON file whose every key is a table.** Not a stopgap: it mirrors `schema.sql` exactly. Moving to SQLite means rewriting `store.rs` and nothing else.

**Raw tags are kept per file.** The graph can therefore be rebuilt entirely without touching the disk, which is what makes the incremental scan possible and will allow undoing an automatic correction.

It is also what makes an upgrade painless. The `relation` table is **inferred** — collaborations from the credits, album copies from the track lists — so a new version of Aède infers more than the stored catalog holds. Rather than refuse to load it, or wait for a rescan the user has no reason to suspect is needed, the catalog carries the version of the rules it was built under: when they no longer match, the relations are recomputed on load, from what is already in memory. Nothing is re-read, and the integrity verdicts — which a rebuilt catalog would lose — stay where they are.

**Scanning is incremental by default.** Same path, size and modification time means the file is not read again. On the demo library: 63 ms on the first pass, 2 ms on the second. `--full` forces a re-read.

**Watched folders accumulate.** `aede scan ~/Music` then `aede scan ~/Live` watches both; a plain `aede scan` refreshes everything. `--replace` keeps only the folders given, and `aede roots --remove` drops one. A watched folder that has become unreachable aborts the scan rather than quietly emptying the catalog.

Dropping a folder from the list does not empty the catalog on its own: its files stay until the next `aede scan` rebuilds the catalog from the folders still watched. Run that scan **without naming a folder** — naming the one just dropped would simply watch it again.

**A row measures what it counts.** In _Appears on_, the duration and the size cover the tracks the artist is on, not the whole album: one guest song is one guest song, not forty minutes of somebody else's record.

**The same album twice is two albums, and the model says why.** A library holds the same record more than once for two opposite reasons: a hi-res copy kept beside the CD rip, or a folder copied and forgotten. Aède keeps both as separate releases — they are two sets of files, in two folders, and the folder is what you act on — and links them with a typed relation:

| Same title, same album artist, same track list | Link            | What it means                                      |
| ---------------------------------------------- | --------------- | -------------------------------------------------- |
| Same encoding on both sides                    | `duplicate`     | Nothing tells the copies apart: one is dead weight |
| Different encoding                             | `other_edition` | The second copy is there on purpose                |

Track positions and titles must match exactly; durations only have to be within three seconds, since two rips of one disc never agree to the millisecond. Two albums merely sharing a name are left unlinked — without MusicBrainz there is nothing reliable to say about them.

`doctor` reports a `duplicate` once, as a warning naming both folders and the space to be recovered, instead of once per track — a copied album used to produce thirteen identical lines. An `other_edition` is reported too, as information: it is a choice, not a defect. The artist page and the album listing mark the rows, and `aede album` names the other folders.

**A guest appearance is not part of a discography.** Singing one track on somebody else's album puts it under _Appears on_, never under _Discography_, and writing or production credits go in a third section. The performer rankings count performing credits only.

_Performing_ means credited in a role that makes the artist audible on the recording: `artist`, `albumartist`, `performer`, `featured`, `conductor`, `remixer`. A composer or a lyricist is not audible, so an artist page states its figures on two labelled lines — `performing:` and `writing:` — rather than one unlabelled count. A band's lyricist genuinely has no performing credit: what the files say is that the band played, not who in it.

**A name is matched exactly first.** `aede album "Danzig"` shows the 1988 record, not whichever of `Danzig 4` or `Danzig II` the catalog holds first — an exact title ends the search. Only when nothing matches exactly does it widen to the titles containing the text, and then it shows **all** of them, saying that it widened. `aede track` follows the same rule.

**A title is not an identifier.** `aede track "So What"` prints every track carrying that name — the studio take, the single, the live rendition — because they are different recordings. `--artist` and `--album` narrow it down, and a list cut short by `--limit` always says so.

**Construction is deterministic.** Files are sorted before processing, so two scans of the same library produce exactly the same identifiers. Without that, no readable diff and no reproducible test.

**`Various Artists` is not an artist**, it is the absence of an album artist. Recording it would pollute every count.

**Sizes are decimal, durations are rounded.** 1 kB is 1000 bytes, the convention macOS Finder and most Linux file managers use, so a figure here matches what the system says about the same files. A duration is rounded to the nearest second rather than truncated, as players do.

**Every listing carries the same measures.** Count, duration and size on disk appear on each entity page and in each listing: what a slice of the library weighs should not depend on the command used to look at it.

**An artist is counted once per track.** Miles Davis as both performer and composer of "So What" is one track, not two.

## Tooling

```sh
tools/check.sh        # formatting, lint, tests, documentation, release build
cargo doc --no-deps --open   # the API documentation
```

Every public item of `aede-core` is documented: the crate sets `#![warn(missing_docs)]`, so a gap is a warning and `tools/check.sh` fails on it. The check also builds the documentation with `RUSTDOCFLAGS="-D warnings"`, which makes a **broken link** an error too — nothing else reads doc comments, so moving an item between modules would otherwise leave dead references behind in silence.

Formatting is `rustfmt` (`rustfmt.toml`); Prettier only covers Markdown, JSON and YAML (`.prettierrc`). The project targets **zero clippy warnings**.

## Tests

```sh
cargo test
```

306 tests: binary parsers (including truncated files and forged signatures), name normalization, graph construction, persistence round-trip, statistics, diagnostics, table alignment, argument parsing, and an end-to-end test that runs the binary.

## Roadmap

**M0 — the catalog (this repository).** Scanning, graph model, statistics, diagnostics, command-line navigation.

**M0.6 — getting music out.** `aede copy`: a selection, written to a player or a card, keeping the tree it sits in. The selection is the query grammar's, so the command adds no filters of its own. What is settled and done: the tree, the companion files (with the catalog's own cover rather than an extension filter), name adaptation for FAT/exFAT probed rather than guessed, verification, resume, and the refusals — a destination that is missing, one inside the library, one without room. What remains: `--compress` through ffmpeg. See [Copying to a player](#copying-to-a-player).

**M0.5 — what the user writes, and how it is asked for.** Favourites, ratings, notes and free-form tags in one annotation store keyed so that a scan can never destroy them; play history and play counts; a real query grammar with ranges, negation and `OR`; saved queries; and export/import of the lot. What remains of it: relations inside the grammar, and today's options becoming shorthand for it rather than a second evaluator. Every one of those records carries an **owner** from its first version, so that the accounts arriving at M2.5 are a second value in a field rather than a migration. None of it needs the network or a database, and the identity design underneath it has to be right before anything else is built on top. See [What the user writes](#what-the-user-writes-favourites-ratings-notes-history), [several users](#which-is-the-same-question-as-several-users) and [Querying](#querying).

**M1 — identification.** MusicBrainz for relations and credits, AcoustID/Chromaprint for badly tagged files, Cover Art Archive for artwork, Wikidata to reach the Wikipedia article in the user's language — in the vast majority of cases it already exists, written by humans, so no machine translation is needed. Move to SQLite. Also: country and formation dates, band line-ups as dated relations, release types, and the completeness report that says which albums are missing from the shelf. The hard part will be matching files to releases: plan for a confidence score and manual correction, never a blind rewrite. See [Identification](#identification-m1).

**M2 — the API.** HTTP server, JSON and WebSocket. To be frozen early: it is the contract between the core and every future client. This is where `serde` becomes worth its place: hand-written serialization is fine for one internal format, but not for an HTTP contract with a dozen types on it. The current `json` module was written for the catalog file, and the move is meant to be mechanical.

**M2.5 — other servers' languages.** Subsonic and OpenSubsonic first: eighty-odd existing clients on every platform, which is the shortest path from "no mobile client" to "thirty of them". Jellyfin afterwards, and timeboxed. Both as translations over the M2 API, never as a second core. See [Speaking other servers' languages](#speaking-other-servers-languages).

**M3 — playback.** Local output, queue, gapless playback, loudness normalization (EBU R128). The decoder written for it also brings the FLAC MD5 check, which verifies the decoded audio rather than the container. See [Playback](#playback-m3).

**M4 — the network.** Remote playback endpoints. `slimproto` gives the best effort-to-result ratio, since it opens up a fleet of existing devices without reinventing anything; UPnP/OpenHome afterwards for commercial hi-fi streamers.

**Lyrics.** Reading them from files and `.lrc` sidecars needs nothing and can happen at once; fetching them belongs to M1 and behind an explicit choice; showing them in time belongs to M3. See [Lyrics](#lyrics).

**Independent of the sequence.** Transcoding through ffmpeg, which depends on nothing above and is held back only by the rule that its output must never re-enter the library — see [Converting files](#converting-files).

Explicitly out of scope: RAAT and the "Roon Ready" certification are proprietary and licensed — there is no technical path to them.

## Notes towards what comes next

Nothing in this part is built. It is written down while the reasoning is fresh,
so that each milestone starts from a position rather than from a blank page.
Anything in it may turn out to be wrong once there is code under it.

## Playback (M3)

### The queue is a selection, not a new idea

Every page in Aède gathers a **selection** — that is what `--csv` and `--m3u`
already render, through one helper that no command knows the details of. A queue
is that same selection with a cursor on it. So the three ways a queue gets
filled are not three features:

- built by hand in the interface, track by track;
- taken from any Aède result — `aede artist Ozzy`, `aede genre metal`,
  `aede albums --year 1991` — anywhere `--m3u` works today;
- read from an M3U file, which is the same list written down.

Which means playback should not need a query language of its own. If a command
can hand its tracks to a playlist, it can hand them to the queue.

The transport is the small part: play, pause, stop, next, previous, seek. One
convention worth settling early because everyone has an opinion about it:
**previous** restarts the current track when more than a few seconds have been
played, and only goes back a track before that. Anything else makes the button
unusable for its actual purpose, which is "wait, play that again".

### Order is a permutation, not a coin flip

Repeat (one track, the whole queue) and shuffle are properties of the _order_,
not of the queue. This distinction is load-bearing:

**A shuffle produces an order, once, and the queue then holds that order.** It
does not draw a new track at each end-of-track. Drawing each time is how players
end up unable to say what comes next, and unable to go back — "previous" has
nothing to be previous to. Producing the order up front costs nothing and buys
both.

**The order is seeded, and the seed is stored with the queue.** Determinism is
the first invariant of this project, and a shuffle is no exception: the same
queue and the same seed give the same order, on any machine, for ever. That
makes a session reproducible, a complaint about a bad sequence reproducible, and
a "what is coming next" panel possible without the server having to commit to
anything it might contradict later. A shuffle that reaches the end and repeats
draws its next seed from the current one, so the second pass is not the first
one again.

### Styles of shuffle

Several are worth having, and they should be _one_ algorithm with two knobs
rather than six algorithms:

| Style      | What it does                                                                                                                                                       |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `uniform`  | The classic. A seeded Fisher–Yates over the queue.                                                                                                                 |
| `by-album` | Shuffles albums, keeps each album in its own order. An album is a sequence somebody intended.                                                                      |
| `spread`   | Uniform, but never two tracks of the same artist within _n_. Fixes "it played four Deicides in a row" without pretending that is not what uniform randomness does. |
| `similar`  | Stays close to the track it started from.                                                                                                                          |
| `journey`  | Deliberately drifts: ends somewhere else, having got there by walking.                                                                                             |
| `discover` | Weighted towards what has never been played, or not for a long time.                                                                                               |

### The smart shuffle, without a language model

The one that needs actual thought: a random that stays in the style of the first
track, may move to another, and never jumps — no Black Metal straight into Pop,
no Bach into rap.

The material is already in the catalog, which is the point of having built a
graph rather than a hierarchy:

- **A genre neighbourhood learned from the library itself.** Two genres are
  close when they keep turning up on the same releases and the same artists.
  Count the co-occurrences, normalise, and you have a weighted graph with no
  hand-written taxonomy in it and no external ontology to argue with. In a
  library like this one, Black Metal and Death Metal will sit next to each
  other because they genuinely do; Black Metal and Pop will have no edge at all,
  because nothing in the library connects them. The graph fits _this_ library
  rather than someone's idea of how music is organised, which is both the
  strength and the limit: a library of two genres has nothing to walk on.
- **The artist relation graph**, which `relations.rs` already builds from shared
  credits. Two artists who played together are near each other whatever their
  tags say.
- **Year and label**, worth small weights: a label is a curator, and a decade is
  a production sound.

From those, a distance `d(a, b)` between two tracks in `[0, 1]`, and then a
walk:

1. take the tracks within radius `r` of the one playing;
2. weight them by closeness — nearer is likelier, but not certain;
3. draw one with the seeded generator;
4. **never take a step longer than `d_max`.**

Rule 4 is the whole guarantee, and it is worth stating on its own: the style may
change only by _walking_, never by jumping. Black Metal reaches Pop only through
whatever lies between them in this library, one bounded step at a time, which is
to say it will usually not get there at all — and that is the desired behaviour,
not a limitation. The rule is local, cheap, and easy to test.

The drift is then a single parameter: `r` grows slowly with the number of tracks
played and resets when the user intervenes. `similar` keeps it low, `journey`
lets it climb, and `uniform` is the same walk with `r` unbounded. Two knobs,
six behaviours.

Plus memory, which every shuffle needs: no track twice within _m_, no artist
within _n_.

**All of this is pure computation over the catalog.** It belongs in
`aede-core`, it is unit-testable with no sound card and no audio at all, and it
should be written and tested before a single sample is decoded. The interesting
half of M3 does not need speakers.

### Volume, and position

Two questions worth answering before they get answered by accident.

**Volume is not Aède's business.** The system mixer owns it. A program that
keeps its own volume alongside the system's gives the user two knobs that
disagree and no way to tell which one is at fault.

**Loudness normalization is Aède's business**, and it is a different thing.
The tags already carry `REPLAYGAIN_*` and, for Opus, `R128_*` — the parsers see
them today and throw them away. M3 reads them, and for files that have none the
decoder can compute EBU R128 and store it exactly as an integrity verdict is
stored: measured once, kept, recomputed only on request. Aède decides what gain
to apply to the stream; the user decides how loud the room is.

**Position depends which position is meant.**

- The point reached in the current track is player state. The transport has to
  expose it — "next" and gapless are meaningless without it — and M2's WebSocket
  is where it belongs. It is not catalog data.
- Where the user stopped, kept for next time, is worth having. But not in
  `catalog.json`: that file is written whole, and a position that moves several
  times a second would rewrite the entire library on every tick. A small session
  file of its own, written often and cheap to lose.
- Resuming mid-track only makes sense for long pieces — an audiobook, a
  concert, an hour of Wagner. For a four-minute track, remembering the queue is
  enough and remembering the second is noise.

### Gapless, which is already half done

The hand-written parsers extract the LAME encoder delay and padding, the Opus
pre-skip and the ALAC magic cookie — none of which a general-purpose tag library
exposes, and all of which exist in this codebase for exactly this milestone.
M3 spends them. That was the bet made at M0, and it is the one worth checking
first: if the numbers turn out to be wrong, everything above is premature.

### What M3 must not become

No writing tags, no reorganising files on disk, no cloud, and no second
catalog. Playback reads; the scan is still the only thing that writes what the
library is.

## What the user writes: favourites, ratings, notes, history

This is the part that needs deciding **first**, before any of it is built,
because it is the only data in the whole program that cannot be recovered. Lose
the catalog and a scan rebuilds it in a minute. Lose the notes and they are
gone.

### One shape, not five

Favourites, ratings, notes and free-form user tags look like four features. They
are one: **something the user wrote about an entity of the catalog.** They
should be one table, one file, one reconciliation, one import/export — and one
place to get right.

```rust
struct Annotation {
    owner: UserRef,             // whose opinion it is — see below
    target: EntityRef,          // what it is about
    loved: bool,                // a favourite
    rating: Option<u8>,         // 1..=5
    note: Option<String>,       // free text
    tags: BTreeSet<String>,     // free labels: "to rip again", "vinyl"
    created_at: u64,
    updated_at: u64,
}
```

One record per target rather than one per fact, which makes the operation of
copying everything said about one album onto another a single record copy
rather than a loop over four kinds.

Any entity can be a target: track, release, artist, label, genre. Not just
albums — a note on a label ("great remasters, bad pressings") is exactly the
kind of thing that gets lost otherwise.

**Tags are the one annotation that is naturally plural**, and the command reads
that way. A record is vinyl _and_ rare _and_ to-rip-again, so all three go on in
one go, and come off the same way:

```sh
aede tag album "Legion" vinyl,rare,to rip again   # or: vinyl, rare, to rip again
aede tag album "Legion" rare --remove             # that one
aede tag album "Legion" rare,vinyl --remove       # those two
aede tag album "Legion" --remove                  # every one of them
```

The comma is what makes this readable without quotes, and it has to be, because
a name is often several words and nobody quotes them: `aede tag album Kind of
Blue jazz` has always worked by taking the last word as the label. A list cannot
therefore mean "every word after the name" — nothing would say where the name
stopped. So the rule is that **a comma binds the words around it into the
label**, and the old single-word shape keeps its exact meaning. The one case the
comma cannot settle is an unquoted multi-word name with no label at all
(`aede tag album Kind of Blue --remove` reads "Blue" as a label, as it always
did); quoting the name settles it, and every confirmation names both the thing
and the labels, so a misreading shows on screen rather than passing silently.

**A favourite does not deserve a table of its own.** It looks like it should:
one boolean, one index, done. But the hard part of any of this is not the value,
it is the stable reference and the reconciliation that keeps it attached across
a rescan — and a second table means a second copy of that, kept in step by hand
for ever. One record per target, and `loved` is a field on it. Fast lookup is an
index, which is not the same thing as a table.

The `owner` is there from the first version, and has a section of its own
further down. On a single-user installation it always holds the same value
and looks like dead weight; it is the cheapest field in the whole design.

### Why the note is not the comment tag

Tempting, and worth saying why not — because the answer is not "purity", it is
the direction of writing.

**The comment tag lives inside the audio file.** Storing a note there means
Aède rewrites tags, which is the one thing this project has refused from the
start. Rewriting a FLAC to record "great remaster" changes the file: it moves
the mtime, so the next scan re-reads it; it invalidates the integrity verdict
that a whole subsystem exists to produce; and it cannot be undone. A note is not
worth touching the audio for.

**The comment tag belongs to whoever tagged the file, and often already says
something.** A library where tracks carry `comment=Vinyl rip, needs replacing`
is a library where writing notes into that field destroys real information —
information Aède reads and can already search on.

**And it does not reach.** A comment is a field on a file. There is no file for
an artist, a label or a genre, and no obvious file for an album — the note would
have to be copied onto every track and kept consistent, or written to one of
them and lost when that one moves.

So: a separate field, in Aède's own store. But the good half of the idea stands,
and it is the half worth keeping — **read the comment as a note, never write
it.** An entity then shows what the tagger wrote and what the user wrote, side
by side, each labelled with where it came from, exactly as this project already
distinguishes a fact read from a tag from one it inferred. `--comment` and
`--comments` then search both, which is a feature nobody has to build twice.

### The identity problem, which is the whole problem

Catalog identifiers are **positions in a vector that every scan renumbers.**
Annotations must therefore never be keyed by them — the same lesson the imported
FlacCompagnon analyses already taught, and it cost a rewrite to learn. An
`EntityRef` is a kind plus a **stable key**:

| Kind         | Stable key                                                    |
| ------------ | ------------------------------------------------------------- |
| Track        | the file path, with name + size as a fallback                 |
| Release      | album artist + title + folder, the key it is already built on |
| Artist       | the normalized name key, until M1 brings MBIDs                |
| Genre, label | the normalized key                                            |

And the same reconciliation as the analyses: an annotation whose target is not
in the catalog is **kept waiting, never dropped.** Rename a folder, rescan, and
the note reattaches when the path comes back — or attaches by name and size if
the file merely moved. Silently deleting what a user wrote because a file moved
is the one unforgivable failure in this program.

At M1 the MBID becomes a second key that survives renaming altogether, and the
path becomes the fallback rather than the other way round.

### Its own file, and it is the one worth backing up

Not in `catalog.json`. Two reasons, and the second is the real one:

- the catalog is derived from disk and reproducible; annotations are not
  reproducible from anything;
- the catalog is written whole. Ratings and play events change constantly, and
  rewriting the entire library to record a click is absurd.

A separate file, human-readable, hand-editable, and small. Export and import are
then almost free, and worth having from the first day: `aede notes --export` /
`--import`, merging rather than replacing, because a merge is what someone
restoring half a backup actually wants.

### Which is the same question as "several users"

There will be accounts: the Subsonic surface has them by definition, and Aède's
own front end will want them. That sounds like a separate, large subject. It is
not — it is **this** subject, seen from the other side, and the boundary drawn
above is already the answer:

|                                                                      | Belongs to  | Same for everyone?                  |
| -------------------------------------------------------------------- | ----------- | ----------------------------------- |
| Files, tracks, releases, artists, credits, relations, genres, labels | the catalog | yes                                 |
| Integrity verdicts, imported analyses                                | the catalog | yes — a measurement, not an opinion |
| Favourites, ratings, notes, tags                                     | a person    | no                                  |
| Play history and counts, queues, saved queries                       | a person    | no                                  |

Facts about the files are read from the disk or measured on it: two people
looking at the same library see the same ones. Everything a person _said or
did_ is theirs, always — including when there is exactly one of them.

**So the rule is: no per-user field ever lands on a catalog entity.** A
`rating` on a release, or a `play_count` on a track, looks harmless while there
is one user and becomes a question with no answer — _whose?_ — the day there are
two, by which time every read in the program assumes the single answer. As of
M0 that boundary is intact: every table in the catalog is a fact. It stayed that
way by luck rather than by intent, which is exactly why it is written down now.

**And the shape that means the migration never happens:** a per-user record
carries an **owner from the first version in which it exists**, and every read
filters by it, even when the only owner is the local one. The single-user case
is then the multi-user case with one user — one code path, exercised on every
run, instead of a second one written blind two years later. It is the same move
as `Window` for paging and the stable `EntityRef` for annotations: decide the
general shape once, then let the simple case be an instance of it.

That is the whole of what M0 owes the subject, and it costs one field.
Authentication is M2's problem — and Subsonic's legacy scheme in particular must
stay inside the compatibility layer rather than reaching the model.
Authorization is M2's too; the working assumption, there to be argued against
rather than from, is that the **library is shared and only the annotations are
private**: scanning, importing and resetting belong to whoever owns the
installation, not to a listener. The catalog itself has no owner, and that is a
decision rather than an oversight.

### Which is also what makes the move to SQLite cheap

M1 replaces JSON with SQLite **as the store**. That is not the end of JSON here:
`aede export` is the faithful dump, a different job, and it stays.

The migration is smaller than it looks, because **most of the catalog does not
need migrating at all.** Everything in it was read from disk and a scan rebuilds
it in a minute; rescanning is a perfectly good migration path for anything
reproducible.

What is not reproducible has to be carried across, and `aede reset` already
names the list, since it is the same one — what a rescan does _not_ bring back:

- the **integrity verdicts**, which can cost an hour of reading;
- the **imported analyses**, which cost a run of another program entirely;
- and, once they exist, the **annotations**.

All three live inside the very file M1 replaces. So M1 either reads the last
JSON catalog once to carry them over, or — better — the annotations are already
in a file of their own by then, which is what the section above argues for on
grounds that have nothing to do with SQLite. Doing M0.5 first shrinks the M1
migration to two tables and makes it a non-event.

Worth noting while on the subject of where things live: `--data <folder>`
chooses the location for one command, and `AEDE_HOME` chooses it for good. The
second is the least discoverable thing in the program, which is why the error
for a `--data` with no value names both.

### Play history, which has a different shape

An annotation is a statement; a play is an event. Append-only:

```rust
struct Play { owner: UserRef, track: EntityRef, at: u64, ms_played: u64, completed: bool }
```

The log itself should be **bounded** — the last few hundred events, shown as
"what did I listen to last night", filterable by artist, album, period. But
counters must **not** be bounded: a `play_count` and a `last_played` that never
forget, because M3's `discover` shuffle asks "what have I never heard", and a
truncated log cannot answer that. Two structures, because they answer two
questions.

The counters are per **owner and track**, not per track — "played eleven times"
is not a fact about the file, and a shared counter would make one listener's
history drive another's shuffle. The library-wide figure is then a sum, computed
when someone asks for it, which is what a total should always be.

`completed` matters more than it looks: a track skipped after eight seconds is
evidence _against_ it, and a rating system that cannot tell a skip from a listen
is measuring the wrong thing.

### Commands

Sketch, following the shape already in use — the page commands already resolve a
name to an entity, and that resolution is what should be reused:

```sh
aede love album "To Hell With God"      # and --remove
aede rate artist Ozzy --stars 4
aede note album "Animals" --text "…"    # --remove, --from album:"…" to copy
aede notes                              # every annotation, filterable, --export/--import
aede favourites                         # and aede history --limit 100
```

and, more usefully, as **filters on what already exists**, which costs nothing
because the filter machinery is built:

```sh
aede albums --loved --rating 5
aede artists --tag "to rip again"
```

None of this needs the network, SQL, or M1. It needs the identity design above
to be right, which is why it is written down before anything is typed.

## Querying

The roadmap says "SQLite at M1" and that has been quietly standing in for a
query language. It should not. **A query language is an interface, not a
storage engine.** Defined on its own it works today over the in-memory catalog
and tomorrow over SQL; defined as "whatever SQLite makes easy", it arrives late
and shaped by the wrong concerns.

Where things actually stand:

| Capability                       | Today                                                                                      |
| -------------------------------- | ------------------------------------------------------------------------------------------ |
| Several criteria at once         | Yes, and any depth of them                                                                 |
| Filters                          | Yes, per command, each refused where it means nothing                                      |
| Numeric and date ranges          | Yes: `year:1990..1999`, `duration:..3:30`, `rating:>=4`                                    |
| Sorting                          | Yes on `query` and `collection`; `artists` still has its own two keys                      |
| Pagination                       | Yes: `--limit`, `--offset`, `--all`, through one `Window`                                  |
| Aggregation and statistics       | Yes: `stats`, `years`, and counts, durations and sizes on every listing                    |
| Search on user tags              | Yes: `tag:`, `rating:`, `loved`, `note:`, and their `album.`/`artist.` forms               |
| Search on relations              | Yes: `artist:` is any credit, and `composer:`, `producer:`, `performer:`… ask who did what |
| `AND` / `OR` / `NOT`             | Yes                                                                                        |
| Saved queries, smart collections | Yes: `aede collection <name> --query "…"`                                                  |

**The options are shorthand for the grammar, not a second implementation.**
`aede albums --genre metal` builds `genre:metal` and hands it to the one
evaluator; a test walks both doors to the same room and demands the same
answer.

One mapping there is a decision rather than a transcription, and it is the kind
that would have gone unnoticed: **`--artist` on an album listing means the
album artist**, so it becomes `albumartist:` and not `artist:`. Mapping it the
obvious way would quietly have started listing every album an artist guests on
as one of their own. No end-to-end test could have caught it either — the
reference library holds nobody who guests on somebody else's record — so the
decision is tested where it is taken, on the expression the options build.

`aede track` went the same way, and its mapping needed the grammar to be
expressible at all: `--artist` there matches **either** a credit **or** the
album's own artist — a track "by Miles Davis" should be found on a Miles Davis
album whether or not he is credited on that particular piece. That is an `OR`,
which is exactly what no pile of options could ever say.

Two things stay outside the grammar, on purpose rather than for want of time.
**`artists --role`** answers about _artists_, and the grammar answers about
tracks; folding one into the other would lose the question, since "who is
credited as a producer" is not "who appears on the tracks that have a
producer". A second domain would need its own fields, and inventing it for one
option would be the wrong trade. **`artist --with`** already goes through a
single model function rather than a filter loop of its own, so routing it
through a query string would add indirection without removing duplication —
and `performing:` now lets anyone ask the same question in the grammar.

The two real gaps are **ranges** and **boolean composition**, and they are the
two that no amount of adding options ever fixes: options compose by AND and
nothing else. One grammar, in the spirit of what beets settled on:

```
genre:metal year:1990..1999 rating:>=4 -label:earache
(artist:ozzy OR artist:dio) added:-1w..
```

The rule that keeps it from becoming a second implementation: **every existing
option is sugar for one term of the grammar.** `aede albums --genre metal`
parses to the same query as `genre:metal`, and there is one evaluator. Options
stay for the common cases, because `--genre metal` is nicer to type than a
quoted expression, and nothing is duplicated.

A **saved query is a smart collection**, and a smart collection is a selection —
which is already the thing `--csv` and `--m3u` render and the thing M3's queue
consumes. "Every 5-star metal album I have never played" becomes a playable
collection with no new machinery at all. That closure is the reason to define
the grammar early rather than bolt filters on for another year.

## Identification (M1)

What MusicBrainz brings, beyond what is already planned.

### Country, formation, membership

There is no widely-used tag for an artist's country of origin — `RELEASECOUNTRY`
exists but that is the country a _release_ came out in, which is a different
question and answers it wrongly (an American pressing of a French band). So this
waits for M1 and comes from the artist entity: its **area**, its **begin and end
dates** (formation and split, with an explicit "ended" flag), and its type
(person, group, orchestra, choir…).

Band membership needs **no new table**: it is an artist-to-artist link, and the
`relation` table exists for exactly that. MusicBrainz's "member of band"
relationship carries begin and end dates and an instrument, so the one model
change is a **dated relation** — an optional period on a link. Which is worth
doing carefully, because "who was in the band in 1979" is a question the graph
should be able to answer, and dated links are how.

Then `aede artists --country FR`, `aede artist "Iron Maiden" --members`, and a
band page that shows a line-up rather than a list of names.

### Editions: single, EP, live, remaster, deluxe

Half of this is cleaner than expected and half is messier.

**Clean:** MusicBrainz release _groups_ carry a primary type — Album, Single,
EP, Broadcast, Other — and secondary types: Compilation, Soundtrack, Live,
Remix, DJ-mix, Demo, Mixtape, Spokenword, Interview, Audiobook, Audio drama,
Field recording. That is the vocabulary, it is stable, and it is exactly what an
interface needs for its icons.

**Messy:** _remaster_ and _deluxe edition_ are **not types.** MusicBrainz keeps
a remaster in the same release group as the original and distinguishes it at
release level — by date, label, catalogue number, barcode, and a disambiguation
comment such as "2011 remaster" — plus an explicit release-to-release "remaster
of" link. So a remaster is not a category to display; it is _another release of
the same thing_, which is a better model anyway and one this catalog can already
express.

Partly available before M1: Picard writes `RELEASETYPE` and `RELEASESTATUS`
into the tags, so a well-tagged library already knows its EPs from its albums.
Titles can be mined for "(Deluxe Edition)" and "Remastered 2011" — but a guess
from a title is a guess, and this project records where a fact came from
(read from a tag, inferred by a rule, fetched from MusicBrainz) rather than
flattening the three into one field that looks equally certain.

### What is missing from the shelf

The completeness report — the thing worth building:

```
Collection completeness
──────────────────────
Pink Floyd
Albums:
██████████████░░░ 82%
✓ The Dark Side of the Moon
✓ Wish You Were Here
✓ Animals
✗ The Final Cut
✗ The Division Bell
```

Three things decide whether this is useful or infuriating:

- **Compare release groups, not releases.** Otherwise every reissue of _Animals_
  counts as an album you are missing, and the figure is noise.
- **Say what the percentage is of.** 82% of studio albums is a fact; 82% of
  everything MusicBrainz holds, bootlegs and DJ-mixes included, is a number that
  will never reach 100 and therefore means nothing. The secondary types are the
  filter, and the heading must name it.
- **An absence is not a defect.** Nobody wants every Frank Zappa release. The
  report answers a question; it does not nag, and it belongs beside `doctor`
  rather than inside it.

Discogs is the obvious second source and adds real depth — pressings, matrix
numbers, plants, editions — but its API terms are far more restrictive than
MusicBrainz's CC0: authentication, rate limits, no commercial use without
permission, no caching beyond serving the immediate user. Worth it for editions
of physical media, not worth it as the primary source, and not worth it at all
before MusicBrainz is working.

**Available now, without any of the above:** `doctor` already reports an
incomplete album, but only by finding _gaps_ — it looks at the track numbers it
has and notices 2 is missing between 1 and 3. An album missing its last three
tracks looks perfectly whole to it. `TRACKTOTAL`/`DISCTOTAL` are in the tags and
are not being read. That is a small fix and does not wait for anything.

## Lyrics

Not built, and worth splitting into three before it is, because the three parts
have nothing in common but the word.

**Reading them is M0 work, not M1.** Lyrics sit in the files already: `USLT`
(and `SYLT` for the timed ones) in ID3, `LYRICS`/`UNSYNCEDLYRICS` in Vorbis
comments, `©lyr` in MP4, `WM/Lyrics` in ASF. The parsers walk past those frames
today without picking them up, and a `.lrc` file beside the audio is a text file
anyone can read. No network, no dependency, no milestone — just fields nobody
has collected yet, and the first thing to do.

Two consequences for the model, both of which the existing rules already
decide. Lyrics **read from a file are a fact about it**, so they belong to the
catalog beside the tags — while a lyric the _user_ typed or corrected is
something they wrote, and belongs to the annotation store. Same boundary as
everywhere else: read versus written. And they are **large text**, which is a
reason to think before pouring them into a file that is rewritten whole on
every scan.

Then `lyrics:` becomes a query field, and "that song that goes something about
a train" stops being unanswerable.

**Fetching them is M1 work, and comes with a caveat that is not technical.**
Lyrics are the _composition_ copyright, which owning a FLAC grants no rights
in — they are legally a different object from the recording. In practice every
comparable open-source project keeps online fetching out of its core: Navidrome
and Jellyfin read files and leave the network to plugins, beets ships a
`lyrics` plugin, foobar2000 and MusicBee do it through add-ons. The one
commercial exception, Plex, pays a licensed provider. Of the free sources,
LRCLIB is the only one an open-source server can query without breaking an
API's own terms — Genius forbids the scraping its lyrics require, and
Musixmatch's free tier is non-commercial and truncated.

So: fetching goes behind an explicit choice, from a source that permits it, and
never on by default. That is a decision about somebody else's rights, and this
project does not get to make it silently on a user's behalf.

**Showing them in time is M3 work.** Synchronised lyrics are what `SYLT` and
enhanced `.lrc` carry, and they only mean anything once there is a playhead to
follow. The parsers should keep the timings when they read them, so that M3
finds them there rather than asking for a re-read of the library.

## Speaking other servers' languages

Aède's own API (M2) is the contract, and it should be designed for Aède rather
than for anyone else. Compatibility surfaces are then **translations on top of
it**, never a second core — the moment a foreign API's model reaches into the
catalog, that model has won.

**Subsonic / OpenSubsonic first, and it deserves higher priority than it
sounds.** The original Subsonic API has been frozen since 2019 at version
1.16.1; OpenSubsonic is the community continuation, backwards-compatible in both
directions and actively specified. Between them they are spoken by something
like eighty clients — Symfonium, Supersonic, Feishin, Tempo, Amperfy and the
rest — on every platform there is. Implementing it is the difference between
"Aède has no mobile client" and "Aède has thirty", without writing an app. It is
plausibly the highest return of any single item in this file.

**Jellyfin afterwards, and with lower expectations.** Its API is documented by a
generated OpenAPI specification, but the specification is thin on meaning: one
enormous polymorphic item type, an Emby-era composite authorization header, and
enough undocumented behaviour that real integrations proceed by watching the
official web client's traffic. Clients emulate it routinely; servers emulating
it appear to be rare, which is itself a signal. Worth doing for the clients it
unlocks, worth doing _after_ Subsonic, and worth timeboxing.

_(One correction to the note that prompted this: no evidence could be found that
Navidrome exposes any part of the Jellyfin API. Its documentation and releases
mention only Subsonic 1.16.1 plus OpenSubsonic extensions, and its own private
API for its web interface. The bridges that exist run the other way — Jellyfin
plugins that read from Navidrome.)_

## Converting files

Transcoding for a phone or a car stereo, driven by ffmpeg, as beets does it.

Two decisions matter more than the feature:

- **ffmpeg is an external program, not a dependency.** It is invoked, its
  absence is detected, and a missing ffmpeg produces a clear message rather than
  a mysterious failure. The dependency rule in `CLAUDE.md` is not being bent for
  this.
- **The converted files must not come back in through the front door.** They go
  to a destination folder that is _not_ watched, and they are never registered as
  library items — the originals stay the library. A converted copy landing under
  a watched folder doubles the library at the next scan, and this is precisely
  the trap beets designed around: its `convert` writes elsewhere and the database
  keeps pointing at the originals. Files already in the target format are copied
  rather than re-encoded, which is worth stealing too.

This is also the first thing in the entire project that would _write audio_, and
that line is worth guarding: it writes new files, in a folder the user names,
and it never touches a source file.

## Licence

MIT — see [LICENSE](LICENSE).
