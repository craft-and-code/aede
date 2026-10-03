# M2 JSON catalog measurements

These measurements are a reproducible **synthetic baseline**, not a validation on a large real library or a recommendation to migrate to SQLite. No audio file or external service was used.

## Reproduce

`bash tools/benchmark-catalog.sh` builds the release-mode benchmark example offline, generates a graph using the real `aede-core` builder, saves it with the current JSON writer, then loads it three times in fresh processes through `store::load`. It reports CSV on standard output and deletes each generated catalog after its measurements. Counts can be supplied as arguments, for example `bash tools/benchmark-catalog.sh 10000 50000`; `AEDE_BENCH_RUNS=5` changes the number of fresh load processes. The script needs macOS or Linux `/usr/bin/time` to report peak resident memory. It creates temporary data under `$TMPDIR` or `/tmp`; leave enough disk space for the largest catalog and avoid running it on a memory-constrained production NAS.

Each generated file has thirteen tag fields, a four-minute FLAC property record, one unique recording identifier per track, twelve tracks per album and ten albums per artist. Paths name nonexistent files. The benchmark exercises graph construction, JSON save and JSON load, **not** filesystem walking, tag decoding, a true scan, network fetches, conclusions, user annotations or concurrent clients. The generator and its small-graph invariant test live in `crates/aede-core/examples/catalog_bench.rs`.

## Local run, 2026-09-26

Machine: Apple M2 Pro, 16 GB RAM, macOS; release build. About 1.7 GB of disk space was available before the run. Other applications were active and macOS reported heavy memory pressure and compression. Numbers are therefore useful for scale and regression comparisons on the same machine, not a portable NAS sizing promise. Times are wall-clock milliseconds measured inside the benchmark process; load figures are medians of three fresh processes. Peak RSS is the median reported by `/usr/bin/time -l`, rounded here. macOS compression can make RSS an incomplete picture of total memory pressure.

| Tracks | `catalog.json` | Build | Save | Load median (range) | Load peak RSS median |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 10,000 | 21.6 MB | 287 ms | 162 ms | 245 ms (245–247) | 246 MB |
| 50,000 | 108.9 MB | 1,377 ms | 802 ms | 1,241 ms (1,212–1,249) | 1.20 GB |
| 100,000 | 218.1 MB | 2,613 ms | 3,223 ms | 3,256 ms (2,946–3,641) | 1.83 GB |
| 200,000 | 438.7 MB | 5,978 ms | 9,122 ms | 7,848 ms (7,647–8,387) | 2.44 GB |

At 100,000 tracks, a release server was started against the generated catalog and five loopback GETs were timed with `curl` per route after startup. Approximate medians: `/status` 0.5 ms, `/tracks?limit=50` 0.75 ms, `/tracks?offset=99950&limit=50` 0.8 ms, `/tracks?q=Track%2012&sort=title&limit=50` 126 ms, and `/tracks?q=Track&sort=title&limit=50` 239 ms. These are single-client, warm-server measurements, not a concurrency or cold-start benchmark. The server was stopped and its temporary catalog removed afterwards.

## Decision boundary

