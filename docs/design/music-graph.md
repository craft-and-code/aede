# The canonical music graph: audit and model

**Status: stages 0–2 implemented.** The catalog now separates placements,
recordings, releases, release groups and works. External relationships remain
attributed evidence, and the command views expose them without rewriting local
tags. Rich relationship credits are the next stage; SQLite remains later.

The objective is to make every musical fact addressable in both directions:
from a performer to the
recordings and editions they contributed to, and from a local file back to the
recording, work, people, labels, and source claims that describe it.

## Stage 0 — what Aède already has

The current catalog is already a graph, rather than a folder hierarchy:

| Present object | What it means today                          | What is already linked                                                                  |
| -------------- | -------------------------------------------- | --------------------------------------------------------------------------------------- |
| `AudioFile`    | one local byte stream                        | raw tags, technical facts, artwork and sidecar paths                                    |
| `Track`        | one file in one position on a local release  | file, release, title, ISRC, recording MBID                                              |
| `Release`      | one local edition in one folder              | tracks, album artist, labels, precise release MBID and release-group MBID               |
| `Artist`       | a person or group named by the local library | aliases, artist MBID, credits and inferred collaborations                               |
| `Credit`       | an artist's role on a track or release       | main, featured, composer, producer, engineer, performer and other tag roles             |
| `Relation`     | an inferred or fetched typed link            | collaboration, edition relationship, and dated group membership in the attributed layer |

`sources.json` is deliberately separate. It keeps MusicBrainz, AcoustID and
other claims with their source identifier, confidence and fetch date; a scan
can therefore never erase a costly external answer or overwrite a local tag.

The audit found these specific gaps; the first three and the label identity
gap are now closed:

1. `Track` combined a local file, its release position, and the abstract
   recording. It is now the local placement and points to `Recording`.
2. A release-group MBID was only a field on `Release`; `ReleaseGroup` is now a
   canonical object linking all known local editions.
3. A musical work was absent. `Work` now joins tagged and source-backed
   recordings by MusicBrainz work ID.
4. Credits have a role but not yet their MusicBrainz relationship identity,
   credited-as name, attributes/instruments, date span, ordering or source
   evidence.
5. Labels were local names only. An explicit tag MBID is now canonical, while
   fetched identity is reconciled as confirmed, agreeing, proposed or
   conflicting evidence.

## Stage 1 — canonical objects

The canonical graph adds the following objects without weakening the local
catalog's meaning:

| Object           | Identity                                                                | It connects                                                                          |
| ---------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `File`           | canonical local path                                                    | bytes on disk → local track placement                                                |
| `TrackPlacement` | release + disc + position + file                                        | one file → one edition position → one recording                                      |
| `Recording`      | MusicBrainz recording ID when known; otherwise a local stable key       | placements, ISRCs, performance credits and works                                     |
| `Release`        | MusicBrainz release ID when known; otherwise local edition key          | placements, labels and release group                                                 |
| `ReleaseGroup`   | MusicBrainz release-group ID                                            | releases/editions, group-level type and first-release facts                          |
| `Work`           | MusicBrainz work ID                                                     | recordings, composers, lyricists, arrangements and work relations                    |
| `Artist`         | MusicBrainz artist ID when known; otherwise current normalized identity | credits, memberships, aliases and relations                                          |
| `Label`          | MusicBrainz label ID when known; otherwise current normalized identity  | release-label relationships and label facts                                          |
| `Credit`         | relationship assertion, not a display string                            | artist → recording, work, release or release group, with role details and provenance |

`Track` will be renamed only when its replacement is complete. During the
model migration it remains the public local placement object, has a
`recording_id`, and continues to own `file_id`, `release_id`, disc and track
number. This prevents a file from being silently conflated with a recording,
while preserving every command that currently acts on a local track.

## Non-negotiable rules

- Local files and tags remain authoritative for local placement and display.
- An external identifier creates a link; it never rewrites a tag.
- A fact is stored with its source, source identifier, confidence and fetch
  time. Inference and fetched assertions are never merged invisibly.
- Unknown is a valid state. A work is not invented from matching titles, and
  two recordings are not merged merely because their names match.
- A relationship can carry its own data: credited-as name, attributes or
  instruments, begin/end dates, ordering and source evidence belong on the
  relationship, not on either endpoint.
- Existing local IDs may remain implementation details. Durable external and
  source keys must not depend on vector position.

## Stage 2 — provenance and reconciliation

The catalog remains what the files say. `sources.json` records what an
external service says, including its source identifier, confidence and fetch
time. The two layers meet only through read-only reconciliation views:

- `aede recording` shows local placements, canonical tag relationships and
  attributed recording-to-work assertions;
- `aede work` can traverse an identified MusicBrainz work even when the files
  do not carry `MUSICBRAINZ_WORKID`, while marking it as external evidence;
- `aede label` distinguishes a local MBID, source confirmation, a name-search
  proposal and a genuine identity conflict;
- approximate matches remain visible evidence but never become traversal
  links or silently update a canonical identifier.

Agreement is retained rather than discarded: “checked and equal” is a
different state from “never checked”. Conflicts likewise remain unresolved
until an explicit future review action can choose one.

## Completed delivery order

1. Add `Recording` and connect each local placement to exactly one recording.
   A MusicBrainz recording ID joins placements; without one, a placement keeps
   its own local recording until evidence says otherwise.
2. Add `ReleaseGroup`, making the existing release-group MBID a navigable
   relationship rather than a field used only for comparisons.
3. Add `Work`, but create it only from a sourced MusicBrainz work identifier.
4. Keep fetched work and label assertions in the attributed layer, with
   confidence and date, and reconcile them without mutation.
5. Add `recording` and `work` traversal views after persistence and ambiguity
   tests.

The next graph stage replaces plain credit roles with relationship assertions
that retain role details and provenance. The current concise role output stays
as a derived view during that transition.

SQLite is intentionally outside these stages. The graph must first be proven
in the current model and JSON persistence; M2 can then migrate one established
model rather than using a database migration to decide musical semantics.

## Reference fixtures

Every model step must cover these cases before a view is added:

- the same recording on an original album, a compilation and a deluxe edition;
- studio and live recordings of the same work;
- a cover version of the same work by another artist;
- a guest musician with an instrument and a credited-as name;
- a composer or lyricist with no audible performance credit;
- a musician leaving and rejoining a group;
- two same-titled works with no identifier, which must remain unrelated;
- a rescan after a file move or retag, preserving sourced facts without
  asserting a new identity.
