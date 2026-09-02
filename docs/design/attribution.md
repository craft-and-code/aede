# The attributed layer (M1.0)

Where a value that did not come from your files is kept, and why it is kept whole.

The roadmap settled the principle before any of M1 was written: **a value from MusicBrainz sits beside the tag, never on top of it.** This note settles the four structural questions that follow from it, so the client written next has somewhere to put its answers rather than inventing one under time pressure.

None of this needs the network. That is the point of building it first: the whole layer can be filled from a JSON fixture and tested offline, and the day the client arrives it changes where the values come from and nothing else.

## Why not simply write into the fields

Writing a fetched genre into `Release.genre` would cost three things at once — saying where the value came from, noticing that the source and the tag differ, and undoing it. And it would not even survive: a scan rebuilds the catalog from the files, so the tag would overwrite it on the next run.

That is not a hypothesis. It is the fault the `analysis` store was designed around, and the same one the scan exclusions hit in their first version. The rule the codebase already follows: **a scan may not destroy what it cannot recompute**, and anything a scan cannot recompute must live where a scan does not reach.

## 1. What it is keyed on: `EntityRef`, which already exists

An imported analysis is keyed on a **path**, because it describes a file and a file is a path. A fetched fact usually describes something that is not a file: an artist has a country, a release has a type, a band has members. Catalog identifiers are positions that a scan renumbers, so they are out.

`user::EntityRef` is exactly the answer and it is already written, already persisted, already reconciled: `artist:…`, `release:…`, `track:…`, `label:…`, `genre:…`, each keyed on what the entity calls itself rather than on where it currently sits. It carries the two behaviours this layer needs and would otherwise have to reinvent — a record whose entity is not in the catalog **waits** instead of being dropped, and a track that moved is found again by name and size.

So the layer reuses `EntityRef` rather than growing a second identity scheme. One identity problem in this codebase, not two.

## 2. Where it lives: a file of its own, `sources.json`

There are three voices in this program and it is worth naming them, because the storage falls out of the distinction:

- **what the files say** — the catalog, rebuilt from disk by every scan and therefore able to hold nothing else;
- **what you say** — favourites, ratings, notes, tags, in `user.json`, which no rescan touches;
- **what others say** — MusicBrainz today, another source tomorrow.

The third belongs beside the second, not inside the first. Imported analyses do live in the catalog and are carried over scan by scan, which was the obvious precedent to copy, and it is the wrong one here for two reasons.

**A rebuild would cost a re-fetch.** An analysis lost costs re-running an import over files that are sitting right there; a fetched value lost costs hundreds of network round trips at one request per second — for a six-hundred-artist library, ten minutes of politely waiting on somebody else's server. That is a different order of loss, and it is the argument that decides it.

**And `reset` would take it with it.** `reset` removes `catalog.json` and nothing else — it already prints *"your notes stay: they are not in this file"*, because annotations live elsewhere. A fetched layer inside the catalog would be destroyed by an operation whose stated purpose is to rebuild from the files, and rebuilding from the files is exactly the case where you least want to ask MusicBrainz for everything again. In its own file it survives, and `reset` says so in the same breath as it says it about notes.

The cost is honest and should be stated: a third store, a third load and save, a third reconciliation pass. The reconciliation is the only part that is hard, and it is not new work — it is `EntityRef`'s, already written for `user.json` and reused here, which is the second reason decision 1 above pays for itself.

## 3. Its shape: typed fields per entity kind, not a bag of strings

The tempting shape is a generic `(entity, field, value, source)` row. It is refused for the reason the catalog is not a table of strings: the display, the query grammar and `doctor` all have to know what a field *means*, and a generic bag pushes that knowledge into string literals scattered across the program. `FileAnalysis` made the same choice — typed optional fields, every one of them absent-able — and that is what lets `doctor` compare two sources of the same measurement.

One record per (entity, source), so two sources describing one artist are two records that can disagree in the open. Each carries:

- the **source** (`musicbrainz`) and the **identifier that source uses** (the MBID), which is what makes a second fetch an update rather than a duplicate;
- **when it was fetched**, so a value can be shown as old;
- a **confidence**, for anything reached by matching rather than by identifier — the roadmap's rule is that a file matched to a release approximately is never treated as certain;
- then the fields themselves, typed per entity kind: an artist's area, begin and end dates and type; a release's primary and secondary types; and so on as each is actually needed.

