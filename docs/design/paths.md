# Paths, and why Windows is not supported yet

A catalog path is a `String`. That one decision, made early and for good reasons, is what keeps Windows off the release page — and the shape of the problem is worth writing down before anybody tries to fix it in an afternoon.

## What the code says, and what it does

`AudioFile::path` has always been documented as:

> Absolute path, with `/` separators, as the scanner walked it.

The second half is true. The first half is a wish. The scanner stores whatever the platform hands it:

```rust
let path_str = path.to_string_lossy().to_string();   // scan.rs
```

On Unix those two statements agree and nothing shows. On Windows they do not, and three helpers that split on `/` and nothing else start answering nonsense:

```
file_name("C:\Users\kcell\Music\Miles Davis\Kind of Blue\01.flac")
  -> "C:\Users\kcell\Music\Miles Davis\Kind of Blue\01.flac"   (the whole path)
folder(...)   -> ""
is_under(..., "C:\Users\kcell\Music") -> false
```

`text::folder` returning the empty string is the expensive one: it feeds `Release.folder` in the graph builder, which is a third of the release key, and it feeds the `EntityRef` an annotation is stored under. Everything folder-shaped follows it down — `--folder` restrictions, disc-folder folding, import grouping by folder, playlists written beside the music, spectrograms, the "is this destination inside your own library" refusal in `copy`.

The Windows leg of CI reports **12 end-to-end failures**, and they are exactly that list. Album grouping survives, which is worth understanding rather than being relieved about: with `folder` empty for every file, all files share one folder, and the release key falls back to matching on the title alone. It looks like it works. On a library with two different albums of the same name it would silently merge them.

## Why it is not a normalisation

The obvious fix — store `/`, convert back at the filesystem — collides with something this code does deliberately.

`canonical()` exists because on macOS `/var` and `/private/var` name one place by two strings that never compare equal, and a `copy` refusal that missed that case is what put it there. It calls `std::fs::canonicalize`. On Windows, `canonicalize` returns a **verbatim** path:

```
\\?\C:\Users\kcell\Music\...
```

The watched roots are stored canonicalized, and every walked file descends from a root, so on Windows essentially the whole catalog is `\\?\`-prefixed today. A verbatim path is passed to Win32 without normalisation, which means it **does not accept `/` as a separator at all**. Rewriting those strings to `\\?\C:/Users/...` would produce paths that:

- Win32 rejects, so every filesystem call and every ffmpeg argument built from a catalog path fails;
- Rust's own `std::path` stops splitting: `parent`, `file_name`, `file_stem`, `join` and `starts_with` treat a verbatim path's tail as one component — so the failures are wrong answers, not errors.

UNC paths survive (`//server/share/...` parses and Win32 accepts it). Only the verbatim form is fatal, and it is the form this code manufactures.

## What the fix actually is

**In: nearly one place.** Seven sites turn a `Path` into a stored `String` — five in `scan.rs`, one in `cli/scan.rs` for the excluded folders, and one that is not ours at all (`analysis.rs`, where the string was written by another program into a report). A single `fn store_path(&Path) -> String` covers them.

**Out: not one place, and it cannot be made one without a type.** Thirteen sites turn a catalog string back into something the filesystem sees, across eight files: `check`, `spectrum` (twice, both as ffmpeg arguments), `playlist` (twice), `scan`, `copy` (`fs::copy`, `rename`, `File::open`, `tags::read`, and ffmpeg for `--compress`), `lyrics`. Two of them do not look like conversions at all — one hands a `&str` straight to `read_dir`, and the ffmpeg calls take the path as an argument without ever building a `PathBuf`.

**And every `canonical()` result destined for a comparison is an entry point too**, not only the ones that get stored: `scope_of`, the `copy` destination guard, `roots --remove`. Normalising the stored side without those turns the "you are copying your library into itself" refusal into a silent pass.

