# Network audio devices

M4 begins with explicit, finite playback from the Terminal: SlimProto first, followed by UPnP AVTransport, OpenHome Playlist and an experimental Google Cast sender. These are initial control profiles, not a complete Lyrion server, a DLNA-certified MediaServer, or a promise that every renderer works. Real-device acceptance is pending.

## What runs where

`aede play` decodes locally, applies the shared DSP and normally uses CPAL for the computer's audio output. `aede cast` sends the **unchanged original file** over HTTP to one selected LAN player; that player decodes it and owns its output clock. No server normalization, EQ, resampling, dither or lossy conversion is applied to these originals. An unsupported advertised format is refused rather than silently converted. This preserves file quality, but the receiving device can have its own processing.

The decoded FLAC MD5 check performed by local/native PCM playback does not run during this encoded-file transfer: Aède does not decode the source here. Decoder integrity checks depend on the receiving player; an explicit library integrity check remains a separate operation.

The command reuses local playback selection: a file, folder, M3U/M3U8, collection or catalogued artist/album/track. The first device profile accepts 1–64 occurrences, preserving order and playlist duplicates. It has no repeat/shuffle/seek options or terminal live queue editing. Keep the process running until playback finishes. Ctrl-C requests device Stop and closes the media listener. Disconnected devices cannot confirm Stop; neither a network transfer nor an unauthenticated device status writes a listening event.

This temporary controller is separate from `aede serve`; it neither requires a running API nor exposes catalog, accounts or administration. The initial authenticated graphical API profiles below reuse these same controllers; a persistent shared device registry and richer device administration remain later work.

The standalone [`aede-devices`](../../crates/aede-devices/README.md) crate implements these protocols and their selected-original HTTP transport. The CLI calls it directly; Subsonic/OpenSubsonic and account authorization stay in `aede-server`. Both transports reuse one original-media MIME/range policy. The existing `aede_server::devices` Rust path remains a reexport of the new crate, without a second implementation.

## SlimProto

On the server, explicitly choose its interface and the expected player's address:

```sh
aede cast /path/to/album --protocol slimproto --bind 192.168.1.10 --device 192.168.1.20
```

On a computer running Squeezelite, connect it to that server:

```sh
squeezelite -s 192.168.1.10:3483
```

For a local software trial, both addresses may be `127.0.0.1`. Start Aède first: it waits up to one minute for the selected player. `--port N` changes the SlimProto TCP port. Allow that port and the temporary media TCP port printed at startup through the LAN firewall. No UDP SlimProto discovery, Lyrion HTTP/JSON control API or remote screen emulation is supplied.

The handshake checks native codec declarations. The first profile accepts original FLAC, MP3 and integer-PCM WAV, with mono/stereo and explicit supported parameters. Float WAV and unknown/unsupported formats are refused. A declared `MaxSampleRate` constrains the selection; if absent, the controller conservatively refuses rates above 48 kHz. Native FLAC precision must be known and at most 24 bits in this initial profile. High-rate WAV also needs a representable SlimProto rate code. All occurrences are preflighted before the first start.

Downloaded EOF and decoder EOF are not treated as completed output. Queue advancement waits for the player's start, decoder completion and drained output report. This deliberately does not claim gapless SlimProto playback. Heartbeats and bounded protocol progress detect a lost player. Device pause/buttons and synchronized multiroom are not part of this first controller.

## UPnP AV and OpenHome

Discover advertised renderers on one chosen interface:

```sh
aede devices --bind 192.168.1.10
```

Use the HTTP description URL shown in the **Endpoint** column, rather than an invented control endpoint:

```sh
aede cast /path/to/album --protocol upnp --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
aede cast /path/to/album --protocol openhome --bind 192.168.1.10 --device http://192.168.1.20:49152/device.xml
```

UPnP uses ConnectionManager/GetProtocolInfo and AVTransport/SetAVTransportURI/Play with DIDL-Lite metadata. A currently active renderer must first be stopped. The finite queue advances only after confirmed playing and then stopped; it does not use `SetNextAVTransportURI` and cannot promise gapless. Errors are explicit.

ConnectionManager must belong to the same renderer as AVTransport, including within an embedded-device description. Aède checks GetPositionInfo/TrackURI before advancing: a foreign URI, or a cleared URI after confirmed playing, ends the controller without sending Stop to audio it no longer owns. A renderer that clears its URI at EOF will therefore need a separately validated compatibility profile before queue advancement can work; this conservative refusal avoids taking over another controller's playback.

