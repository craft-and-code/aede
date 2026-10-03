# Your Thoughts, Your Data: Annotating in Aède

Your music library is more than just a collection of audio files; it is a personal journey. Aède treats your thoughts on a track, an album, an artist, a label, or a genre with the reverence they deserve.

One note per entity, preserved **exactly as typed**. Blank lines and all. A note in Aède is not a sterile database field to be tidied up: there is no auto-wrapping, no silent trimming, and no reflowing. Your words remain entirely yours.

```sh
aede note album "Kind of Blue" --text "the 1997 remaster is the one"
aede note album "Kind of Blue" --file notes/kind-of-blue.md
vim /tmp/note.md && aede note artist "Miles Davis" --file /tmp/note.md
somecommand | aede note artist "Miles Davis" --file - --append
aede note artist "Miles Davis"            # reads it back
aede note artist "Miles Davis" --remove
aede note album "Legion" --from album:"Once Upon the Cross"
```

The `--file` flag elevates a simple note into a crafted piece of writing. Draft it in your favorite editor, or pipe it in directly with `-`. Use `--append` to add new thoughts over time—because a revelation you have today shouldn't overwrite the memory you recorded a month ago. They are simply separated by a blank line.

When displayed, your writing takes its rightful place on the page, cleanly separated from the technical metadata:

```text
Yours

  ★★★★★   ♥   vinyl

Notes

  # Kind of Blue

  The 1997 remaster is the one: the first three sides
  run fast on the original pressings.

  written 3 days ago
```

### The Elegance of Markdown and Pure Ownership

**Markdown is the soul of your notes.** Aède embraces Markdown's simple elegance by getting completely out of its way. We store the exact bytes you provide and hand them back untouched. Rendering headings, bold text, and emphasis is a joyous task left to the front end (like M2).

Two critical principles of **data ownership** follow from this design:

1. **Security:** The text is strictly **untrusted user input**. The front end must meticulously escape it before weaving it into HTML to protect the community of users.
2. **Integrity:** The storage layer must _never_ "helpfully" rewrite your input. The day Aède silently reformats a note is the day that note stops belonging to you. We refuse to cross that line.

## Preserving Your Legacy

```sh
aede notes --export -o backup.json
aede notes --import backup.json
```

If you lose your catalog database, a quick scan rebuilds the index in minutes. But if you lose your annotations, a piece of your musical history is gone forever. This makes your notes the single most precious asset in your library. The export tool respects this by generating a file that is beautifully simple: readable, searchable, and fully repairable by hand.

**Importing is an act of merging, never destroying.** If you restore a partial backup, Aède gracefully combines it with your current library. An import will _never_ wipe out what is already there—that would be a catastrophic violation of your trust.

When both datasets contain information about the same entity, Aède resolves it logically:

- **Notes:** The note written **last** wins. The overwritten note isn't silently deleted; it is respectfully counted and reported out loud.
- **Play Counts:** Aède keeps the larger of the two numbers. Since counts always grow, neither dataset invalidates the other's listening sessions.

And because it is built on solid, predictable logic, importing the exact same backup twice changes absolutely nothing. Peace of mind, guaranteed.

Listening events are merged by full content and multiplicity, so distinct or repeated listens in the same second survive, without multiplying them on a repeated import. Independent reattachment journals retain distinct snapshots even if their local sequence numbers coincide. Present store tables must be JSON arrays; malformed table shapes are refused instead of becoming empty data. Human display escapes terminal control characters, while stored text and JSON/CSV exports preserve the original values.

The [Subsonic/OpenSubsonic adapter](server/subsonic.md) also keeps private ordered playlists and client-declared scrobbles here. Playlist duplicates and order are preserved; these are separate from smart collections and exported M3U files. Each playlist keeps its owner and stable ID; the newer complete version wins on import. On an equal timestamp the existing version is kept and counted in the merge report. Scrobbles retain Unix milliseconds without inventing played duration/completion and merge by content and multiplicity like other listens. Both tables are optional additions to personal format 2, so older stores remain readable. Complete personal exports and backups include them.

## Recovering waiting annotations

Scans retain annotations whose targets are missing. An automatic move requires one candidate matching previously captured file size, stream properties, identifying tags and any known fingerprint; a filename alone is never enough. Legacy annotations without evidence remain waiting. An album uses its canonical release folder, so removing one disc does not detach the album note.

```sh
aede notes --waiting
aede notes --relink "track:/old/01.flac" --to "track:/new/01.flac" --dry-run
aede notes --relink "track:/old/01.flac" --to "track:/new/01.flac"
aede notes --relinks
aede notes --undo-relink 1 --dry-run
aede notes --undo-relink 1
```

Copy the source reference from `--waiting`. For `--to`, use an exact reference or a unique name prefixed by its kind, such as `track:So What` or `album:Legion`; ambiguous names are refused. Quote references containing spaces. A move requires a waiting source and an existing target of the same kind. Conflicting personal data is refused. Notes, listening events, counts, relationship endpoints, playlist entries and client scrobbles move for the current owner only. Waiting/relink summaries count affected playlists, even when a playlist repeats the track, and count client scrobbles separately from duration-bearing `Play` events. Undo preserves the original wording and playlist order and refuses if affected data was edited or new conflicting data appeared. Undo also prevents the same old track reference from being automatically moved again. Identity evidence and decision history travel in personal exports/backups.
