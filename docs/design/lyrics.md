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

**Showing them in time is M3 work.** Synchronised lyrics are what `SYLT` and
enhanced `.lrc` carry, and they only mean anything once there is a playhead to
follow. The parsers should keep the timings when they read them, so that M3
finds them there rather than asking for a re-read of the library.
