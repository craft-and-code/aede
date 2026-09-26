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
