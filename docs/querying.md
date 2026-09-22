# Queries: Querying Your CDthèque with Precision

Options on standard command-line tools compose by `AND` and by nothing else. That ceiling is what a real query grammar lifts: there is no `--genre metal OR --genre jazz`, no "everything except this label", no "between 1990 and 1999" in simple flags — and all three come free with Aède's unified parser.

Whether you are seeking to isolate overlooked masterpieces from the 1990s, retrieve a personal note scribbled about a Japanese pressing, or extract a set of tracks for the road, the syntax remains constant, predictable, and rigorous.

```sh
aede query "genre:metal year:1990..1999 label:earache"
aede query "(artist:ozzy OR artist:dio) album.rating:>=4"
aede query "loved played:0" --m3u          # what I love and have never played
aede query "lossless:false size:>50000000" # big, and not lossless
```

## Available Fields and Graph Relations

The search engine explores the entirety of the catalog's relational structure. Fields can be prefixed to target specific structural levels (`album.`, `artist.`, `track.`) or inferred from the execution context.

| Field                    | Type         | Description                   | Examples                                   |
| :----------------------- | :----------- | :---------------------------- | :----------------------------------------- |
| `title`                  | Text         | Track title                   | `title:Interstellar`                       |
| `artist` / `albumartist` | Text         | Performer or release artist   | `artist:Coltrane`, `albumartist:Metallica` |
| `album`                  | Text         | Album title                   | `album:"Kind of Blue"`                     |
| `recording`              | Text/ID      | Canonical recorded performance | `recording:9f…`, `recording:"So What"`      |
| `work`                   | Text/ID      | Composition realised by it    | `work:"All Along the Watchtower"`           |
| `releasegroup`           | Text/ID      | Album identity across editions | `releasegroup:5c…`                           |
| `genre`                  | Text         | Musical genre                 | `genre:=Jazz`, `genre:metal`               |
| `label`                  | Text         | Record label imprint          | `label:"Blue Note"`                        |
| `year`                   | Range/Number | Release year                  | `year:1994`, `year:1985..1995`             |
| `duration`               | Duration     | Track length                  | `duration:..4:00`, `duration:3:30..5:00`   |
| `size`                   | Bytes        | File size in bytes            | `size:>50000000`                           |
| `codec` / `format`       | Text         | Audio codec or container      | `codec:flac`, `format:mp3`                 |
| `bitrate` / `samplerate` | Number       | Audio stream parameters       | `bitrate:>=320k`, `samplerate:96000`       |
| `lossless`               | Boolean      | Lossless compression status   | `lossless:true`, `-lossless`               |
| `compilation`            | Boolean      | Multi-artist compilation flag | `compilation:true`                         |
| `played`                 | Counter      | Play count                    | `played:0`, `played:>=10`                  |
| `comment`                | Text         | File-level comment tag        | `comment:"vinyl rip"`                      |
| `lyrics`                 | Text         | Embedded or sidecar lyrics    | `lyrics:train`                             |
| `path`                   | Text         | Absolute file path            | `path:"/FLAC/Ozzy"`                        |
| `instrument`              | Text         | Credit instrument or attribute | `instrument:"electric guitar"`              |
| **Annotations**          |              |                               |                                            |
| `rating`                 | Numeric      | Personal star rating (1–5)    | `rating:>=4`, `album.rating:5`             |
| `loved`                  | Boolean      | Personal favorite status      | `loved`, `-loved`, `track.loved`           |
| `tag`                    | Text         | User tag/label                | `tag:vinyl`, `album.tag:audiophile`        |
| `note`                   | Text         | User Markdown note content    | `note:remaster`, `artist.note:concert`     |

### Credits and Who Did What

A music catalog is a graph, not a flat table. Aède indexes distinct creative roles so you can query liner notes with surgical accuracy:

- **Role Fields:** `composer`, `lyricist`, `producer`, `engineer`, `performer`, `conductor`, `remixer`, `featured`, `mainartist`.
- **Audible Class (`performing`):** Matches anyone audible on the recording — capturing a guest rapper's verse without matching the songwriter behind the scenes.
- **Global Credit (`artist:`):** Matches any credit in any role across the track or album. `artist:ozzy artist:"zakk wylde"` requires both individuals to appear anywhere on the record, while role fields isolate their specific contributions.

```sh
aede query "composer:rhoads mainartist:ozzy"   # Ozzy singing what Randy wrote
aede query "producer:\"rick rubin\" year:1990.."
aede query "guest:\"zakk wylde\""               # guest on a non-compilation release
aede query "compilationartist:\"miles davis\""  # performer on a compilation
aede query "contributor:\"rick rubin\""         # non-performing contribution
aede query "with:\"zakk wylde\""                # co-performer on the same track
```

The graph identity fields (`recording`, `work`, `releasegroup`) accept either
the displayed title or the MusicBrainz identifier. `instrument` searches the
attributes attached to a credit, while `guest`, `compilationartist`,
`contributor` and `with` project the corresponding participation links back to
the tracks they explain. The aliases `collaborator` and `compilation-artist`
are accepted as well.

