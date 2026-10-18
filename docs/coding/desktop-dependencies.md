# Desktop dependency findings

The desktop lockfile is audited independently of the CLI:

```sh
python3 tools/audit-dependencies.py --manifest-path desktop/Cargo.toml
npm audit --prefix desktop
```

The audit remains strict: there is no ignore list for desktop findings and no `continue-on-error` in its CI job. macOS verification does not establish that the complete Linux dependency tree is advisory-free.

## Corrected TLS dependency

Both desktop clients now resolve Rustls 0.23.46. The previous 0.23.45 resolution was reported by OSV, including [RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html). The fixed version is reused without a new product package or a certificate-verification bypass.

## Outstanding GTK3 findings

Tauri 2.12.1 on Linux resolves `glib 0.18.5` through GTK3 (`gtk 0.18.2`, WebKit2GTK, Tao/Wry and Muda). [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), also [GHSA-wrw7-89jp-8q8g](https://osv.dev/vulnerability/GHSA-wrw7-89jp-8q8g), concerns the unsound `VariantStrIter` string-array iterator. The upstream fix is glib >=0.20, outside GTK3's compatible 0.18 requirement. Authored desktop code has no GLib calls; the reviewed GTK/GIO/WebKit/Tao/Wry/Muda/Tauri sources have no `VariantStrIter` or `array_iter_str` caller. This source review is not a patch or a general safety guarantee. The Linux dependency remains flagged, and future native-stack changes require another review.

[proc-macro-error 1.0.4 / RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) is an unmaintained GTK3 build dependency with no patched version. The strict audit reports it too. The CLI audit does not acquire these GTK dependencies, and macOS/Windows native UI does not compile them.

No exception has been approved or applied. Before publishing Linux App artifacts, resolve the dependency findings or obtain an explicit, narrowly scoped maintenance decision. The release workflow creates drafts; it does not establish security approval merely because compilation succeeds. The registry audit sends only package names and versions and does not cover pinned Git sources.
