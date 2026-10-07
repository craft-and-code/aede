//! Mandatory software acceptance; the selected sink is not a hardware device.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};

use aede_core::playback::exact_output::{ExactPcmAdapter, ExactSampleRepresentation};
use aede_dsp::ChannelLayout;

use super::exact_driver_tests::{EligibleSink, drive};
use super::*;

const RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];
const FRAMES: usize = 4233;

#[derive(Clone, Copy, Debug)]
struct SourceCase {
    bits: u32,
    channels: u16,
    rate: u32,
}

impl SourceCase {
    fn name(self) -> String {
        format!("pcm-{}-{}-{}", self.bits, self.channels, self.rate)
    }

    fn format(self) -> IntegerPcmFormat {
        IntegerPcmFormat::new(
            self.rate,
            if self.channels == 1 {
                ChannelLayout::MONO
            } else {
                ChannelLayout::STEREO
            },
            self.bits,
        )
        .unwrap()
    }

    fn samples(self) -> Vec<i32> {
        let minimum = -(1 << (self.bits - 1));
        let maximum = (1 << (self.bits - 1)) - 1;
        let pattern = [
            0,
            minimum,
            maximum,
            -1,
            1,
            -2,
            2,
            -257,
            256,
            -256,
            255,
            17,
            -19,
            -1023,
            1024,
            minimum + 1,
            maximum - 1,
        ];
        let mut samples = Vec::with_capacity(FRAMES * usize::from(self.channels));
        for frame in 0..FRAMES {
            for channel in 0..self.channels {
                samples.push(if frame < 16 || frame.is_multiple_of(257) {
                    0
                } else if frame == 32 {
                    if channel == 0 { maximum } else { 0 }
                } else if frame == 33 {
                    if channel == 1 { minimum } else { 0 }
                } else {
                    let index = if channel == 0 { frame } else { frame * 7 + 5 };
                    pattern[index % pattern.len()]
                });
            }
        }
        samples
    }

    fn outputs(self) -> &'static [ExactSampleRepresentation] {
        use ExactSampleRepresentation::*;
        if self.bits == 16 {
            &[Signed16Le, PackedSigned24Le, Signed32Le]
        } else {
            &[PackedSigned24Le, Signed32Le]
        }
    }
}

fn cases() -> impl Iterator<Item = SourceCase> {
    [16, 24].into_iter().flat_map(|bits| {
        [1, 2].into_iter().flat_map(move |channels| {
            RATES.into_iter().map(move |rate| SourceCase {
                bits,
                channels,
                rate,
            })
        })
    })
}

struct SourceLibrary(PathBuf);

impl SourceLibrary {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede_exact_acceptance_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn flac(&self, case: SourceCase) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/bit-perfect")
            .join(format!("{}.flac", case.name()))
    }

    fn wav(&self, case: SourceCase) -> PathBuf {
        let path = self.0.join(format!("{}.wav", case.name()));
        let width = (case.bits / 8) as usize;
        let samples = case.samples();
        let payload = samples.len() * width;
        let padding = payload % 2;
        let alignment = case.channels * width as u16;
        let mut bytes = Vec::with_capacity(44 + payload + padding);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&((36 + payload + padding) as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&case.channels.to_le_bytes());
        bytes.extend_from_slice(&case.rate.to_le_bytes());
        bytes.extend_from_slice(&(case.rate * u32::from(alignment)).to_le_bytes());
        bytes.extend_from_slice(&alignment.to_le_bytes());
        bytes.extend_from_slice(&(case.bits as u16).to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(payload as u32).to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes()[..width]);
        }
        bytes.resize(bytes.len() + padding, 0);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for SourceLibrary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct RepresentationSink {
    transport: EligibleSink,
    representation: ExactSampleRepresentation,
    adapter: Option<ExactPcmAdapter>,
    encoded: Vec<u8>,
    block: Vec<u8>,
}

impl RepresentationSink {
    fn new(representation: ExactSampleRepresentation, maximum_frames: usize) -> Self {
        Self {
            transport: EligibleSink::with_maximum_frames(maximum_frames),
            representation,
            adapter: None,
            encoded: Vec::new(),
            block: Vec::new(),
        }
    }

