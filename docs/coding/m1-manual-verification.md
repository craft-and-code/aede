# M1 manual verification

**Status (2026-10-03):** representative offline and live-source passes complete. The owner also confirmed the optional paths listed below in real use, closing the remaining manual-verification gaps from the September pass.

## Additional real-use checks confirmed by the owner

On 2026-10-03, the owner reported successful manual checks of:

- AcoustID identification with an API key;
- Fanart.tv fetching with an API key;
- a successful lyrics download;
- a successful cover download for an album with no existing artwork.

These are owner-confirmed functional checks, separate from the disposable-copy rehearsal described below. All four previously outstanding M1 paths are now manually verified.

## Completed offline

- `tools/check.sh` passed: formatting, lint, tests, documentation, and release build.
- `cargo test --offline --no-default-features -q` passed. Three end-to-end tests that exercise `fetch` were conditioned on the `fetch` feature after this check exposed that they were being run in a build where `fetch` deliberately refuses to run. The default build still runs all three tests.
- In an isolated vault, the release binary scanned the repository's 19 audio fixtures, built a catalog with 3 albums and 3 artists, and reused all 19 files on the next scan without rereading them.
- The catalog's statistics, artist and album pages, source summary, review list, missing-album report, and doctor report were inspected. `fetch --dry-run` planned three passes without requesting anything. An exclusion used without `--fanart` was refused.
- An offline source document was imported into the isolated vault. An identified artist fact appeared with its source and URL, survived a scan, and disappeared when that source was forgotten. An approximate track-to-work claim stayed out of graph queries while pending, became traversable when accepted, and disappeared from those queries when rejected. Undo returned it to pending.
- The checked audio fixture had the same SHA-256 digest before and after the CLI pass. No files in the repository were changed by the manual commands.

## Completed on a disposable music copy

The nine-track *Race of Cain* album was copied from `/Users/kcell/Desktop/tmp` into a temporary music folder. Its Aède vault was separate from the user's normal vault. All online commands operated on that copy.

- The first scan found nine tracks, one album, and one artist. The next scan reused all nine audio files without rereading them. `fetch --dry-run` identified one artist and one album and made no requests.
- A live MusicBrainz fetch stored one artist and one release claim. Artist and album pages showed their source URLs and kept the claims beside the local tags. Release type, original date, and label agreed with the tags; the sourced genre was explicitly shown as absent from local tags. The Norway country filter found the artist.
- The fetched membership listed Neige from 2007, and the 2007 album page derived the same line-up. The discography pass produced two missing studio albums; `missing --all` also displayed three excluded compilations with reasons.
- The summary pass requested French first and English second. The available English Wikipedia article appeared with its page URL, language, and CC BY-SA 4.0 credit. The recording/work credit pass stored answers for all nine identified recordings.
- A second scan preserved the source layer. Repeating the summary, discography, and credit passes reported that they were already done and made no requests. No approximate match arose from this tagged album; the offline pass above exercised review acceptance, rejection, and undo.
- The LRCLIB pass requested lyrics for one copied track and reported that the service had none; it wrote no sidecar. The cover pass correctly skipped an album with existing embedded artwork. AcoustID and Fanart.tv could not be tested because their API keys were absent. A successful lyric write and cover download were therefore not exercised.
- An isolated backup held the catalog and 12 source records. Removing the 11 MusicBrainz records left the Wikipedia record intact; restoring the backup brought all 12 records back. After these commands, all nine copied FLAC files had the same SHA-256 digests as the originals.

The September representative pass found no defect in the exercised M1 paths. Its remaining optional checks were completed later by the owner, as recorded above.
