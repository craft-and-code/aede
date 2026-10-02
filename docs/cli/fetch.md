# fetch — Enrich artists, albums, tracks, and artwork from chosen sources

fetch is explicit network access. Without a pass option it asks MusicBrainz about artists and albums. Choosing one or more passes runs those requested families; lyrics are never enabled implicitly. Existing paths are folder restrictions, other positional words are artist/album-name restrictions. Both kinds can be combined and apply independently to every pass.

Ordinary runs reuse stored answers and existing local artwork; --full asks again about stored answers but does not overwrite local images. --dry-run lists requests without sending them. Large runs can ask for confirmation; --yes accepts them. Completed answers are saved progressively, so retry after a failure reuses completed work. With the local Unix server, work survives the CLI’s disconnection and can be stopped with cancel.

Known MusicBrainz identifiers support precise recording/work/edition credits; fuzzy name proposals and conflicting identities remain separate evidence for review. --credits and --recordings are aliases. --identify uses stored acoustic fingerprints and AEDE_ACOUSTID_KEY; --fanart/--logos require AEDE_FANARTTV_KEY. Put service keys in the process environment, never a public page or URL.

Images and .lrc lyrics are sidecars, not rewritten tags. New sidecars are published without replacing existing files; their filesystem must support hard links, otherwise the download refuses the unsafe publication (for example FAT/exFAT). Artist assets use a shared artist folder when one exists, otherwise the data assets directory. Album artwork lives beside the album/in artwork/. On fetch, --images and --size require --covers. --banners requires --logos or --fanart; every --no-* exclusion requires --fanart. Positive --logos/--banners cannot be combined with their corresponding negative exclusions. Summaries retain source, licence and preferred language. A server-submitted fetch uses the server’s environment and paths, not the connecting machine’s.

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

Each requested pass names its scope and reports stored/downloaded answers, refusals and failures. “Left alone” means no unambiguous answer was chosen, not that tags were changed. Individual HTTP failures can be counted while later requests and passes continue, so exit code 0 does not guarantee every request succeeded; read the per-pass report. An unusable option or storage failure stops the run, and unrecovered rate limiting can stop it with already completed work saved. --dry-run prints requests and “nothing was asked”; refusing a large-run confirmation also asks nothing. Rescan after downloaded artwork/lyrics to discover their sidecars. Review ambiguous identity proposals before relying on them.

## Related reading

[fingerprint](fingerprint.md), [credits](credits.md), [review](review.md), [missing](missing.md).

Detailed existing guide: [sources.md](../sources.md).
