# M4 network-device hardening review

## Scope

This source review covers the initial SlimProto, UPnP AVTransport, OpenHome and Google Cast controllers, discovery/framing/XML/HTTP parsers, certificate pins, selected-original media serving, CLI option admission and the named-administrator UPnP API. Earlier pending progressive-check, account and UPnP work is preserved. No live device, user audio or account is modified.

## Demonstrated correction

A first Cast LOAD acknowledgement containing a different media identity was rejected, but its supplied session was still considered owned. Error cleanup therefore sent STOP to that foreign media. Subsequent status messages already relinquished ownership on a content mismatch; the initial acknowledgement now does the same before cleanup. This is an ownership/interoperability defect, not evidence of remote code execution or an account-data leak.

The existing scripted receiver gains an initial-content-takeover variant. Its regression fails before correction by observing STOP and passes afterwards. The existing load-refusal and subsequent-session-takeover scenarios remain intact; no test is removed or duplicated. Ordinary finite queues, duplicate occurrences and stopping an owned session remain covered.

## Boundaries and performance

Inspected controls include explicit private IPv4 endpoints, same-authority device URLs, refusal of redirects/traversal, bounded HTTP/XML/DNS/Cast/SlimProto frames, partial-frame reader ownership, bounded message queues and transfer slots, exact certificate pins with handshake signatures, read-only source identity checks and exact-login/role/lease admission. No additional demonstrated defect is established in these inspected paths during this pass. No speculative optimization, new dependency, protocol rewrite or change to the audio callback is introduced. Existing finite selection, memory and timeout budgets remain; no whole-system performance improvement is claimed.

The CLI controller remains separate from the catalog API. An initial server-owned UPnP profile now exists, so crate READMEs, architecture and roadmap wording are corrected where they still described all authenticated control as future work. The French device guide now describes this bounded API too. Persistent registry, richer controls and other authenticated protocols remain future work.

## Verification (2026-10-19)

- All 57 device-crate tests pass, including the failing-before-correction ownership variant, real loopback TLS/media fixtures and bounded simulated controllers.
- All 340 server library tests and both CLI device-command tests pass: 399 selected Rust tests in total.
- All-target warning-denied Clippy passes for aede-devices and aede-server; Rustfmt and diff checks pass.
- Warning-denied Rustdoc passes for both crates. The website renderer checks all 115 documentation pages in both languages, including local links and fragments.

The first formatting check identifies only formatting in the newly added scenario, which is corrected before the final clean check. No complete root release gate or physical device/output test is claimed. Windows/Linux execution, actual decoder support, acoustic continuity, gapless and multiroom acceptance remain separate. Phémios is not rebuilt or modified; no commit is created.

## Neighboring ownership confirmation (2026-10-19)

No additional demonstrated defect is established in this follow-up. The inspected paths include initial and subsequent Cast identity checks, LOAD refusal and Stop confirmation, UPnP URI ownership and OpenHome playlist token/ID checks, cancellation before transport admission, controller reader/writer teardown, file-worker slot retention and exact-login API ownership. Existing pending corrections are preserved without production or test changes.

All 57 device tests, seven network-audio server tests and two CLI device tests pass unchanged (66 selected cases). All-target warning-denied device/server Clippy, Rustfmt and diff checks pass. The earlier full-server/Rustdoc/bilingual documentation checkpoint remains recorded above; it was not rerun in this documentation-only follow-up. No Phémios modification/rebuild, physical receiver acceptance or commit.

## Shared graphical profile integration (2026-10-19)

The existing named-administrator network-audio service now exposes OpenHome, Google Cast and SlimProto through the same source-validated, exact-login-owned finite controller as UPnP. The additive network_audio_profiles capability preserves legacy UPnP requests. Explicit all-profile discovery yields typed, bounded rows and partial failure information; SlimProto peers are manually configured. Cast certificate observation shares discovery admission, rechecks the owner before publication and never establishes trust automatically. Protocol-specific options are validated before catalog/source work; arbitrary paths, silent replacement and unsupported option combinations remain refused. Existing protocol/media implementations are reused without new dependencies.

All 342 server library tests (including two added profile-option cases) and all 57 device tests pass. All-target server/device Clippy and warning-denied Rustdoc pass, as do Rustfmt, diff checks and 115 bilingual documentation pages/link validation. Clippy initially identifies duplicate branches in the new test fixture; those branches are combined and verification passes afterwards. Phémios independently passes 1,074 frontend tests, 43 selected network/federation core cases, strict App/component-explorer TypeScript and core Clippy without an App rebuild. No physical receiver, Windows/Linux execution, release build, commit or publication is claimed. The embedded engine binary is intentionally not regenerated.

## Graphical profile lifecycle confirmation (2026-10-19)

No new demonstrated production defect is established in this follow-up. The review follows profile validation, exact-login authorization/recheck, owner replacement/revocation, discovery and Cast inspection result lifetimes, local shutdown before publication, compensating Stop for retired accepted starts, heartbeat cancellation and controller teardown. Nine added frontend cases exercise OpenHome, Cast and SlimProto accepted starts after account replacement, cancellation and disposal; they pass without production changes, preserve the original login for Stop and never expose the retired run. Existing actual-logout, role-loss and late certificate-inspection cases remain intact.

All 1,083 frontend cases and 66 selected Aède Rust cases (57 devices, nine network-audio/profile cases) pass, with App TypeScript, frontend targeted lint, device/server all-target Clippy, Rustfmt and diff checks. This pass changes tests/documentation only; prior pending product changes are preserved. No App/engine rebuild, native visual or physical receiver acceptance, commit or publication.
