# Source measurements and FlacCompagnon

**Integrated LUFS** measures the perceptually weighted loudness of a programme using gates that exclude sufficiently quiet material. **Sample peak** is the largest stored PCM sample. **True peak** estimates the reconstructed waveform between samples, which can exceed the sample peak. These answer different questions; neither is a verdict that music sounds better.

## Reuse analysis instead of changing files

```sh
aede analyze /path/to/album --json
aede scan /path/to/music
aede play /path/to/song.flac --normalize track
```

`analyze` runs [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/)'s acoustic analysis on catalogued albums; `--json` also saves an album report. Scan imports supported reports and retains attributed results in conclusions. Imported results can appear in track details and supply track LUFS/true peak for DSP when they still match the current file. A saved report is not a ReplayGain tag-writing pass. The [analysis-import guide](../imported-analyses.md) explains report recognition, timestamps, source attribution and retained data.

In track playback, a valid tag for the requested scope is preferred. If absent, Aède checks current FlacCompagnon track analysis, then a fresh derived cache. The other metadata scope can provide fallback where the requested scope is unavailable and no suitable measurement is ready. Invalid selected tags are reported rather than hidden. Album data must describe the whole ordered programme: individual track reports do not become an album measurement by averaging.

## Learn during listening

Missing loudness is observed from original decoded PCM **before** downmix, rate conversion, gain and tone. No full-selection predecode delays startup. The current gain is fixed; newly learned data applies on a later listen. Measurement publication happens outside the decode loop, after complete decoding and an unchanged file-identity check (path, size and modification time).

An interrupted/skipped/failed track cannot be saved as a complete track measurement. Album capture needs every track of the complete uninterrupted programme in order. Without valid album tags/cache, the current album keeps an unchanged level rather than adopting each track's loudness independently. A file changed during playback invalidates capture. Silence, insufficient measurable programme or an unsupported source layout can yield no usable LUFS; unknown is kept explicit rather than computed from an unlabelled channel guess.

The current derived loudness-cache method is version 4; older derived caches are discarded because earlier boundary/true-peak handling could produce incorrect values. Imported FlacCompagnon analysis remains distinct and is not erased by that cache invalidation. A stale imported result remains evidence but is not silently used as a fresh gain decision.

## Measurement limits

Integrated LUFS is available at high sample rates. The current true-peak meter does not oversample at **192 kHz and above**, so true peak is deliberately unknown there. A sample peak is not substituted and labelled true peak. Finite-source snapshots resolve the delayed interpolation tail in a separate peak-only meter; this appends neither silence to playback nor fake programme content to LUFS gates.

Source measurements describe the file's original programme. Output measurements describe guarded PCM submitted before device conversion; see [Output](output.md). Neither measures the analogue DAC/speakers. FLAC audio MD5 verification is not implemented in the playback path; consult the [integrity guide](../integrity.md) and [check command](../cli/check.md) for the actual file-checking scope rather than treating successful playback as a complete integrity certificate.
