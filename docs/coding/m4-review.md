# M4 network-device ownership and boundaries

## Shared profiles

SlimProto, UPnP AVTransport, OpenHome and Google Cast reuse `aede-devices`; the CLI remains separate from catalogue HTTP. The `network_audio_upnp` legacy profile and additive `network_audio_profiles` capability share one named-administrator service with exact source/login ownership, finite selections and a five-second heartbeat lease. Logout/revocation, lease loss and shutdown request cancellation. See [device contracts](../server/devices.md) and the [crate README](../../crates/aede-devices/README.md).

Discovery is explicit and bounded. All-profile discovery returns typed rows and partial failures; SlimProto peers can be configured manually. Cast certificate inspection observes a fingerprint but never establishes trust automatically. Protocol-specific options, replacement consent and source selection are validated before work. Clients compensate retired accepted starts with Stop under the original login, not a replacement account.

## Ownership and input admission

Initial and subsequent Cast replies relinquish ownership if media identity differs from the requested content. In particular, a foreign initial LOAD response must not receive STOP during error cleanup. LOAD refusal, foreign takeover and failed Stop confirmation remain distinct from stopping an owned session.

UPnP URI ownership, OpenHome playlist token/ID checks and SlimProto framing preserve their protocol-specific rules. Private IPv4 endpoints, same-authority URLs, redirect/traversal refusal, bounded HTTP/XML/DNS/Cast/SlimProto frames, certificate pins/handshake signatures, transfer slots and selected-original source identity checks remain separate admission boundaries. No arbitrary filesystem path or silent external-session replacement is admitted.

## Verification and limits

Loopback TLS/media fixtures and scripted controllers cover initial/subsequent takeover, cancellation before admission, exact-login replacement, reader/writer teardown and retained media-worker slots. Frontend regressions exercise accepted OpenHome/Cast/SlimProto starts after replacement, cancellation and disposal; retired run/certificate replies cannot publish into a successor profile. These are software evidence, not physical receiver acceptance.

Finite queue/memory/time budgets do not imply whole-system throughput or arbitrary decoder support. Native Windows/Linux execution, real receiver interoperability, acoustic continuity, gapless and multiroom behavior require independent acceptance. Persistent registries and richer controls remain roadmap work. Use [current state](current-state.md) for checkpoints and [engineering rules](engineering-rules.md) for the gate rather than a per-pass list of unchanged test results.
