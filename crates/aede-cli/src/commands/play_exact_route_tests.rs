use super::*;

fn id(value: &str) -> DeviceId {
    DeviceId::new(cpal::ALL_HOSTS[0], value)
}

fn source(bits: u32) -> IntegerPcmFormat {
    IntegerPcmFormat::new(44_100, ChannelLayout::STEREO, bits).unwrap()
}

fn candidate(format: SampleFormat, effective_bits: u32) -> Candidate {
    Candidate {
        advertised: cpal::SupportedStreamConfigRange::new(
            2,
            44_100,
            44_100,
            cpal::SupportedBufferSize::Unknown,
            format,
        ),
        layout: Some(ChannelLayout::STEREO),
        effective_bits: Some(effective_bits),
        actual_rate: Some(44_100),
        transparency: Transparency::DirectUnity,
    }
}

fn refusal(source: IntegerPcmFormat, candidates: &[Candidate]) -> String {
    match plan_candidates(source, candidates) {
        Ok(_) => panic!("invalid route was admitted"),
        Err(error) => error,
    }
}

#[test]
fn a_missing_named_device_never_uses_the_default_output() {
    let requested = id("missing-device");
    let result = choose_device::<u8>(
        Some(&requested),
        |_| None,
        || panic!("named selection must never query a default device"),
    );
    let error = result.unwrap_err();
    assert!(error.contains("requested audio device is unavailable"));
    assert!(error.contains("missing-device"));
}

#[test]
fn a_named_device_keeps_the_explicit_selection() {
    let requested = id("chosen-device");
    assert_eq!(
        choose_device(
            Some(&requested),
            |found| {
                assert_eq!(found, &requested);
                Some(7)
            },
            || panic!("named selection must never query a default device"),
        )
        .unwrap(),
        7
    );
}

#[test]
fn ordinary_implicit_selection_uses_only_the_default_device() {
    assert_eq!(
        choose_device(
            None,
            |_| panic!("no named device was requested"),
            || Some(9)
        )
        .unwrap(),
        9
    );
    assert_eq!(
        choose_device::<u8>(None, |_| panic!("no named device was requested"), || None)
            .unwrap_err(),
        "no default audio output device"
    );
}

#[test]
fn strict_selection_without_a_device_refuses_before_querying_a_default_host() {
    let request = NativeRequest {
        device: None,
        strict: true,
        ..Default::default()
    };
    match resolve_output(&request) {
        Ok(_) => panic!("strict selection admitted an implicit device"),
        Err(error) => assert!(error.contains("explicitly selected native device ID")),
    }
}

#[test]
fn exact_rate_integer_output_wins_over_a_preferred_float_at_another_rate() {
    let integer = candidate(SampleFormat::I16, 16);
    let mut float = candidate(SampleFormat::F32, 24);
    float.advertised = cpal::SupportedStreamConfigRange::new(
        2,
        48_000,
        48_000,
        cpal::SupportedBufferSize::Unknown,
        SampleFormat::F32,
    );
    float.actual_rate = Some(48_000);
    let plan = plan_candidates(source(16), &[float, integer]).unwrap();
    assert_eq!(plan.config.sample_format(), SampleFormat::I16);
    assert_eq!(plan.config.sample_rate(), 44_100);
    assert_eq!(plan.output.sample_rate(), 44_100);
}

#[test]
fn each_exact_software_representation_retains_separate_effective_precision() {
    for format in [
        SampleFormat::I16,
        SampleFormat::I24,
        SampleFormat::I32,
        SampleFormat::F32,
        SampleFormat::F64,
    ] {
        let plan = plan_candidates(source(16), &[candidate(format, 16)]).unwrap();
        assert_eq!(plan.config.sample_format(), format);
        assert_eq!(plan.output.effective_bits(), Some(16));
        assert_eq!(plan.output.layout(), ChannelLayout::STEREO);
    }
    let plan = plan_candidates(source(24), &[candidate(SampleFormat::I32, 24)]).unwrap();
    assert_eq!(plan.output.effective_bits(), Some(24));
    assert_eq!(
        plan.output.representation(),
        ExactSampleRepresentation::Signed32Le
    );
}

#[test]
fn a_32_bit_container_with_16_effective_bits_refuses_a_24_bit_source() {
    assert!(
        refusal(source(24), &[candidate(SampleFormat::I32, 16)])
            .contains("cannot preserve 24-bit source PCM with 16 effective bits")
    );
}

