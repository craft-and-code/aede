# missing — List credited studio albums that the local shelf does not hold

missing compares trusted MusicBrainz discographies already fetched with the local shelf. First run fetch --discography. Without that evidence, the command explains that no comparison is available rather than pretending the collection is complete.

An exact release-group ID already present anywhere in the catalog establishes that an album is held, including an edition filed under another primary artist. Title matching is only a fallback for that artist's local albums without a release-group ID; equal titles cannot override two different explicit IDs. The artist still needs a shelf of their own before their discography is compared.

Normally it lists absent studio albums and leaves singles, live records, compilations/demos and your set-aside releases out. --all lifts those editorial/personal exclusions and pagination, with a reason column. This is a metadata comparison, not an ownership/purchase or lending system; edition identification and incomplete external discographies can affect the result.

--forget TITLE sets one missing release aside without erasing what MusicBrainz said. --list shows those personal decisions. --forget --remove TITLE puts it back. Multiple equally matching releases are refused so no wrong album is hidden. This comparison uses MusicBrainz discographies; --source is refused rather than silently ignored.

`--remove` requires `--forget`; `--list` and `--forget` cannot be combined. `--limit`, `--offset` and `--all` apply to the normal missing-album report and are refused when listing or changing set-aside decisions.

The CLI lists, removes and applies only the `local` owner's set-aside decisions, including the missing-album hint on artist pages. Another account's choices remain private and do not hide albums from this report. A complete operator backup still preserves every owner's personal data.

## Syntax and arguments

```text
aede missing [name…]
```

Optional artist/album name terms; the set-aside action needs one unambiguous missing release.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--forget` | Remove stored data/decisions in this command’s scope. It does not delete audio or retag files. |
| `--list` | List the individual stored records or decisions instead of the normal summary/operation. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Include non-studio and personally set-aside missing releases as well as all rows; print why each was normally hidden. |
| `--remove` | Undo or remove the personal value named by this command; the scope is described below. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede fetch --discography "Miles Davis"
aede missing "Miles Davis"
aede missing --forget "A missing album"
aede missing --list
aede missing --forget --remove "A missing album"
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[fetch](fetch.md), [artist](artist.md), [rules](rules.md).

Detailed existing guide: [sources.md](../sources.md).
