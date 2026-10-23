# Dependency audit policy and baseline

## Scope and reproduction

The CLI/engine and desktop App are separate Cargo workspaces, with independent lockfiles and `deny.toml` policies. The development tools are installed separately from application dependencies:

```sh
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
```

From the repository root:

```sh
cargo audit --file Cargo.lock
cargo audit --file desktop/Cargo.lock
cargo deny --locked check
cargo deny --manifest-path desktop/Cargo.toml --config desktop/deny.toml --locked check
```

These are explicit network operations: the tools update the RustSec advisory database and consult registry information. Keep them separate from the offline root and desktop gates. Plain `cargo audit` does not repair dependencies; do not run `cargo audit fix`, `cargo update` or change manifests as part of a reporting-only audit.

`cargo-audit` checks the two full lockfiles without OS/architecture filtering. `cargo-deny` enables all workspace features without a target allowlist, including build/dev dependencies and platform-specific dependency branches. This is a dependency-graph audit performed on macOS, not a native Linux/Windows build or execution check.

## Policies

- Advisory ignores are empty. Vulnerabilities, transitive unsoundness, unmaintained packages and yanked versions remain blocking in `cargo-deny`. Existing OSV compatibility decisions are not silently converted into RustSec exclusions.
- Licenses are checked for build and development dependencies too, against explicit lists covering the current MPL-2.0 project and selected permissive license expressions. Unlisted expressions fail. This is a machine-readable policy check, not a legal opinion or a completeness review of bundled notices.
- Unknown registries and Git repositories are denied. The engine allows only crates.io and the existing FlacCompagnon Git repository, requiring a tag or revision in addition to its lockfile pin. The App currently allows no Git source.
- Parallel crate versions and wildcard declarations are warnings. Existing path dependencies and tagged inherited workspace dependencies omit registry versions; their publication hygiene is distinct from an unpinned downloaded package. No manifest was changed to satisfy these warnings. Duplicate versions across optional/platform branches are not automatically redundant or vulnerable.

The existing [OSV/npm workflow](../../.github/workflows/security.yml) remains unchanged. The new RustSec policies intentionally expose findings for review before dependency updates or CI-policy changes.

## Baseline: 2026-10-23

Source baseline: `bfd95c5e`, with only the audit policies/documentation added for this run. Host toolchain: Rust/Cargo 1.99.0, `aarch64-apple-darwin`. Tools: `cargo-audit 0.22.2`, `cargo-deny 0.20.2`. RustSec database: 1,350 advisories, revision `6e868779ba1c38dc4a1bc8090b8c22d15dc69e56`, last updated `2026-10-23T16:21:05+02:00`.

All project Cargo manifests and both lockfiles were fingerprinted before installation/auditing and compared afterward: their bytes are unchanged. No application dependency was added, updated, removed or patched. No new advisory exception was introduced.

| Workspace | Lockfile entries | `cargo audit` | `cargo deny` |
| --- | --- | --- | --- |
| CLI/engine | 260 | Exit 0, one unmaintained warning | Exit 1, `paste` maintenance advisory blocks |
| Desktop App | 502 | Exit 0, unmaintained/unsound/yanked warnings | Exit 1, three advisory/yanked findings block |

RustSec reports zero entries in the `vulnerabilities` category in both audit JSON reports. This must not be summarized as “no security issue”: the App's GLib memory-safety defect is classified separately as `unsound`, and plain `cargo audit` returns success despite these informational warnings. The stricter `cargo-deny` policy rejects it.

