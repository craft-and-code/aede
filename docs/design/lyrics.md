# Lyrics

Not built, and worth splitting into three before it is, because the three parts
have nothing in common but the word.

**Reading them was M0 work, and it is done.** Lyrics sit in the files already:
`USLT` in ID3, `LYRICS`/`UNSYNCEDLYRICS` in Vorbis comments, `©lyr` in MP4 — all
three already folded into one `lyrics` field by `tags::canonical_key` — and a
`.lrc` file beside the audio is a text file anyone can read. No network, no
dependency, no milestone.

```sh
aede track "Crazy Train" --lyrics
aede query "lyrics:aboard"           # that song that goes something about a train
aede search "all aboard" --lyrics    # …and where the line actually is
```

`search --lyrics` sits beside `--comments` and `--notes` and follows their rule:
off by default, because a common word in a song would bury the album that
actually bears the name, and in a section of its own, because a hit found in a
song was found by another route than a hit on a name. It shows the **line** that
matched rather than the song — a table cell holding four hundred lines is one
nobody can read.

Both sources go through one parser, because a tag can perfectly well hold LRC:
plenty of taggers write the synced text straight into `LYRICS`, and a reader
that only understood plain text would show a page of `[00:12.34]` to somebody
who asked for the words. Timestamps are kept as read — `mm:ss`, `mm:ss.cc` and
`mm:ss.mmm` are all written in the wild — so that M3 finds them in hand rather
than asking for a re-read of the library. A chorus timed twice (`[00:12][01:44]
the chorus`) is two lines, because the file means both and a player that kept
only the first would fall silent at its second turn. The `[ar:]`, `[ti:]` and
`[by:]` headers are dropped, since they repeat what the tags already say and a
`.lrc` is not where an artist's name is settled; `[offset:]` is applied, since
it exists precisely to shift a timing made against another encoding.

**Where they live follows the rule the rest of the catalog follows.** Tag lyrics
are already in it, because raw tags are kept per file. A sidecar is not in the
file, so storing its text as though it were a tag would make the catalog lie
about what the file says: the catalog keeps the sidecar's **path**, and the text
is read when somebody asks. It is one small file, sitting next to the music, and
reading it is always current where a copy would go stale.

The `.lrc` is read from the **walk**, not carried over from the previous scan.
A lyrics file dropped beside a track nobody touched has to attach on the next
scan, and the track's own bytes have not changed to announce it — the same
reasoning that makes cover art a property of the folder rather than of the file.

They are shown behind `--lyrics` rather than on the track page by default: a
song is longer than everything else that page says put together, and the page is
read to learn what a file _is_.

One consequence is still ahead: a lyric the _user_ typed or corrected is
something they wrote, and belongs to the annotation store, not to the catalog.
Same boundary as everywhere else — read versus written — and nothing writes
lyrics yet.

**Fetching them is M1 work, and comes with a caveat that is not technical.**
Lyrics are the _composition_ copyright, which owning a FLAC grants no rights
in — they are legally a different object from the recording. In practice every
comparable open-source project keeps online fetching out of its core: Navidrome
and Jellyfin read files and leave the network to plugins, beets ships a
`lyrics` plugin, foobar2000 and MusicBee do it through add-ons. The one
commercial exception, Plex, pays a licensed provider. Of the free sources,
LRCLIB is the only one an open-source server can query without breaking an
API's own terms — Genius forbids the scraping its lyrics require, and
Musixmatch's free tier is non-commercial and truncated.

LRCLIB is also the right place to **learn the network path** before MusicBrainz:
no key, no account, a JSON response of six fields, and a failure that costs
nothing — no lyrics is not a broken catalog, where a bad MusicBrainz match
attaches the wrong record to an album. It matches on artist, title, album and
**duration**, all four of which this catalog holds exactly. Two operational
notes for whoever writes that client: identify the program honestly in the
`User-Agent` (LRCLIB drops connections from agents it has come to distrust —
Jellyfin's server agent is refused outright, with no status code at all, the
connection simply closing), and remember that the whole database is published as
SQLite dumps, which is the polite way to fill ten thousand tracks rather than
ten thousand requests.

So: fetching goes behind an explicit choice, from a source that permits it, and
never on by default. That is a decision about somebody else's rights, and this
project does not get to make it silently on a user's behalf.

### It is built, and this is the shape it took

```sh
aede fetch --lyrics                 # every track that has none
aede fetch "Crazy Train" --lyrics   # narrowed, by title or by artist
aede fetch --lyrics ~/Music/Alastis # narrowed by shelf, whatever it is called
aede fetch --lyrics --dry-run       # what would be asked, asking nothing
```