#[test]
fn unknown_effective_precision_is_not_inferred_from_integer_or_float_width() {
    for format in [
        SampleFormat::I16,
        SampleFormat::I24,
        SampleFormat::I32,
        SampleFormat::F32,
        SampleFormat::F64,
    ] {
        let mut output = candidate(format, 16);
        output.effective_bits = None;
        assert!(refusal(source(16), &[output]).contains("known downstream precision"));
    }
}

#[test]
fn channel_count_does_not_replace_known_channel_association() {
    let mut unknown = candidate(SampleFormat::I32, 24);
    unknown.layout = None;
    assert!(refusal(source(24), &[unknown]).contains("channel mapping is unknown"));
    let mut wrong = candidate(SampleFormat::I32, 24);
    wrong.layout = Some(ChannelLayout::MONO);
    assert!(refusal(source(24), &[wrong]).contains("cannot change the source layout"));
}

#[test]
fn advertised_rate_does_not_replace_observed_output_rate() {
    let mut unknown = candidate(SampleFormat::I32, 24);
    unknown.actual_rate = None;
    assert!(refusal(source(24), &[unknown]).contains("effective sample rate is unknown"));
    let mut converted = candidate(SampleFormat::I32, 24);
    converted.actual_rate = Some(48_000);
    assert!(refusal(source(24), &[converted]).contains("cannot change the source rate"));
}

#[test]
fn unknown_or_modifying_routes_are_refused_despite_exact_pcm_capabilities() {
    let mut unknown = candidate(SampleFormat::I32, 24);
    unknown.transparency = Transparency::Unknown;
    assert!(
        refusal(source(24), &[unknown]).contains("transparency and unity system gain are unknown")
    );
    let mut modifying = candidate(SampleFormat::I32, 24);
    modifying.transparency = Transparency::Modifying;
    assert!(refusal(source(24), &[modifying]).contains("mixes or modifies"));
}

#[test]
fn the_planner_never_resamples_downmixes_or_selects_unsigned_output() {
    let mut channels = candidate(SampleFormat::I32, 24);
    channels.advertised = cpal::SupportedStreamConfigRange::new(
        1,
        44_100,
        44_100,
        cpal::SupportedBufferSize::Unknown,
        SampleFormat::I32,
    );
    assert!(refusal(source(24), &[channels]).contains("no supported exact-rate, exact-layout"));
    assert!(
        refusal(source(24), &[candidate(SampleFormat::U32, 32)])
            .contains("no supported exact-rate, exact-layout")
    );
    assert!(refusal(source(24), &[]).contains("no supported exact-rate, exact-layout"));
}

#[test]
fn an_eligible_candidate_can_follow_incompatible_or_unknown_candidates() {
    let mut unknown = candidate(SampleFormat::F32, 24);
    unknown.effective_bits = None;
    let plan = plan_candidates(
        source(24),
        &[
            candidate(SampleFormat::I16, 16),
            unknown,
            candidate(SampleFormat::I24, 24),
        ],
    )
    .unwrap();
    assert_eq!(plan.config.sample_format(), SampleFormat::I24);
}

#[test]
fn cpal_i24_keeps_lower_aligned_container_semantics() {
    for bits in [16, 24] {
        let source = source(bits);
        let plan = plan_candidates(source, &[candidate(SampleFormat::I24, 24)]).unwrap();
        assert_eq!(plan.config.sample_format().sample_size(), 4);
        assert_eq!(plan.output.representation().bytes_per_sample(), 3);
        let value = -1_i32;
        let canonical = value << (24 - bits);
        let native = cpal::I24::from(canonical);
        let mut packed = Vec::new();
        ExactPcmAdapter::new(source, plan.output)
            .unwrap()
            .encode_block(&[value, 1], &mut packed)
            .unwrap();
        assert_eq!(native.inner(), canonical);
        assert_eq!(&packed[..3], &canonical.to_le_bytes()[..3]);
        assert_ne!(canonical, value << (32 - bits));
    }
}

#[test]
fn backend_refusals_explain_actual_pinned_cpal_limits() {
    assert!(backend_refusal("WASAPI", "endpoint").contains("shared WASAPI"));
    assert!(backend_refusal("WASAPI", "endpoint").contains("exclusive"));
    assert!(backend_refusal("CoreAudio", "device-uid").contains("AudioUnit conversion"));
    assert!(backend_refusal("ALSA", "hw:CARD=0,DEV=0").contains("only a direct-route candidate"));
    for name in [
        "default",
        "plughw:CARD=0,DEV=0",
        "dmix",
        "pulse",
        "pipewire",
        "custom",
    ] {
        assert!(backend_refusal("ALSA", name).contains("not eligible"));
    }
    assert!(backend_refusal("FutureHost", "output").contains("no verified"));
}
