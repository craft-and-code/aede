# Windows release correction and validation

## Scope and source

The maintainer authorizes correcting the Windows failures identified in [release run 39992380120](https://github.com/craft-and-code/aede/actions/runs/39992380120), adding tests, running available checks and committing without publication. Base main is `19aec35a848cbc41e82d1f0039f2bf4246eded10`, the existing remote v0.7.0 commit. Version numbers, release tags, workflows, dependency manifests/lockfiles and advisory policies are unchanged. No push, workflow dispatch, installer publication or Phémios dependency update is part of this correction.

That historical Windows run fails two scan-root tests, one forgetting-preview assertion and two device-media tests; desktop Clippy separately rejects a Unix-only mutable builder. Four Mac/Linux jobs pass and publication is skipped. Historical CI evidence does not establish Windows acceptance of the new candidate.

## Corrections and regressions

- Scan-root and media fixtures construct raw traversal with OsString rather than joining onto a canonical verbatim Windows path, which could remove `..`. Assertions now verify that ParentDir reaches the subject. All original tests and behavioral cases remain enabled.
- New root tests distinguish valid existing-directory parents from file/missing ancestors, including an existing final target, relative inputs and an empty root. A Windows-only case covers drive-relative/root-relative admission. Media has a Windows-only drive/UNC/verbatim traversal matrix that must refuse before filesystem/network access.
- The new existing-target regression exposes a real shortcut defect on the Mac host too: canonicalization accepts file/../retained when retained exists. The first added case fails on the original implementation; expanded before-fix tests also reproduce this for a relative file parent. Missing-parent and valid-directory contrasts remain explicit. Resolution now bypasses that shortcut whenever a raw parent exists and resolves only the absolute base before walking the unvalidated tail. The existing finite link/component budgets, private data retention, selection/result checks and lossless UTF-8 refusal remain in force. No path identity/schema migration or new unsafe code.
- Forgetting previews compare against the fixture's canonical data directory, matching the actual output contract. Noninteractive refusal, explicit consent, no pipe prompt, original-store snapshots and selected-directory isolation assertions remain intact.
- Only the disposable media test file gains a writable handle for set_modified, without truncation or byte changes. Production source readers remain read-only; stale identity and original-byte assertions remain intact.
- Desktop archive creation uses an immutable builder except inside its Unix configuration binding. Exclusive creation and 0700 mode are preserved. The existing profile/archive retention and permission tests remain applicable; no lint allowance is added.

## Execution evidence

Local logs and fingerprint records are retained under ignored `target/windows-release-fix/`. Commands run at the repository root on macOS arm64 / Rust 1.99.0 with CARGO_INCREMENTAL=0. Before correction, the new absolute/relative existing-target assertions fail; the original root suite remains preserved. The initial sandboxed media suite cannot bind four loopback listeners (EPERM); the identical suite passes with local-socket permission, without code/timeout/filter changes. This setup failure is retained separately from the assertion failures.

After correction, the focused root, media and forgetting suites pass on the host. Complete root/desktop gate results and sanitized evidence digests follow below. Host checks do not execute cfg(windows) cases, Windows file/ACL semantics or native Windows Clippy.

## Retained alerts and acceptance

Fresh cargo-audit reports zero entries in the separate vulnerabilities category but retains `paste` RUSTSEC-2024-0436 in the engine, and GLib RUSTSEC-2024-0429 plus proc-macro-error RUSTSEC-2024-0370 in the App. Both cargo-deny checks still exit 1; both engine unresolved-workspace-dependency internal diagnostics and duplicate/declaration warnings remain visible. License/source checks pass. Empty ignore lists, graph scope and every dependency resolution are unchanged. These are not cleared by host functional gates. See [dependency assessment](dependency-audit.md).

Native Windows execution of the corrected candidate, including both new Windows-only cases, all three original failed Rust targets and desktop warnings-denied Clippy, remains required. Linux/MSRV, installers, physical audio/devices and independent campaign acceptance are not inferred from this host. The stable-release prerequisite remains blocked until a later approved, fully validated Aède release is published; neither the v0.7.0 tag nor a draft qualifies as published stable.

## Final host results

Both full gates exit 0 on the corrected source: **2,934 root Rust cases across 52 suites**, none failed/ignored, and **76 desktop Rust cases**, **38 Vitest cases** and **2 Node cases**, none skipped. Root helpers/15 terminal cases, required FFmpeg coverage, all-target warnings-denied Clippy, Rustdoc, release CLI build and native App executable build pass. The complete site checks validate 116 bilingual pages; the executed source inventory lists 2,556 active library/binary unit cases (strict rounded display still more than 2,500). This inventory is separate from executed integration/doc tests.

The first complete root gate fails because this new validation page is not named by README/navigation. README now references it; the unmodified gate then passes. Both that failed run and the sandboxed loopback refusal remain in the ledger. No test is disabled/ignored, assertion removed, timeout increased or advisory suppressed to obtain the final result. Nine new test functions are retained: seven execute on this host and two are Windows-only, explicitly NOT RUN here. All original test functions/cases remain.

All nine recorded manifest/lock/policy/release-workflow hashes match the starting bytes. The maintained version stays 0.7.0 and the existing remote v0.7.0 tag is untouched. Fresh audits use RustSec database 84d943a6deeb8cc6c6f632531aaf4f1c62275239 (1,351 advisories). Both cargo-audit exits are 0 with the warnings above; both cargo-deny exits remain 1, without any ignore or policy change. Host gates do not clear these advisory controls, independent review or native Windows acceptance.

### Sanitized execution ledger

Repository-root cwd; all Rust checks use locked/offline resolution except explicit advisory-database network access. Logs below live under ignored target/windows-release-fix/. The `media` exit 101 is the sandbox loopback EPERM; `media-final` uses the identical test selection with socket permission. The `root-identity-before` exit 101 retains the two pre-correction existing-target assertion failures. Root/desktop gates run with CARGO_INCREMENTAL=0; no sibling-client source or existing user library is used.

| Check | Actual command | Exit | Sanitized log SHA-256 |
| --- | --- | --- | --- |
| root-identity-before | `cargo test --locked --offline -p aede-cli --bin aede commands::scan::root_identity::tests::` | 101 | `d4c6980345068b6d621e148d8e3ef6d848ba0a03cf4b294a5127b55d28749919` |
| root-identity-final | `cargo test --locked --offline -p aede-cli --bin aede commands::scan::root_identity::tests::` | 0 | `2e96fc3755b333c0e83ce1249368bf8897f2f0904d191d440e9401340755de30` |
| media | `cargo test --locked --offline -p aede-devices media::tests::` | 101 | `57587de6222491e81bce85d262a76696205b29ee9707527c31db0add36509456` |
| media-final | `cargo test --locked --offline -p aede-devices media::tests::` | 0 | `d157de0abfc56aa710b90f432bec2d97048c6aa2737583697345c00492a443ea` |
| forget-confirmation | `cargo test --locked --offline -p aede-cli --test forget_confirmation` | 0 | `803ad16ee170de0d5f3b1b61e669507f753297d3f4f1d48945a43af927fdc6c0` |
| root-gate-initial | `CARGO_INCREMENTAL=0 bash tools/check.sh` | 101 | `127df10bc33a9185c05069669e54d0a390fdaccdf703fe8225981f54280b799c` |
| root-gate | `CARGO_INCREMENTAL=0 bash tools/check.sh` | 0 | `5608d8921df00121b774c62f571aada6dc7fe19b39ec4c1ff3de9ff4cd4b6926` |
| desktop-gate | `CARGO_INCREMENTAL=0 bash desktop/tools/check.sh` | 0 | `c11e863a5fbeb0bd661c3ff4a7eb65171e2106eb21334da57439b5c284955f15` |
| audit-engine | `cargo audit --json --file Cargo.lock` | 0 | `15928e34127567ab01b062efaa9c676e7f3b0ff3655f2aff6848909964048926` |
| audit-desktop | `cargo audit --json --file desktop/Cargo.lock` | 0 | `8c491f96cc59520f6983ff9f2618b1f40b24e92060b3787991030f678c5d0cd5` |
| deny-engine | `cargo deny --locked check` | 1 | `e600862d5ff50760f8d0117cd6962226029d18479b141ecdcb913d1fa5f82a28` |
| deny-desktop | `cargo deny --manifest-path desktop/Cargo.toml --config desktop/deny.toml --locked check` | 1 | `7b1f2ffedc64f67fd38627afdfd4d2b91031755ad767faa6131853b8c23a048a` |