| Component | Dependency path and scope | Finding | Review before any change |
| --- | --- | --- | --- |
| `paste 1.0.15` | Engine → `aede-core` → `lofty 0.25.1` → `paste` | [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html): unmaintained; no patched version listed | Review an upstream Lofty change or explicitly reviewed alternative. This notice alone is not an exploit demonstration. |
| `glib 0.18.5` | App → Tauri/GTK3 stack, Linux branch | [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html): unsound `VariantStrIter`; patched in GLib Rust crate >=0.20 | Existing [GTK compatibility assessment](desktop-dependencies.md) remains a recorded exception in the OSV audit only. No authored Rust caller of `VariantStrIter`/`array_iter_str` was found in this run; this is not a complete transitive reachability or exploitability proof. A forced 0.20 upgrade is not assumed compatible with GTK3 0.18. |
| `proc-macro-error 1.0.4` | App → GTK3 macro dependencies | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html): unmaintained; no patched version listed | Existing GTK build-stack limitation; evaluate upstream migration rather than replacing the package blindly. |
| `clipboard-win 5.4.1` | App → `tauri-plugin-clipboard-manager 2.5.0` → `arboard 3.7.0` → `clipboard-win`, Windows branch | Version is yanked from crates.io; no associated vulnerability ID in these results | New item relative to the documented GTK exceptions. Review the upstream reason and compatible resolution before changing the lockfile. A yank is not itself proof of a vulnerability. |

Both workspaces pass the configured license-expression and source checks. Duplicate-version warnings cover eight engine crate names and thirty App crate names. Additional wildcard warnings concern five engine packages and one App package; they do not imply an unpinned registry download in the audited lockfiles.

### Tool limitation retained visibly

`cargo-deny 0.20.2` emits two `bug[unresolved-workspace-dependency]` diagnostics while checking the engine's inherited `flaccompagnon-core` declarations in `aede-cli` and `aede-core`. Cargo metadata and the source check resolve the tagged, SHA-pinned Git package; the diagnostic occurs in the wildcard/publication check. It was not suppressed, and manifests were not rewritten to hide it. The bans summary reports zero errors with thirteen warnings, but these extra internal diagnostics mean the engine declaration-hygiene pass cannot be described as fully clean. The App's bans summary has zero errors and thirty-one warnings, without this internal diagnostic.

Raw audit JSON, deny JSONL and the local summary are retained under the ignored `target/dependency-audit-2026-10-23/` directory. These local files include tool-generated provenance; they are not committed project sources. The findings above are the durable, sanitized review record.

## Proposed remediation, investigated on 2026-10-23

At the investigation stage, no dependency change had been applied. The following resolution was first tested with Cargo's read-only `--dry-run` mode; all original manifest/lock fingerprints were unchanged. The subsequently approved clipboard update and its verification are recorded below.

### Clipboard: narrowly update the existing transitive package

The fresh official crates.io index lists `clipboard-win 5.4.1` as yanked and `5.4.2`/`5.4.3` as not yanked. The installed `arboard 3.7.0` declares `clipboard-win = "5.3.1"` (caret-compatible through 5.x), with its `std` feature. A version upgrade of Tauri, its clipboard plugin or Arboard is not required by that constraint.