OpenHome checks Playlist/ProtocolInfo, TracksMax, repeat/shuffle state and existing entries before inserting the ordered queue. An existing queue is refused unless **`--replace`** explicitly authorizes replacement. Repeat or shuffle already active on the device is refused rather than silently changed. The device holds its queue, while Aède stays alive to serve originals and detect playlist edits. A replaced/externally edited queue ends the controller rather than attributing another selection to it. A failed partial insert can leave already inserted entries on the device; Stop is requested and Aède reports failure, without restoring an earlier playlist. UPnP/OpenHome do not change device volume. SlimProto also preserves gain by default; `--device-volume 0..100` explicitly requests a linear digital gain percentage (0 mutes, 100 is unity). A fresh Squeezelite process can start muted; add, for example, `--device-volume 20` to the Aède command and adjust deliberately. This is not a logarithmic loudness or acoustic-volume guarantee.

Devices must advertise a compatible `http-get` MIME entry. This is a format declaration, not a guarantee about channel count, bit depth, decoder limits or firmware behavior. Software mocks verify protocol shapes and boundaries; validate the intended formats on the actual renderer. Discovery is explicit and bounded, uses SSDP M-SEARCH, and never adds background network activity to ordinary catalog commands.

Completed OpenHome entries remain on the device, but their temporary capability URLs expire when Aède exits. Replaying those entries requires a new casting session; they are not a persistent playable device library.

## Google Cast

This initial sender connects from Aède to a selected receiver's Cast V2 TLS endpoint and supplies original-file URLs to Google's Default Media Receiver (`CC1AD845`). It reuses the existing JSON/Tokio/Rustls stack; no new third-party package or unsafe code is introduced. It is an experimental interoperability profile, not Google's official Sender SDK or a certified integration. A real Chromecast/Nest/third-party receiver trial is still required.

Discover endpoints with a separate explicit DNS-SD pass:

```sh
aede devices --bind 192.168.1.10 --protocol googlecast
```

Discovery queries `_googlecast._tcp.local`, requests unicast mDNS responses and accepts complete PTR/SRV/TXT/A associations whose address matches the responding LAN peer. It runs for at most three seconds, processes at most 256 packets/32 endpoints and does not connect to advertised hostnames. Segmented announcements, IPv6, Cast groups, discovery across routed networks and receiver authentication are outside this first discovery profile. If a receiver is absent from the list, its known literal IPv4 endpoint may still be selected explicitly; the operating system's multicast route/firewall must reach the chosen LAN.

Cast certificates are not ordinary public HTTPS certificates. Inspect the chosen endpoint without casting or completing a trusted TLS session:

```sh
aede devices --bind 192.168.1.10 --protocol googlecast --device 192.168.1.20
```

This prints the observed leaf-certificate SHA-256 only. On first use, independently identify the receiver on a trusted LAN, then explicitly supply those **64 hexadecimal digits**, without colons:

```sh
aede cast /path/to/album --protocol googlecast --bind 192.168.1.10 --device 192.168.1.20 --device-certificate SHA256_FROM_THE_VERIFIED_RECEIVER
```

`SHA256_FROM_THE_VERIFIED_RECEIVER` is a placeholder, not a valid pin. The default control port is 8009; an explicit `IP:PORT` endpoint is supported. `--port` and `--device-volume` remain SlimProto-only. Cast preserves the receiver's volume. A changed certificate refuses casting; re-identify the receiver before changing its pin, including after a reset or certificate rotation. Discovery/inspection do not persist trust. Exact leaf pinning and verified TLS handshake signatures authenticate possession of that pinned key; they do not verify Google's manufacturer/device-authentication chain, certificate dates or Web-PKI hostname rules. There is no accept-any-certificate or insecure casting mode. The temporary media stream remains HTTP on the trusted LAN.

An existing application session is refused unless **`--replace`** explicitly authorizes replacement. The finite selection keeps order and repeated occurrences. LOAD/GET_STATUS and heartbeat messages are bounded; the next original starts only after `IDLE`/`FINISHED` for the current media session. Cancellation, interruptions, unknown states and load errors are not successful completion. A changed application, session or supplied media identity ends the controller; it does not Stop a foreign session. Ctrl-C requests Stop for the owned media session and reports missing/failed Stop confirmation. A cancelled or disconnected load without an assigned session may already have started on the receiver; Aède closes its media listener and reports that Stop could not be confirmed. Local file/network operations cannot guarantee that a device has discarded bytes it already buffered.

