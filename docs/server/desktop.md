# Parent-owned desktop engine

The ordinary server remains independent. An explicitly launched desktop parent may set `AEDE_DESKTOP_LEASE=1` and run `aede serve --port=0 --data=<isolated application store>`. This profile refuses TLS, fixed ports and non-loopback listeners. It neither discovers nor changes another installation.

The parent supplies one UTF-8 JSON line on stdin, bounded to 2048 bytes, with exactly `username` and `password`. A new isolated store bootstraps its administrator and an empty catalog. A retained store must authenticate the same existing enabled administrator; corrupt stores and mismatched credentials are refused, never reset. The parent owns secret generation and protected storage. Credentials are not arguments or readiness output.

Readiness is one JSON line: `{"desktopProtocol":1,"address":"http://127.0.0.1:<assigned port>"}`. The parent retains stdin after bootstrap. EOF triggers the same graceful shutdown path as a termination signal; accepted jobs keep their normal drain semantics. A closed or crashed parent therefore releases its child without a global process search. Delegated job children remove the desktop lease environment so they cannot accidentally bootstrap or wait for a second owner.

Administrative JSON scans additionally accept `unavailable_folders`, with the same count, absolute-path and byte bounds as `folders`. These explicit roots skip traversal and retain previously catalogued files, even if an unmounted volume leaves an accessible empty mountpoint. Exclusions retain precedence. A subsequent scan without this declaration evaluates the mounted filesystem normally. The desktop host supplies OS mount evidence; Aède does not guess that every empty root is offline, mount network shares, or clear unavailable media. CLI delegated scan handling accepts this information only in the existing owned job profile.

The public `serve_with_options` Rust API remains unchanged. `serve_with_owner_lease` accepts an optional atomic owner-closed flag in addition to the same options.
