# Discogs — label profiles and future styles

**Status:** A label-profile fallback is implemented for `aede label <name>` when no Wikipedia biography is available. Release styles remain deferred.

The label page follows the Discogs URL attached to the MusicBrainz label on each visit unless `--offline` is given. It never searches Discogs by name and never treats a name match as an identifier. A response must repeat both the numeric label ID and the expected name before its profile is stored in `sources.json` as a distinct `discogs` claim. A failed check does not erase the stored copy; the label page shows it with an update notice, while other readers and exports hide copies older than five hours. The display puts “Data provided by Discogs.” and a link to the label page beside the text. Discogs provides the original profile, normally in English; its French website does not translate that field. Discogs markup is rendered as plain text: linked labels and artists are looked up by Discogs ID and shown by name, independently of the local catalog, and presentation tags are removed. The original markup is kept to avoid repeating those linked-entity requests when the profile has not changed; a stored profile with unresolved artist codes is reprocessed on the next successful visit. No images are fetched.

The display preserves the profile's source line breaks and blank lines, wrapping long lines individually. The label-code line is emphasized in bold when terminal styling is enabled. This also applies to stored profiles without another lookup.

The terminal shows an animated loading line during MusicBrainz and Discogs requests, including rate-limit waits and linked-name lookups. The line is cleared on success or failure; redirected output contains no animation.

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
- **Licence:** label notes are listed as CC0 data. Images are restricted data;
  the label-profile fallback does not fetch them.
- **Freshness and attribution:** the API terms prohibit displaying data more
  than six hours out of date and require a linked “Data provided by Discogs.”
  notice. Aède uses a five-hour display window and retains the fetch timestamp.
  The terms also say not to keep cached content longer than needed. Aède drops
  expired Discogs profiles from general read views and exports, while retaining
  the local copy across unrelated writes so it can be refreshed on the next visit.
  The label page deliberately shows that retained copy with an update notice
  when a refresh is unavailable. If the source has changed and the copy cannot
  be checked for more than six hours, this requested offline fallback may not
  satisfy Discogs' display limit; a notice cannot substitute for revalidation.

Sources: <https://www.discogs.com/developers/>, <https://support.discogs.com/hc/en-us/articles/360009334593-API-Terms-of-Use>

## What implementing it would look like

Sketched, not decided.

1. `fetch --styles` (or `--discogs`), a second pass over what `fetch` already
   stored, in the shape `--summaries` and `--discography` already have.
2. One request per album, at the service's rate: roughly five minutes over a
   hundred-album library with no token, two with one.
3. A `discogs` source in `sources.json` for release styles, alongside the label
   profiles now stored there. The store is keyed on source name already.
4. Styles into a new field on `ReleaseFacts`, compared with the tag by
   `verdict_set` and shown in the `sources` panel like genres.
5. A token, if wanted at all, as an **optional** `AEDE_DISCOGS_TOKEN`: it works
   without one and goes faster with one. No secret is written to disk by this
   program.

## Why release styles remain set aside

Three sources are already fetched — MusicBrainz, Wikipedia, the Cover Art
Archive — and each one is a rate limit to honour, a shape to parse, a licence
to carry and a set of failure messages to word. A fourth earns its place when
somebody wants what only it has. Styles are a real gap; nobody has yet said it
is a gap that hurts.
