//! Attach the admitted MP4 edit to packets; preserve compressed decoder pre-roll.
use super::{aac_config::FRAMES, aac_mp4::Plan};
use symphonia::core::{
    errors::{Error, Result},
    formats::{Cue, FormatOptions, FormatReader, Packet, SeekMode, SeekTo, SeekedTo, Track},
    io::MediaSourceStream,
    meta::Metadata,
};

pub(super) struct EditedReader {
    inner: Box<dyn FormatReader>,
    tracks: Vec<Track>,
    selected: u32,
    plan: Plan,
    read: u64,
    failed: bool,
}
impl EditedReader {
    pub(super) fn new(
        inner: Box<dyn FormatReader>,
        selected: u32,
        params: symphonia::core::codecs::CodecParameters,
        plan: Plan,
    ) -> Result<Self> {
        if inner.tracks().len() > 32 {
            return Err(Error::Unsupported("AAC track count exceeds native profile"));
        }
        let mut tracks = inner.tracks().to_vec();
        let track = tracks
            .iter_mut()
            .find(|t| t.id == selected)
            .ok_or(Error::DecodeError("AAC selected track disappeared"))?;
        track.codec_params = params;
        Ok(Self {
            inner,
            tracks,
            selected,
            plan,
            read: 0,
            failed: false,
        })
    }
    fn selected_packet(&mut self, packet: Packet) -> Result<Packet> {
        let at = self
            .read
            .checked_mul(FRAMES)
            .ok_or(Error::DecodeError("AAC packet clock overflow"))?;
        let duration = (self.plan.media_end.saturating_sub(at)).min(FRAMES);
        if self.read >= self.plan.packets
            || packet.ts() != at
            || packet.dur() != duration
            || packet.trim_start != 0
            || packet.trim_end != 0
            || packet.data.is_empty()
            || packet.data.len() > 65536
        {
            return Err(Error::DecodeError(
                "AAC packets disagree with the admitted MP4 timeline",
            ));
        }
        let skip = self.plan.delay.saturating_sub(at).min(FRAMES);
        let end = self.plan.media_end.saturating_sub(at).min(FRAMES);
        let valid = end
            .checked_sub(skip)
            .ok_or(Error::DecodeError("invalid AAC edit interval"))?;
        self.read += 1;
        Ok(Packet::new_trimmed_from_boxed_slice(
            self.selected,
            at.saturating_sub(self.plan.delay),
            valid,
            skip as u32,
            (FRAMES - end) as u32,
            packet.data,
        ))
    }
}
impl FormatReader for EditedReader {
    fn try_new(_: MediaSourceStream, _: &FormatOptions) -> Result<Self> {
        Err(Error::Unsupported("use the shared file probe"))
    }
    fn cues(&self) -> &[Cue] {
        self.inner.cues()
    }
    fn metadata(&mut self) -> Metadata<'_> {
        self.inner.metadata()
    }
    fn tracks(&self) -> &[Track] {
        &self.tracks
    }
    fn default_track(&self) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == self.selected)
    }
    fn seek(&mut self, _: SeekMode, _: SeekTo) -> Result<SeekedTo> {
        Err(Error::Unsupported(
            "AAC seeks require reopening and progressive pre-roll",
        ))
    }
    fn into_inner(self: Box<Self>) -> MediaSourceStream {
        self.inner.into_inner()
    }
    fn next_packet(&mut self) -> Result<Packet> {
        if self.failed {
            return Err(Error::DecodeError("AAC packet reader is retired"));
        }
        let result = match self.inner.next_packet() {
            Ok(packet) if packet.track_id() == self.selected => self.selected_packet(packet),
            Ok(packet) => Ok(packet),
            Err(Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof
                    && self.read != self.plan.packets =>
            {
                Err(Error::DecodeError("incomplete AAC MP4 packet stream"))
            }
            Err(e) => Err(e),
        };
        if result.as_ref().err().is_some_and(
            |e| !matches!(e,Error::IoError(io) if io.kind()==std::io::ErrorKind::UnexpectedEof),
        ) {
            self.failed = true;
        }
        result
    }
}