    fn canonical_output(&self, source_bits: u32) -> Vec<i32> {
        let width = self.representation.bytes_per_sample();
        assert!(self.encoded.len().is_multiple_of(width));
        self.encoded
            .chunks_exact(width)
            .map(|bytes| {
                let (sample, container_bits) = match self.representation {
                    ExactSampleRepresentation::Signed16Le => {
                        (i32::from(i16::from_le_bytes([bytes[0], bytes[1]])), 16)
                    }
                    ExactSampleRepresentation::PackedSigned24Le => (
                        i32::from_le_bytes([
                            bytes[0],
                            bytes[1],
                            bytes[2],
                            if bytes[2] & 0x80 == 0 { 0 } else { 0xff },
                        ]),
                        24,
                    ),
                    ExactSampleRepresentation::Signed32Le => (
                        i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                        32,
                    ),
                    _ => panic!("the integrated integer acceptance does not certify float routes"),
                };
                let shift = container_bits - source_bits;
                let original = sample >> shift;
                assert_eq!(
                    sample,
                    original << shift,
                    "widening must add only zero bits"
                );
                original
            })
            .collect()
    }
}

impl Write for RepresentationSink {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        panic!("exact playback cannot enter the processed sink")
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for RepresentationSink {
    fn requires_exact(&self) -> bool {
        true
    }

    fn consumed_frames(&self) -> Option<u64> {
        self.transport.consumed_frames()
    }

    fn write_exact(&mut self, samples: &[i32], bytes: &[u8], offset: usize) -> io::Result<usize> {
        let accepted = self.transport.write_exact(samples, bytes, offset)?;
        self.adapter
            .unwrap()
            .encode_block(
                &samples[offset / 4..(offset + accepted) / 4],
                &mut self.block,
            )
            .map_err(io::Error::other)?;
        self.encoded.extend_from_slice(&self.block);
        Ok(accepted)
    }

    fn pause(&self) -> Res {
        self.transport.pause()
    }

    fn resume(&self) -> Res {
        self.transport.resume()
    }
}

impl SessionOutput for RepresentationSink {
    fn exact_needs_reopen(&mut self, format: IntegerPcmFormat) -> Result<bool, Box<dyn Error>> {
        self.transport.exact_needs_reopen(format)
    }

    fn prepare_exact(
        &mut self,
        format: IntegerPcmFormat,
    ) -> Result<ExactOutputFormat, Box<dyn Error>> {
        self.transport.prepare_exact(format)?;
        let route = ExactOutputFormat::new(
            format.sample_rate(),
            format.layout(),
            self.representation,
            Some((self.representation.bytes_per_sample() * 8) as u32),
        )?;
        self.adapter = Some(ExactPcmAdapter::new(format, route)?);
        Ok(route)
    }

    fn needs_reopen(&mut self, _: PcmFormat) -> Result<bool, Box<dyn Error>> {
        panic!("exact playback cannot negotiate a processed route")
    }

    fn prepare(&mut self, _: PcmFormat) -> Result<PcmFormat, Box<dyn Error>> {
        panic!("exact playback cannot prepare a processed route")
    }

    fn ensure_started(&mut self) -> Res {
        self.transport.ensure_started()
    }

    fn close_input(&mut self) {
        self.transport.close_input();
    }

    fn drain_progress(&mut self) -> Result<output::DrainProgress, Box<dyn Error>> {
        self.transport.drain_progress()
    }

    fn host_tail(&self) -> Duration {
        self.transport.host_tail()
    }

    fn finish(&mut self) -> Res {
        self.transport.finish()
    }

