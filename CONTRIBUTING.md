# Contributing to Aède

The mechanics — fork, branch, pull request — are GitHub's, and you already know
them or the interface will tell you. This file is about the part no interface
tells you: **what this project accepts, what it refuses, and why**, so that
nobody spends an evening on a change that was never going to be merged.

Issues and questions are welcome before any code. A design disagreement is much
cheaper to have in a paragraph than in a diff.

## Before anything: `tools/check.sh`

```sh
tools/check.sh
```

Formatting, clippy with `-D warnings`, the test suite, the documentation build
(broken intra-doc links are errors), and a release build. It must be green
before a pull request, and there are no exceptions — not "it's only a comment",
not "the failure is unrelated". If it fails for a reason you did not cause, say
so in the pull request rather than working around it.

`cargo test` needs nothing installed. The conversion tests skip themselves,
loudly, when ffmpeg is absent.

## What will be refused, however good the code

These are settled decisions, not oversights. If you think one is wrong, open an
issue and argue it — that conversation is welcome. A pull request that simply
does the opposite is not.

**A new dependency.** The current list is one crate, `lofty`. Adding another is
a decision to argue for first, against three criteria at once: it does something
we could not do as well ourselves, it is maintained and widely used, and its own
dependency tree is small enough to read. `serde` is planned for M2 and no
earlier.

**Replacing the hand-written parsers.** `tags/` reads FLAC, MP3, MP4, Ogg, WAV
and AIFF from their specifications, and `lofty` is a fallback for the containers
that have no parser here. Those parsers extract things no general-purpose
library exposes — the LAME encoder delay and padding, the ALAC magic cookie, the
Opus pre-skip — and the playback engine needs them.

**Writing to the user's files.** Aède does not rewrite tags, rename files, or
reorganise folders. Rewriting a file moves its modification date, invalidates
the integrity verdict a whole subsystem exists to produce, and cannot be undone.
`copy` writes, but only new files, only outside the library, and never over a
source. Keep that line where it is.

**Reaching the network, or adding a database.** Neither exists yet, and each is
a milestone of its own with its own design. Do not introduce one "while passing
by".

**Flattening the graph.** The `credit` and `relation` tables are the model, not
an implementation detail to simplify into an album → artist hierarchy.

## What the code is expected to look like

Read `CLAUDE.md` first. It is the design-rationale document: every invariant in
it has a paragraph saying *why*, usually because getting it wrong once cost
something. It is longer than this file and it is the one that matters.

A few habits that are unusual enough to trip up a first contribution:

**The reasoning lives in the doc comments.** A function that made a
non-obvious choice explains it where the choice is, not in a commit message
nobody will find again. This is why the documentation build is part of
`check.sh`.

**Tests are named as sentences, and assert on behaviour.**
`a_copy_inside_the_library_is_refused`, not `test_copy_2`. The name should say
what would be broken if it failed.

**A test must be able to fail.** Before submitting one, break the thing it
covers and watch it go red. A test that passes against a deliberately broken
implementation is worse than no test, because it reports coverage that is not
there — this has happened here more than once, and each time the fix was to move
the assertion to where the difference is actually observable.

**Parsers never panic.** No `unwrap`, `expect`, `panic!`, direct indexing or
unchecked slicing anywhere in `tags/` or `audit/`. A truncated or corrupt file
must yield an error or a partial result.

**Construction is deterministic.** Two scans of the same library produce the
same identifiers. Sort before iterating; prefer `BTreeMap`/`BTreeSet` wherever
order can leak into output.

**An option that cannot be honoured is refused, not ignored.** Every option is
declared, guarded to the commands it means something on, and named in the help.
There is a test that walks the command table against the help text.

## Reporting a bug

The useful ones say what you ran, what happened, and what you expected — and,
where the answer depends on the library, enough about it to reproduce: the
container and codec, whether the path holds unusual characters, whether the
folder is on an external or network volume.

Two things are worth checking first, because they have each been the real cause
more than once: whether the path involves a symbolic link (on macOS `/tmp` and
`/var` are links, and paths under them exist in two spellings), and whether
`aede doctor` already reports something about the files in question.

Do not attach audio you do not have the right to share. A `aede file <path>`
output, or a file you produced yourself with `tools/demo-library.sh`, is almost
always enough.

## Licence

Contributions are accepted under the MIT licence of the project — see
[LICENSE](LICENSE). By opening a pull request you are saying the work is yours
to give.
