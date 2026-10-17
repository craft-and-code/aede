# fetch — Enrich artists, albums, tracks, and artwork from chosen sources

fetch is explicit network access. Without a pass option it asks MusicBrainz about artists and albums. Choosing one or more passes runs those requested families; lyrics are never enabled implicitly. Existing paths are folder restrictions, other positional words are artist/album-name restrictions. Both kinds can be combined and apply independently to every pass.

Ordinary runs reuse stored answers and existing local artwork; --full asks again about stored answers but does not overwrite local images. --dry-run lists requests without sending them. Large runs can ask for confirmation; --yes accepts them. Completed answers are saved progressively, so retry after a failure reuses completed work. With the local Unix server, work survives the CLI’s disconnection and can be stopped with cancel.

A completed Discogs label batch with unavailable profiles reports a final partial failure but lets subsequent selected artwork passes run. Provider throttling and unusable passes still stop the run. Discogs label profiles do not supply artist biographies or banners. For a newly added band, run ordinary `fetch` first, then `fetch --summaries --covers --images --fanart`; Fanart.tv needs `AEDE_FANARTTV_KEY`, and uncertain MusicBrainz proposals need explicit review before secondary services may use them. Desktop administration can request this ordering within one task using `musicbrainz:true`.

Known MusicBrainz identifiers support precise recording/work/edition credits; fuzzy name proposals and conflicting identities remain separate evidence for review. Secondary discography, summary and artwork passes follow only exact or explicitly accepted identities; rejected proposals are skipped. Accepting a match preserves its original confidence score. --credits and --recordings are aliases, and recording requests match artist names, album titles and track titles. --identify uses stored acoustic fingerprints and AEDE_ACOUSTID_KEY; --fanart/--logos require AEDE_FANARTTV_KEY. Put service keys in the process environment, never a public page or URL.

Track passes `--identify`, `--lyrics` and recording credits match the local track title, its main artist or its album title. Recording-credit selection checks each local placement, including when the same recording has different titles on different releases; its completed lookup remains shared by recording identity. Identification also retains its file-path name matching.

A label ID learned from a release requires a trusted release claim before `--labels` can use it as an exact lookup; otherwise the label remains a name search. The Discogs fallback requires a trusted MusicBrainz label claim, including on the online label page; only a trusted Wikipedia biography suppresses that fallback. A cached front-cover URL also requires a trusted artwork claim before reuse. A malformed Wikidata or Wikipedia response counts as a failed request, preserving any biography and its credit rather than caching an empty answer; a valid empty sitelinks table or summary remains a completed absence.

After an accepted MusicBrainz match, the independent Discogs lookup is identified by its confirmed numeric ID/name while the original MusicBrainz score and review stay unchanged. Existing Discogs rejections remain effective. Refresh older approximate Discogs copies with `--full` or the online label page; no migration runs during browsing.

Unreadable recording/edition relationship tables or consumed credit/work targets, AcoustID recording metadata and Wikidata/Fanart image tables are failures, not completed empty answers. They leave prior source evidence and decisions intact and remain retryable. Optional absent metadata and valid empty tables retain their existing meaning. A portrait already present as JPEG or PNG is left alone in both ordinary and `--full` portrait runs, before any provider request. An existing JPEG/PNG label logo also avoids another image transfer; its Fanart.tv metadata lookup can still run, including with `--full`.

The same relationship-table admission applies to MusicBrainz artist and label lookups, and artist search entries. A present unreadable `relations` value cannot replace previously retained links or memberships during a refresh. An absent table, a valid empty table and unknown relationship objects keep their existing meaning.

Images and .lrc lyrics are sidecars, not rewritten tags. New sidecars are published without replacing existing files; their filesystem must support hard links, otherwise the download refuses the unsafe publication (for example FAT/exFAT). Artist assets use a shared artist folder when one exists, otherwise the data assets directory. Album artwork lives beside the album/in artwork/. On fetch, --images and --size require --covers. --banners requires --logos or --fanart; every --no-* exclusion requires --fanart. Positive --logos/--banners cannot be combined with their corresponding negative exclusions. Summaries retain source, licence and preferred language. A server-submitted fetch uses the server’s environment and paths, not the connecting machine’s.

Fallback artist/label cache identifiers must be single ordinary path components. Path-shaped identifiers and existing symbolic links or non-directory entries in the cache hierarchy have no destination. This checks the observed hierarchy; it does not protect against another local process replacing directories concurrently.

`--covers` uses an edition ID first, then a release-group ID already in local tags or trusted source evidence. A cover removed since an earlier fetch can reuse its saved address. A remembered cover path is checked on disk, so deletion does not require a rescan before recovery; an existing entry or an unreadable presence check keeps the non-replacement behavior. An explicit `--size` or `--full` asks the index again, so a previously saved thumbnail width does not override the new request. A newly confirmed front replaces an earlier negative answer even if its transfer fails, allowing an ordinary run to retry it. JPEG and static PNG pixels are fully decoded before publication, and their containers must end correctly. Incomplete containers and undecodable pixel streams are refused. Animated PNG is refused explicitly. Images are limited to 32 MiB, 8192 pixels per axis and 16 million pixels; choose a smaller archive `--size` if an original exceeds those limits. Validation requires no external helper. JPEG has no integrity checksum, so this checks decodability rather than certifying an unchanged original.

An existing front cover is reused before downloading image bytes, even before a rescan and under alternative filenames recognized by the scanner. It is neither a new download nor a failed transfer. An index lookup can still run under the existing ordinary/`--full` rules; extra images are handled independently.

