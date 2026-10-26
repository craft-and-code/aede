# Spectrograms and decoded audio

A spectrogram shows measured frequency content over time. A cutoff can help investigate a file, but it does not prove a lossy origin: recordings, mastering and earlier filtering can also limit bandwidth. A picture is neither a checksum nor an integrity verdict.

```sh
aede spectrum "$HOME/Music/Example Artist" --dry-run
aede spectrum "$HOME/Music/Example Artist"
aede spectrum --size full --full
```

Without folders, the command covers the catalogue. A named folder limits work to its catalogued files. `--threads` selects concurrent workers; the server's library-generation sequence uses one spectrogram worker. Decode and FFT work still compete for CPU and I/O, so a large collection should be processed deliberately.

## Shared Rust measurements and rendering

The candidate implementation reuses FlacCompagnon's independent Rust spectrogram engine through `flaccompagnon-core`. Decoding, measuring and rendering remain separate. The shared default uses a Hann window, mean channel power and peak pixel aggregation, with calibrated dBFS/bin, decoded-rate frequency limits and labels. Opposed stereo channels are not summed to mono before FFT, so their spectrum does not disappear through phase cancellation. A new render is not claimed to be pixel-identical to the historical FFmpeg filter.

Native PCM formats and uncompressed DSF/DFF DSD64/128 mono/stereo need no FFmpeg executable or external font installation. DSD uses the wideband measurement decoder at 352.8/705.6 kHz PCM, independently of its 88.2 kHz listening profile and listening low-pass. The graph never uses the original one-bit clock as the PCM Nyquist limit.

The candidate mono/stereo Opus path reuses shared libopus decoding without resolving FFmpeg. Unsupported Opus profiles, APE, WavPack, Musepack and Speex retain an optional external decoder. AAC/ALAC and other compressed WAV/AIFF profiles may also use it if native decoding fails. For 32-bit FLAC, Aède first reuses its existing Rust reader; currently unsupported correlated-stereo modes retain the external compatibility decoder. Only this compatibility decoding uses FFmpeg; FFT, levels, labels and PNG encoding remain Rust. The external decoder supplies a bounded, self-describing float WAV stream without forcing catalogue rate/channel hints. Its actual output properties define the image. No FFprobe process is required. Missing FFmpeg affects those selected codecs, not ordinary native files; discovery is lazy and reused within the CLI batch.

Higher-rate or multichannel uncompressed DSD remains outside the first shared native profile; FlacCompagnon's explicit optional compatibility path is separate from native admission. DST-compressed DFF and SACD ISO are not promised by that fallback. The FLAC 32-bit correlated-stereo guard is intentional: the [upstream decoder issue](https://github.com/pdeljanov/Symphonia/issues/585) involves a 33-bit side channel, so changing a version number alone is not evidence that this profile can be safely admitted.

The source is decoded twice with bounded buffers: first to count real frames, then to place FFT windows on the exact timeline. No complete-track PCM buffer or temporary converted audio is created. Memory also includes the bounded FFT and plotted image. Labels use optional source metadata and actual decoded properties; unknown precision stays unknown.

## Images, freshness and publication

Each PNG goes beside the source in `spectrograms/<track stem>.png`. Plot dimensions exclude the shared renderer's labels/margins:

- `--size half` (default): 900×470 plotted pixels.
- `--size full`: 1800×940 plotted pixels, matching the shared detailed preset.

Changing size alone preserves the existing image; use `--full` to redraw deliberately. Encoded PNG size depends on its content and compression.

A small versioned source stamp is stored near the PNG header. Freshness checks read at most eight KiB and compare the renderer profile, source size and full modification timestamp, including nanoseconds. They do not hash the entire music file. Historical unstamped PNGs remain viewable and regenerate once on the next generation pass. Renderer/measurement policy changes must bump the stamp generation. A matching stamp is a freshness heuristic, not authentication of the image or source.

Rendering writes into an isolated temporary directory, then publishes one complete PNG atomically. The previous image survives decoder errors, changed inputs and cooperative cancellation. Destination links and special files are refused before work and publication. The output cannot replace the source path. This is not a filesystem sandbox against an OS process concurrently replacing parent directories.

Cancellation is checked between decode/FFT stages and before publication. An already running OS read or PNG encode finishes before its next check. Delegated server-job interruption retains its existing process/group cancellation policy. Completed pictures remain available; a failed track does not abandon the other selected tracks, but makes the final command/task report failure.

## Delivery boundary

This adapter and its shared DSD decoder are unpublished candidates. The normal dependencies still select published FlacCompagnon/Aède sources. Candidate checks use an isolated composite workspace, without committing local dependency paths or changing stable-source selection. Publish the shared decoder source, publish the compatible FlacCompagnon component, then update Aède's analysis dependency before normal client builds use the new renderer. Fixture checks do not establish physical listening, full real-library codec coverage or target-platform throughput.

The candidate cache generation includes native Opus decoding. Previous-generation images are marked stale rather than being mistaken for newly decoded native results; publication still preserves the old PNG on failure or cancellation.
