# note — Read or write a personal note

note KIND NAME reads the current note when no write/remove option is supplied. There is one note per entity. --text supplies short text, --file reads a UTF-8 file, and --file - reads piped standard input. Choose one text source; --from KIND:NAME copies another note.

New text replaces the note unless --append adds it separated by a blank line. --remove clears the note and refuses conflicting write actions. Markdown, blank lines and wording are preserved; terminal output shows the source text, while a graphical client can render it safely.

Export personal annotations with notes --export and back up user.json through backup. Searching --notes or query album.note:TEXT finds your prose. A personal note is distinct from the comment metadata embedded in audio.

Personal annotations are stored separately in user.json. They never retag or rename music. The kind can be artist, album, track, genre or label. Use the displayed catalog spelling and quote a multiword name; if the selection is ambiguous, refine it rather than guessing.

## Syntax and arguments

```text
aede note <kind> <name>
```

Entity kind then name; --from references another entity as KIND:NAME.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--remove` | Undo or remove the personal value named by this command; the scope is described below. |
| `--text TEXT` | Write the personal note text. Names with spaces are consumed until the next option. |
| `--file FILE_OR_-` | Read a UTF-8 note from FILE. A lone - reads standard input from a pipe. |
| `--append` | Append supplied text to the existing note, separated by a blank line. Without it, supplied text replaces the note. |
| `--from KIND:NAME` | Copy a note from another reference, such as album:"Kind of Blue". Refused with another source of note text. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede note album "Kind of Blue" --file kind-of-blue.md
aede note album "Kind of Blue"
aede note album "Kind of Blue" --text "A new observation" --append
aede note album "Kind of Blue" --remove
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[notes](notes.md), [search](search.md), [backup](backup.md).

Detailed existing guide: [annotating.md](../annotating.md).
