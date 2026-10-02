# Catalog and data safety

Aède catalogs local music; it does not track purchases or borrowed physical discs. Scanning reads files and tags but never rewrites the originals. Copy creates separate destination files; analysis, artwork, lyrics and playlists can create explicit sidecars beside music.

## The graph vocabulary

| Object | Meaning | Example |
| --- | --- | --- |
| Release / album | An edition with track positions and local files | One CD pressing of an album |
| Release group | The shared album identity across editions | The album before choosing a pressing |
| Recording | One recorded performance | The same performance on an original and compilation |
| Track | A recording’s placement in a release | Track 3 of disc 1 |
| Work | A composition | The work performed by different recordings |
| Artist / credit / relation | People, their roles and attributed links | Guitarist, producer, composer, membership |

Titles alone do not establish shared identities. MusicBrainz IDs and explicit relationships distinguish editions/performances and classical work parts. Copy the Open/Continue commands from entity pages when precision matters.

## Four persistent stores

| File | Contains | Recovery |
| --- | --- | --- |
| `catalog.json` | Tag-derived graph and watched/excluded folders | Rebuild by scan; roots must be supplied after reset |
| `conclusions.json` | Integrity, fingerprints, imported/derived analyses | Back up; changed sources make old results stale |
| `user.json` | Notes, stars, favorites, labels, history, collections and choices | Irreplaceable personal work; back up |
| `sources.json` | Attributed external claims and review decisions | Back up; some facts can be fetched again |

External facts stay beside local tags and carry provenance/confidence. A proposed or conflicting identity requires review before it becomes a trusted graph link. Your notes preserve their Markdown text; rendering belongs to a client, not a storage rewrite.

## Folder access and consistency

The music folder needs read access for scanning/playback. The Aède data folder needs read/write access for updates and must not be writable by unrelated users/groups when serving. A sidecar-writing command also needs write access at its output location. A server executes delegated writes with its account’s permissions.

Do not delete `.aede.lock`, hand-edit live JSON, or run old writers against a current server. All current writers share a data lock. On Unix the running server coordinates compatible CLI updates through its private socket; without it they run locally. Reads of an old snapshot do not mean an update was lost.

## What a backup protects

`aede backup FILE` saves the four readable stores in a versioned document, including personal history. It does not copy music, downloaded images, lyrics or playlists. Save those separately and keep an off-machine copy. `export` is for data inspection/sharing; `rules` is for reproducible choices; neither substitutes for a complete backup plus your original files.

Preserve the old data before an upgrade/restore. Rehearse restore into a separate empty `--data` folder. A restored catalog is a dated snapshot; a later scan reconciles actual files. See [backup](../cli/backup.md), [restore](../cli/restore.md), [sources](../cli/sources.md) and [review](../cli/review.md).
