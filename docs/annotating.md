# What you think of it

## Writing things down

One note per thing — a track, an album, an artist, a label, a genre — and it is
kept **exactly as typed**, blank lines and all. A note is not a field to be
tidied: no wrapping, no trimming, no reflowing.

```sh
aede note album "Kind of Blue" --text "the 1997 remaster is the one"
aede note album "Kind of Blue" --file notes/kind-of-blue.md
vim /tmp/note.md && aede note artist "Miles Davis" --file /tmp/note.md
somecommand | aede note artist "Miles Davis" --file - --append
aede note artist "Miles Davis"            # reads it back
aede note artist "Miles Davis" --remove
aede note album "Legion" --from album:"Once Upon the Cross"
```

`--file` is what makes a note a _written_ thing rather than a command-line
argument: write it in a real editor, pipe it in with `-`. `--append` adds to
what is there, separated by a blank line, because two thoughts a month apart
are not one paragraph.

It gets a section of its own on every page, below the marks:

```
Yours

  ★★★★★   ♥   vinyl

Notes

  # Kind of Blue

  The 1997 remaster is the one: the first three sides
  run fast on the original pressings.

  written 3 days ago
```

**Markdown is the intended format**, and deliberately not handled here. Aède
stores the bytes it was given and prints them unchanged; rendering headings and
emphasis is the front end's job at M2. Two things follow for whoever writes that
front end: the text is **untrusted user input**, so it must be escaped before it
reaches any HTML, and the storage must never start "helpfully" rewriting it —
the day Aède reformats a note is the day the note stops being the user's.

## Backing up what cannot be rebuilt

```sh
aede notes --export -o backup.json
aede notes --import backup.json
```

Lose the catalog and a scan rebuilds it in a minute. Lose this and it is gone,
so it is the one file worth a backup — and the export is the file itself:
readable, greppable, repairable by hand.

**Import merges, it never replaces.** Someone restoring half a backup wants
their two halves, and an import that emptied what was already there would be
the one operation in this program able to lose everything at once. Where both
sides know a thing, the one written **last** wins, and the one that lost is
counted out loud rather than dropped in silence. Play counters take the larger
of the two, since a count is a total and neither side ever counted the other's
listens. Importing the same backup twice changes nothing.
