# Options, syntax and output

An option changes one command’s behavior. Aède separates true shared options from reusable command-scoped families and specialized flags. Never assume that a familiar option applies everywhere: the command’s own table is the practical reference.

## Shared presentation and data options

| Option | Meaning |
| --- | --- |
| `--data FOLDER` | Use this Aède data directory, not a music folder to scan. Applies wherever the operation reads/writes stored data. |
| `--no-color` | Disable terminal ANSI colors. |
| `--help`, `-h` | Show index or current command help and stop. |
| `--version`, `-v`, `-V` | Print executable version and stop. |

Data precedence is `--data`, `AEDE_HOME`, `$XDG_DATA_HOME/aede`, `~/.local/share/aede`, then `.aede` in the working folder if HOME is unset. A direct file inspection needs no catalog; playback still uses the data directory for catalog selections/history. Use one location consistently with the server.

## How to type values

```sh
aede albums --limit 10
aede albums --limit=10
aede track "So What" --artist "Miles Davis"
aede play "/path/to/audio.flac" --bass -2
aede note album "Kind of Blue" --file -
aede file -- ./-track.flac
```

Both `--option value` and `--option=value` work. Paths/numbers/keywords take one token. Name-valued options (`--artist`, `--album`, `--with`, `--genre`, `--label`, `--comment`, `--role`, `--country`, `--text`, `--from`, `--tag`, `--query`, `--collection`, `--instrument`) consume words until the next option: put the positional name first. Quote multiword paths and names to make boundaries explicit. `--` ends option parsing for a path starting with a dash. A lone `-` is accepted as standard input for notes. `--bass -2`/`--treble -2` specially accept a negative numeric value; `--bass=-2` also works.

Short forms are separate options: `-j` means command-scoped `--json`, `-o FILE` means command-scoped `--output FILE`. `-o=FILE` is supported. Do not combine short letters (`-jo`); that is not a declared alias. Unknown/missing-valued options are validated even before help/version. Repeating the same option currently keeps the last value; write it once to avoid accidental replacement.

## Reusable families

Pagination: `--limit N`, `--offset N`, `--all` only on the commands below. N is a whole number; limit must be positive, offset starts at zero, all and limit are incompatible. Ordinary paged lists generally default to 50; some reports choose smaller subsidiary lists. Export preserves the actual filtered/paged rows, so request all deliberately.

Output: choose one of CSV, JSON or M3U where supported. `--output`/`-o` writes a real export, not arbitrary human pages. `--separator` requires CSV; `export --tracks` requires CSV. CSV uses RFC-compatible quoting; tools that split blindly on commas can corrupt quoted titles. `--separator tab` avoids that for simple tab-delimited shell workflows. M3U contains file paths and does not copy sound.

Export destinations are checked before a command changes personal data. Active Aède stores and writer locks, existing audio files, symbolic links and non-regular targets are refused. An ordinary report is replaced atomically after its full content is written; missing report folders may be created. Human terminal output escapes control instructions in notes, paths and metadata. JSON/CSV exports preserve original values; treat CSV text as untrusted data when opening it in a spreadsheet.

Confirmation: `--yes` exists only for reset, history, fetch, backup and restore. It skips that command’s actual confirmation; it is not a universal “ignore errors” option. Without a terminal, a needed confirmation refuses unless deliberately accepted.

Work reuse: `--full` belongs to scan/check/spectrum/fetch/fingerprint with different meanings; analyze uses `--force`. Never infer behavior from the name alone.

## Complete recognized-option matrix

This inventory follows the current dispatcher and handlers. It lists accepted command scope, not a promise that every combination is valid. Specialized constraints are documented per command. Current known handler gaps: `missing --source` and `merge --source` are accepted but not applied; `sources --source` filters list/forget and selects template provenance, while summary/export currently include every source. `--compress`/`--quality` belong semantically to copy although the current dispatcher does not reject those recognized names on every unrelated command. Use only their documented copy scope.


