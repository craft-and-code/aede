# Native PNG spectrograms

Aede uses the same Rust computation and PNG renderer as FlacCompagnon, through
its tagged flaccompagnon-core dependency and the independent
flaccompagnon-spectrogram crate included in that release. No sibling checkout,
Python process, installed font or external image renderer is required.

## Generate images

```sh
aede spectrum
aede spectrum ~/Music/Album --dry-run
aede spectrum ~/Music/Album --size standard
aede spectrum ~/Music/Album --size full --full
```

The default green plot is 1200×627; full is 1800×940. These are plotted areas;
the frame adds the application signature, title, decoded audio properties,
time/frequency axes and a level color bar. The legacy --size half spelling is
accepted as an alias for standard. Every image is a lossless PNG.

The level scale is fixed at −120…0 dBFS/bin, with no automatic per-track gain.
Channels are transformed separately before their powers are combined, preserving
opposite-polarity content. PNG metadata record transform/pooling settings. A
spectral cutoff remains an observation, not a proof of lossy provenance.

## Dependencies and publication

Native PCM and the admitted uncompressed DSF/DFF DSD64/128 mono/stereo profile
need no FFmpeg executable in the integrated candidate. DSD measurement uses the
352.8/705.6-kHz wideband PCM profile, independently of listening filters.
WavPack, Musepack, Speex and modern APE use the shared native reader within its
[explicit profiles and limits](design/native-additional-codecs.md). Unsupported
native configurations retain optional compatibility decoding. A missing decoder
fails the affected track while other selected tracks continue.

Images remain in spectrograms/ beside the source audio. A private temporary
output is published only after successful rendering. Existing images survive
failure; links/special destinations are refused before work and publication.
This is the existing publication policy, not a filesystem sandbox against
hostile concurrent parent-directory replacement. Original audio stays read-only.

Without a path the command considers the catalog. --threads controls concurrent
tracks; --dry-run lists work before decoding. Existing PNGs are skipped when
their versioned source stamp is current. The stamp compares source size and the
full modification timestamp, including nanoseconds; historical unstamped images
regenerate once. Changing size alone keeps a current image; --full redraws it.
Measurement/render policy versions invalidate earlier cached results.

The shared crate exposes dimensions, windows, overlap, frequency limits/scales,
channel selection, pooling, custom palettes, dB bounds, labels, fonts, margins,
axes/legend/grid and physical annotations. Aede currently selects shared green
defaults. Its CLI exposes size, rather than promising unsupported style flags.

The server still serves image/png through its existing authenticated routes.
Phémios can display the new dimensions without a protocol change; rebuild its
embedded engine after publishing the corresponding Aede version.

The source is decoded in two bounded passes; no complete-track PCM allocation
or converted temporary audio is created. Source changes, native data failures
and cooperative cancellation preserve the previous complete image. An already
running OS read or PNG encoding cannot be forcibly interrupted. Once native
admission succeeds, a later data failure cannot recover a shorter prefix through
an external decoder. The output cannot replace its source audio.

The shared decoder and adapters remain source candidates pending coordinated
Aède/FlacCompagnon publication. Published pins are retained; compatible local
source tests do not certify every profile, native platform or physical output.
