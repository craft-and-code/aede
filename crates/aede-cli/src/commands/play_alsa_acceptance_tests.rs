//! Opt-in actual ALSA lifecycle checks, separate from digital-output capture.

use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use aede_core::playback::format::IntegerPcmFormat;
use aede_dsp::ChannelLayout;

use super::AlsaOutput;
use super::acceptance_fixtures::SilentWav;

fn required_integer(name: &str) -> u32 {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("opt-in ALSA acceptance requires {name}"))
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be an unsigned decimal integer"))
}

#[test]
fn direct_linux_alsa_entry_refuses_plugins_before_host_access() {
    let source = IntegerPcmFormat::new(48_000, ChannelLayout::STEREO, 24).unwrap();
    for device in ["default", "plughw:0,0", "dmix", "pipewire", "pulse"] {
        let error = match AlsaOutput::open(source, device) {
            Ok(_) => panic!("strict output unexpectedly opened {device}"),
            Err(error) => error,
        };
        assert!(error.contains("explicit hw:"), "{device}: {error}");
    }
}

#[test]
#[ignore = "requires explicit ALSA device/rate/bits/channels; opens real hardware and sends verified silence"]
fn explicit_linux_alsa_hardware_lifecycle() {
    let device = std::env::var("AEDE_TEST_ALSA_DEVICE")
        .expect("opt-in ALSA acceptance requires AEDE_TEST_ALSA_DEVICE=hw:CARD=...,DEV=...");
    let rate = required_integer("AEDE_TEST_ALSA_RATE");
    let bits = required_integer("AEDE_TEST_ALSA_BITS");
    let channels = required_integer("AEDE_TEST_ALSA_CHANNELS");
    assert!(
        [44_100, 48_000, 88_200, 96_000, 176_400, 192_000].contains(&rate),
        "select one declared acceptance-grid rate"
    );
    assert!(
        [16, 24].contains(&bits),
        "select 16 or 24 valid source bits"
    );
    let layout = match channels {
        1 => ChannelLayout::MONO,
        2 => ChannelLayout::STEREO,
        _ => panic!("AEDE_TEST_ALSA_CHANNELS must be 1 or 2"),
    };
    let source = IntegerPcmFormat::new(rate, layout, bits).unwrap();
    let fixture = SilentWav::new(source);
    let samples = fixture.verified_samples(source);
    // Actual host admission is mandatory: a refusal fails this selected case,
    // and cannot become a software-only success or a fallback device.
    let mut output = AlsaOutput::open(source, &device)
        .unwrap_or_else(|error| panic!("selected ALSA acceptance route refused: {error}"));
    let deadline = Instant::now() + Duration::from_secs(5);
    output
        .pause()
        .expect("retain pause intent before hardware startup");
    output
        .ensure_started()
        .expect("request explicit startup while paused");
    let mut accepted = 0;
    while accepted < samples.len() {
        match output.write_exact(&samples[accepted..]) {
            Ok(count) => {
                assert!(count > 0 && count.is_multiple_of(channels as usize));
                assert!(count <= samples.len() - accepted);
                accepted += count;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "ALSA publication stalled");
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("ALSA publication failed: {error}"),
        }
    }
    while output.producer.snapshot().consumed_frames == 0 {
        output.check().expect("prefill must remain healthy");
        assert!(Instant::now() < deadline, "ALSA hardware prefill stalled");
        thread::sleep(Duration::from_millis(2));
    }
    output.check().expect("held startup must remain healthy");
    assert!(!output.shared.started.load(Ordering::Acquire));
    assert_eq!(output.consumed_frames(), 0, "prefill is not playback");
    output.resume().expect("release startup pause intent");
    while !output.shared.started.load(Ordering::Acquire) {
        output
            .check()
            .expect("explicit startup must remain healthy");
        assert!(Instant::now() < deadline, "ALSA explicit startup stalled");
        thread::sleep(Duration::from_millis(2));
    }
    if output.can_pause {
        output.pause().expect("pause admitted hardware");
        let paused_frames = output.consumed_frames();
        thread::sleep(Duration::from_millis(10));
        output.check().expect("paused hardware must remain healthy");
        assert_eq!(output.consumed_frames(), paused_frames);
        output.resume().expect("resume admitted hardware");
    } else {
        println!("Active hardware pause was not exercised: ALSA reports it unsupported.");
    }
    output.close_input();
    loop {
        let progress = output
            .drain_progress()
            .expect("ALSA drain must remain healthy");
        if progress.drained {
            assert_eq!(progress.consumed_frames, Some(fixture.frames as u64));
            break;
        }
        assert!(Instant::now() < deadline, "ALSA hardware drain stalled");
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(accepted, samples.len());
    assert_eq!(output.consumed_frames(), fixture.frames as u64);
    assert_eq!(
        output
            .write_exact(&samples[..channels as usize])
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::BrokenPipe,
        "closed input cannot publish another frame"
    );
    output
        .finish()
        .expect("report joined host completion errors");
    assert_eq!(std::fs::read(&fixture.path).unwrap(), fixture.bytes);
    println!(
        "ALSA lifecycle passed: device={device}, source={source:?}, output={:?}, hardware_format={}, active_pause_exercised={}, accepted_frames={}, ALSA_reported_frames={}. No digital capture was compared.",
        output.exact_format(),
        output
            .integer_description()
            .expect("known ALSA integer representation"),
        output.can_pause,
        accepted / channels as usize,
        output.consumed_frames(),
    );
}
