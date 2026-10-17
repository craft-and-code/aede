# credit — Add a manual credit or exclude/restore one sourced credit without touching tags

credit stores an explicit attributed manual credit or excludes/restores one exact sourced credit. First inspect relations to copy the stable edge ID, and inspect recording/work/album for a precise target identity.

Choose exactly one action. --add takes one positional scope recording:ID, work:ID or release:ID and requires --artist and --role. A work must be linked to a local recording; an edition must exist locally. --artist-id and --instrument qualify the statement. The same exact manual credit cannot be added twice. Recording/work duplicate checks follow the currently attached musical scope across file placements, so adding an earlier representative does not allow another copy. Excluded credits remain present and are restored with `--undo`; editions retain their exact local scope.

Adding a recording MBID later does not allow a second copy of a CLI credit originally targeted through a still-present local file. Its original relationship ID and exclusion remain intact. This check follows only current placement aliases; it does not infer older MBIDs or collapse arbitrary imported manual relationship IDs.

A `work:ID` credit concerns the composition and reaches all local recordings linked to that exact work MBID by tags or trusted evidence. It remains one attributed claim, with its original host and reversible exclusion; equal titles under different IDs do not extend its scope. A `recording:ID` or `release:ID` credit stays limited to that performance or edition.

If a retained manual edition snapshot already names an edition MBID, changing or removing that tag does not move its earlier credits. Adding another credit to that same local album is refused while its current edition differs, rather than silently reassigning the retained claims. Manual edition snapshots created without an edition identifier retain their existing local attachment behavior.

New direct recording credits retain the recording MBID when it is known. Changing or removing that identifier does not transfer those credits to the file's new performance: the original evidence remains stored, and another direct credit is refused while the captured identity differs. Composition credits on the same observation stay independent. Older or local-only snapshots without a captured recording ID keep their path attachment; reading does not infer or migrate an earlier identity.

--exclude ID removes the chosen sourced assertion from navigation/search while retaining its evidence; tag-read credits cannot be excluded. --undo ID restores an existing exclusion. Direct exclusions/restorations refuse new-credit fields and positional scope. Correct one source credit by excluding it, then adding your attributed correction. These decisions travel with rules; audio tags never change.

## Syntax and arguments

```text
aede credit --add recording:<ID>|work:<ID>|release:<ID> --artist=<name> --role=<role> [--artist-id=<MBID>] [--instrument=<name>] | aede credit --exclude=<credit ID> | --undo=<credit ID>
```

For --add: exactly one scope. Replace RECORDING_ID/CREDIT_ID with IDs displayed by Aède; no angle brackets in real input.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--undo ID` | Undo the exact review decision (review) or restore an excluded sourced credit (credit). |
| `--add` | Add a manual credit. Follow it with exactly one positional scope, such as recording:<ID>; artist and role are required. |
| `--artist-id MBID` | Optional MusicBrainz artist ID for a precise manual credit; obtain the ID from artist navigation or MusicBrainz. |
| `--instrument NAME` | Optional instrument/attribute attached to a manual credit. |
| `--role ROLE` | Keep the people or contributions with this credit role; on credit, specify the new role. |
| `--artist NAME` | Filter by artist name on albums/track; on credit, name the credited person. Multiword names are accepted. |
| `--exclude FOLDER_OR_ID` | Exclude the specified folder (roots) or sourced credit ID (credit), without editing audio. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede relations --source musicbrainz
aede credit --add recording:RECORDING_ID --artist "Jane Doe" --role producer
aede credit --exclude CREDIT_ID
aede credit --undo CREDIT_ID
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[relations](relations.md), [recording](recording.md), [work](work.md), [rules](rules.md).

Detailed existing guide: [commands.md](../commands.md).
