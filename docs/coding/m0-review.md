# M0 catalogue, storage and copy safeguards

M0 includes local metadata/graph construction, persistence/conclusions, queries/personal interchange, diagnostics and copy/conversion. This consolidates the former chronological reviews; regression mechanics remain in declared sibling suites and Git. [Current state](current-state.md) records qualified verification, not a claim that every input/interleaving is covered.

## Native readers and file admission

Readers preserve container boundaries, checked arithmetic and unknown properties rather than guessing unsupported codecs. ID3v2.4 unsynchronisation applies after consuming each encoded frame; v2.3 retains whole-tag handling. Group headers, original-date priority, leading ID3 on FLAC and recording-versus-placement identifier fields remain distinct. Ogg duration scanning uses complete page boundaries rather than matching header-shaped packet bytes; synthetic structure fixtures do not replace real codec tests or whole-file CRC verification.

MP4 audio selection must not be overwritten by video tracks. Extensible WAV uses valid precision rather than padded slot width. Unsupported compressed AIFC does not claim PCM depth/duration; MPEG layer and WavPack hybrid/float properties remain explicit. Native metadata takes priority over stale legacy fallback tags. Invalid/truncated/high-rate descriptors fail or retain unknown facts without overflow.

Path-based metadata, artwork, integrity, lyrics and imported-report readers admit regular files before opening; static FIFOs/special files must not block. Case-insensitive accepted sidecar extensions remain attachable. These checks are not a sandbox against hostile concurrent ancestor replacement.

## Scan and graph identity

Plural artist/original-date diagnostics follow the builder's actual fallback rules. Feature splitting operates on original string byte offsets, preserving Unicode names and the earliest matching marker. Indexed label identifiers do not spill into another name. Late release-group evidence keeps internal/public identities consistent; conflicting identifiers are not silently merged.

Cache reuse requires usable source identity. Unknown/pre-epoch modification precision cannot collapse into a reusable zero stamp. Retained unavailable roots are not evidence of deletion or permission to recreate their folders. Inverse graph memberships must agree with forward references. Duplicate/edition inference requires positive known duration and compatible channels/properties, not matching display labels alone; unknown evidence never advertises recoverable bytes.

## Queries, private interchange and presentation

CLI windows/options are validated before empty-result returns or cached fast paths. JSON/CSV/M3U selections apply the requested window exactly once, and empty machine output stays machine-readable. Valueless flags reject inline values (`--yes=false` is not confirmation); incompatible actions/formats and extra positional arguments are refused. `--output` without a usable format cannot silently succeed.

Literal filters preserve punctuation and quote semantics. Runnable continuation commands remain complete, shell-quoted and untruncated; terminal display width/control escaping stays separate from machine exports. Aggregate totals saturate rather than wrapping. Album integrity summaries distinguish partial/no-check evidence from verified intact results.

Personal listing/removal/relink/merge/exclusion operations retain their owner namespace, including through accepted aliases. Notes preserve trailing newlines and explicit append/source semantics; rules imports reject unrelated media/preference tables. Raw personal imports use the same bounded loader as ordinary stores. Backup parts remain independently recoverable when conclusions or optional stores are unreadable. See [account isolation](account-isolation.md).

## Retained analyses

Attachment and convergence preserve exact source/date identity, prefer applicable current-byte evidence over stale exact paths and keep the newest valid dated result. Incremental/full scanning must not duplicate waiting and attached rows. Unknown source fields and original timestamps survive reuse/export. Imported numeric fields are range-checked rather than narrowed with wrapping casts; malformed supplied fields are refused rather than replaced by defaults. See [M2 analysis/publication](m2-server-review.md#analysis-and-derived-file-publication).

## Copy and conversion

Planning protects originals, detects source/destination aliases and portable collisions, and preserves intentional selected M3U order. Playlist parsing retains legitimate native whitespace/colon filenames without reinterpreting external URI entries as selected local files. Reserved Windows names and trailing dot/space rules are rechecked after Unicode shortening.

Resume probes FFmpeg only when an encode is required; verification/replacement/new outputs retain their stricter tool admission. Ogg stream tags remain mapped and unsupported precision loss is refused rather than silently turning 32-bit or floating audio into lower-depth integer output. Encoder stdout is discarded and stderr fully drained with a 4,096-byte retained tail; interrupted reads retry, failed readers terminate/reap children, and failed outputs never replace the prior destination. [Copy documentation](../copying.md) retains operation-level rules.

## Resource measurements


### Diagnostic graph traversal

Duplicate/other-edition diagnostics previously traversed every relation once per release. They now traverse release-to-release edges once, retaining pair deduplication and the same diagnostics. The repository example can be run with:

```sh
cargo run --offline -p aede-core --release --example doctor_bench
```

Before/after measurement compiled the same harness with `rustc --edition=2024 -C opt-level=3 -C lto=thin`, linking the respective existing release `aede-core` artifacts. This is a matched harness comparison, not a claim to rebuild both full workspaces with independently verified identical compiler flags. It uses the same in-memory synthetic graph and five timed diagnostic runs per size. Construction is outside the timed region; each result is checked empty, with one artist, one track per distinct album and eight relations per album (8,000, 16,000 and 32,000 observed edges). Median times on this Mac:

| Albums | Before | After |
| ---: | ---: | ---: |
| 1,000 | 12.373 ms | 2.502 ms |
| 2,000 | 44.559 ms | 4.925 ms |
| 4,000 | 167.560 ms | 10.613 ms |

The 4,000-album probe improves about 15.8×. This measures that diagnostic workload, not whole-app startup, large real-library memory or target-NAS acceptance.

### Verbose encoder output

The same confined 32 MiB-output probe measured maximum process RSS of 76,840,960 bytes before and 7,274,496 bytes after; elapsed times were 0.78 and 0.76 seconds. Measurement used `/usr/bin/time -l` on the `aede-core` unit-test binary in test/debug profile, running only `copy::transcode::tests::excessive_encoder_diagnostics_are_drained_and_bounded_without_publishing_output` with `--exact` and `--nocapture`. The binary was rebuilt with the same Cargo command/profile for each version; each version had one instrumented run. This calls the real `copy::transcode::convert` API with the fake encoder, not the CLI or entire application. The returned diagnostic is bounded, its final cause survives, the child exits and temporary files are cleaned. This single illustrative output-capture measurement is not an audio-encoding throughput benchmark.


## Verification and limits

The regression suite covers real encoded fixtures plus structural synthetic controls, malformed files, owner isolation, exact output bytes and failure-preserving publication. Existing meaningful tests are retained. Benchmarks above isolate their named workloads; they do not establish full-app startup, peak RSS, arbitrary concurrent filesystems, target-NAS capacity or physical audio output. Full/feature-specific procedures are in [engineering rules](engineering-rules.md) and [current state](current-state.md).
