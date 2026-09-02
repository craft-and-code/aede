# Discogs — researched, and set aside

**Status: not implemented, and not scheduled.** This page exists so that the
work of finding out is not done twice. If the demand turns up, everything
needed to decide is here.

## What it would bring

Ranked by what this catalog does not already have.

|               | MusicBrainz        | Discogs                                                                                                    |
| ------------- | ------------------ | ---------------------------------------------------------------------------------------------------------- |
| **Styles**    | nothing equivalent | a two-level taxonomy — a broad `genres` list and a specific `styles` list — filled on almost every release |
| **Credits**   | uneven             | producer, engineer, mixing, mastering, photography, per release and per track                              |
| **Pressing**  | label only         | label, catalogue number, country, format (180 g, reissue, promo…)                                          |
| **Cover art** | —                  | out of reach: images need authentication and are not openly licensed                                       |

**Styles are the only one where Discogs systematically beats MusicBrainz.** The
comparison machinery for it already exists: `sources::verdict_set` compares two
sets of genre-like strings and reports overlap rather than equality, which is
exactly the shape a style list has.

## The part that was expected to be hard, and is not

The note that held this back said Discogs "needs an API token design". It does
not, and the reason is worth writing down because it generalises.

Discogs requires authentication for **search**. It does not require it for a
**lookup by identifier** — artist, release, master and label all answer
unauthenticated. And this program never needs to search, because it already
holds the identifiers: MusicBrainz returns a `discogs` URL relationship, Aède
already asks for `url-rels` on every artist lookup, and
[`sources::ArtistFacts::discogs`] has been storing that address since M1.1. It
is printed by `aede sources --whence`.

So the route in is:

```
MusicBrainz artist  →  relations[] type "discogs"  →  discogs.com/artist/12345
                                                      api.discogs.com/artists/12345
```

Which buys three things at once:

- **no search**, therefore no token on the critical path;
- **no name matching** — arriving by identifier means `Confidence::Identified`
  rather than `Matched`, the difference between a value that can be displayed
  and one that has to be read twice;
- for albums, the same route needs **one word** added to
  `musicbrainz::RELEASE_INCLUDES` (`url-rels`).

The general shape, worth remembering beyond this page: **before designing
credentials for a service, check whether the identifiers you already hold let
you skip the endpoint that demands them.**

## The service's terms, as of September 2026

- **Rate limit:** 25 requests per minute unauthenticated, 60 authenticated,
  over a moving 60-second window. `X-Discogs-Ratelimit` headers report usage.
  Note that unauthenticated is _slower_ than MusicBrainz's one per second.
- **User-Agent:** mandatory and must identify the application. A default `curl`
  or browser string is explicitly refused.
- **Authentication**, when wanted: a personal access token is the simple form —
  a `token=` query parameter or an `Authorization` header. OAuth 1.0a exists for
  applications acting on behalf of other users, which this is not.
- **Licence:** the core database is CC0. **Images are not**, and they need
  authentication. Nothing here would touch cover art — that stays with the
  Cover Art Archive, which is open.

Source: <https://www.discogs.com/developers/>

## What implementing it would look like

Sketched, not decided.

1. `fetch --styles` (or `--discogs`), a second pass over what `fetch` already
   stored, in the shape `--summaries` and `--discography` already have.
2. One request per album, at the service's rate: roughly five minutes over a
   hundred-album library with no token, two with one.
3. A `discogs` source in `sources.json`, alongside `musicbrainz`, `wikipedia`
   and `coverartarchive`. The store needs no change; it is keyed on source name
   already.
4. Styles into a new field on `ReleaseFacts`, compared with the tag by
   `verdict_set` and shown in the `sources` panel like genres.
5. A token, if wanted at all, as an **optional** `AEDE_DISCOGS_TOKEN`: it works
   without one and goes faster with one. No secret is written to disk by this
   program.

## Why it is set aside

Three sources are already fetched — MusicBrainz, Wikipedia, the Cover Art
Archive — and each one is a rate limit to honour, a shape to parse, a licence
to carry and a set of failure messages to word. A fourth earns its place when
somebody wants what only it has. Styles are a real gap; nobody has yet said it
is a gap that hurts.