The upstream [5.4.2 commit](https://github.com/DoumanAsh/clipboard-win/commit/38c3c3fef175c20ed8f8e33dacb9b1e3bb8eca87) replaces an unchecked HTML-fragment pointer copy with a bounds-checked string slice and adds an invalid-fragment regression. The [5.4.3 commit](https://github.com/DoumanAsh/clipboard-win/commit/ed27008ae5d1f10cfbb266de039f0fb35fd05a56) additionally adjusts the delay between clipboard-open attempts. Both upstream commits are dated 2026-10-23. This establishes a real upstream corrective change; it is not an independently confirmed statement of the publisher's reason for yanking every older version.

The completed investigation command was:

```sh
cargo update --manifest-path desktop/Cargo.toml -p clipboard-win --precise 5.4.3 --dry-run
```

The dry run reported only `clipboard-win 5.4.1 -> 5.4.3` and explicitly confirmed that it did not write the lockfile. The approved update below repeats that command without `--dry-run`. Native Windows build and clipboard acceptance remain required: copy the login/address/password, verify sensitive-password handling where supported, and confirm that clipboard contention does not hang the console. A macOS build does not exercise this Windows-only implementation. The App currently exposes fixed native text/secret writes, not HTML reads or general webview clipboard access; no exploitability claim is inferred from that limited surface.

## Approved clipboard update and verification: 2026-10-23

`cargo update --manifest-path desktop/Cargo.toml -p clipboard-win --precise 5.4.3` changed only that package's version and checksum in `desktop/Cargo.lock`. Fingerprints confirm that all project Cargo manifests and the engine lockfile remain unchanged. No new direct dependency, parent-library upgrade or advisory exemption was introduced.

The complete independent desktop gate passed on macOS: 76 Rust tests with none failed or ignored, 38 Vitest cases, 2 Node cases, format/lint checks, frontend and component-documentation builds, version coherence, Clippy, Rustdoc and the native build. Storybook still reports a Node `module.register()` deprecation and a bundle-size warning; neither caused a gate failure. This does not establish Windows clipboard runtime acceptance.

Both auditors were rerun against RustSec revision `84d943a6deeb8cc6c6f632531aaf4f1c62275239` (1,351 advisories, last updated `2026-10-23T16:42:16+02:00`). The yanked `clipboard-win` finding disappeared. The newer database also added [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html), matching Tokio in both otherwise unchanged dependency graphs. Its advisory date is 2026-10-13; its appearance here reflects the database revision, not a Tokio update in this change.

| Workspace | `cargo audit` after update | `cargo deny` after update |
| --- | --- | --- |
| CLI/engine | Exit 0; `paste 1.0.15` unmaintained and `tokio 1.53.1` unsound | Exit 1; two advisory errors |
| Desktop App | Exit 0; `proc-macro-error 1.0.4` unmaintained, `glib 0.18.5` and `tokio 1.53.2` unsound; no yanked warning | Exit 1; three advisory errors |

The Tokio advisory concerns Unix `AsyncFd` constructor I/O safety, including a possible use-after-free; see the [upstream correction](https://github.com/tokio-rs/tokio/pull/8587). RustSec lists `>=1.51.6, <1.52.0` or `>=1.53.3` as patched. A source search found no authored `AsyncFd` calls in `crates/`, `desktop/core/` or `desktop/src-tauri/`; that is not a transitive reachability or exploitability assessment. Tokio has deliberately not been updated in this clipboard-only change.

License and source checks still pass. The thirteen engine and thirty-one App declaration/duplicate warnings, plus both engine `unresolved-workspace-dependency` internal diagnostics, remain visible. Both audit reports still have zero entries in the separate `vulnerabilities` category; the unsoundness findings above remain security concerns and block `cargo-deny`. Before/after reports are retained separately under `target/dependency-audit-2026-10-23/`.

## Approved Tokio update: 2026-10-23

The subsequent focused correction updates `tokio 1.53.1 -> 1.53.3` in the engine lockfile and `1.53.2 -> 1.53.3` in the independent App lockfile. Only Tokio's version and checksum change; Cargo accepts both lockfiles with `--locked --offline`. Manifests, enabled features, other package resolutions and both advisory policies remain unchanged. The registry lists 1.53.3 as non-yanked with Rust 1.71 as its minimum, below both project toolchain baselines. The earlier investigation used `--dry-run` in each workspace before approval.

The [1.53.3 release](https://github.com/tokio-rs/tokio/releases/tag/tokio-1.53.3) adds `AsyncFd::register`/`register_with_interest` and deprecates the older constructors; it does not remove those older APIs. RustSec treats this release as patched, but upgrading cannot make continued misuse of the deprecated constructors safe. No `AsyncFd` references were found in authored engine/App sources or the available source/target files of 622 distinct non-Tokio packages from the two resolved metadata graphs. This textual source review is not a complete reachability proof for generated code, aliases or unexamined platform behavior. No application API migration or new `unsafe` code was needed.

Both auditors were rerun against RustSec revision `84d943a6deeb8cc6c6f632531aaf4f1c62275239` (1,351 advisories). Neither lockfile now matches RUSTSEC-2026-0386. The original audit and post-clipboard reports remain separate from the `*-after-tokio` evidence files under `target/dependency-audit-2026-10-23/`.

| Workspace | `cargo audit` after Tokio update | `cargo deny` after Tokio update |
| --- | --- | --- |
| CLI/engine | Exit 0; `paste 1.0.15` unmaintained warning remains | Exit 1; one advisory error |
| Desktop App | Exit 0; `proc-macro-error 1.0.4` unmaintained and `glib 0.18.5` unsound warnings remain | Exit 1; two advisory errors |

The thirteen engine and thirty-one App declaration/duplicate warnings and both engine internal workspace-dependency diagnostics remain visible. License and source checks pass. No new advisory ignore or target exclusion was introduced; these workspace audits must not be described as entirely clean.

Both complete gates passed on macOS with Tokio 1.53.3. The engine passed 2,927 Rust tests across 52 suites with none failed or ignored, required FFmpeg regressions, helper/site/release/terminal checks, Clippy, Rustdoc and the optimized CLI build. The independent App gate passed 76 Rust tests, 38 Vitest cases and 2 Node cases, frontend/component documentation, version coherence, Clippy, Rustdoc and its native build; it rebuilt and verified the embedded CLI before running real temporary-profile HTTPS tests. Storybook's existing Node deprecation and bundle-size warning remain visible. An initial root test compilation stopped because the disk was full; generated incremental caches and obsolete server build artifacts were removed, then both complete gates succeeded with `CARGO_INCREMENTAL=0`. No sources or test cases were removed. Native Linux/Windows validation remains pending in the existing CI matrix; no remote run was triggered here.

## Remaining remediation plan

The original GLib, macro-maintenance and declaration/tool plan below is retained without suppressions. Tokio is resolved by the explicit version update above, subject to the recorded source-review and native-platform limits.

1. **GLib unsoundness:** prioritize actual Linux remediation. No newer 0.18.x patch is present in the fresh registry index. The upstream [VariantStrIter correction](https://github.com/gtk-rs/gtk-rs-core/pull/1343) exists, but GLib >=0.20 cannot simply satisfy GTK3's 0.18 dependency. First check whether a supported Tauri/GTK stack can include a corrected compatible release; otherwise assess a minimal, traceable backport with an affected-iterator regression and native Linux tests. A local backport needs its own source/patch evidence and must not be presented as a clean upstream-version audit. Keep the original advisory visible until the exact resolution is established.
2. **`paste` through Lofty:** the fresh index shows that Lofty 0.25.2, 0.25.3 and 0.25.4 still require `paste ^1.0.15`; upgrading Lofty alone to one of those versions does not remove the notice. Prepare an upstream-compatible replacement/removal of that macro dependency, or evaluate a narrowly reviewed fork if no maintained release is available. Preserve tag-reading behavior, format fixtures, the Rust 1.89 baseline and the root gate before adopting it. No blanket transitive override is proposed.
3. **`proc-macro-error` through GTK macro crates:** investigate a maintained upstream replacement in `glib-macros`/`gtk3-macros`. Replacing the top-level package name does not automatically migrate those callers. Any proposed patch must preserve macro expansion, errors/spans and supported native builds. Keep its maintenance warning separate from GLib's runtime memory-safety issue.
4. **Declaration/tool hygiene:** triage duplicate versions by real feature/target inclusion before attempting consolidation. Do not force unrelated major versions together. Review versionless local path declarations separately from registry vulnerabilities, and reduce the cargo-deny workspace/Git diagnostic to an upstream reproducer or validate an upstream tool fix. Do not disable the diagnostic to label that pass clean.

Each adopted correction should have a focused dependency diff, regression evidence and a before/after audit. The original baseline stays intact; corrected items are recorded as resolved by their actual source/version change. Keep advisory ignore lists empty, retain all-target/all-feature coverage and use a failing warning policy when interpreting `cargo-audit` as a security gate. Unresolved findings remain explicit blockers rather than being hidden behind an aggregate success result.

## Coverage limits

Matching a package to an advisory does not establish exploitability; absence of a match does not establish safety. RustSec matching and source approval do not review FlacCompagnon's Git code, the locally patched ALSA code, external native/system libraries, bundled JavaScript, npm dependencies or audio files. The separate npm/OSV checks and source/native-platform review remain necessary. Only the explicitly approved `clipboard-win` and Tokio lockfile updates were applied; no automatic blanket fix or new advisory exemption was introduced.