| Option | Commands / scope |
| --- | --- |
| `--data` | Shared (see above) |
| `--port N` | [serve](serve.md) |
| `--replace` | [scan](scan.md), [copy](copy.md) |
| `--remove` | [roots](roots.md), [relation](relation.md), [missing](missing.md), [collection](collection.md), [love](love.md), [rate](rate.md), [note](note.md), [tag](tag.md), [played](played.md), [history](history.md) |
| `--limit N` | [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [import](import.md), [review](review.md), [relations](relations.md), [missing](missing.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [history](history.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--sort ORDER` | [query](query.md), [collection](collection.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [labels](labels.md), [years](years.md) |
| `--severity error\|warning\|info` | [doctor](doctor.md) |
| `--artist NAME` | [credit](credit.md), [albums](albums.md), [track](track.md) |
| `--album TITLE` | [track](track.md) |
| `--with NAME` | [artist](artist.md) |
| `--separator ";" / --separator tab` | [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md) |
| `--csv` | [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md) |
| `--tracks` | [export](export.md) |
| `--m3u` | [query](query.md), [collection](collection.md), [genre](genre.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--year YYYY` | [albums](albums.md) |
| `--output FILE / -o FILE` | [sources](sources.md), [rules](rules.md), [relations](relations.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md) |
| `--threads N` | [scan](scan.md), [analyze](analyze.md), [check](check.md), [copy](copy.md), [spectrum](spectrum.md) |
| `--force` | [analyze](analyze.md) |
| `--show-results` | [analyze](analyze.md) |
| `--json-layout album\|artist` | [analyze](analyze.md) |
| `--genre NAME` | [albums](albums.md) |
| `--label NAME` | [albums](albums.md) |
| `--json / -j` | [analyze](analyze.md), [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [relations](relations.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [years](years.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md), [export](export.md), [scan](scan.md) |
| `--no-color` | Shared (see above) |
| `--yes` | [reset](reset.md), [backup](backup.md), [restore](restore.md), [fetch](fetch.md), [history](history.md) |
| `--forget` | [import](import.md), [sources](sources.md), [missing](missing.md), [merge](merge.md) |
| `--pending` | [import](import.md) |
| `--list` | [import](import.md), [sources](sources.md), [missing](missing.md), [merge](merge.md), [fingerprint](fingerprint.md) |
| `--members` | [artist](artist.md) |
| `--no-scan` | [roots](roots.md) |
| `--lyrics` | [fetch](fetch.md), [track](track.md), [search](search.md), [play](play.md) |
| `--simple` | [playlist](playlist.md) |
| `--artists` | [playlist](playlist.md) |
| `--extras none\|cover\|images\|all` | [copy](copy.md) |
| `--dry-run` | [copy](copy.md), [spectrum](spectrum.md), [playlist](playlist.md), [fetch](fetch.md), [extract](extract.md), [fingerprint](fingerprint.md), [scan](scan.md), [notes](notes.md) |
| `--verify-existing` | [copy](copy.md) |
| `--playlists` | [copy](copy.md) |
| `--waiting` | [notes](notes.md) |
| `--relink REFERENCE` | [notes](notes.md) |
| `--to REFERENCE` | [notes](notes.md) |
| `--undo-relink ID` | [notes](notes.md) |
| `--relinks` | [notes](notes.md) |
| `--verify` | [copy](copy.md) |
| `--safe-names` | [copy](copy.md) |
| `--raw-names` | [copy](copy.md) |
| `--collection NAME` | [copy](copy.md) |
| `--compress FORMAT` | [copy](copy.md) |
| `--quality SETTING` | [copy](copy.md) |
| `--source NAME` | [import](import.md), [sources](sources.md), [review](review.md), [relations](relations.md), [missing](missing.md), [merge](merge.md) |
| `--compilations` | [albums](albums.md) |
| `--no-compilations` | [albums](albums.md) |
| `--role ROLE` | [credit](credit.md), [artists](artists.md), [artist](artist.md) |
| `--comment TEXT` | [albums](albums.md), [track](track.md) |
| `--comments` | [search](search.md) |
| `--notes` | [search](search.md) |
| `--offset N` | [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [import](import.md), [review](review.md), [relations](relations.md), [missing](missing.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [history](history.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--all` | [stats](stats.md), [doctor](doctor.md), [credits](credits.md), [import](import.md), [review](review.md), [relations](relations.md), [missing](missing.md), [query](query.md), [collection](collection.md), [favourites](favourites.md), [notes](notes.md), [history](history.md), [artists](artists.md), [countries](countries.md), [albums](albums.md), [genres](genres.md), [genre](genre.md), [labels](labels.md), [label](label.md), [artist](artist.md), [album](album.md), [track](track.md), [search](search.md) |
| `--help` | Shared (see above) |
| `--version` | Shared (see above) |
| `--full` | [scan](scan.md), [check](check.md), [spectrum](spectrum.md), [fetch](fetch.md), [fingerprint](fingerprint.md) |
| `--follow-symlinks` | [scan](scan.md) |
| `--include-hidden` | [scan](scan.md) |
| `--exclude FOLDER_OR_ID` | [roots](roots.md), [credit](credit.md) |
| `--stars N` | [rate](rate.md) |
| `--text TEXT` | [relation](relation.md), [note](note.md) |
| `--from KIND:NAME` | [note](note.md) |
| `--tag LABEL[,LABEL]` | [relations](relations.md), [relation](relation.md), [notes](notes.md) |
| `--file FILE_OR_-` | [note](note.md) |
| `--append` | [note](note.md) |
| `--query EXPRESSION` | [copy](copy.md), [collection](collection.md), [artists](artists.md), [albums](albums.md) |
| `--export` | [sources](sources.md), [rules](rules.md), [notes](notes.md) |
| `--import FILE` | [sources](sources.md), [rules](rules.md), [notes](notes.md) |
| `--template` | [sources](sources.md) |
| `--summaries` | [fetch](fetch.md) |
| `--discography` | [fetch](fetch.md) |
| `--covers` | [fetch](fetch.md) |
| `--size VALUE` | [spectrum](spectrum.md), [fetch](fetch.md) |
| `--images` | [fetch](fetch.md), [extract](extract.md) |
| `--country NAME` | [artists](artists.md) |
| `--identify` | [fetch](fetch.md) |
| `--credits` | [fetch](fetch.md) |
| `--add` | [credit](credit.md) |
| `--artist-id MBID` | [credit](credit.md) |
| `--instrument NAME` | [credit](credit.md) |
| `--recordings` | [fetch](fetch.md) |
| `--lang CODE` | [fetch](fetch.md) |
| `--portraits` | [fetch](fetch.md) |
| `--logos` | [fetch](fetch.md) |
| `--fanart` | [fetch](fetch.md) |
| `--no-logo` | [fetch](fetch.md) |
| `--no-label-logo` | [fetch](fetch.md) |
| `--no-portrait` | [fetch](fetch.md) |
| `--no-background` | [fetch](fetch.md) |
| `--no-banner` | [fetch](fetch.md) |
| `--no-album-cover` | [fetch](fetch.md) |
| `--no-cdart` | [fetch](fetch.md) |
| `--banners` | [fetch](fetch.md) |
| `--labels` | [fetch](fetch.md) |
| `--online` | [label](label.md) |
| `--offline` | [label](label.md) |
| `--accept ID` | [review](review.md) |
| `--reject ID` | [review](review.md) |
| `--undo ID` | [credit](credit.md), [review](review.md) |
| `--interactive` | [review](review.md) |
| `--graph` | [export](export.md) |
| `--normalize off\|track\|album` | [play](play.md) |
| `--bass DB` | [play](play.md) |
| `--treble DB` | [play](play.md) |