These fields read both explicit local tags and relationships obtained with
`aede fetch --credits`. Source records attached by an exact, non-conflicting
identifier are eligible automatically; an approximate or conflicting claim is
eligible only after `aede review --accept=<ID>`. Pending and rejected claims
remain visible evidence and do not silently turn into query relationships. The
source index is built once per query, so a large library is not rescanned for
every track and every term.

### Strict Semantics and Boolean Symmetry

1. **Errors over Silent Emptiness:** A query naming an entity that does not exist in the vault (e.g., a non-existent genre or unknown artist) returns an explicit **error** rather than an empty result set. An empty list says "no tracks match these constraints," while an error tells you "this term does not exist in your catalog."
2. **Boolean Symmetry:** Flags evaluate identically regardless of notation: `lossless:false` and `-lossless` produce identical execution plans.

## Lyrics, Sidecars, and Text Precision

The `lyrics:` field inspects text embedded directly inside audio containers (such as Vorbis `LYRICS`, ID3 `USLT` or `©lyr` atoms) as well as external `.lrc` sidecar files residing alongside the audio track.

```sh
aede query "lyrics:train"
```

Catalog scanning indexes embedded tags instantly at zero runtime I/O cost. Sidecar `.lrc` files are opened and parsed strictly when queried. To read full, time-synced lyrics on screen, run:

```sh
aede track "Crazy Train" --lyrics
```

## Searching What You Wrote: Annotations & Scopes

Every annotation added via `aede note`, `aede rating`, or `aede tag` is immediately searchable.

```sh
aede query "tag:vinyl"              # tracks carrying that label
aede query "album.tag:vinyl"        # tracks whose album carries it
aede query "note:remaster"          # notes containing "remaster"
aede query "artist.note:live"       # notes attached to the artist
aede query "album.rating:>=4 -played"
```

### Scope Isolation and Diagnostic Guidance

The structural scope (`track`, `album`, `artist`) is part of the question. A bare `rating`, `tag`, or `note` queries the **track**. If you rated an _album_ rather than individual tracks, `aede query "rating"` returns no matches because track-level ratings were requested.

Rather than failing silently, Aède detects when annotations exist at adjacent structural levels and offers diagnostic guidance:

```
$ aede query "rating"
nothing matches "rating"
  1 track if you ask it of the album — that is where you wrote it
  aede query "album.rating"
```

### The `loved` Inheritance Exception

Unlike ratings or notes, a favorite flag (`loved`) represents a broad qualitative signal ("this matters to me"). Requiring a curator to flag every individual track on a beloved 12-track album is redundant.

Therefore, a bare `loved` query evaluates hierarchically: it matches if the favorite flag is set on the **track**, or inherited from its **album**, or inherited from its **artist**:

```sh
aede query "loved played:0"   # unplayed tracks loved directly or via album/artist
```

To restrict the query to an exact structural level, specify the prefix explicitly: `track.loved`, `album.loved`, or `artist.loved`.

### Querying Field Existence

To search for the presence or absence of an annotation rather than matching text inside it, pass the field name alone or prefixed with a negation sign `-`:

```sh
aede query "note"        # any item holding a user note
aede query "-rating"     # items that have never been rated
aede query "tag"         # items carrying at least one custom tag
```

To perform a text search for literal words like "note" or "rating", scope them to a text field: `title:note`.

## Disambiguating Comments, Notes, and Lyrics

Aède distinguishes between three types of textual metadata based on data provenance:

1. **File Comments (`comment`):** Metadata embedded inside the audio file container by tagging software (e.g., rip source, pressing details).
2. **User Notes (`note`):** External Markdown text authored by you and stored safely in `user.json`.
3. **Lyrics (`lyrics`):** The performance text itself, stored in embedded tags or `.lrc` files.

When running `aede search`, comment, note, and lyrics searching are opt-in flags to prevent free prose from obscuring exact title or artist matches:

```sh
aede search --comments "vinyl rip"
aede search --notes "remaster"
aede search --lyrics "all aboard"
```

### Output Formatting and Provenance Tracking

- **Line-Level Matches for Lyrics:** Searching lyrics prints the exact matching line rather than dumping multi-line poem blocks into terminal result tables.
- **JSON Provenance:** Search results in JSON output explicitly identify match origin via the `found_in` key (`found_in: comment`, `found_in: note`, or `found_in: lyrics`).

## Saved Collections & Smart Playlists

A query worth typing twice can be converted into a **saved collection**:

```sh
aede collection wishlist --query "loved played:0"
aede collection wishlist                 # view current contents
aede collection wishlist --m3u           # export as an M3U playlist
aede collections                         # list all saved collections and track counts
aede collection wishlist --remove
```

### Smart Collections vs. Static Playlists

A saved collection stores the **formula**, not a static list of files. Every time a collection is queried or exported (`--m3u`, `--csv`, `--json`), Aède re-evaluates the query against the current catalog state. As new albums are scanned or tags are updated, collections refresh automatically.

Syntax validation occurs at definition time: passing an invalid query expression to `aede collection --query` is rejected immediately, preventing silent failures during future exports.
