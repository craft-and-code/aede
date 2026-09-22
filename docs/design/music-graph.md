# The canonical music graph: audit and model

**Status: stages 0–6 implemented.** The catalog now separates placements,
recordings, releases, release groups and works. External relationships remain
attributed evidence, and rich recording and work credits keep their exact
scope, role details and provenance without rewriting local tags. SQLite remains
later.

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

The audit found these specific gaps; all five are now closed:

1. `Track` combined a local file, its release position, and the abstract
   recording. It is now the local placement and points to `Recording`.
2. A release-group MBID was only a field on `Release`; `ReleaseGroup` is now a
   canonical object linking all known local editions.
3. A musical work was absent. `Work` now joins tagged and source-backed
   recordings by MusicBrainz work ID.
4. Credits originally had only a role. They now retain their MusicBrainz
   relationship identity, credited-as name, attributes/instruments, date span,
   ordering and source evidence.
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

## Stage 3 — rich credits

`aede fetch --credits` performs one identifier lookup per recording and asks
for three connected layers in the same response:

- artist relationships directly attached to the recording, such as performer,
  producer, engineer, conductor or remixer;
- the recording-to-work relationship, including qualifiers such as live,
  cover, instrumental, partial or medley;
- artist relationships on each linked work, such as composer, lyricist,
  writer or arranger.

Every external credit retains the MusicBrainz artist ID, canonical name,
credited-as spelling, relationship type, stable type identifiers and direction,
instruments and other attributes, begin/end dates, ordering, source, confidence
and fetch time. Its scope remains explicit: a composer on the work is not
presented as though they performed on the recording.

The local catalog uses the same richer relationship vocabulary. In particular,
Picard/Vorbis fields such as `PERFORMER:guitar` keep the instrument on the
credit instead of flattening it into a generic performer. Local tag assertions
remain marked `tags`; fetched assertions remain in `sources.json`.

The credits are visible from `track`, `album`, `artist`, `recording`, `work`
and the source comparison panel. The track JSON view exports local and sourced
credits separately, so provenance and scope cannot disappear in a machine-
readable view. `--recordings` remains a compatibility alias for `--credits`.

## Stage 4 — relations between objects

Every canonical object can now participate in the typed relation graph. The
following links are derived from local identities and stored in both directions:

- track placement ↔ recording and track placement ↔ release;
- recording ↔ work;
- release edition ↔ release group;
- release ↔ label and release ↔ album artist;
- artist ↔ recording for each exact credit role;
- artist ↔ release for guest appearances, compilation appearances and
  non-performing contributions;
- artist ↔ artist collaborations;
- duplicate and other-edition release relationships.

The inverse link is explicit rather than reconstructed differently by every
caller. A label therefore reaches the same releases that each release identifies
as its label; a work reaches the same recordings that name that work; an artist's
discography, guest appearances and compilation appearances cannot silently
collapse into one list.

Rich source relationships are not copied into this local relation table.
MusicBrainz memberships remain dated source evidence, but now expose both the
local endpoint and the related local artist when that MusicBrainz identity is
also present. Source-backed works and credits follow the same rule established
in stages 2 and 3.

The existing entity pages expose these links where they are useful: a track
shows its recording, canonical or sourced works and release group; an album
shows every local edition in its release group; membership rows expose the
related MusicBrainz identity; and global name search includes recordings,
works and release groups.

## Stage 5 — navigation views

Every graph page now names the exact command that continues to adjacent
objects. These are ordinary, copyable CLI commands rather than terminal-only
links, so they remain useful through a pipe, over SSH and in saved output.

- `track` leads to its recording, works, album, release group, label and
  credited artists;
- `recording` leads to each local placement, album, work and locally known
  credited artist;
- `work` leads back to every recording and locally known credited artist;
- `album` leads to its album artist, labels, release group, related editions
  and credited artists;
- `artist` exposes direct commands for the filtered album list, dated
  memberships and missing-album report;
- `label` leads to an exact filtered album listing;
- the new `release-group` page lists every local edition and validates both
  directions of the edition relationship;
- `search` carries a command that opens every result.

Navigation prefers MusicBrainz identifiers for recordings, works, release
groups and editions. `album` therefore accepts a precise release MBID in
addition to a title, preventing two pressings with the same title from becoming
an ambiguous dead end. Names are shell-quoted centrally, including apostrophes,
so the displayed commands can be copied without being reassembled by hand.

## Stage 6 — relational search

The graph is now part of the query language, not only the navigation layer.
Every relation is projected back onto the tracks it explains, so graph filters
compose with the existing Boolean, range and annotation syntax:

- `recording`, `work` and `releasegroup` accept canonical titles or MusicBrainz
  identifiers;
- `instrument` searches instruments and other attributes carried by credits;
- `guest` and `compilationartist` distinguish appearances by release type;
- `contributor` isolates non-performing roles such as composer or producer;
- `with` (also `collaborator`) requires at least two audible artists on the
  same track.

Certain relationships from `sources.json` participate without being copied
into the tag-built graph. This makes works, roles and instruments obtained by
`fetch --credits` queryable while preserving their provenance boundary;
approximately matched source records remain evidence only. Those relationships
are indexed once when a query starts rather than re-walked for every track.

For example, `aede query 'work:"War Pigs" instrument:guitar'` finds guitar
performances of a composition, while `aede query 'guest:"Zakk Wylde"'` finds
guest appearances without confusing them with the artist's own discography.
The query tests include a shared work, a compilation appearance, a guest with
an instrument, and a non-performing contributor.

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
6. Enrich local tag credits and source-backed MusicBrainz credits with role
   details, identifiers, exact scope and provenance.
7. Retrieve recording and nested work relationships in one request, then expose
   them consistently across the entity views and JSON output.
8. Derive the complete bidirectional local relation graph, with separate
   participation kinds for discography, guest work, compilations and
   non-performing contributions.
9. Make recording, work and release-group identities searchable and preserve
   externally dated memberships as navigable source evidence.
10. Add the missing release-group page and precise edition lookup by release
    MBID.
11. Give every entity page concise, copyable paths to its adjacent objects.
12. Turn global search results into entry points by printing their open
    command.

13. Extend the query language across the graph: `recording`, `work` and
    `releasegroup` identities, `instrument` attributes, and `guest`,
    `compilationartist`, `contributor` and `with` participation filters now
    project canonical relationships back onto matching tracks.

The remaining graph work is quality and resolution: explicit conflict handling,
confidence-aware proposals, and a review path for ambiguous source claims.

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
