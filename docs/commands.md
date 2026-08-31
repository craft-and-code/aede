# Commands, options and output

## Where each option applies

Three groups, and an option that a command cannot honour is **refused**, never ignored. So is an **argument**: `aede artists ozzy` used to list every artist and drop the word, which looks like an answer. It now says what to type instead.

`export` describes the **catalog**: `--csv` gives one row per album, `--tracks` one row per track. It takes no argument.

The **listings** — `albums`, `artists`, `genres`, `labels`, `years` — turn into a table of exactly what they show, filters included. This is how several albums land in one file:

```sh
aede albums --csv --artist="Deicide" --output=deicide.csv
aede albums --csv --year=1990
aede albums --csv --compilations --output=compilations.csv
aede artists --csv --limit=100 --output=artists.csv
```

`album`, `artist`, `track` and `search` describe a **selection**: `--csv` and `--m3u` both apply to it, as a table of tracks or as a playlist. For an artist, that means the tracks they are audible on; for a search, the track hits and not the artists or albums found.

```sh
aede album "To Hell With God" --csv --output=album.csv
aede artist "Deicide" --csv | sort -t, -k9 -n     # sorted by size
```

`aede album` takes **one** title — the words are joined so a title can be typed without quotes — and says which command lists several when given more.

The same holds for an option whose value is a **name**: `--artist`, `--album`, `--with`, `--genre` and `--label` take the words that follow, up to the next option. `--limit`, `--year`, `--output` and the rest take exactly one word, because a number or a path is one word.

```sh
aede artist Ozzy --with Zakk Wylde        # no quotes needed anywhere
aede artist Ozzy --with "Zakk Wylde"      # the same thing
aede track So What --artist Miles Davis --limit 1
```

Put the positional before the option: `aede track --artist Miles Davis So What` gives the whole tail to `--artist`, and the command then says it was given no title — rather than answering a question you did not ask.

`--output <file>`, or `-o`, writes wherever these produce text, and states where it went instead of filling the terminal.

```
$ aede stats

Library

  Tracks                        20
  Albums                         6
    of which compilations        1
  Artists                        8
  Total duration              38 s
  Size on disk              1.3 MB

Quality

                       Count      Size
  ───────────────────  ─────  ────────  ────────────────────
  Lossless (CD)           11  399.3 kB  ████████████████████
  Hi-res                   4  664.3 kB  ███████·············
  Lossy (>= 256 kbps)      3  248.3 kB  █████···············
```

## Getting the data out

Three formats, because they answer three different questions.

**JSON** (`aede export`) is the faithful dump: ten linked tables, one per table of the model. It is what rebuilds a catalog or feeds another program.

**CSV** (`aede export --csv`) is for a spreadsheet, and a spreadsheet cannot hold a graph. It writes **one row per album** — artist, title, year, track and disc counts, duration, size, formats, sample rates, bit depths, label, catalogue number, genres, integrity, folder — which is the view from above: sort by size to find what to re-rip, filter on `lossless` to see what is left to replace. `--tracks` switches to one row per track when the album is too coarse.

Its values are **raw**: `duration_ms` and `size_bytes`, not `4:20` and `31.2 MB`. A column that cannot be added up is a column that cannot be used.

`--separator=;` for Excel in a French or German locale, `--separator=tab` for a TSV. Fields are quoted per RFC 4180, so a title carrying a comma or a quotation mark does not shift every column that follows.

**M3U** (`--m3u`) is not an export of the catalog but of a **selection**: whatever is on screen becomes a playlist.

```sh
aede album "To Hell With God" --m3u --output=deicide.m3u8
aede search coltrane --m3u --output=coltrane.m3u8
mpv --playlist=deicide.m3u8
```

Paths are absolute, so the playlist works wherever it is saved; `#EXTINF` carries the duration and the title, so a player shows them without opening every file. Without `--output` it goes to standard output, which a shell supporting process substitution can hand straight to a player — `mpv --playlist=<(aede artist "Ozzy Osbourne" --m3u)`.

## Paging through a result

Every listing shows **50 rows** by default and says which ones they are:

```
  1–50 of 312 albums — --offset=50 for the next page, --all for every row
```

Three options, one window, the same everywhere:

```sh
aede albums                        # the first 50
aede albums --limit 50 --offset 50 # the next 50
aede albums --all                  # every row, however many
aede albums --all --csv -o all.csv # and into a file
```

`--offset` is what a front end needs: the order of every listing is deterministic, so page two is genuinely the rows after page one. `--all` says "everything" by name rather than by an encoding to remember — `--limit=0` is refused, since it would show nothing, and so is `--limit abc`, which used to fall back on the default and answer a question nobody asked.

Nothing is printed when everything fit, so the line keeps meaning something. A window past the end says so rather than showing an empty screen that reads as an empty library.

`-o` is short for `--output`.
