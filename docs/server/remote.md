# Encrypted remote access

The default `aede serve` remains HTTP on this computer's loopback address. An explicit HTTPS listener lets authenticated clients on another computer use the catalog, their own personal data and the [audio contract](playback.md). Initialize [accounts](accounts.md) locally before enabling it. HTTPS startup refuses absent or unreadable accounts; a legacy administrative token cannot replace a session.

## Configure HTTPS

Provision a PEM certificate chain and matching, unencrypted PEM private key. The certificate must cover the hostname clients use and be trusted by those clients. Both paths must be regular files, without symlinks; each file is limited to 256 KiB and the chain to 16 certificates. On Unix the private key must have no group/other access, for example mode 0600. Aède does not issue certificates, renew them or reload changed certificates while running: restart after replacement. On Windows, protect these files with service-account ACLs; Aède automatically checks permission mode bits only on Unix.

```sh
aede serve --bind 0.0.0.0 --port 8787 \
  --tls-cert /private/aede/fullchain.pem \
  --tls-key /private/aede/key.pem \
  --authority music.example:8787
```

Replace the paths and hostname with your configuration. `--bind` accepts an IP literal, including IPv6; it is the listening address. `--authority` is the exact public `HOST:PORT`, without a scheme, path, credentials or wildcard; bracket an IPv6 hostname. Its port can differ from the listening port when an explicit network port mapping preserves TLS. The TLS CLI refuses port 0. All three TLS options are required together. A non-loopback listener without TLS is refused; HTTPS can also listen on loopback for private use.

Configure DNS, routing and your firewall separately. There is no automatic port forwarding or reverse-proxy header trust. TLS terminates in Aède; forwarding plaintext from a TLS-terminating proxy to an exposed Aède HTTP listener is unsupported. Verify with a client that checks the certificate and uses the configured authority; do not disable certificate verification.

## Authenticate clients

The native login and bearer-session contract is the same as for local [accounts](accounts.md), using `https://music.example:8787` and `wss://music.example:8787` in place of HTTP and WS. Every native catalog and personal request requires a current session. The separate [Subsonic/OpenSubsonic adapter](subsonic.md) uses revocable application keys for its authenticated `/rest` operations; only its extension discovery is public. Owners come from the credential, never a client-supplied identifier. An auditor can read the catalog and their own personal views; playback and other personal mutations require a user or administrator.

Each request must contain one Host matching the configured authority. An absolute-form target must use HTTPS and the same authority. A supplied browser Origin must be the exact matching HTTPS origin; native clients may omit Origin. Forwarded headers do not change these checks. No cross-origin permission, browser login screen or cookie session is implemented. Native passwords and bearer tokens must never enter URLs. Adapter credentials use protocol query/form parameters; redact them from logs and prefer form POST where the method/client supports it.

## Administration stays local

HTTPS disables the entire `/api/admin` family, including account administration, installation jobs and transitional personal routes for `local`. This applies even to an administrator session and even when HTTPS listens on loopback. Legacy `AEDE_ADMIN_TOKEN` access is unavailable on HTTPS. Use `/api/me/v1` for your own data; manage accounts and run scan/fetch with the trusted local CLI. The private Unix command channel keeps its local scope.

## Deployment limits

An HTTP response that makes no write progress for ten seconds is disconnected. Shutdown gives established HTTPS connections, including WebSockets, at most fifteen seconds to drain. The five-second final background-worker wait described in the [operating guide](../operating.md) starts only after accepted local jobs/commands have finished.

At most 64 live TCP/TLS connections are admitted, including upgraded WebSockets. TLS negotiation and HTTP headers have ten-second deadlines; headers are limited to 64 fields and a 32 KiB parser buffer. Routed work has two bounded blocking-worker slots and a fifteen-second response deadline; work continuing after that deadline keeps its slot until it exits. Saturated connections close before HTTP; busy request work returns `503 request_limit`, deadline expiry `503 request_timeout` and worker failure `503 request_failed`. Retry reads after other work finishes. Notification and audio sockets recheck sessions, so logout, revocation and credential changes stop access without waiting for expiry. Audio also has four decoding slots and its acknowledgement window.

This HTTPS transport serves the native client contract and the implemented Subsonic/OpenSubsonic adapter, with their distinct authentication and audio semantics. Aède includes no mobile player. NAS packaging, real-library memory limits, target-device performance and physical playback still require deployment validation; see the [operating guide](../operating.md) and the [Compatible Aède requirements](compatible-aede.md).
