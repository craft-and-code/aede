# What other sources say

There are three voices in this program, and keeping them apart is the whole design:

- **what the files say** — the catalog, rebuilt from your folders by every scan;
- **what you say** — favourites, ratings, notes and tags, in a file no rescan touches;
- **what somebody else says** — MusicBrainz today, another source tomorrow, in `sources.json`.

A value from a source **sits beside your tags and never on top of them**. Nothing is rewritten, nothing is merged, and every value can be traced to whoever said it, dated, and removed. Where a source and your tags disagree, both are shown and `doctor` reports it — which of the two is right is not something this program can know.

## Asking MusicBrainz

```sh
aede fetch                      # every artist and every album
aede fetch manson               # only what the name matches — person or record
aede fetch --dry-run            # say what would be asked, ask nothing
aede fetch --full               # ask again about what is already held
```

**Artists and albums, in one run.** The albums are the half that matters most, and it is worth saying why: there is no tag for where a musician is from, so what MusicBrainz says about an *artist* can only ever be added beside your library. An album is different — Picard writes `RELEASETYPE`, `DATE` and `LABEL`, so your files have an opinion and MusicBrainz has one, and the two can disagree. That disagreement is the whole point of this store, and it lives on the albums.

The most common one is the date. Your `DATE` tag says 1997 because that is the reissue you ripped; MusicBrainz says the album first appeared in 1959. Neither is wrong, and Aède shows both rather than choosing.

**One request per album, whatever your tags carry.** If they hold the MusicBrainz album identifier, the answer comes back as a certainty and brings the label with it in the same request. If they hold only the release-group identifier, it is still a certainty, without the label — a release group has none, and filling it from whichever pressing answered first would attribute one edition's label to the album itself. If they hold neither, the title and the album artist are searched, scored, and refused below 70 rather than guessed.

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

## Asking Wikipedia

MusicBrainz holds no biography, but it holds the link to one. Following it is **an option on `fetch`**, not a command of its own:

```sh
aede fetch --summaries          # follow it, for every artist already fetched
aede fetch --summaries --full   # ask again about what is already held
```

`aede fetch` tells you when there is something to follow, so you do not have to remember the option — including when there is nothing left to fetch and it only has that to say:

```
→ 402 stored, 3 left alone, 0 failed
  381 of them have a wikidata link — aede fetch --summaries reads the article
```

```
Fetch

  every artist has already been asked about (--full asks again)
  381 of them have a wikidata link — aede fetch --summaries reads the article
```

This is a **second pass over what `fetch` already stored**, not a second search. It reads the `wikidata` link MusicBrainz gave for each artist, asks Wikidata which article that entity has, and then asks Wikipedia for that article's opening paragraph. Two requests per artist, on top of the one already made — which is why you ask for it rather than getting it by default.

The article is looked for **in your own language first**, taken from your `LANG` setting, and in English after. For a great many artists English is the only article there is, so it is always the fallback and never the first choice.

Artists MusicBrainz gave no Wikidata link for are not asked about at all. An artist with a link but no article in either language is recorded as *asked, and there is nothing* — so the next run does not ask again.

### The credit is part of the text

Wikipedia articles are **CC BY-SA**. Reusing the text obliges naming where it came from and under what terms, so Aède stores the paragraph, the page, the language and the licence as **one inseparable value**: there is no way to keep the words without the credit, because the program offers none. Wherever the paragraph is shown, the credit is shown under it:

```
  Marilyn Manson is an American rock band formed in Fort Lauderdale,
  Florida, in 1989.
  https://en.wikipedia.org/wiki/Marilyn_Manson_(band) — CC BY-SA 4.0
```

`aede sources --forget --source=wikipedia` removes all of it, the same as any other source.

## What is missing from the shelf

MusicBrainz knows what your artists recorded. Comparing that with what you have is a wish list:

```sh
aede fetch --discography    # browse everything credited to each artist
aede missing                # what is credited to them and not here
aede missing davis          # narrowed, by artist or by title
```

**Nothing is fetched by `missing`.** The answer is worked out, each time you ask, from the discography `fetch --discography` stored — so an album stops being listed the day you add it, with nothing to update and nothing to go stale. That is the same rule this whole store follows: keep the answer, derive the verdict.

```
Missing

  Artist       Album              Year
  ───────────  ─────────────────  ────
  Miles Davis  Sketches of Spain  1960
  Miles Davis  Bitches Brew       1970
  2 studio albums
  singles, live records and compilations are left out
```

**Studio albums only**, and that filter is deliberate. A full discography holds every single, every live recording and every compilation somebody ever assembled; reporting all of them as missing would be true and useless, since nobody's shelf holds every single ever pressed.

An album already here is recognised by its **MusicBrainz release-group identifier** when your tags carry one, and by its **title** otherwise — with the same spelling rules that decide two artists are one name, so "Kind Of Blue" on your shelf is not reported as missing "Kind of Blue".

**Only artists who have a shelf here.** The catalog holds an artist for every credit it reads — a guest on one track, a composer, one name among fifty on a compilation. Being in the catalog is not the same as having a place in the library, and the first version did not draw that line: one Rolling Stones track on a compilation produced their entire studio discography as *missing*, and it did so for every passing credit at once until the report was mostly that.

So an artist is only considered when at least one album **of their own** is here — when they are the album artist of something on the shelf. What this reports is an *incomplete* discography, which means one that was started. The same rule decides who `fetch --discography` bothers to browse, so the pass does not spend a request a second on answers the report would never show.

## What MusicBrainz does not have

**No biography.** MusicBrainz is a database of identifiers and relationships, not of prose. What Aède reads today is what an artist lookup answers: type, area, formation and end dates, whether the group is still active, and the short `disambiguation` — "US industrial metal band" — which is written to tell two artists apart rather than to describe one.

A lookup also brings the **genres** its editors voted for, other **names** the artist goes by, and the **links** it holds — official site, Discogs, and Wikidata. All from the same request: `inc=genres+tags+aliases+url-rels` rides on the call already being made, which matters at one request per second.

Genres are shown beside your genre tag rather than over it, like everything else here — and compared as **sets**, not as sentences. MusicBrainz answers `pop, dance-pop, electropop, europop` where your files say `Rock, Pop`; that is not a disagreement. Your tags say the record is pop *and* rock, MusicBrainz says pop and three finer words for it, and nobody is contradicting anybody. A shared name is agreement; only two lists with nothing at all in common are reported as a difference. Where MusicBrainz has no voted genre it falls back to its free **tags**, which are a different thing — a crowd writes "seen live" there — so the two lists are never merged.

Still not read: **relationships between artists**, which is band membership with instruments and dates. The roadmap wants those as dated links in the graph, a change to the model rather than a field to display, so asking for them now would store an answer with nowhere to put it.

**Wikipedia fills that gap, and Wikidata is the door** — see [Asking Wikipedia](#asking-wikipedia) below.

**No opinion about your files.** Nothing fetched is ever written into a tag, and no fetched value changes what a scan finds. If you delete `sources.json`, you lose only the time it took to ask.
