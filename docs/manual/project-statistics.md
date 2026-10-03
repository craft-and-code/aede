# Project statistics

These measurements describe the authored source tree used to build this documentation. They are generated at publication time, rather than maintained as figures in a README. Counts measure project size and coverage scope; they do not measure code quality or prove that tests pass.

## Generated measurements

<!-- project-statistics -->

## What is counted

Code lines are **physical lines**, including comments and blank lines, in the supported source files under `crates/`, `tools/` and `site/`. Nonblank lines are reported separately. The crate table separates production, test/support and example code. Test code lines include integration tests and fixtures written in a source language; that is a different measure from the active unit-test inventory.

Generated output, build caches, vendored files, hidden directories, symlinks, music/images, JSON fixtures, Cargo manifests/lockfiles and Markdown are excluded from code lines. Documentation has its own file/page counts. A published page counts once per language, including an explicitly labelled English fallback when a topic has no French translation. The script reuses the publisher's page catalog rather than a separate list.

**Active Rust unit tests** come from compiled workspace library and binary test harnesses. The script lists registered tests, subtracts ignored tests and publishes a conservative rounded lower bound such as “more than 1,400 active unit tests”. It does not count `#[test]` text. Integration tests, example tests, doctests and Python helper tests are outside this TU count, even though they remain part of verification.

The inventory records the toolchain, host and feature configuration: conditional tests can differ between platforms or feature sets. Listing a test does not execute it. The published TU figure is available only when an inventory of the current Rust/Cargo sources has been supplied; a missing or stale inventory cannot silently become a current test count. See the [verification workflow](../coding/engineering-rules.md) and [current state](../coding/current-state.md) for actual validation results.

## Reproduce the report

From the repository root, Python 3.9 or later is sufficient for source measurements; no package installation or network request is needed:

```sh
python3 tools/project-stats.py
python3 tools/project-stats.py --json
```

To include active TUs, first provide the existing Rust/native build prerequisites from the [installation guide](install.md). Fetch the locked dependencies explicitly if they are not already cached, then build/list test harnesses offline:

```sh
cargo fetch --locked
python3 tools/project-stats.py --tests --output target/project-stats.json
python3 tools/build-site.py --check --project-stats target/project-stats.json
```

`--tests` does not run the test bodies, play audio or access music files. Use `tools/check.sh` for the complete checks; it also generates the inventory and publishes the documentation from it. The Site workflow generates its own inventory for the Linux/default-feature build. The machine-readable JSON retains exact measurements and provenance for verification; the public TU display stays rounded. Do not commit generated reports from `target/` or `dist-site/`.

For another configuration, use `--no-default-features` and/or `--features "..."`. Cargo respects the existing `CARGO_TARGET_DIR`; an alternative inventory can be reused with `--test-inventory PATH` only while its Rust/Cargo source fingerprint still matches. Running the publisher without `--project-stats` still generates fresh source statistics and explicitly leaves the TU figure unavailable.