The old pre-M2 measurements in [Architecture](../design/architecture.md#when-this-becomes-a-database) used a different generated document and machine conditions; their disk size and memory figures must not be directly compared with this run. The new baseline confirms that JSON load and resident graph memory grow materially with library size. Basic paginated reads remain quick at 100,000 synthetic tracks, while broad filtered/sorted reads cost hundreds of milliseconds. On a small-memory NAS, the startup footprint may matter more than single-request latency.

Do **not** migrate storage on these results alone. The next decision needs a target NAS memory budget and acceptable startup/search times, a repeat under lower memory pressure, and at least one anonymized measurement from a large real catalog if one becomes available. A real scan and concurrent-client load are still unmeasured. If JSON misses those targets, profile the parser and listing paths first, then compare a small SQLite prototype against the same workload while preserving JSON compatibility and user data. Until then JSON remains the supported store; there is no SQLite toggle to implement yet.

## Local evaluation and scan scaling, 2026-10-03

The M0/M0.5/M0.6 follow-up measured targeted changes on this macOS host. Before/after fixtures and result counts were identical. These are synthetic wall-clock medians, with other applications active; they do not measure end-to-end scans of real audio, NAS hardware, cold filesystem caches or concurrent HTTP clients. No timing thresholds were added to tests.

| Workload | Mode | Before | After |
| --- | --- | ---: | ---: |
| 10,000 tracks, `artist:Artist` plus artist sort | release, 3 runs | 288.9 ms | 21.4 ms |
| 10,000 tracks, `genre:Genre` plus artist sort | release, 3 runs | 266.2 ms | 18.1 ms |
| 10,000 annotated tracks, `rating:>=4 played:>=2` plus artist sort | release, 3 runs | 603.5 ms | 7.5 ms |
| Parse 10,000 separate `-title:x` terms | release, 3 runs | 918.9 ms | 2.2 ms |
| Reconcile 2,000 attached notes, identity evidence already primed | debug library, optimized driver, 3 runs | 68.9 ms | 11.8 ms |
| Reconcile 8,000 attached notes, identity evidence already primed | debug library, optimized driver, 3 runs | 1,087.0 ms | 51.9 ms |
| Rescan 4,000 unavailable folders, preserving 4,000 old tracks | debug, 3 runs | 17,871 ms | 183 ms |
| Plan 1,200 albums and 19,200 companions, hot filesystem cache | release, 9 measured runs after 3 warmups | 194.3 ms | 174.5 ms |

Query timings include construction of borrowed credit/genre/personal-data indexes, evaluation and sorting, while catalog construction and parsing the short expression are outside the interval. The owner scope and first-row behavior of legacy personal duplicates are preserved; reassignment of public context references falls back to the current data. Negative-term parsing borrows token suffixes instead of cloning the rest of the expression. Reconciliation indexes reference resolution and identity updates. Scan retention indexes unreadable paths and ancestors rather than comparing every old track with every failure. Companion planning enumerates each album directory once; its 20,400 outputs retain the same relative-path/size/kind hash (`ee65262916462704`). The latter improvement varied around 10–14% on repeated hot-cache measurements.

Reproduce the final workloads with:

```sh
cargo run --offline --release -p aede-core --example catalog_bench -- query 10000
cargo run --offline --release -p aede-core --example catalog_bench -- parse 10000
cargo rustc --offline -p aede-core --example user_reconcile_bench -- -C opt-level=3
target/debug/examples/user_reconcile_bench
cargo run --offline -p aede-core --example scan_retention_bench -- 4000
cargo run --offline --release -p aede-core --example audit_copy_bench
```

The query/reconciliation examples use only in-memory synthetic data. The retention example walks nonexistent synthetic roots without writing music. The copy-planning example creates and removes a disposable tree of small fixture files; setup is outside the timing. Before figures refer to the unindexed implementation from the same review, not the older storage baseline above. JSON startup size and remote query concurrency remain separate measurements.

## M2 HTTP hardening comparison, 2026-10-03

The previous release (`42386df`) and the hardened release served the same 10,000-track catalog generated by `catalog_bench generate`. A temporary version-2 personal store contained one count per track and 500 recent plays, all owned by `local`; every history row had count 3. Requests used a fictitious installation token on loopback. Each route had one warmup followed by seven timed requests over separate connections; responses were checked for 50 items, the expected total and the preserved history count. Both servers shut down normally and their temporary data was removed. No source music, external service or listening device was involved.

| Route, 50-row pages | Before median | After median |
| --- | ---: | ---: |
| `/api/v1/status` | 0.339 ms | 0.264 ms |
| `/api/v1/tracks?limit=50` | 0.451 ms | 0.429 ms |
| `/api/v1/tracks?q=Track&sort=title&limit=50` | 22.750 ms | 22.894 ms |
| `/api/v1/query?q=title%3ATrack&sort=title&limit=50` | 309.244 ms | 31.046 ms |
| `/api/admin/v1/history?limit=50` | 309.048 ms | 269.498 ms |
| `/api/admin/v1/history?offset=450&limit=50` | 311.269 ms | 271.009 ms |

The broad query benefits from computing normalized sort keys once per result and rendering only its requested page. History indexes the owner's counts once and renders only the page, while still reading and validating the current disk catalog/user store under the shared lock; that load remains the dominant cost here. Original track listing is effectively unchanged. Small sub-millisecond differences are measurement noise, not a throughput claim.

These are single-client warm measurements on the same macOS development host, with other applications active. They do not measure NAS capacity, cold startup, concurrent sessions, TLS or audio throughput. The table is an observed comparison rather than a test threshold. To recreate the workload, generate the catalog with `target/release/examples/catalog_bench generate 10000 <temporary-catalog.json>`, seed the personal rows described above, start the release server with `--data <temporary-directory> --port 0`, and measure the listed routes after startup. The existing catalog benchmark script builds the generator offline.
