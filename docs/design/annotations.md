# What the user writes

## What the user writes: favourites, ratings, notes, history

This is the part that needs deciding **first**, before any of it is built,
because it is the only data in the whole program that cannot be recovered. Lose
the catalog and a scan rebuilds it in a minute. Lose the notes and they are
gone.

## One shape, not five

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

## Why the note is not the comment tag

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

## The identity problem, which is the whole problem

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

## Its own file, and it is the one worth backing up

Not in `catalog.json`. Two reasons, and the second is the real one:

- the catalog is derived from disk and reproducible; annotations are not
  reproducible from anything;
- the catalog is written whole. Ratings and play events change constantly, and
  rewriting the entire library to record a click is absurd.

A separate file, human-readable, hand-editable, and small. Export and import are
then almost free, and worth having from the first day: `aede notes --export` /
`--import`, merging rather than replacing, because a merge is what someone
restoring half a backup actually wants.

## Which is the same question as "several users"

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

## Which is also what makes the move to SQLite cheap

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

## Play history, which has a different shape

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

**And it can be taken back**, which every other thing the user writes could
already do:

```sh
aede played "So What" --remove     # undo the last listen, log and counter together
aede history --remove              # forget the lot, after confirmation
```

The two structures move together or not at all. The log is bounded and the
counter is not, so a removal touching one only would leave a track "played
three times" with two plays behind it and nothing on screen to say which was
right. Clearing the whole history is confirmed like `reset` — nothing on disk
remembers what was played, so there is no undo — and it reports how many
listens and how many counters went, because "your history is cleared" is a
claim nobody can check and a number is.

## The commands it would take

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
