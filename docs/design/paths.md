# Catalog paths on Unix and Windows

## Native spelling is the storage contract

The scanner stores absolute paths in the platform's native spelling. Windows paths may use a drive (`C:\Music\…`), a network share (`\\nas\music\…`), or the verbatim forms returned by canonicalization (`\\?\C:\Music\…`, `\\?\UNC\nas\music\…`). Roots, exclusions and file paths retain those strings when saved and loaded.

Do not replace backslashes with slashes in stored absolute paths. In particular, a verbatim Windows path must keep its backslashes when passed to the filesystem or an external tool. The existing scanner and filesystem consumers already preserve that spelling; no new storage format or path-key migration is required.

## Catalog string operations

`text::file_name`, `text::folder` and `text::is_under` recognize native separators, including Windows drive, UNC and verbatim paths when tested on Unix. Verbatim paths accept only backslash separators. Ordinary Windows paths accept both separators. On Unix, backslashes inside a POSIX file name remain literal characters.

Folder membership checks require a component boundary: `C:\Music-backup` is not inside `C:\Music`. Drive roots retain their separator (`C:\`, not the drive-relative `C:`). These are lexical helpers, not filesystem identity checks: they do not resolve symlinks, fold case, remove `..`, or equate ordinary and verbatim prefixes. CLI scopes and watched roots are canonicalized before comparison, as is the copy destination guard.

`text::relative_under` produces `/`-separated **relative output only**, for exported playlists and copy plans. The source path stays native. Filesystem paths and copy destinations are compared as `Path` values in tests, not by their rendered separator choice.

This fixes folder filtering, album edition separation, disc-folder folding, sidecar lookup, relative playlists and copy-tree planning without rewriting track references, exclusions or imported analysis paths.

## Existing catalogs

An older Windows scan could give every release an empty folder because the helpers recognized only `/`. Run `aede scan` again with the corrected executable to rebuild album grouping; unchanged audio files can still use the incremental scan cache. Keep the existing data folder, especially `user.json` and `sources.json`.

Track references keep their exact keys. Previously malformed **release** references with an empty folder cannot safely identify an edition after the albums are split. Their annotations remain stored and waiting, rather than being guessed onto an album or deleted. No automatic reattachment of these ambiguous legacy references is provided.

## Verification and release status

Regression tests exercise drive, UNC and verbatim paths, edition/disc grouping, folder boundaries, playlist output, copy protection and personal-data round trips. Native filesystem and CLI tests previously ignored specifically for the separator bug are enabled again on Windows; the copy fixture uses a legal Windows filename while retaining the Unix test of `?` sanitization.

Tests of Windows-shaped strings on macOS do not establish native Windows compatibility. The Windows CI job must validate real scanning, sidecars and copying before a Windows release is enabled. The release matrix therefore remains macOS/Linux pending that validation. Local CLI delegation and `aede cancel` still use Unix sockets and remain unavailable on Windows; that transport limitation is separate from catalog paths.
