# Aède — Engineering Rules

This document contains stable engineering rules and lessons learned from the implementation of Aède.

It is deliberately separate from `CLAUDE.md`: Claude Code should consult this document when a task touches one of these areas.

---

## 1. Deterministic construction

Scanning the same library twice must produce the same identifiers.

Therefore:

- sort collections before iteration when order affects output or identifiers;
- never allow `HashMap` iteration order to determine persisted identifiers;
- prefer `BTreeMap` / `BTreeSet` where deterministic ordering matters.

---

## 2. Graph model

Aède is a graph.

Credits and relations are first-class concepts.

Do not replace the graph with an album → artist hierarchy for convenience.

The graph must continue to support questions such as:

- which recordings feature an artist?
- which artists collaborated on a recording?
- what roles does an artist have?
- where does an artist appear outside their own releases?

---

## 3. Local data versus external claims

Aède never treats an external source as a replacement for the user's local metadata.

When external information disagrees with local information:

- retain both;
- identify the source;
- make the disagreement visible;
- do not silently choose the external value.

External information is a claim, not an instruction to rewrite the library.

---

## 4. Provenance

Any data obtained from an external source must retain enough provenance to answer:

- who supplied it?
- what entity does it describe?
- when was it obtained, when relevant?
- what additional attribution/licensing information is required?

For licensed prose, the text and attribution must travel together.

Never expose a path that can return externally supplied prose without its required attribution.

---

## 5. Derived data

Derived information must remain distinguishable from information directly read from files or supplied by an external source.

Examples:

- inferred relations;
- fingerprints;
- duplicate classifications;
- missing discography entries.

If derived information becomes stale, it should normally be recomputed rather than treated as authoritative source data.

---

## 6. Versioning

### Storage format

An incompatible persisted-data change requires a `FORMAT_VERSION` change.

Adding a new optional field is normally backward-compatible and should not require a format bump.

The reader must handle older data deliberately rather than failing accidentally.

### Derived rules

Rules used to derive persisted/in-memory information may require their own version.

Changing an inference rule does not necessarily invalidate the underlying source data.

---

## 7. Path handling

A folder is not a string prefix.

Never determine whether one path belongs to another with a naive string `starts_with`.

Use the project's path-aware helper.

Path handling must remain correct across:

- macOS;
- Linux;
- Windows path representations;
- symbolic links where applicable;
- temporary directories.

Persisted paths require particular care because changing their spelling can break user annotations keyed by path.

---

## 8. Parsers

Binary parsers must be defensive.

Always consider:

1. truncated input;
2. invalid or lying length fields;
3. forged or invalid signatures;
4. unexpected end of file;
5. arithmetic overflow.

Parser code must not panic on malformed input.

In `tags/` and `audit/`:

- no unchecked indexing;
- no unchecked slicing;
- no `unwrap`;
- no `expect`;
- use the project's safe byte-reading helpers;
- use checked or saturating arithmetic where input values are untrusted.

---

## 9. Tag parsing

The project intentionally retains specialised handwritten parsers where they expose information needed by Aède.

`lofty` is a fallback, not a replacement for the project's specialised parsing paths.

Do not change this architecture without discussion.

---

## 10. Audio fingerprints

A fingerprint is a measurement produced by Aède.

It is not an external claim.

AcoustID's response is an external claim and must remain in the source layer.

A fingerprint match must not automatically modify tags.

Keep the confidence/score associated with external identification.

Before computing a fingerprint or contacting an external service, check whether the required identifier is already available locally.

---

## 11. Network access

Network access is explicit.

Normal local catalog operations must remain offline.

External passes must:

- obey the service's rate limits;
- identify themselves correctly where required;
- save progress during long runs where appropriate;
- avoid retry behaviour that violates service rules;
- stop rather than hide service failures.

Never make a feature network-dependent merely because an external lookup is convenient.

---

## 12. CLI contract

A command must answer the question it was asked.

It must not:

- silently ignore unknown options;
- silently ignore unsupported options;
- silently fall back from invalid values;
- silently discard positional arguments;
- print a successful-looking answer after rejecting part of the request.

Invalid input must be rejected explicitly.

If a command accepts a value, parse that value strictly.

---

## 13. Help is part of the interface

Every command and option exposed by the CLI must be represented consistently across:

- parsing;
- validation;
- help;
- command dispatch;
- tests.

A new command or option must not exist in only one of those layers.

When an option is accepted, verify that it actually affects the command that accepts it.

### Advice the program prints is part of that contract

An error or a hint that names a command line is a promise the same binary must
keep. `aede artist --members` told a reader with no line-up to run
`aede fetch --artists --full`, and `--artists` belongs to `playlist`: the
program advised something it then refused, in the next breath, using a table two
hundred lines from the sentence.

