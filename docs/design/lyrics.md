# Lyrics

Not built, and worth splitting into three before it is, because the three parts
have nothing in common but the word.

**Reading them is M0 work, not M1.** Lyrics sit in the files already: `USLT`
(and `SYLT` for the timed ones) in ID3, `LYRICS`/`UNSYNCEDLYRICS` in Vorbis
comments, `©lyr` in MP4, `WM/Lyrics` in ASF. The parsers walk past those frames
today without picking them up, and a `.lrc` file beside the audio is a text file
anyone can read. No network, no dependency, no milestone — just fields nobody
has collected yet, and the first thing to do.

Two consequences for the model, both of which the existing rules already
decide. Lyrics **read from a file are a fact about it**, so they belong to the
catalog beside the tags — while a lyric the _user_ typed or corrected is
something they wrote, and belongs to the annotation store. Same boundary as
everywhere else: read versus written. And they are **large text**, which is a
reason to think before pouring them into a file that is rewritten whole on
every scan.

Then `lyrics:` becomes a query field, and "that song that goes something about
a train" stops being unanswerable.

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

So: fetching goes behind an explicit choice, from a source that permits it, and
never on by default. That is a decision about somebody else's rights, and this
project does not get to make it silently on a user's behalf.

**Showing them in time is M3 work.** Synchronised lyrics are what `SYLT` and
enhanced `.lrc` carry, and they only mean anything once there is a playhead to
follow. The parsers should keep the timings when they read them, so that M3
finds them there rather than asking for a re-read of the library.