The conservative original-audio profile accepts FLAC, MP3, Ogg Vorbis/Opus, AAC in MP4 or ADTS, and integer PCM WAV, with known mono/stereo channels and sample rate. Lossless formats require known precision of at most 24 bits and rate at most 96 kHz; lossy formats are limited to 48 kHz. ALAC, AIFF, floating/32-bit WAV, unsupported containers and missing parameters are refused rather than transcoded. This is a profile preflight, not per-receiver codec negotiation: actual model/firmware decoding still needs validation, and LOAD failure stops the queue. No DSP, normalization, quality reduction or listening-history event is performed. Sequential LOAD does not establish gapless or synchronized multiroom. Pause/seek on the receiver may be observed, but CLI live controls, reconnection and a persistent Cast queue remain future work.

The capability/peer-restricted media listener provides GET/HEAD byte ranges and bounded OPTIONS/CORS responses for the receiver's web player. No account credentials, filesystem paths or catalog access are sent to Google or the receiver; only selected media URLs and track titles are supplied. Default Media Receiver availability may involve Google's services; no fully offline operation is promised for this protocol.

References: [Cast receiver architecture](https://developers.google.com/cast/docs/web_receiver), [media messages](https://developers.google.com/cast/docs/media/messages), [supported media](https://developers.google.com/cast/docs/media), [Chromium CastMessage schema](https://github.com/thibauts/node-castv2/blob/master/lib/cast_channel.proto). Google Cast is a trademark of Google LLC.

## LAN and file boundaries

Both `--bind` and the device must use specific private, link-local or loopback IPv4 addresses. Wildcard listeners, public addresses, DNS names and IPv6 are outside this first profile. Device description/control URLs are HTTP, without credentials or fragments. Discovered descriptions must match the SSDP response peer; control URLs and URLBase must keep the same host and port. Redirects, path traversal, DTDs/custom XML entities and oversized descriptions/messages are refused.

Many legacy players cannot use Aède's authenticated HTTPS contract. Their temporary HTTP media listener therefore uses a fresh random session capability, exact peer IP matching and a selection-only route; it provides no general file browser or API. Treat capability URLs as secrets. Plain HTTP and device control are for a trusted LAN, not Internet forwarding. IP matching is an admission restriction, not cryptographic device identity.

Each original is read-only. Source size, precise modification time, regular-file status, and Unix file identity are checked at preparation/opening and during transfer. Changed or link-replaced paths stop the transfer. At most four requests hold transfer slots; headers, timeouts and 32 KiB/two-chunk queues bound memory/work. Byte ranges and HEAD reuse the existing original-media range policy. Filesystem/driver operations can still stall; controller shutdown has a bounded runtime wait, rather than promising to interrupt the operating system. Files and tags are never rewritten.

## Next acceptance and M4 work

Test real Squeezelite/Squeezebox, UPnP/OpenHome and Cast receivers, original FLAC/MP3/WAV, seeking performed by the renderer, pause/resume and disconnect/reconnect behavior. Capture device output under load before claiming gapless or synchronized multiroom. Then add continuous joins, richer transport/metadata controls, a device registry, authenticated API operations and an explicit history-evidence policy. Browsing Aède directly from another DLNA control point additionally needs MediaServer/ContentDirectory; controlling a renderer alone does not provide that server role.

The [roadmap](../design/roadmap.md) distinguishes these delivered profiles from optional later protocols. Primary wire references: [Squeezelite](https://github.com/ralph-irving/squeezelite), [UPnP AV specifications](https://openconnectivity.org/developer/specifications/upnp-resources/upnp/mediaserver4-and-mediarenderer3/) and [OpenHome Playlist service](https://github.com/openhome/ohNet/blob/master/OpenHome/Net/Service/Upnp/OpenHome/Playlist1.xml).

## Community trials

These initial adapters are experimental until device reports establish interoperability. Squeezelite can provide a software endpoint without buying a network streamer; use a deliberate `--device-volume` when its initial digital gain is muted. A software endpoint does not replace physical hardware/output testing.

For a useful report, include Aède's version/commit, OS, player model/firmware or Squeezelite version, chosen protocol, codec/container, sample rate, bit depth and channel count. Describe whether connection, first sound, track transition, renderer-side pause/seek, end-of-queue and Ctrl-C work. Attach the exact error and a sanitized command. Remove session-capability URLs, private names and library paths from screenshots/logs.

Start with one ordinary stereo FLAC, then an album, a duplicate-entry M3U, MP3 and supported integer WAV. Try disconnection during transfer and a changed device playlist. Report elapsed transition behavior; do not describe gapless as measured without an appropriate captured output. Share a synthetic/non-private fixture when reproduction needs a file. Keep a first-format/device failure visible rather than changing several settings at once.

## Phémios UPnP API profile

The additive `network_audio_upnp` capability exposes explicit finite UPnP AVTransport control to a named administrator session (including the managed-local administrator). Legacy administrative tokens, users and auditors are refused in this initial profile. AirPlay is not implemented by this profile.

- `POST /api/admin/v1/network-audio/discover` accepts `{ "bind": "192.168.1.10" }`. The IPv4 address belongs to the Aède host, not the remote control computer. One bounded discovery may run at a time; the response contains `items` with device `name` and validated description `endpoint`.
- `POST /api/admin/v1/network-audio/play` accepts that `bind`, a selected description URL as `device`, and 1–64 `tracks` containing exact catalogue track references. File paths cannot be supplied. Existing source-identity validation precedes the unchanged-original controller; an already active network session is refused, regardless of owner. No takeover or replacement is implicit.
- `GET /api/admin/v1/network-audio` returns only the current exact login's `{id,status}` or `null`. Polling renews its five-second client lease. Another session cannot read or renew it.
- `POST /api/admin/v1/network-audio/stop` accepts `{id}` and only the owning login. `stopping` means a request, not confirmed silence. Worker completion distinguishes `completed`, `stopped`, `failed` and `stop_unconfirmed`.

All network-audio routes recheck the captured administrator login after bounded request-body reception. A logout or rights change during reception cannot read or renew a run, admit discovery or certificate inspection, resolve playback, or dispatch Stop.

Session invalidation/revocation, server shutdown, supervision disposal or missing owner heartbeats requests transport cancellation. The worker retains the existing protocol timeouts and best-effort Stop semantics; an unresponsive device may have buffered audio. The selected media listener closes with the worker. Status `running` describes the controller, not an acknowledged audio frame or listening event. No private listening history is inferred.

Phémios presents this first profile in its output settings, with a complete same-source queue rather than a mixed Local/Server selection. Its local PCM controls, DSP, crossfade, seek, repeat and volume are not remote UPnP controls. A future persistent transport registry must extend those controls and supervised-account admission before they are exposed to other roles. No hardware, Windows or Linux acceptance is established by host software tests alone.

## Shared graphical network-player selector

The additive `network_audio_profiles` capability extends the same named-administrator routes to OpenHome, Google Cast and SlimProto, in addition to legacy UPnP. Existing `{bind}` discovery and playback requests retain their UPnP meaning. Discovery with `{bind,all:true}` also returns OpenHome and Cast rows with explicit `protocol`; duplicate physical endpoints remain distinguishable by protocol. Bounded partial discovery errors are reported separately. SlimProto uses an explicitly supplied literal private IPv4 peer: the player must connect to Aède's selected bind on TCP port 3483 after Play, within the initial one-minute connection window. No automatic SlimProto discovery or existing Lyrion-session takeover is claimed.

Playback optionally accepts `protocol` (`openhome`, `googlecast`, `slimproto`, `upnp`), `replace` (default false) and `certificate_sha256` (Cast only). The existing protocol validators refuse options they cannot honor before resolving media. OpenHome/Cast replacement is explicit; no replacement of another running Aède controller is allowed. Catalog references, 64-occurrence limits, source checks, exact-login ownership and the five-second client lease are shared by every profile.

`POST /api/admin/v1/network-audio/inspect` accepts `{bind,device}` and returns `{sha256}` after observing a Cast leaf certificate without completing a trusted handshake or starting playback. It shares the bounded discovery slot. Inspection is not pairing: the operator must verify the observed fingerprint on a trusted LAN before passing it to Play. Phémios retains this decision only in the current source/login presentation; it does not disable TLS validation or install certificates.

Phémios uses one network-device selector, ordered OpenHome, Cast, SlimProto, UPnP. Dedicated network Play/Stop reuse the existing Aède controllers; local PCM controls do not become remote volume/seek/DSP controls. Original files travel directly from the selected embedded or remote Aède engine to the receiver. These remain experimental profiles pending physical/platform acceptance.
