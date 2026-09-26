# What Aède worked out about the bytes — a store of its own

**Status: implemented in M2, with legacy-catalog and backup migration.** Raised while `aede backup` was being
built, from a fair question: if the catalog can be rebuilt by a scan, why does
a backup carry it at all?

## The catalog is not one kind of thing

It reads as one file and it holds four:

|                                                            | rebuilt by a scan?                                                                                    |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| files, tags, artists, releases, tracks, credits, relations | **yes**, entirely — read from the disk and derived from the tags                                      |
| `roots` and `excluded`                                     | **no.** A scan re-reads the folders it already watches; from nothing, somebody has to name them again |
| `integrity` and `fingerprint` on each file                 | **no.** Aède's own conclusions about the bytes: hours of reading and decoding                         |
| `analyses` — imported FlacCompagnon reports                | **no.** Somebody else's measurements, imported by hand                                                |

So "the catalog is derived from the disk" is true of most of it and false of
the part that costs the most. That sentence was written in the `backup` module
and in a question put to a reader, and it deserved the correction it got.

The **decision** it was used to justify is unaffected: all three stores go into
a backup, precisely _because_ the catalog holds things a scan does not bring
back. What follows is a different change, for a different reason.

## The reason, and it is not size

A backup is a JSON document holding the three stores whole. Measured, on a
synthetic library of twelve tracks an album:

| tracks  | catalog.json |
| ------- | ------------ |
| 10 000  | 12.4 MB      |
| 50 000  | 62.5 MB      |
| 200 000 | 252.0 MB     |

Twelve megabytes is not a problem worth an architecture. A chromaprint
fingerprint measures **974 bytes** for a four-minute track (base64, algorithm
1, measured with ffmpeg rather than guessed), so a fully fingerprinted 50 000
track library carries about 49 MB of fingerprint inside those 62.5 MB — most
of the file, and still not a reason on its own.

The reason is **durability**. `store::FORMAT_VERSION` guards the catalog, and
a document of another version is _refused rather than read approximately_ —
the right rule, and the same one `user.json`, `sources.json` and the backup
envelope follow. But the consequence today is that the day that number
changes, everybody's integrity verdicts and fingerprints go with it. Hours of
decoding, thrown away by a schema change to something else entirely, because
they happen to live in the same file as the rows that changed.

The backup already applies the fix one level up: three stores nested in one
envelope, **each refused on its own**, so a catalog this build cannot read
still restores the notes. The same argument applies one level down.

## The shape, and the precedent is already here

A fourth store, `conclusions.json`, keyed the way `analysis::FileAnalysis`
already is: on the **path**, with the size and the mtime beside it.

That is not a new idea in this codebase; it is the idea `analyses` was built
on, and the reasoning on `FileAnalysis::path` transfers word for word:

- identifiers are positions a scan renumbers, so an identifier would have to be
  remapped after every scan — a path does not move;
- a record may describe a file the catalog does not hold _yet_, and can wait
  for the day a scan brings it in;
- `still_applies(size, mtime)` is already the rule that expires one, and it is
  already the rule that carries an integrity verdict and a fingerprint across a
  rescan. The key is identical; only the address has to change.

The four voices then have four files, which is the shape the rest of the
program already argues for: **what the disk says** (`catalog.json`), **what
Aède worked out about the bytes** (`conclusions.json`), **what you say**
(`user.json`), **what other sources say** (`sources.json`).

Reading a legacy catalog attaches its embedded conclusions in memory without
writing a file. The next catalog save, under the shared data-directory writer
lock, persists them before replacing the catalog. This also preserves results
for files absent from the new scan. Full scans, backups and graph exports carry
legacy conclusions without depending on a read-side migration. An unreadable
independent conclusions store is an error, not permission to overwrite it.

## What it does not solve

`roots` and `excluded` stay in the catalog, and no scan invents them. A
catalog will therefore never be purely derived, and a backup will always have
reason to carry it. Anyone tempted to conclude "so the backup can skip the
catalog" should stop here: it cannot.

## Why M2 and not now

It touches the model, the store, the scan's carry-over, `doctor`, `check`,
`track` and `fingerprint` — a day's work with a migration to write, since the
point is not to destroy the verdicts while moving them. M1 has artist identity
and dated relations in front of it, and both are larger. Nothing degrades in
the meantime: the verdicts are safe as long as `FORMAT_VERSION` does not move,
and a backup keeps them either way.

**The trigger to do it sooner:** any change that would bump
`store::FORMAT_VERSION`. Move the conclusions out _first_, in their own
commit, and the bump then costs a rescan and nothing else.