Nothing else can catch this. The sentence is a `format!` argument in one file
and the rule is a table in another, and no test that runs the binary will ever
type every sentence it can print. So the table is a `const` at module level
(`OPTION_SCOPE`, `OPTIONS`) rather than a literal inside `main`, and
`main_tests.rs` reads the crate's own source for every `aede <command> …
--<option>` it prints and checks each against it.

Lifting a rule out of a function so that a test can read it is a good enough
reason on its own.

### A message written across source lines needs its backslashes

A long string continued with a trailing `\` drops the newline **and** the
indentation of the next line. Lose one backslash and nothing fails — it
compiles, the suite is green — and the user reads a sentence with eighteen
spaces in the middle of it. That shipped, and was found by a reader rather than
by anything here.

`main_tests.rs` now looks for a run of five spaces inside an open string
literal, in a line long enough to be a sentence. Deliberately narrow: a wider
rule flagging any literal still open at the end of its line reported eighteen
perfectly good continuations, which is how a test becomes something people
switch off.

---

## 14. CLI output

Output is designed for humans first.

Maintain:

- stable table structure;
- correct display widths;
- correct pluralisation — **including what follows the count**. `ui::plural`
  agrees the noun and cannot agree the verb after it, so a hard-coded one gives
  "1 track already have a lyrics file beside them". A phrase that reads
  correctly at one and at many is the fix, not a second branch per line:
  prefer a participle ("already with a lyrics file alongside") to a verb.
  Only running the program finds these;
- useful empty-result messages;
- explicit filtering information;
- explicit truncation/limits;
- useful next steps after errors.

An empty result must be distinguishable from an error or an empty library.

A command should preserve the shape of its answer regardless of whether it finds zero, one or many results.

---

## 15. Destructive operations

Destructive commands must:

1. explain what will be removed;
2. distinguish rebuildable data from non-rebuildable data;
3. require explicit confirmation in interactive use;
4. require explicit confirmation for non-interactive destructive operation.

Never assume that lack of input means "yes".

---

## 16. Tests

Tests are behavioural contracts.

Prefer testing:

- what the user observes;
- what the persisted data contains;
- what the command accepts or rejects;
- what an external service request actually does;
- what survives a scan or reload.

Avoid tests that merely reproduce implementation structure.

### Bug fixes

Every bug fix starts with a failing test.

### Fixtures

A new supported audio format requires an appropriate real fixture.

Fixtures representing external services must be based on verified real responses. Never invent service response data merely to make a fixture convenient.

This is not only about field names. A documented shape can be read correctly and
still be understood backwards. MusicBrainz returns a membership relation on both
artists with a `direction`, and a parser written from the documentation mapped
`"backward"` to the wrong end — which does not lose data, it **inverts** it:
Black Sabbath would have appeared in the list of Ozzy Osbourne's members. Two
live answers, one for a person and one for a band, settled it in a minute.

The same two answers also showed that `member of band` alone misses a solo
artist's entire band (they are `instrumental supporting musician`), and that an
`attributes` list holds `original` beside real instruments — so a field named
`instruments` would have put a lie in a column header.

Fetch the real answer before writing the parser, not after it fails.

### Test isolation

Tests must not depend on process-global mutable state.

In particular, do not make tests race through environment variables or shared temporary directories.

A test's temporary data must be isolated from every other test.

---

## 17. Tests beside implementation

Unit tests normally live beside the implementation.

When a test module becomes larger than approximately 200 lines, move it to a sibling test file while preserving the same module relationship and access to private implementation details.

The split must not change the number or meaning of tests.

### A sibling test file must be declared

A `*_tests.rs` that no module declares is compiled by nothing and run by
nothing, and `cargo test` is green either way — a missing `mod` is not an error
anywhere in Rust. The failure is silent and looks exactly like coverage.

The declaration is one line in the module under test:

```rust
#[cfg(test)]
#[path = "widget_tests.rs"]
mod tests;
```

`crates/aede-cli/tests/docs.rs` walks both `src/` trees and fails on a
`*_tests.rs` no module names. It happened once, to a file holding tests written
against a real bug.

---

## 18. API design

Follow standard Rust API conventions.

Prefer:

- `&str` over `&String`;
- `&[T]` over `&Vec<T>`;
- minimal visibility;
- explicit domain types;
- meaningful names;
- small focused functions.

Avoid abstraction for abstraction's sake.

### Three answers where two would lie

Where a value can be *unknown* as well as true or false, say so in the type.

`Membership::covers(year)` returns `Option<bool>`: a membership with no start
date places nobody, and one the source says has ended without saying when places
nobody either — the end is somewhere, and "somewhere" cannot be compared with a
year. Returning `false` there would hand the caller a silence dressed as
knowledge, and the caller would have had no way to count what it could not
place.

The same distinction already runs through `ArtistFacts::active` and
`Membership::over`: a band with no end date may be one that never stopped or one
nobody has filled in, and a reader shown the same thing for both learns nothing.

### Two meanings need two functions, not a comment

Where two different quantities share a primitive type, a single function that
takes one of them is a trap, however clearly its parameter is documented.

`ui::ago` takes an *elapsed span in seconds*. Ten call sites wrote
`ago(now - at)`; the eleventh wrote `ago(created_at)`, which reads correctly in
English and printed "56 years ago" for something a second old — a Unix
timestamp is a plausible number of seconds, so nothing could reject it. The fix
was `ui::since(at)`, which takes the moment and does the arithmetic once.

Keep the part with rules in it pure: `since` reads the clock, `ago` remains a
function of its argument and stays testable without one.

---

## 19. Error handling

Errors must tell the user:

1. what happened;
2. why it matters when useful;
3. what they can do next.

Never swallow an error silently.

If processing can continue after an individual failure, report the failure rather than pretending it did not happen.

---

## 20. Dependencies

Adding a dependency requires justification.

Before adding one, establish:

- why the standard library is insufficient;
- why existing dependencies cannot provide the required functionality;
- what the dependency replaces or enables;
- whether its maintenance status is acceptable;
- whether its dependency tree is reasonable.

Do not add a dependency merely to save a small amount of code.

---

## 21. Documentation

Documentation should describe stable project behaviour.

Do not document:

- conversation history;
- temporary implementation details;
- rejected ideas that have no continuing relevance;
- facts that can be obtained directly from the code.

Comments explain why.

Documentation explains behaviour and design.

---

## 22. One source of truth

Do not maintain several independent copies of the same rule.

Examples:

- command lists;
- option applicability;
- path handling;
- role vocabulary;
- source attribution;
- derived-data rules.

If the same rule appears in multiple places, look for a way to centralise it or make one representation authoritative and test the others against it.
