# M1 manual verification

**Status (2026-10-03):** representative offline and live-source passes complete. The additional reported functional checks listed below close the remaining manual-verification gaps from the September pass.

## Additional reported functional checks

Successful manual checks were reported on 2026-10-03 for:

- AcoustID identification with an API key;
- Fanart.tv fetching with an API key;
- a successful lyrics download;
- a successful cover download for an album with no existing artwork.

These reported functional checks are separate from the disposable-copy rehearsal described below. All four previously outstanding M1 paths are now manually verified.

## Completed offline

- `tools/check.sh` passed: formatting, lint, tests, documentation, and release build.
- `cargo test --offline --no-default-features -q` passed. Three end-to-end tests that exercise `fetch` were conditioned on the `fetch` feature after this check exposed that they were being run in a build where `fetch` deliberately refuses to run. The default build still runs all three tests.
- In an isolated vault, the release binary scanned the repository's 19 audio fixtures, built a catalog with 3 albums and 3 artists, and reused all 19 files on the next scan without rereading them.
- The catalog's statistics, artist and album pages, source summary, review list, missing-album report, and doctor report were inspected. `fetch --dry-run` planned three passes without requesting anything. An exclusion used without `--fanart` was refused.
- An offline source document was imported into the isolated vault. An identified artist fact appeared with its source and URL, survived a scan, and disappeared when that source was forgotten. An approximate track-to-work claim stayed out of graph queries while pending, became traversable when accepted, and disappeared from those queries when rejected. Undo returned it to pending.
- The checked audio fixture had the same SHA-256 digest before and after the CLI pass. No files in the repository were changed by the manual commands.

## Completed on a disposable music copy

A nine-track album was copied into a temporary music folder with an isolated Aède data folder. All online commands operated on that disposable copy.

- The first scan found nine tracks, one album, and one artist. The next scan reused all nine audio files without rereading them. `fetch --dry-run` identified one artist and one album and made no requests.
- A live MusicBrainz fetch stored one artist and one release claim. Artist and album pages showed their source URLs and kept the claims beside the local tags. Release type, original date, and label agreed with the tags; the sourced genre was explicitly shown as absent from local tags. The country filter found the identified artist.
- Fetched, dated membership appeared in the line-up derived for the release year. The discography pass produced two missing studio albums; `missing --all` also displayed three excluded compilations with reasons.
- The summary pass requested French first and English second. The available English Wikipedia article appeared with its page URL, language, and CC BY-SA 4.0 credit. The recording/work credit pass stored answers for all nine identified recordings.
- A second scan preserved the source layer. Repeating the summary, discography, and credit passes reported that they were already done and made no requests. No approximate match arose from this tagged album; the offline pass above exercised review acceptance, rejection, and undo.
- The LRCLIB pass requested lyrics for one copied track and reported that the service had none; it wrote no sidecar. The cover pass correctly skipped an album with existing embedded artwork. AcoustID and Fanart.tv could not be tested because their API keys were absent. A successful lyric write and cover download were therefore not exercised.
- An isolated backup held the catalog and 12 source records. Removing the 11 MusicBrainz records left the Wikipedia record intact; restoring the backup brought all 12 records back. After these commands, all nine copied FLAC files had the same SHA-256 digests as the originals.

The September representative pass found no defect in the exercised M1 paths. The additional reported checks above cover the remaining optional paths.
