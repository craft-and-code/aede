use crate::decode::StreamSource;
use crate::test_support::Wave;
use std::path::Path;

pub(super) struct Source(Wave);

impl Source {
    pub(super) fn new(bytes: &[u8]) -> Self {
        let source = Self(Wave::new(&[], 1));
        std::fs::write(source.path(), bytes).unwrap();
        source
    }

    pub(super) fn path(&self) -> &Path {
        &self.0.0
    }
}

pub(super) fn native_flac() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
    )
    .unwrap()
}

pub(super) fn flac_with_metadata(block_type: u8, body: &[u8], leading_id3: bool) -> Vec<u8> {
    let original = native_flac();
    let mut audio_start = 4;
    loop {
        let header = &original[audio_start..audio_start + 4];
        let length = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
        audio_start += 4 + length;
        if header[0] & 0x80 != 0 {
            break;
        }
    }
    let mut bytes = if leading_id3 {
        b"ID3\x04\0\0\0\0\0\0".to_vec()
    } else {
        Vec::new()
    };
    bytes.extend_from_slice(&original[..42]);
    let stream_info_header = bytes.len() - 38;
    bytes[stream_info_header] &= 0x7f;
    bytes.push(0x80 | block_type);
    bytes.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    bytes.extend_from_slice(body);
    bytes.extend_from_slice(&original[audio_start..]);
    bytes
}

pub(super) fn matroska_with_timestamp_scale(scale: u64) -> Vec<u8> {
    fn element(tag: &[u8], body: &[u8]) -> Vec<u8> {
        assert!(body.len() < 127);
        let mut bytes = tag.to_vec();
        bytes.push(0x80 | body.len() as u8);
        bytes.extend_from_slice(body);
        bytes
    }
    let ebml = element(
        &[0x1a, 0x45, 0xdf, 0xa3],
        &element(&[0x42, 0x82], b"matroska"),
    );
    let info = element(
        &[0x15, 0x49, 0xa9, 0x66],
        &element(&[0x2a, 0xd7, 0xb1], &scale.to_be_bytes()),
    );
    let mut audio = element(&[0xb5], &48_000_f64.to_be_bytes());
    audio.extend(element(&[0x9f], &[1]));
    audio.extend(element(&[0x62, 0x64], &[32]));
    let mut track = element(&[0xd7], &[1]);
    track.extend(element(&[0x73, 0xc5], &[1]));
    track.extend(element(&[0x86], b"A_PCM/FLOAT/IEEE"));
    track.extend(element(&[0xe1], &audio));
    let mut segment = info;
    segment.extend(element(
        &[0x16, 0x54, 0xae, 0x6b],
        &element(&[0xae], &track),
    ));
    let mut bytes = ebml;
    bytes.extend(element(&[0x18, 0x53, 0x80, 0x67], &segment));
    bytes
}

use super::PcmStreamDecoder;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use symphonia::core::{
    audio::AudioBufferRef,
    codecs::{CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult},
    errors::{Error, Result},
    formats::{Cue, FormatOptions, FormatReader, Packet, SeekMode, SeekTo, SeekedTo, Track},
    io::MediaSourceStream,
    meta::Metadata,
};

struct PanicFormatReader {
    inner: Box<dyn FormatReader>,
    calls: Arc<AtomicUsize>,
}

impl FormatReader for PanicFormatReader {
    fn try_new(_: MediaSourceStream, _: &FormatOptions) -> Result<Self> {
        Err(Error::Unsupported(
            "test wrapper is installed after probing",
        ))
    }

    fn cues(&self) -> &[Cue] {
        self.inner.cues()
    }

    fn metadata(&mut self) -> Metadata<'_> {
        self.inner.metadata()
    }

    fn seek(&mut self, mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        self.inner.seek(mode, to)
    }

    fn tracks(&self) -> &[Track] {
        self.inner.tracks()
    }

    fn default_track(&self) -> Option<&Track> {
        self.inner.default_track()
    }

    fn next_packet(&mut self) -> Result<Packet> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            panic!("controlled packet-reader panic");
        }
        self.inner.next_packet()
    }

    fn into_inner(self: Box<Self>) -> MediaSourceStream {
        self.inner.into_inner()
    }
}

struct PanicDecoder {
    inner: Box<dyn Decoder>,
    calls: Arc<AtomicUsize>,
}

impl Decoder for PanicDecoder {
    fn try_new(_: &CodecParameters, _: &DecoderOptions) -> Result<Self> {
        Err(Error::Unsupported(
            "test wrapper is installed after probing",
        ))
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[]
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn codec_params(&self) -> &CodecParameters {
        self.inner.codec_params()
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            panic!("controlled codec panic");
        }
        self.inner.decode(packet)
    }

    fn finalize(&mut self) -> FinalizeResult {
        self.inner.finalize()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.inner.last_decoded()
    }
}

pub(super) fn panic_on_next_packet(
    mut stream: PcmStreamDecoder,
) -> (PcmStreamDecoder, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let StreamSource::Pcm(mut pcm) = stream.source else {
        panic!("PCM fixture");
    };
    pcm.format = Box::new(PanicFormatReader {
        inner: pcm.format,
        calls: Arc::clone(&calls),
    });
    stream.source = StreamSource::Pcm(pcm);
    (stream, calls)
}

pub(super) fn panic_on_next_decode(
    mut stream: PcmStreamDecoder,
) -> (PcmStreamDecoder, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let StreamSource::Pcm(mut pcm) = stream.source else {
        panic!("PCM fixture");
    };
    pcm.decoder = Box::new(PanicDecoder {
        inner: pcm.decoder,
        calls: Arc::clone(&calls),
    });
    stream.source = StreamSource::Pcm(pcm);
    (stream, calls)
}
