//! Test-only acceptance through all three public consumers, without Tauri/output devices.
use aede_audio::native::{ApePcm16Decoder, Decoder};
use aede_core::playback::decoder::FileDecoder;
use md5::{Digest, Md5};
use std::{
    fs::{self, File},
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "native-ape-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("NATIVE_APE_FIXTURES").unwrap()).join(name)
}
fn expected(name: &str, frame: u64) -> i16 {
    if name == "ramp.ape" {
        (frame % 30001) as i16 - 15000
    } else {
        0
    }
}
fn floats(name: &str, pcm: &[f32], at: &mut u64) {
    assert!(pcm.len().is_multiple_of(2));
    for frame in pcm.as_chunks::<2>().0 {
        let sample = f32::from(expected(name, *at)) / 32768.;
        assert_eq!(*frame, [sample, -sample], "{name} frame {at}");
        *at += 1;
    }
}
fn check_consumers() {
    for (name, total) in [
        ("ramp.ape", 132300),
        ("silence-3950.ape", 882000),
        ("silence-3970.ape", 882000),
    ] {
        let source = fixture(name);
        let original = fs::read(&source).unwrap();
        let mut native = Decoder::open(&source).unwrap().unwrap();
        let mut at = 0;
        while let Some(pcm) = native.next_chunk().unwrap() {
            floats(name, &pcm, &mut at);
        }
        assert_eq!(at, total);
        let mut aede = FileDecoder::open_native(&source).unwrap();
        let mut pcm = vec![0.; 8192];
        let mut at = 0;
        loop {
            let count = aede.read_frames(&mut pcm).unwrap();
            if count == 0 {
                break;
            }
            floats(name, &pcm[..count * 2], &mut at);
        }
        assert_eq!(at, total);
        let mut fc = flaccompagnon_core::decode::PcmStreamDecoder::open(&source).unwrap();
        let mut at = 0;
        while let Some(pcm) = fc.next_chunk().unwrap() {
            floats(name, &pcm, &mut at);
        }
        assert_eq!(at, total);
        let (image, _) = flaccompagnon_core::spectrogram::analyze_file(
            &source,
            &flaccompagnon_core::spectrogram::AnalysisOptions {
                width: 64,
                height: 32,
                ..Default::default()
            },
            None,
            &|| false,
        )
        .unwrap();
        assert_eq!(image.signal().frames, total);
        let scratch = Scratch::new();
        fs::copy(&source, scratch.0.join("disc.ape")).unwrap();
        fs::write(scratch.0.join("disc.cue"),"FILE disc.ape APE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 00:01:00").unwrap();
        let mut library = phemios_cd::Library::default();
        library.open_image(&scratch.0.join("disc.cue")).unwrap();
        let disc = library.snapshot()[0].disc.as_ref().unwrap().clone();
        let mut at = 0;
        for track in [1, 2] {
            disc.with_stream(
                track,
                || true,
                |reader| {
                    while let Some(chunk) = reader.next_chunk()? {
                        for frame in chunk.frames() {
                            floats(name, &frame, &mut at);
                        }
                    }
                    reader.seek(1)?;
                    assert!(reader.next_chunk()?.is_some());
                    Ok(())
                },
            )
            .unwrap();
        }
        assert_eq!(at, total);
        assert_eq!(fs::read(source).unwrap(), original);
    }
}
#[test]
fn three_consumers_decode_and_render_without_external_tools() {
    if std::env::var_os("NATIVE_APE_CHILD").is_none() {
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "three_consumers_decode_and_render_without_external_tools",
                "--nocapture",
            ])
            .env("PATH", "")
            .env("NATIVE_APE_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    check_consumers();
}
#[test]
fn original_pcm16_seeks_keep_every_sample_across_old_and_modern_frames() {
    for (name, total) in [
        ("ramp.ape", 132300),
        ("silence-3950.ape", 882000),
        ("silence-3970.ape", 882000),
    ] {
        let mut d = ApePcm16Decoder::open(File::open(fixture(name)).unwrap()).unwrap();
        for start in [0, 1, 73727, 73728, 294911, 294912, 589824, total - 1, total]
            .into_iter()
            .filter(|n| *n <= total)
        {
            d.seek(start).unwrap();
            let mut at = start;
            while let Some(pcm) = d.next_chunk().unwrap() {
                for bytes in pcm.as_chunks::<4>().0 {
                    let sample = expected(name, at);
                    assert_eq!(
                        *bytes,
                        [sample.to_le_bytes(), (-sample).to_le_bytes()]
                            .concat()
                            .as_slice()
                    );
                    at += 1;
                }
            }
            assert_eq!(at, total);
        }
    }
}
#[test]
fn old_crc_failures_never_publish_a_recovered_prefix() {
    let scratch = Scratch::new();
    let mut bytes = fs::read(fixture("silence-3970.ape")).unwrap();
    bytes[44] ^= 1;
    let source = scratch.0.join("damaged.ape");
    fs::write(&source, &bytes).unwrap();
    let mut native = Decoder::open(&source).unwrap().unwrap();
    assert!(native.next_chunk().is_err());
    assert!(native.next_chunk().is_err());
    let mut aede = FileDecoder::open_native(&source).unwrap();
    assert!(aede.read_frames(&mut [0.; 1176]).is_err());
    let mut called = false;
    assert!(
        flaccompagnon_core::decode::visit_pcm(
            &source,
            &mut |_| {
                called = true;
                Ok(())
            },
            &|| false
        )
        .is_err()
    );
    assert!(!called);
    fs::write(
        scratch.0.join("disc.cue"),
        "FILE damaged.ape APE\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
    )
    .unwrap();
    let mut library = phemios_cd::Library::default();
    library.open_image(&scratch.0.join("disc.cue")).unwrap();
    let disc = library.snapshot()[0].disc.as_ref().unwrap().clone();
    assert!(
        disc.with_stream(1, || true, |reader| reader.next_chunk().map(|_| ()))
            .is_err()
    );
    assert_eq!(fs::read(source).unwrap(), bytes);
}
#[test]
fn sdk_frame_layout_matches_a_real_nonzero_3970_reference() {
    let source = PathBuf::from(std::env::var_os("NATIVE_APE_REFERENCE").unwrap());
    let mut original = ApePcm16Decoder::open(File::open(&source).unwrap()).unwrap();
    let mut reference = Vec::new();
    while let Some(pcm) = original.next_chunk().unwrap() {
        reference.extend(pcm);
    }
    assert_eq!(reference.len(), 690843 * 4);
    // Independently decoded FFmpeg PCM; SHA-256 and source provenance are in
    // the canonical fixture README. No reference executable runs in this test.
    assert_eq!(
        format!("{:x}", Md5::digest(&reference)),
        "2a89424a9abea373c15f8a1ccb78d892"
    );
    for start in [294911, 294912, 589824, 0, 690842, 690843] {
        original.seek(start).unwrap();
        let mut at = start as usize * 4;
        while let Some(pcm) = original.next_chunk().unwrap() {
            assert_eq!(pcm, reference[at..at + pcm.len()]);
            at += pcm.len();
        }
        assert_eq!(at, reference.len());
    }
    let compare = |pcm: &[f32], at: &mut usize| {
        for sample in pcm {
            let value = i16::from_le_bytes(reference[*at..*at + 2].try_into().unwrap());
            assert_eq!(*sample, f32::from(value) / 32768.);
            *at += 2;
        }
    };
    let mut aede = FileDecoder::open_native(&source).unwrap();
    let mut pcm = [0.; 8192];
    let mut at = 0;
    loop {
        let count = aede.read_frames(&mut pcm).unwrap();
        if count == 0 {
            break;
        }
        compare(&pcm[..count * 2], &mut at);
    }
    assert_eq!(at, reference.len());
    let mut fc = flaccompagnon_core::decode::PcmStreamDecoder::open(&source).unwrap();
    let mut at = 0;
    while let Some(pcm) = fc.next_chunk().unwrap() {
        compare(&pcm, &mut at);
    }
    assert_eq!(at, reference.len());
    let mut library = phemios_cd::Library::default();
    let scratch = Scratch::new();
    fs::copy(&source, scratch.0.join("disc.ape")).unwrap();
    fs::write(
        scratch.0.join("disc.cue"),
        "FILE disc.ape APE\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
    )
    .unwrap();
    assert!(
        library.open_image(&scratch.0.join("disc.cue")).is_err(),
        "a non-integral CD source must not be cropped"
    );
}

#[test]
fn shared_rust_pngs_keep_standard_and_large_plot_geometry() {
    use flaccompagnon_core::spectrogram::{
        AnalysisOptions, ImageLabels, RenderOptions, analyze_file, render_png,
    };
    for (width, height) in [(1200, 627), (1800, 940)] {
        let (data, info) = analyze_file(
            &fixture("ramp.ape"),
            &AnalysisOptions {
                width,
                height,
                ..Default::default()
            },
            None,
            &|| false,
        )
        .unwrap();
        let options = RenderOptions::default();
        let png = render_png(
            &data,
            &ImageLabels {
                generator: "Native audio acceptance".into(),
                title: "Original ramp reference".into(),
                audio_description: flaccompagnon_core::spectrogram::audio_description(&info),
                ..Default::default()
            },
            &options,
        )
        .unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            u32::from_be_bytes(png[16..20].try_into().unwrap()),
            width + options.margins[0] + options.margins[2]
        );
        assert_eq!(
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
            height + options.margins[1] + options.margins[3]
        );
        if let Some(directory) = std::env::var_os("NATIVE_AUDIO_CAPTURE_DIR") {
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join(format!("native-{width}.png")), png).unwrap();
        }
    }
}
