# Troubleshooting

Read the full error first: Aède refuses unsupported arguments/options instead of silently ignoring them. Most remedies preserve original files. Use `aede help COMMAND` for the actual executable’s version and options.

## The command is not found

Run `./aede` from the extracted folder on macOS/Linux or `.\aede.exe` in PowerShell. Otherwise put its directory on `PATH` and open a new terminal. `PATH` locates executables; `--data` locates the catalog. These are different settings.

## No catalog / empty library

Run `aede scan "/path/to/music"` with the same `--data` as the failing command. `--data` never scans music. Check `roots`, `stats` and read permissions. Folder names with spaces need quotes. Do not solve a wrong-data-folder problem by deleting the real user store.

## A watched drive is unplugged

Reconnect it before scan. Aède refuses rather than forgetting every absent track. If you deliberately stop using it, `roots --remove FOLDER` updates the watched list; read the immediate-rescan behavior and use `--no-scan` if appropriate. A reset deletes watched roots and requires naming them again.

## An option/name is rejected

Plural `albums`/`artists` list; singular `album TITLE`/`artist NAME` open. Put positional title before a name-valued option: `track "So What" --artist "Miles Davis"`. Use one export format, and add `--csv` for `--separator`/export `--tracks`. `--limit=0` is invalid; use `--all` when offered. Unknown entity terms in a query can be errors, distinct from a valid query with no matching tracks.

## Copy/download cannot write

Check free space and permissions at the destination. copy refuses paths overlapping your library. Sidecar downloads require a filesystem supporting hard links for atomic non-overwriting publication; FAT/exFAT does not satisfy that requirement. Do not bypass it by overwriting originals. `--dry-run` previews work but can still validate required tools.

## Metadata is missing after fetch

Use the right pass: lyrics require `--lyrics`, discographies `--discography`, credits `--credits`. Inspect `sources --list`, `review` and `credits` for pending/untrusted/empty/unidentified distinctions. Existing completed answers are reused; `--full` asks again intentionally. Service keys must be available in the process actually doing the work, including the server when delegated. Do not paste keys into an issue.

## Playback tool/device failure

Check FFmpeg/ffplay in the same terminal’s `PATH`. Linux release archives require ffplay. Format tag support does not imply native decoder support. Unknown multichannel layouts are refused; known layouts can downmix to stereo without LFE. Windows keyboard transport is not implemented. Device interruptions/underruns can remain audible despite correct file decoding; the project has not measured physical gapless behavior on all devices. Read the active DSP stage report before interpreting a lower level as a file defect.

## Ctrl-C did not stop scan/fetch

If the Unix server accepted a delegated task, closing the CLI does not cancel it. Use `cancel TASK_ID` with the same data directory; saved fetch answers remain. HTTP jobs use the administrative cancellation route. If the server restarted, old in-memory task IDs expire.

## Recover or report a defect

Keep the data directory and make a backup before recovery. Stop the server for a production restore and rehearse in a separate directory. For a bug report include executable version, operating system, redacted command/error and a minimal safe reproduction. Avoid publishing private music paths, notes, service secrets or full personal-store dumps. See [data safety](catalog.md) and [backup](../cli/backup.md).
