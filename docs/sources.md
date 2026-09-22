# What Other Sources Say

Three voices coexist in this program, and separating them is the core of the project:

- **what the files say** — the catalog, rebuilt from your folders with each scan;
- **what you say** — favorites, notes, ratings, and tags, in a file that a new scan never touches;
- **what a third party says** — MusicBrainz, Wikipedia, Fanart.tv, and the other explicit sources, in `sources.json`.

A value coming from a source **sits alongside your tags and never overwrites them**. Nothing is rewritten, nothing is merged, and every piece of data can be traced back to its author, dated, and removed. When a source and your tags disagree, both are displayed and `doctor` flags it — determining which one is right is beyond the scope of this program.

## Querying MusicBrainz

```sh
aede fetch                      # all artists and all albums
aede fetch manson               # only matching the name (artist or release)
aede fetch ~/Music/Alastis      # only what is on this shelf
aede fetch --dry-run            # show what would be requested without sending anything
aede fetch --full               # re-query what is already kept
```

A name and a folder correspond to two distinct questions, and `fetch` accepts both — see [Restricting an execution by folder](#restricting-an-execution-by-folder).

**Artists and albums, in a single pass**. Albums are the most important part: no tag indicates a musician's origin, so what MusicBrainz says about an artist can only be added alongside your library. It is different for an album — Picard records `RELEASETYPE`, `DATE`, and `LABEL`; so your files have an opinion, MusicBrainz has another, and the two can diverge. This disagreement is the reason for this storage model, and it applies to albums.

Date is the most common case. Your `DATE` tag says 1997 because that is the year of the reissue you ripped; MusicBrainz indicates that the album was first released in 1959. Neither is wrong, and Aède displays both rather than choosing one.

**One request per album, regardless of your tags**. If they contain the MusicBrainz album ID, the response arrives with certainty and brings the label in the same request. If they only contain the release group ID (release-group), the response remains certain, but without the label — a release group doesn't have one, and completing it with the label of the first matching pressing would attribute an edition's label to the album itself. If they contain neither, the album title and artist are searched, evaluated, and rejected below 70% instead of being guessed.

MusicBrainz allows **one request per second**; scanning a large library therefore takes time. The program displays the estimated duration before starting and asks for confirmation above 20 artists. It saves after each response, so an interrupted run loses nothing and a second run only costs the remaining requests.

Its search server sometimes returns a temporary `503` code, even when respecting the rate limit. The program waits — three attempts, with increasing delays — and stops only if the rejections persist.

A second run queries nothing it already possesses. `--full` **allows re-querying**, which is also required after an update that reads a field not previously stored:

```sh
aede fetch --full manson
```

Some artists return with no data stored, which confirms the system is working properly. A response that does not clearly concern your artist is ignored rather than guessed.

## Prose Language

The summary is fetched in a **single** language and kept as such; the choice belongs to `fetch` and not to the subsequent display:

```sh
aede fetch --summaries                       # terminal language, English fallback
aede fetch --summaries --lang=fr             # French, English fallback
aede fetch --summaries --full --lang=fr ozzy # replaces already stored data
```

Without `--lang`, the terminal's `LANG` or `LC_ALL` variable is read (`fr_FR.UTF-8` for French). **English always remains the last resort**, never excluded: for many artists, it is the only available article. The pass indicates what it is looking for before sending the request:

```
searching for articles in fr, en, in that order
```

An artist without a French article gets the English version, and the credit below the paragraph specifies the source read:

```
  https://en.wikipedia.org/wiki/Ozzy_Osbourne — in en — CC BY-SA 4.0
  aede fetch --summaries --full --lang=fr "ozzy osbourne" asks for another
```

This last line makes it possible to differentiate between an ignored preference, a non-existent article in your language, or data retrieved before setting a preference.

`--full` is necessary to replace text that is already stored.

## Identified or Matched

If your files went through **Picard**, they already contain MusicBrainz IDs, and `fetch` uses them: it queries the artist directly instead of searching for a name.

It is **exact** — the response concerns the requested ID, so the registry displays `identified` instead of a percentage. A percentage indicates search indexing quality, never that an artist is 88% correct.

And the **response is more complete**: a search result is abbreviated, whereas a direct lookup returns the full entity — current band activity or the disambiguation line used by MusicBrainz to distinguish homonymous artists.

Without an ID, the name is searched and the result comes with a score. Below 70%, nothing is saved, and two equivalent answers are rejected rather than arbitrated:

```
? Nirvana: several answers are equally good: Nirvana, Nirvana (UK)
? Sh: the closest was "Shellac" at 61%, not close enough
```

## Manual Corrections

Retrieved values are not the final word. Records are indexed by **(entity, source)**, which means anything you classify under your own source won't be overwritten by MusicBrainz: a subsequent `aede fetch --full` will add its line next to yours without touching it.

Generate a document with the right keys:

```sh
aede sources --template --source=manual --output=fix.json "Kind of Blue"
```

Fill in the desired fields (`null` if data is missing):

```json
{
  "entity": "release:miles davis|kind of blue|/Users/you/Music/Miles/Kind of Blue",
  "source": "manual",
  "facts": {
    "primary_type": "Album",
    "first_released": "1959-08-17",
    "label": "Columbia",
    "secondary_types": []
  }
}
```

Import it:

```sh
aede sources --import=fix.json
```

`--source=manual` protects this entry. Any name works (`manual`, yours, `discogs`…), the key being to avoid `musicbrainz`, as a source only replaces what **it** previously stated.

## Viewing and Deleting

```sh
aede sources                    # one line per source: how much, how much lands
aede sources --list             # every record, and whether the catalog places it
aede sources --export --output=backup.json
aede sources --forget --source=musicbrainz
```

`aede artist` and `aede album` display a "What sources say" block, matching each value against your tag:

```
  Source                        Field           Says        Your tags
  ────────────────────────────  ──────────────  ──────────  ────────────────────
  musicbrainz 88% · 2 days ago  release type    Album       nothing in your tags
  musicbrainz 88% · 2 days ago  first released  1959-08-17  matches your tags
  musicbrainz 88% · 2 days ago  label           Blue Note   Columbia
  musicbrainz: https://musicbrainz.org/release-group/c9fdb94c-…
```

**The last line indicates the source address of the response**, one per source. This identifier is essential for any manual verification — opening the page, running the request with `curl`, or fixing data at the source.

The ID is included in the URL and can be copied separately. An album address points to its **release group**, which defines the album as a whole rather than a specific pressing.

Three distinct states are differentiated: a value confirmed by your tags, a value in contradiction, and a value missing from your tags.

Two labels deserve clarification:

- `from` corresponds to the geographical area according to MusicBrainz (country, city, or region).
- `note` corresponds to disambiguation, written to distinguish homonymous artists.

## Secondary Passes and Combined Runs

Running `aede fetch` alone queries MusicBrainz for your artists and albums. Options enable complementary passes over the identified catalog:

```sh
aede fetch --summaries      # the Wikipedia article behind each wikidata link
aede fetch --discography    # everything MusicBrainz credits to each artist
aede fetch --lyrics         # missing words from LRCLIB, as .lrc sidecars
aede fetch --covers         # the front image of every album that has none
aede fetch --portraits      # Wikidata first, then Fanart.tv as fallback
aede fetch --labels         # identify record labels through MusicBrainz
aede fetch --credits        # recording/work credits and work links for recordings with an MBID
aede fetch --logos          # artist and identified-label logos from Fanart.tv
aede fetch --fanart         # every supported Fanart.tv image family
```

`--credits` never searches a recording by title: it follows only a
`MUSICBRAINZ_RECORDINGID` already attached to a local recording. One lookup
then keeps direct recording roles, the linked works and their creative roles,
including credited-as spellings, instruments and qualifiers, dates, ordering
and relationship identifiers. Existing data fetched with the former
`--recordings` option is refreshed once into this richer form; `--recordings`
continues to work as an alias.

**Each accepts names and folders**:

```sh
aede fetch --discography "pink floyd"
aede fetch --covers manson portishead      # a list is fine
aede fetch --summaries mika                # nothing here matches mika
aede fetch --lyrics ~/Music/Alastis        # that shelf, whatever it is called
```

A name targets an artist by name, and an album by title **or** artist.

## Restricting an Execution by Folder

All `fetch` options accept folders, following the same logic as `check`, `playlist`, and `fingerprint`:

```sh
aede fetch ~/Music/Alastis                 # the artists and albums on that shelf
aede fetch --lyrics ~/Desktop/test/Alastis # the words, for those tracks only
aede fetch --covers --lyrics ~/Music/80s   # several passes, one shelf
aede fetch ozzy ~/Music/80s                # that person, on that shelf
```

**Any existing path on disk is treated as a folder; everything else is treated as a name**. The run displays the folders taken into account before querying:

```
  only what is under /Users/you/Music/Alastis

Lyrics
```

Both criteria apply independently: name defines **who**, folder defines **where**.

A folder not scanned by the catalog is **rejected**:

```
no file in the catalog is under "~/Music/New".
It is on disk, so this catalog was scanned 3 days ago and has not seen it — a folder added since is not in it yet.
Add it: aede scan "~/Music/New"
```

Options can be combined and execute sequentially:

```sh
aede fetch --covers --discography          # both, in one go
aede fetch --summaries --discography --covers --dry-run
```

Execution order follows a centrifugal logic starting from the artist: identity, discography, visuals.

## Querying Wikipedia

MusicBrainz does not store full biographies, but keeps a link to them. Fetching them is an **option of** `fetch`:

```sh
aede fetch --summaries          # follow it, for every artist already fetched
aede fetch --summaries --full   # ask again about what is already held
```

```
→ 402 stored, 3 left alone, 0 failed
  381 of them have a wikidata link — aede fetch --summaries reads the article
```

This is a **second pass on** `fetch` **data**, not a new search. The program reads the `wikidata` link, queries Wikidata to get the corresponding article, then retrieves the first paragraph from Wikipedia (two queries per artist).

Articles are searched **primarily in the system language** (`LANG` variable), then in English.

## Credit is Part of the Text

Wikipedia articles are licensed under **CC BY-SA**. Using the text requires citing the source and license. Aède stores the paragraph, page, language, and license as **an inseparable value**:

```
  Marilyn Manson is an American rock band formed in Fort Lauderdale,
  Florida, in 1989.
  https://en.wikipedia.org/wiki/Marilyn_Manson_(band) — CC BY-SA 4.0
```

`aede sources --forget --source=wikipedia` deletes the whole set.

## Images Already Present in Your Files

Before downloading anything, a local solution exists. An album without a `cover.jpg` file in its folder can extract its visual **from inside the audio files**:

```sh
aede extract                    # write it out, everywhere it is missing
aede extract ~/Music/Manson     # only under these folders
aede extract --images           # the back and the booklet too, in artwork/
aede extract --dry-run          # say which folders, write nothing
```

**The program reads your files without ever modifying them**. No writing occurs inside audio files. The image is created alongside (folder, playlist, spectrogram). Works across **all formats** (FLAC, ID3, MP4, Ogg).

Extraction runs **per folder**. A folder that already contains an image is skipped.

## Other Embedded Visuals

In addition to the front cover, `--images` extracts back cover, booklet, or disc into an `artwork/` subfolder so as not to interfere with front cover detection by media players:

```
/music/Miles Davis/Kind of Blue/
    cover.jpg
    artwork/
        back.jpg
        booklet-01.jpg
        booklet-02.jpg
        media.jpg
```

## Album Cover Art

Retrieving visuals from Cover Art Archive (MusicBrainz).

```sh
aede fetch --covers                 # albums with no cover, at 1200 px
aede fetch --covers --size=500      # 250, 500, 1200 or original
aede fetch --covers --images        # the back and the booklet too, in artwork/
aede fetch --covers --dry-run       # say which albums, ask nothing
```

**Run** `aede extract` **first**. `--covers` only downloads if no image is present (neither embedded nor in the folder). There is no `--replace` flag to prevent accidental overwrites.

Downloaded images are saved as `cover.jpg` alongside audio tracks. Audio files are never altered. Only valid files (JPEG/PNG) are saved to avoid corruption from HTML error pages.

## Fanart.tv Artwork

Fanart.tv complements the Cover Art Archive with artist artwork, label marks, and additional album visuals. It requires a free application key in `AEDE_FANARTTV_KEY`:

```sh
export AEDE_FANARTTV_KEY="your-key"
aede fetch --fanart
```

`--fanart` enables every supported family in one pass:

| Family          | Selection rule                                        | Destination                                                                                  |
| --------------- | ----------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Artist logo     | HD first, standard resolution as fallback             | `logo.jpg` or `logo.png` beside the artist's music, or in `assets/artists/<MusicBrainz ID>/` |
| Label logo      | Best-liked available logo                             | `assets/labels/<MusicBrainz ID>/`                                                            |
| Artist portrait | Best-liked portrait                                   | `artist.jpg` or `artist.png` beside the artist's music, or in `assets/`                      |
| Background      | **4K first**, 1080p only when no 4K background exists | `background.jpg` or `background.png` beside the artist's music, or in `assets/`              |
| Banner          | Best-liked wide banner                                | `banner.jpg` or `banner.png` beside the artist's music, or in `assets/`                      |
| Album cover     | Best-liked Fanart.tv album cover                      | `artwork/cover.jpg` or `artwork/cover.png` inside the album folder                           |
| cdART           | One image per disc                                    | `artwork/media.jpg`, or numbered `media-01.jpg`, `media-02.jpg`, and so on                   |

Start from everything and remove only what you do not want:

```sh
aede fetch --fanart --no-logo
aede fetch --fanart --no-label-logo --no-banner
aede fetch --fanart --no-portrait --no-background
aede fetch --fanart --no-album-cover --no-cdart
```

The complete exclusion list is `--no-logo`, `--no-label-logo`, `--no-portrait`, `--no-background`, `--no-banner`, `--no-album-cover`, and `--no-cdart`. An exclusion without `--fanart` is refused instead of silently ignored. Contradictory requests such as `--fanart --logos --no-logo` or `--fanart --banners --no-banner` are refused as well.

The narrower forms remain available for compatibility and focused runs:

```sh
aede fetch --logos              # artist and identified-label logos only
aede fetch --logos --banners    # the same artist response also yields a banner
```

Each family has its own completion record. If backgrounds are excluded today, a later `aede fetch --fanart --no-portrait` can still fetch them without repeating image families already completed. `--full` deliberately asks again. Existing JPEG and PNG files are never overwritten, and downloaded bytes are checked as images before anything is written.

## Missing from the Shelf

```sh
aede fetch --discography    # browse everything credited to each artist
aede missing                # what is credited to them and not here
aede missing davis          # narrowed, by artist or by title
```

`missing` performs **no network request**. It calculates the result from stored discography data.

```
Missing

  Artist       Album              Year
  ───────────  ─────────────────  ────
  Miles Davis  Sketches of Spain  1960
  Miles Davis  Bitches Brew       1970
  2 studio albums
  9 records left out — --all lists them here, with the reason for each: singles,
  live records, compilations, demos, and anything you set aside
```

## Exclude or Re-include an Album

```sh
aede missing --forget "Sweet Dreams"            # stop listing it
aede missing --list                             # what you set aside
aede missing --list manson                      # narrowed, by artist or by title
aede missing --forget "Sweet Dreams" --remove   # put it back
```

Choices are saved in `user.json` and indexed on the MusicBrainz release group ID.

The default filter only keeps **studio albums** from artists having at least one complete album in the local library.

## Identifying a File by Acoustic Fingerprint

Acoustic fingerprint computed from decoded audio signal to identify mislabelled files or missing metadata.

```sh
aede fingerprint                # decode and work out what the audio is
aede fingerprint ~/Music/Rips   # only under these folders
aede fingerprint --full         # every file, not just the nameless ones
aede fingerprint --list         # print what is stored, to compare it
aede fetch --identify           # ask AcoustID what it hears
```

By default, properly tagged files or files already holding a MusicBrainz ID are skipped.

## Comparison and Duplicate Detection

`aede doctor` groups files sharing the same acoustic fingerprint to identify strictly identical recordings, regardless of their tags:

```
  ! the same audio — 2 files are the same recording (379.1 kB recoverable)
      /music/Copie/track07.flac
      /music/Original/01.flac
```

Prerequisites: Chromaprint (`ffmpeg` or `fpcalc`) and an AcoustID API key configured in `AEDE_ACOUSTID_KEY`. The system identifies and suggests matches, but never alters audio files.

## What MusicBrainz Does Not Contain

- **No full biography** (only type, area, dates, and disambiguation note are provided). Wikipedia/Wikidata fills this gap.
- **No overwriting of your files**: retrieved data remains independent of your original tags, and deleting `sources.json` does not affect your local files.
