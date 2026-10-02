# roots — List, add, remove, or exclude watched folders

Without an argument, roots lists watched roots and excluded folders. A root is a folder Aède remembers for future scans; it is not the folder holding catalog.json. Existing music remains on disk when a root is removed.

Use roots --remove FOLDER to stop watching it, roots --exclude FOLDER to keep a subfolder out, and roots --exclude FOLDER --remove to undo that exclusion. Changes normally trigger a scan immediately. --no-scan records the change but leaves the currently visible catalog until a later scan. Removing the final root handles the now-empty catalog without deleting personal annotations.

The ordinary no-option listing does not add folders. To add a watched folder, use scan FOLDER. Trying to remove a folder that is not watched or undo an exclusion that does not exist returns an error.

## Syntax and arguments

```text
aede roots [folder…]
```

No argument for the list. With --remove alone, name the watched folder. --exclude takes its own folder value.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--remove` | Undo or remove the personal value named by this command; the scope is described below. |
| `--exclude FOLDER_OR_ID` | Exclude the specified folder (roots) or sourced credit ID (credit), without editing audio. |
| `--no-scan` | Update watched/excluded folders without rescanning immediately. Run scan later to apply the change to the catalog. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede roots
aede roots --exclude "$HOME/Music/Temporary"
aede roots --exclude "$HOME/Music/Temporary" --remove
aede roots --remove "/Volumes/Archive musicale" --no-scan
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md).

Detailed existing guide: [library.md](../library.md).