Fields are added when a fetcher fills them, not in advance. A field nothing writes is a field nothing tests.

## 4. Agreement is stored, but the verdict is derived

The roadmap requires that agreement be recorded: *"checked against MusicBrainz and it matches"* and *"never checked"* are two different states, and a layer that only kept disagreements could not tell them apart.

The way to honour that is **not** to store a verdict. A stored "agrees" goes stale the moment the user re-tags the file, and then the catalog holds a claim it has stopped being able to justify. What is stored is the **answer itself, whole**; the verdict — agrees, differs, no tag to compare — is computed on read, from the value beside the tag.

That is what makes "does my tag still match?" an **offline** question, answerable from the catalog the day after a re-tagging, with no second fetch to re-derive something the program had already been told. A few hundred bytes per release buys it. The precedent is again in the codebase: raw tags are kept per file for exactly this reason, so the graph can be rebuilt without touching the disk.

## What M1.0 delivers

The layer, `sources.json` and its round trip, the reconciliation across a rescan, the display beside the tag with its source, the disagreement report in `doctor`, and an offline way to load values from a fixture so all of it is tested without a network. No client, no rate limiter, no matching.

M1.1 then adds MusicBrainz — `ureq` with `rustls`, decided ahead of time and recorded in [Architecture](architecture.md#dependencies) — a throttle honouring the **one request per second per IP** MusicBrainz enforces, and the descriptive `User-Agent` it requires. The matching problem and its confidence score belong there, on top of a layer that already exists.

## M1.2 — prose, and the licence that comes with it

MusicBrainz answers with identifiers, dates and relationships. It never answers with a sentence about the artist, because that is not what it is for. The sentence exists on Wikipedia, and the way from one to the other is the `wikidata` relationship MusicBrainz already returns: the entity holds a *sitelink* per language, and the sitelink is the article title.

So reaching a paragraph is two requests on top of the one already made:

```
MusicBrainz artist  →  wikidata: …/wiki/Q11649
                       Special:EntityData/Q11649.json   →  sitelinks.enwiki.title
                       en.wikipedia.org/…/summary/<title>  →  extract
```

That is why it is `fetch --summaries` rather than part of `fetch`: it triples a run that already takes ten minutes over a large library, and the summary is the one thing here nobody needs in order to file their music. The article is looked for in the reader's own language first — taken from `LANG`, so nobody has to say it twice — and in English after, because for a great many artists that is the only article there is.

### Why the text and its credit are one value, not two fields

Wikipedia text is **CC BY-SA**. It may be reused, and attribution has to travel with it. A `summary` field beside a separate optional `source_url` would make it *possible* — and therefore eventually certain — to hold the words without the credit: one code path that fills the first and forgets the second, one export that copies one and not the other, and the project is quietly out of compliance.

So they are the same value. `Prose { text, url, lang, licence }` cannot be constructed without all four, is written to `sources.json` as one nested object, and is read back only when all four are present — a row that lost its attribution somewhere is a row this build will not repeat. There is deliberately no function anywhere that returns bare article text as a `String`.

### Why the licence is stored on every row

It is the same string for every Wikipedia article, so a copy per record looks like waste — about twenty-four bytes each, ten kilobytes over a large library. Reading it from a constant at display time would save that, and cost two things.

The licence is a **fact about the fetch**, like `fetched_at`: it says what these words were taken under, not what the current build believes they would be taken under now. Wikimedia has already moved once — CC BY-SA 3.0 to 4.0 — and a constant makes that upgrade silently relabel every paragraph fetched before it, which is exactly the class of claim this layer refuses to make.

And `Prose` is not Wikipedia's type. It is prose with its terms, so the next source of a biography — a plugin, a label, a discography site under quite different terms — files it in the same field and the record says which. Putting the licence in the module of the source that usually supplies it would make the second source a special case of the first.

The normalised middle ground — a table of licences at the top of the file, rows pointing at a key — is the one option to avoid outright. It re-creates, at the file level, the very thing the type exists to prevent: a row whose credit is somewhere else and can go missing.

### What a record with no article means

An entity with no Wikipedia article in any language asked for is stored as a record with an empty summary, not skipped. "Asked, and there is no article" is exactly what this layer exists to keep apart from "never asked" — and without it, every run would ask about the same artists forever.