The folder is read the way every option of `fetch` reads one — anything typed
that exists on the disk is a folder, anything else is a name — and
[the manual](../sources.md#narrowing-a-run-by-folder) has the rule. It matters
most here: a shelf is the natural unit for a decision about somebody else's
rights, and *the words for this record* should not have to be spelled as a
search for a word that happens to match nothing else.

**The typed option is the consent.** The caveat above is printed on every run,
before anything is asked — not once, not in a file nobody opens. It is
deliberately *not* doubled by a prompt each time: a confirmation asked every
time is a confirmation nobody reads, which would leave the caveat less read
than printing it plainly does. A long run is still agreed to, on the same
threshold as the ordinary fetch, and that is about the ten minutes rather than
about the rights.

**A track that already has words is never asked about.** The catalog answers
that offline and exactly — the words are in the file's tags, or a `.lrc` is
beside it — and the header says how many were left out for each reason, because
a filter the reader cannot see is a trap. There is deliberately no `--replace`:
overwriting a lyrics file somebody wrote or corrected is not something this
should be able to do by accident, which is the refusal `--covers` already makes
about artwork.

**Each answer is written as a `.lrc` beside its track**, which is the file every
player already looks for and the one the walk already picks up. It is
registered nowhere: the next scan discovers it exactly as it would one put there
by hand. Nothing is written into an audio file, here or anywhere in this
program. The check that the file does not already exist is made **again** at the
moment of writing, not only in the survey: the two are separated by a network,
and a `.lrc` that appeared meanwhile — by hand, or from a second copy of the
program — must not be overwritten by an answer asked for before it existed.

**The timed form wins where there is one.** The service holds both, and they are
two different things: one is a page to read, the other a file a player can
follow. Taking the plain form when a timed one exists would throw away something
that cannot be recovered.

**An instrumental is an answer, not an absence.** The service says so, and it is
a finished question — telling a reader "no lyrics found" about one would send
them looking for a fault that is not there. Nothing is written for it: an empty
`.lrc` would put a claim on somebody's disk that nobody made, and would then
hide the track from every later run.

**A track the service has nothing for is asked about again on the next run.**
Said plainly rather than hidden, because it is the one cost here a reader might
not expect. A successful answer records itself — the `.lrc` is on the disk and
the track is skipped from then on — so only the misses come back. Recording
those would mean a store of negative claims about a service that gains entries
every day, which is a worse trade than one wasted request on a run somebody
typed on purpose. Above five hundred requests the run says so and names the
published database dump, which is the polite way to fill a whole library.

Its throttle is MusicBrainz's, shared with the rest of the program: one client,
one rate limiter, and being slower than a free service requires is never the
wrong mistake.

### Three things the live answers said that the documentation did not

The parser was written from the documented shape and then checked against real
responses — a hit, a hit with fewer criteria, and a miss. The field names held.
These did not.

**The length is not the service's requirement, it is this program's.** A request
carrying only `artist_name` and `track_name` came back `200`. So the album and
the duration are Aède's insistence rather than LRCLIB's, and the reason has to
be stated as ours: a song has a studio take, a live one and three covers, they
share a title, and the length is what tells them apart. The catalog measures it
on the stream rather than reading it from a tag, so asking the narrow question
costs nothing. A track whose length could not be read is skipped, and the line
that skips it now says why *this program* declined rather than blaming the
service.

**An empty album is left out of the address, not sent empty.** They are two
different questions — an omitted parameter asks with one criterion fewer, where
`album_name=` states that the album is called nothing — and only the first has
been seen to work.

**A miss is a `404` with a well-formed body**, `{"name":"TrackNotFound",…}`.
Read on its own that document is indistinguishable from a service having a bad
day: it is the *status* that says "not found". Which is what made
`Refusal::Missing` necessary rather than merely tidy.

Two smaller ones, both kept in the fixture because a parser written from the
documentation would have guessed at them: the timestamps carry a space after the
bracket (`[00:00.15] All aboard!`), and the blank lines between verses are timed
too — kept, because a player following the timings needs to know when the
singing stops. There is also a newer `lyricsfile` field, a richer per-line
format with start and end times; `syncedLyrics` is what a `.lrc` is, so that is
what is written.

One thing the service's answer taught the rest of the program. A `404` used to
arrive as `Refusal::Failed("the service answered 404")`, and the cover pass read
it back out with `detail.contains("404")` — a comparison that stops working the
day the wording changes and says nothing when it does. Here it would have been
worse than a wart: a library of four hundred tracks holds plenty LRCLIB has
never seen, and a run reporting four hundred failures would describe a working
service as broken. `Refusal::Missing` is now its own thing, and both passes read
it.

**Showing them in time is M3 work.** Synchronised lyrics are what `SYLT` and
enhanced `.lrc` carry, and they only mean anything once there is a playhead to
follow. The parsers should keep the timings when they read them, so that M3
finds them there rather than asking for a re-read of the library.
