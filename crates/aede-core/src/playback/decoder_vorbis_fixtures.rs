//! Temporary variants of the checked-in Vorbis fixture; never user audio.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub fn original() -> Vec<u8> {
    include_bytes!("../../tests/fixtures/track.ogg").to_vec()
}

pub struct TempVorbis {
    path: PathBuf,
}

impl TempVorbis {
    pub fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede_vorbis_{}_{}.audio",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).expect("temporary Vorbis variant");
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempVorbis {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn pages(bytes: &[u8]) -> Vec<&[u8]> {
    let mut pages = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let segments = usize::from(bytes[offset + 26]);
        let body = bytes[offset + 27..offset + 27 + segments]
            .iter()
            .map(|&length| usize::from(length))
            .sum::<usize>();
        let end = offset + 27 + segments + body;
        pages.push(&bytes[offset..end]);
        offset = end;
    }
    pages
}

fn checksum(page: &mut [u8]) {
    page[22..26].fill(0);
    let crc = crate::audit::crc::crc32_ogg(page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
}

pub fn repaged(origin: i64) -> Vec<u8> {
    let source = original();
    let pages = pages(&source);
    assert_eq!(pages.len(), 3);
    let audio = pages[2];
    let segments = usize::from(audio[26]);
    let lacing = &audio[27..27 + segments];
    let body = &audio[27 + segments..];
    let split = lacing
        .iter()
        .enumerate()
        .filter(|(_, length)| **length < 255)
        .nth(1)
        .expect("second audio packet")
        .0
        + 1;
    let first_bytes = lacing[..split]
        .iter()
        .map(|&length| usize::from(length))
        .sum::<usize>();
    let mut result = Vec::new();
    result.extend_from_slice(pages[0]);
    result.extend_from_slice(pages[1]);
    for (sequence, flags, granule, lace, payload) in [
        (
            2u32,
            0u8,
            576 + origin,
            &lacing[..split],
            &body[..first_bytes],
        ),
        (
            3,
            4,
            44_100 + origin,
            &lacing[split..],
            &body[first_bytes..],
        ),
    ] {
        let mut page = audio[..27].to_vec();
        page[5] = flags;
        page[6..14].copy_from_slice(&(granule as u64).to_le_bytes());
        page[18..22].copy_from_slice(&sequence.to_le_bytes());
        page[26] = u8::try_from(lace.len()).expect("page lacing count");
        page.extend_from_slice(lace);
        page.extend_from_slice(payload);
        checksum(&mut page);
        result.extend_from_slice(&page);
    }
    result
}

pub fn final_page(granule: u64, eos: bool) -> Vec<u8> {
    let source = original();
    let pages = pages(&source);
    let mut result = pages[..2].concat();
    let mut final_page = pages[2].to_vec();
    final_page[5] = if eos { 4 } else { 0 };
    final_page[6..14].copy_from_slice(&granule.to_le_bytes());
    checksum(&mut final_page);
    result.extend_from_slice(&final_page);
    result
}
