# Desktop dependency findings

The desktop lockfile is audited independently of the CLI:

```sh
python3 tools/audit-dependencies.py --manifest-path desktop/Cargo.toml --desktop-compatibility-warnings
npm audit --prefix desktop
```

The audit fails for every finding except the exact approved GTK3 combinations below, reported as visible warnings only when the explicit option is selected for this repository’s desktop manifest. Without that option they remain errors. There is no `continue-on-error`, package-wide exemption or exception for another version/advisory. macOS verification does not establish that the Linux dependency tree is advisory-free.

## Corrected TLS dependency

Both desktop clients now resolve Rustls 0.23.46. The previous 0.23.45 resolution was reported by OSV, including [RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html). The fixed version is reused without a new product package or a certificate-verification bypass.

## Outstanding GTK3 findings

Tauri 2.12.1 on Linux resolves `glib 0.18.5` through GTK3 (`gtk 0.18.2`, WebKit2GTK, Tao/Wry and Muda). [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), also [GHSA-wrw7-89jp-8q8g](https://osv.dev/vulnerability/GHSA-wrw7-89jp-8q8g), concerns the unsound `VariantStrIter` string-array iterator. The upstream fix is glib >=0.20, outside GTK3's compatible 0.18 requirement. Authored desktop code has no GLib calls; the reviewed GTK/GIO/WebKit/Tao/Wry/Muda/Tauri sources have no `VariantStrIter` or `array_iter_str` caller. This source review is not a patch or a general safety guarantee. The Linux dependency remains visibly flagged as an approved compatibility warning, and future native-stack changes require another review.

[proc-macro-error 1.0.4 / RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) is an unmaintained GTK3 build dependency with no patched version. The audit reports it as an approved maintenance warning for exactly version 1.0.4 and that identifier. The CLI audit does not acquire these GTK dependencies, and macOS/Windows native UI does not compile them.

The maintainer explicitly approved exactly glib 0.18.5 with RUSTSEC-2024-0429 and GHSA-wrw7-89jp-8q8g, and proc-macro-error 1.0.4 with RUSTSEC-2024-0370. This compatibility decision does not patch GLib; replace the exception with an upstream correction when the compatible GTK3/Tauri stack permits it. The release workflow creates drafts; it does not establish security approval merely because compilation succeeds. The registry audit sends only package names and versions and does not cover pinned Git sources.