    fn abort(&mut self) -> Res {
        self.transport.abort()
    }
}

#[test]
fn flac_wav_rate_grid_preserves_exact_repeated_joins_at_every_integer_output_width() {
    let library = SourceLibrary::new();
    for (index, case) in cases().enumerate() {
        let flac = library.flac(case);
        let wav = library.wav(case);
        let paths = [flac.clone(), wav.clone(), flac];
        let snapshots: Vec<_> = paths
            .iter()
            .map(std::fs::read)
            .collect::<Result<_, _>>()
            .unwrap();
        let expected = case.samples().repeat(3);
        for &representation in case.outputs() {
            let mut sink = RepresentationSink::new(representation, [1, 37, 1021][index % 3]);
            let (result, histories) = drive(&paths, &mut sink, Default::default());
            result.unwrap_or_else(|error| panic!("{case:?}/{representation:?}: {error}"));
            assert_eq!(sink.transport.source, Some(case.format()));
            assert_eq!(
                sink.transport.samples, expected,
                "{case:?}/{representation:?}"
            );
            assert_eq!(
                sink.canonical_output(case.bits),
                expected,
                "{case:?}/{representation:?}"
            );
            assert_eq!(
                sink.transport.opens, 1,
                "compatible containers share output"
            );
            assert_eq!(sink.transport.finished, 1);
            assert!(!sink.transport.aborted);
            assert_eq!(histories.len(), 3);
            for (history, path) in histories.iter().zip(&paths) {
                assert_eq!(history.path, *path);
                assert!(history.completed);
                assert_eq!(
                    history.played_ms,
                    FRAMES as u64 * 1000 / u64::from(case.rate)
                );
            }
            for (path, original) in paths.iter().zip(&snapshots) {
                assert_eq!(
                    std::fs::read(path).unwrap(),
                    *original,
                    "source is read-only"
                );
            }
        }
    }
}

#[test]
fn every_source_rate_and_layout_seeks_to_exact_frames_without_claiming_whole_source_completion() {
    let library = SourceLibrary::new();
    for case in cases() {
        let expected = case.samples();
        let skipped = case.rate as usize / 1000;
        for path in [library.flac(case), library.wav(case)] {
            let original = std::fs::read(&path).unwrap();
            let representation = case.outputs()[0];
            let mut sink = RepresentationSink::new(representation, 19);
            let (result, histories) = drive(
                std::slice::from_ref(&path),
                &mut sink,
                crate::args::PlaybackOptions {
                    seek_ms: 1,
                    ..Default::default()
                },
            );
            result.unwrap_or_else(|error| panic!("{case:?}/{path:?}: {error}"));
            let suffix = &expected[skipped * usize::from(case.channels)..];
            assert_eq!(sink.transport.samples, suffix, "{case:?}");
            assert_eq!(sink.canonical_output(case.bits), suffix, "{case:?}");
            assert_eq!(histories.len(), 1);
            assert_eq!(histories[0].path, path);
            assert!(!histories[0].completed);
            assert_eq!(
                histories[0].played_ms,
                (FRAMES - skipped) as u64 * 1000 / u64::from(case.rate)
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }
}

#[test]
fn high_resolution_md5_failures_keep_prior_complete_history_and_refuse_the_next_source() {
    let library = SourceLibrary::new();
    for rate in RATES {
        let case = SourceCase {
            bits: 24,
            channels: 2,
            rate,
        };
        let good = library.flac(case);
        let original = std::fs::read(&good).unwrap();
        let mut invalid = original.clone();
        assert_eq!(&invalid[..4], b"fLaC");
        assert_eq!(invalid[4] & 0x7f, 0);
        assert_eq!(&invalid[5..8], &[0, 0, 34]);
        assert!(invalid[26..42].iter().any(|&byte| byte != 0));
        invalid[26] ^= 1;
        let bad = library.0.join(format!("bad-{}.flac", case.name()));
        std::fs::write(&bad, &invalid).unwrap();
        let later = library.wav(case);
        let paths = [good.clone(), bad.clone(), later];
        let mut sink = RepresentationSink::new(ExactSampleRepresentation::PackedSigned24Le, 23);
        sink.transport.consume_writes = true;
        let (result, histories) = drive(&paths, &mut sink, Default::default());
        assert!(result.unwrap_err().to_string().contains("MD5"), "{case:?}");
        assert!(sink.transport.aborted);
        let expected = case.samples().repeat(2);
        assert!(sink.transport.samples.len() > expected.len() / 2);
        assert!(sink.transport.samples.len() <= expected.len());
        assert_eq!(
            sink.transport.samples,
            expected[..sink.transport.samples.len()]
        );
        assert_eq!(sink.canonical_output(case.bits), sink.transport.samples);
        assert_eq!(histories.len(), 2, "the later source is not played");
        assert_eq!(histories[0].path, good);
        assert!(histories[0].completed);
        assert_eq!(
            histories[0].played_ms,
            FRAMES as u64 * 1000 / u64::from(rate)
        );
        assert_eq!(histories[1].path, bad);
        assert!(!histories[1].completed);
        let consumed_bad = sink.transport.samples.len() / 2 - FRAMES;
        assert_eq!(
            histories[1].played_ms,
            consumed_bad as u64 * 1000 / u64::from(rate)
        );
        assert_eq!(std::fs::read(&good).unwrap(), original);
        assert_eq!(std::fs::read(&bad).unwrap(), invalid);
    }
}