**Then there is the migration.** Catalog paths are persisted verbatim in `catalog.json`, and they are embedded in the `EntityRef` tokens of `user.json` — the keys under which every favourite, rating, note, tag and play count is filed. Change the spelling and:

- the incremental scan misses on every file and re-reads the library (slow, but correct);
- carried-over `excluded` folders and imported analyses keep the old spelling and detach, and the name-and-size rescue cannot fire, because `text::file_name` on a `\`-spelled key returns the whole path;
- annotations fall to `waiting`. They are not destroyed — nothing deletes them — but they are invisible until re-keyed, and release-level annotations have no fallback at all.

So load-time normalisation in `store.rs` and `user.rs`, and `FORMAT_VERSION` from 1 to 2 so an old catalog is rebuilt rather than silently mixed.

The honest shape is a `CatalogPath` newtype with `from_scan(&Path)` and a verbatim-aware `to_native() -> PathBuf`. About twenty call sites, all findable — once the field stops being a `String`, the compiler lists them. Several commits, not one.

One more thing that would otherwise be discovered late: the current tests are written in POSIX paths throughout — `text.rs`, `playlist.rs`, `spectrum.rs`, `copy/mod.rs`, `user.rs`, `analysis.rs`. They would all keep passing and prove nothing. The work needs tests whose paths come from the platform.

## Which tests fail there, and why so few

On Windows, `Path` and `PathBuf` compare by **components**, and `components()` splits on `\` and `/` alike. So `Path::new("a/b") == Path::new("a\\b")` is _true_, and an assertion that stays in path-land passes even when the value renders with backslashes. A test only fails once the value has crossed into `String` — `to_string_lossy`, `display().to_string()`, `format!` — or once it has gone through one of the `/`-only helpers and the program itself has misbehaved.

That is why exactly one of the eleven tests in `copy::tests` broke: it is the only one that stringifies the plan before comparing.

```rust
.map(|i| i.relative.to_string_lossy().to_string())   // renders `\` on Windows
…
assert_eq!(places, vec!["Danzig/1994 Danzig 4/02.flac".to_string(), …]);
```

The two mechanisms, then, are:

1. **Rendering** — the value is produced through `std::path` and compared against a `/` literal as text.
2. **Behaviour** — `text::is_under`, `text::folder` or `text::file_name` answers wrongly for a native path, so the program does the wrong thing and an ordinary assertion fails. This is what the twelve end-to-end tests are really about.

Fourteen tests carry the `cfg_attr` in total. One of them, `only_what_is_lossless_is_encoded_on_the_way_out`, is currently masked by the ffmpeg gate — CI installs no ffmpeg, so it skips everywhere — and it is marked anyway: the day ffmpeg reaches a runner it would fail on its _first_ assertion, which reads as "conversion is broken" rather than "paths are broken", and a misleading failure is worse than a loud one.

The dozens of remaining path tests pass on Windows for a reason worth naming: they feed `/`-spelled literals into `/`-only helpers, so the helper's assumption holds by construction. **They are not evidence that the code works there. They are the tests that would have caught this had their paths come from the platform**, and they are the first thing to change when `CatalogPath` is written.

## What was decided instead

Windows is **not** in the release matrix. Publishing a binary that builds the catalog wrongly, on the platform where nobody would think to check, is worse than publishing nothing for it.

The Windows leg of CI stays, and the twelve tests carry:

```rust
#[cfg_attr(windows, ignore = "catalog paths are `/`-separated; see docs/design/paths.md")]
```

so that leg is green, still guards the parsers and everything else against regression on Windows, and prints twelve ignored tests every run. **The ignore list is the debt**: enumerable, attached to the reason, and it disappears line by line when the work is done. A permanently red pipeline is a pipeline nobody reads.

## The rule this leaves behind

A path is compared as a path, never as the text that renders it, and a path that must survive being written down needs one spelling chosen at the boundary — not a hope that every platform spells it the same way. The comment on `AudioFile::path` stated the invariant correctly for years while nothing enforced it. An invariant that only the documentation believes is not an invariant.
