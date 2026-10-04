# Real-output gapless measurement

An uninterrupted PCM stream is necessary for gapless playback, but it does not establish what a device emits. A slow file open, a decoder stall, a starved output queue, host scheduling or a device restart can still introduce silence. A remote PCM acknowledgement is likewise a client declaration, not proof that a DAC played those frames.

The dependency-free [`tools/gapless.py`](../../tools/gapless.py) prepares a quiet continuous test signal, exercises native local output with bounded optional load, and measures track-join timing in a separately recorded capture. It neither installs software nor starts recording. Only its explicit `run` command starts playback. It uses synthetic files and separate data folders, never a music library or account store.

## Evidence levels

| Evidence | What it establishes | What it does not establish |
| --- | --- | --- |
| Exact whole/split PCM and synthetic analyzer tests | Decoder/session continuity and whether the analyzer detects deliberately inserted gaps/dropped samples | Host scheduling or a physical output |
| Aède consumed-frame, shortage and host-xrun diagnostics | How the native device callback serviced its bounded queue | The final waveform at a DAC |
| Digital device loopback, such as an explicitly selected virtual route | Timing through the captured output/capture software path | A DAC, cable or ADC |
| A cable from a DAC output to an ADC line input | The captured physical output path, including converters and clocks | Every other output, format, load or computer |

A built-in microphone records the room as well as the loudspeaker. It is unsuitable for precise sample-continuity acceptance. A software capture must remain labelled digital loopback even when a multi-output device also sends audio to a loudspeaker.

## Prepare the route and fixture

Use a suitable audio interface with a line-output-to-line-input cable, or an already installed digital loopback route. Aède uses the system default output device; select the intended interface as the default before each trial, and choose its corresponding capture input explicitly. Choose a quiet output level and disable system effects, automatic input gain, noise suppression and unrelated audio. Avoid feedback by disabling live input monitoring. Record the audio host, output/input models, route, selected rates, channel mapping and device sample format in a small record beside the captures. Do not save account names, private music paths or credentials.

On macOS, the following inventory does not play or record audio:

```sh
system_profiler SPAudioDataType -json
ffmpeg -hide_banner -f avfoundation -list_devices true -i ''
```

The FFmpeg inventory normally ends with an input-opening error after listing devices. A list with no audio entries supplies no usable capture route. Do not interpret a successful synthetic test as a substitute for a missing device.

Create the probe, using a new directory for each trial and parameter set:

```sh
python3 tools/gapless.py prepare /tmp/aede-gapless-48k --rate 48000 --flac
```

This writes `whole.wav`, three corresponding WAV/FLAC parts, `split.m3u` and `manifest.json`. Without `--flac`, no FFmpeg invocation is needed. The carrier and distinct timing markers remain below -20 dBFS. The stereo channels have opposite polarity so an incorrect mono sum is detectable. Track boundaries avoid the usual 1024-frame processing alignment, and the split WAV samples concatenate exactly to the whole reference. There is no deliberate silence at a join. These files have no personal metadata.

The default probe lasts approximately nine seconds. Use `--seconds 8` for longer per-track load trials. Repeat at 44.1, 48 and 96 kHz; if the output device cannot use a source rate, record the negotiated conversion. These matching-format joins are the primary gapless contract. A format change that closes and reopens a device is a separate measurement.

## Capture and play

Start recording before playback in a separate terminal or suitable recording application. This macOS example uses a line-input or loopback device index obtained from the inventory above:

```sh
ffmpeg -nostdin -n -hide_banner -f avfoundation -i ':DEVICE_INDEX' \
  -t 20 -ac 2 -c:a pcm_s16le /tmp/aede-gapless-48k/split-idle.wav
```

Replace `DEVICE_INDEX` with the actual input index. Choose a capture duration longer than the full probe plus startup; for three eight-second tracks, use at least thirty-five seconds instead of the example's twenty. Keep the capture route and its native sample rate identical across trials; the analyzer reads the rate from the WAV. The example retains 16-bit PCM for the dependency-free analyzer. An independently saved higher-resolution original capture may also be retained. If converting a capture copy, state any rate conversion; do not align, trim or repair each track separately.

Once capture is running, explicitly start native output:

```sh
python3 tools/gapless.py run /tmp/aede-gapless-48k --binary target/release/aede
```

The runner requires native output; it refuses if that backend has no device rather than falling back to another path. It disables normalization, uses flat tone by default, plays each occurrence once without shuffle, and retains both output and error diagnostics. Each run has a separate `data-*` folder. A checksum identifies the binary. Logs and JSON records refuse replacement.

Repeat the whole signal with `--whole` and capture it as `whole-idle.wav`. Then repeat both under bounded CPU and I/O load, recording `split-load.wav` and `whole-load.wav`:

