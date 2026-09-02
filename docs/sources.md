# What other sources say

There are three voices in this program, and keeping them apart is the whole design:

- **what the files say** — the catalog, rebuilt from your folders by every scan;
- **what you say** — favourites, ratings, notes and tags, in a file no rescan touches;
- **what somebody else says** — MusicBrainz today, another source tomorrow, in `sources.json`.

A value from a source **sits beside your tags and never on top of them**. Nothing is rewritten, nothing is merged, and every value can be traced to whoever said it, dated, and removed. Where a source and your tags disagree, both are shown and `doctor` reports it — which of the two is right is not something this program can know.

## Asking MusicBrainz

```sh
aede fetch                      # every artist in the library
aede fetch manson               # only the ones whose name matches
aede fetch --dry-run            # say what would be asked, ask nothing
aede fetch --full               # ask again about what is already held
```

MusicBrainz allows **one request per second**, so a large library takes a while: the command says how long before it starts, and asks you to confirm past twenty artists. It saves after every single answer, so an interrupted run loses nothing and a second run costs only what is left.

Its search server sometimes answers `503` for a moment even when you are well within the rate. That is waited out — three attempts, waiting longer each time — and the run only stops if the refusals keep coming, which is what a real rate limit looks like from the outside.

A second run asks about nothing it already holds. **`--full` is what re-asks**, and it is also what you need after an update that reads a field the previous answer did not store:

```sh
aede fetch --full manson
```

Some artists come back with nothing stored, and that is the design working.

An answer that is not clearly about your artist is left alone rather than guessed at. Nothing arbitrary is ever filed.

## Identified, or matched

If your files have been through **Picard**, they already carry MusicBrainz identifiers, and `fetch` uses them: it looks the artist up rather than searching for a name. That is worth two things.

It is **exact** — the answer is about the identifier that was asked for, so the record reads `identified` instead of a percentage. A percentage is a search saying how well its index matched, never a statement that the artist is 88% the right one.

And it is a **fuller answer**: a search result is abbreviated, while a lookup returns the entity — whether a band is still active, and the one-line description MusicBrainz uses to tell same-named artists apart.

Without an identifier, the name is searched and the result carries a score. Below 70 nothing is stored, and two equally good answers are refused rather than arbitrated:

```
? Nirvana: several answers are equally good: Nirvana, Nirvana (UK)
? Sh: the closest was "Shellac" at 61%, not close enough
```

## Correcting it by hand

Fetched values are not the last word. Records are keyed on **(entity, source)**, which means anything you file under a source of your own is not something MusicBrainz can overwrite: a later `aede fetch --full` adds its row beside yours and leaves yours alone.

Three steps. First, ask for a document with the right keys — an entity's key is how it names itself, and it is the one thing you cannot guess:

```sh
aede sources --template --source=manual --output=fix.json "Kind of Blue"
```

Then fill in what you want. Everything is optional; leave `null` where you have nothing to say:

```json
{
  "entity": "release:miles davis|kind of blue|/Users/you/Music/Miles/Kind of Blue",
  "source": "manual",
  "facts": {
    "primary_type": "Album",
    "first_released": "1959-08-17",
    "label": "Columbia",
    "secondary_types": []
  }
}
```

Then take it back in:

```sh
aede sources --import=fix.json
```

`--source=manual` is what protects it. Any name works — `manual`, your own, `discogs` if you copied it from there — and the point is only that it is not `musicbrainz`, because a source only ever replaces what **it** said before.

## Seeing and removing it

```sh
aede sources                    # one line per source: how much, how much lands
aede sources --list             # every record, and whether the catalog places it
aede sources --export --output=backup.json
aede sources --forget --source=musicbrainz
```

`aede artist` and `aede album` show a "What sources say" block, with each value next to the tag it can be judged against:

```
  Source                        Field           Says        Your tags
  ────────────────────────────  ──────────────  ──────────  ────────────────────
  musicbrainz 88% · 2 days ago  release type    Album       nothing in your tags
  musicbrainz 88% · 2 days ago  first released  1959-08-17  matches your tags
  musicbrainz 88% · 2 days ago  label           Blue Note   Columbia
```

Three different things, deliberately distinguished: a value your tags confirm, one they contradict, and one they say nothing about. A field with no tag counterpart at all — where an artist is from, for instance — leaves the last column empty rather than claiming your tags are missing something they were never meant to hold.

Two labels are worth explaining, because both were wrong at first. **from** is MusicBrainz's *area*, which may be a country, a city or a region: "Seattle" is a valid answer, so calling the column "country" was a mistake. **note** is its `disambiguation`, written to tell two artists who share a name apart rather than to describe either — so it is often useful ("US industrial metal band") and sometimes not ("the band"), and labelling it "known as" oversold it.

A record that waits is not a failure either: it describes something this catalog does not hold yet, and it is kept until it does.

## What MusicBrainz does not have

**No biography.** MusicBrainz is a database of identifiers and relationships, not of prose. What Aède reads today is what an artist lookup answers: type, area, formation and end dates, whether the group is still active, and the short `disambiguation` — "US industrial metal band" — which is written to tell two artists apart rather than to describe one.

A lookup also brings the **genres** its editors voted for, other **names** the artist goes by, and the **links** it holds — official site, Discogs, and Wikidata. All from the same request: `inc=genres+tags+aliases+url-rels` rides on the call already being made, which matters at one request per second.

Genres are shown beside your genre tag rather than over it, like everything else here. Where MusicBrainz has no voted genre it falls back to its free **tags**, which are a different thing — a crowd writes "seen live" there — so the two lists are never merged.

Still not read: **relationships between artists**, which is band membership with instruments and dates. The roadmap wants those as dated links in the graph, a change to the model rather than a field to display, so asking for them now would store an answer with nowhere to put it.

**Wikipedia is the next step, and Wikidata is the door.** An article is written by people, exists in your own language, and is what a biography actually is. It also carries an obligation the rest of this does not: Wikipedia is CC BY-SA, so attribution has to travel with the text — a design question rather than a parsing one, and the reason it is its own step.

**No opinion about your files.** Nothing fetched is ever written into a tag, and no fetched value changes what a scan finds. If you delete `sources.json`, you lose only the time it took to ask.
