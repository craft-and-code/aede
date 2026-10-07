# Your first library

Start with one small, well-tagged album folder before indexing a large archive. Replace every example title/path with one actually in your music. Examples use `aede` on `PATH`; use `./aede` or `.\aede.exe` from an extracted archive.

## 1. Build the catalog

```sh
aede scan "$HOME/Music"
aede stats
aede roots
```

`scan` reads recursively and remembers the folder. `stats` checks the result, and `roots` shows what future scans will read. `aede scan` later refreshes those roots. A missing/unplugged root refuses the refresh rather than silently emptying that part of the library. Add a second folder with another `scan FOLDER`; it does not replace the first.

## 2. Find an album and navigate

```sh
aede albums --limit 10
aede album "Kind of Blue"
aede track "So What" --artist "Miles Davis"
aede artist "Miles Davis"
aede search coltrane
```

Plural commands list entities; singular commands open one. Read the copyable `Open`/`Continue` commands in the output to follow related objects. If an edition is ambiguous, use its displayed MusicBrainz release ID. A file tag identifies local metadata; an external credit remains explicitly sourced.

## 3. Play locally

```sh
aede play "Kind of Blue"
```

In macOS/Linux terminals and the native Windows console, Space pauses/resumes, `n` advances, `p` goes back/restarts, and `q` stops without Enter. Redirected input disables interactive controls. Windows console/device acceptance remains pending. Playback may require FFmpeg/ffplay depending on the format/build. Playback defaults to without-effects, with normalization off and flat tone. Add `--playback=dsp` for automatic album/track normalization, or read the [playback reference](../cli/play.md) for the strict policy, device discovery, keys and current limits.

## 4. Add your own information

```sh
aede love album "Kind of Blue"
aede rate album "Kind of Blue" --stars 5
aede tag album "Kind of Blue" evening,jazz
aede note album "Kind of Blue" --text "My preferred edition"
aede album "Kind of Blue"
```

These values go into personal data, never audio tags. A note can instead come from a Markdown file with `--file notes.md`. Album ratings require `album.rating` in a track query; bare `rating` targets tracks. `loved` is intentionally inherited from a favorite album/artist.

## 5. Ask a question and save it

```sh
aede query "loved played:0"
aede collection Unheard --query "loved played:0"
aede collection Unheard
aede collection Unheard --m3u --output unheard.m3u8
```

A collection stores the question and reevaluates it later. M3U contains paths, not copied audio. Use [copy](../cli/copy.md) to create a separate player/card selection.

## 6. Enrich only when you choose

```sh
aede fetch --credits "$HOME/Music"
aede fetch --summaries --lang en "Miles Davis"
aede fetch --lyrics "$HOME/Music"
```

These commands contact the named sources. Fetched facts do not rewrite local tags; lyrics create missing `.lrc` sidecars. Start with a narrow album folder or `--dry-run`, and read [fetch](../cli/fetch.md) for keys, folder permissions and evidence trust.

## 7. Preserve your work

```sh
aede backup aede-backup.aede
aede check
aede doctor
```

The Aède bundle saves catalog/conclusions/personal/source data, not your original music. Back up audio and sidecars separately. `check` inspects supported container checksums; `doctor` diagnoses metadata/catalog issues without fixing them automatically. Continue with [daily workflows](workflows.md) or the complete [CLI options reference](../cli/options.md).
