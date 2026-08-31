# Querying

The roadmap says "SQLite at M1" and that has been quietly standing in for a
query language. It should not. **A query language is an interface, not a
storage engine.** Defined on its own it works today over the in-memory catalog
and tomorrow over SQL; defined as "whatever SQLite makes easy", it arrives late
and shaped by the wrong concerns.

Where things actually stand:

| Capability                       | Today                                                                                                                               |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Several criteria at once         | Yes, and any depth of them                                                                                                          |
| Filters                          | Yes, per command, each refused where it means nothing                                                                               |
| Numeric and date ranges          | Yes: `year:1990..1999`, `duration:..3:30`, `rating:>=4`                                                                             |
| Sorting                          | Yes on `query` and `collection`; `artists` still has its own two keys                                                               |
| Pagination                       | Yes: `--limit`, `--offset`, `--all`, through one `Window`                                                                           |
| Aggregation and statistics       | Yes: `stats`, `years`, and counts, durations and sizes on every listing                                                             |
| Search on user tags              | Yes: `tag:`, `rating:`, `loved`, `note:`, and their `album.`/`artist.` forms; `aede search <text> --notes` reads the notes as prose |
| Search on relations              | Yes: `artist:` is any credit, and `composer:`, `producer:`, `performer:`… ask who did what                                          |
| `AND` / `OR` / `NOT`             | Yes                                                                                                                                 |
| Saved queries, smart collections | Yes: `aede collection <name> --query "…"`                                                                                           |

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