The `--covers --images` pass records completion under `coverartarchive-artwork` only after every extra image succeeds, including a valid answer with no images. An existing `artwork/` directory alone does not mean the pass finished. Retries skip individual image files already saved. Use `--full` after deleting extras you want downloaded again; existing images remain untouched. Lyrics already on disk are skipped even before a rescan, and matching requires at least one whole second of known duration.

Deleting a lyrics sidecar allows it to be fetched again before a rescan. The pass checks both the remembered path and the usual `.lrc` destination on disk; existing entries are still never replaced.

LRC metadata headers alone do not count as lyrics in a tag or provider answer. A readable plain answer remains usable when the synced field contains only headers; a response with no readable lyrics creates no sidecar and remains retryable. Timed empty cues retain their meaning, and an existing sidecar is still never replaced.

Discography pages are stored only as a complete answer: malformed pages, repeated entries, inconsistent totals and the ten-page limit leave the previous answer intact. A successful complete answer, including an empty discography, is cached across restarts; `--full` asks again. Older non-empty lists remain usable. An older empty list without completion evidence is fetched once to distinguish an answer from an artist never browsed. Unreadable stored entries invalidate the whole discography so it can be requested again; other artist facts remain available.

## Syntax and arguments

```text
aede fetch [name… | folder…] [options]
```

Zero or more existing folders or name terms. Quote a multiword name. A path must exist to be treated as a folder.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--yes` | Skip the confirmation when this command asks for one. Inspect the intended operation first; use deliberately for unattended execution. |
| `--full` | Query stored source answers again; never overwrite local artwork or audio. |
| `--summaries` | Fetch Wikipedia introductory prose via Wikidata, with language, attribution and licence. |
| `--discography` | Fetch MusicBrainz releases credited to artists; missing compares these with the local catalog. |
| `--covers` | Fetch missing front covers from Cover Art Archive; local images are never replaced. |
| `--identify` | Ask AcoustID about stored fingerprints; first compute them with fingerprint. Requires AEDE_ACOUSTID_KEY. |
| `--credits` | Fetch recording, composition and exact-edition MusicBrainz credits and explicit work/part relationships using known IDs. |
| `--recordings` | Compatibility spelling of --credits; identical behavior. |
| `--portraits` | Fetch an artist portrait through Wikidata first, then Fanart.tv when needed. |
| `--logos` | Fetch artist and identified-label logos from Fanart.tv; requires AEDE_FANARTTV_KEY. |
| `--fanart` | Fetch supported Fanart.tv families in one metadata pass: logos, portrait, banner, background, album cover and cdART. Requires its API key. |
| `--no-logo` | Exclude artist logos from --fanart. Requires --fanart. |
| `--no-label-logo` | Exclude label logos from --fanart. Requires --fanart. |
| `--no-portrait` | Exclude portraits from --fanart. Requires --fanart. |
| `--no-background` | Exclude backgrounds from --fanart. Requires --fanart. |
| `--no-banner` | Exclude banners from --fanart. Requires --fanart. |
| `--no-album-cover` | Exclude album covers from --fanart. Requires --fanart. |
| `--no-cdart` | Exclude disc images from --fanart. Requires --fanart. |
| `--banners` | Also fetch a wide artist banner with --logos or the complete --fanart pass. |
| `--labels` | Identify record labels through MusicBrainz. Name-only proposals remain reviewable claims. |
| `--discogs` | Retrieve identifier-linked Discogs profiles for labels without Wikipedia prose. Combine with `--labels` to identify labels first; fresh profiles are reused unless `--full`. |
| `--size VALUE` | With fetch --covers: 250, 500, 1200 (default) or original. With spectrum: half (default) or full. |
| `--lang CODE` | Preferred language for --summaries prose, for example fr. English remains the fallback; without a prose pass this setting does not create a summary request. |
| `--images` | With --covers only, also download missing non-front Cover Art Archive images into artwork/. Refused without --covers. |
| `--dry-run` | Preview the work without making downloads or creating outputs. It can still validate tools and inputs. |
| `--lyrics` | On track, display lyrics; on search, search their text; on fetch, download missing .lrc sidecars from LRCLIB. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede fetch --dry-run "$HOME/Music/Jazz"
aede fetch --credits "$HOME/Music/Jazz"
aede fetch --summaries --lang fr "Miles Davis"
aede fetch --lyrics "$HOME/Music/Jazz"
aede fetch --covers --images "$HOME/Music/Jazz"
aede fetch --fanart --no-background --no-cdart
```

## Result and errors

Each requested pass names its scope and reports stored/downloaded answers, refusals and failures. “Left alone” means no unambiguous answer was chosen, not that tags were changed. Individual HTTP failures can be counted while later requests and passes continue, so exit code 0 does not guarantee every request succeeded; read the per-pass report. An unusable option or storage failure stops the run. HTTP 503 receives bounded increasing waits; a persistent refusal stops the run. HTTP 429 stops immediately without that retry sequence. Already completed work remains saved. --dry-run prints requests and “nothing was asked” without creating sidecars, source records or the writer lock; refusing a large-run confirmation also asks nothing. Rescan after downloaded artwork/lyrics to attach their sidecars to catalog navigation. Review ambiguous identity proposals before relying on them.

## Related reading

[fingerprint](fingerprint.md), [credits](credits.md), [review](review.md), [missing](missing.md).

Detailed existing guide: [sources.md](../sources.md).