```sh
python3 tools/gapless.py run /tmp/aede-gapless-48k --cpu-workers 2 --io-load
python3 tools/gapless.py run /tmp/aede-gapless-48k --whole --cpu-workers 2 --io-load
```

At most eight CPU workers are accepted. The I/O worker cycles two 16 MiB temporary files, requests writeback and reads them again; it is not a cold-cache or NAS simulation. Both kinds of worker stop when playback ends or at the timeout, and the runner cleans up only its own temporary load files. The default playback timeout is thirty seconds and can be increased to at most ninety. Choose the worker count from the test host and report it; a particular count does not prescribe a universal performance budget.

Also exercise non-flat tone with, for example, `--bass 6 --treble -3`, using a new prepared directory. Preserve identical settings in each whole/split pair. Use three or more separate idle and loaded trials before drawing conclusions about reproducibility. Native PCM remote clients must be tested through their actual receiving buffer and device with the same probes and capture procedure; local CLI measurements cannot establish remote-client output behavior.

## Analyze the capture

```sh
python3 tools/gapless.py analyze /tmp/aede-gapless-48k/manifest.json \
  /tmp/aede-gapless-48k/split-idle.wav --kind analog \
  --output /tmp/aede-gapless-48k/split-idle.analysis.json
```

Use `--kind digital-loopback` for software capture and `--kind synthetic` for generated files. This classification is explicit and cannot be inferred from a WAV. Analyze the left channel by default; use the corresponding source/capture routing, not the stereo sum. For matching right-channel capture, use `--channel 1 --reference-channel 1`; a mono input carrying the right output instead uses `--reference-channel 1`. Convert a copy to uncompressed 16-bit PCM WAV if the recorder supplies another format. Do not change its rate unintentionally.

The analyzer finds distinct marker templates by normalized correlation. It first estimates a constant capture latency, then locates later markers within a bounded search window. It estimates clock drift from marker intervals **within tracks only**; fitting across joins could hide a gap. After removing that common drift, it reports the delay step between marker pairs before and after each join. A positive step suggests inserted time; a negative step suggests dropped audio. Fixed route latency and fixed gain do not count as a join gap.

The report includes correlation confidence, estimated drift in ppm, a conservative marker-timing uncertainty and the configured delay-step tolerance (default 0.5 ms, with a minimum resolution of one capture frame). Low correlation, missing ending markers, excessive drift, a changed whole reference or malformed input refuse a verdict. Timing uncertainty above that resolution is inconclusive. A join fits the tolerance only when its absolute measured step **plus** uncertainty stays inside the limit; noise never expands the allowed gap. `--window-ms` may be increased for a larger suspected interruption, up to one second. `--search-seconds` extends the initial-latency search up to fifteen seconds.

A result within tolerance means only that the measured marker delays are continuous to the stated uncertainty. It is not a proof of sample-perfect playback and does not exclude a click, a short drop between markers, noise, equal-length silence/substitution, or altered samples with unchanged timing. The report always leaves `hardware_acceptance` false: final acceptance requires route evidence, diagnostics and waveform inspection. For analog captures, compare the whole/split residual and the waveform around each split with one common clock/latency model; never independently realign the two sides of a join. For a digital route, additionally compare frame count and boundary samples at a tolerance appropriate to the selected conversion/dither. Inspect the final burst/markers and the tail so a truncated ending cannot pass merely because earlier joins look correct.

## Acceptance record

Retain the source manifest/checksum, binary checksum/revision, route/settings, idle/loaded whole/split captures, analysis JSON and Aède logs. Record:

- negotiated rate/channels/sample format and any source/output conversion;
- CPU-worker count, bounded I/O load, timeout, operating system and host;
- every join's delay step, confidence and timing uncertainty;
- consumed-frame, missing-frame, shortage-callback and host-xrun counters;
- waveform observations at joins and the final tail, including any limitation;
- repetition count and the precise scope accepted (digital path or physical DAC/ADC path).

An unexplained shortage, host xrun, drift discontinuity, waveform drop/click or missing tail prevents a seamless-playback claim. If failures appear only under load, investigate synchronous file opening/decoding and queue margin using the retained logs; do not increase timing tolerance merely to accept them. Different formats, outputs and machines require their own trials.

## Current evidence

At the 2026-10-04 development checkpoint, read-only CoreAudio and AVFoundation inventories exposed no output or capture devices to the execution context. No real audio device was opened, no audio was played or recorded, and physical gapless acceptance remains pending. The synthetic analyzer regressions prove detection of deliberate gaps and dropped samples, plus refusal of unsuitable captures; they do not establish hardware scheduling or audibility.

See [DSP review](dsp-review.md#deferred-physical-verification-protocol), [playback architecture](../design/playback.md) and [local play](../cli/play.md).
