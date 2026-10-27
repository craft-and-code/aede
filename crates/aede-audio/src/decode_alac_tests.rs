use super::*;

fn source() -> CodecParameters {
    let mut cookie = vec![0; 24];
    cookie[..4].copy_from_slice(&4096u32.to_be_bytes());
    cookie[5] = 24;
    cookie[9] = 2;
    cookie[20..].copy_from_slice(&96000u32.to_be_bytes());
    let mut input = CodecParameters::new();
    input.codec = CODEC_TYPE_ALAC;
    input.sample_rate = Some(48000);
    input.n_frames = Some(48013);
    input.extra_data = Some(cookie.into_boxed_slice());
    input
}

#[test]
fn missing_mp4_layout_uses_codec_properties_without_changing_timeline_or_cookie() {
    let input = source();
    let resolved = parameters(&input).unwrap();
    assert_eq!(resolved.sample_rate, Some(96000));
    assert_eq!(resolved.channels.unwrap().count(), 2);
    assert_eq!(resolved.bits_per_sample, Some(24));
    assert_eq!(resolved.n_frames, input.n_frames);
    assert_eq!(resolved.extra_data, input.extra_data);
}

#[test]
fn malformed_alac_configuration_cannot_allocate_or_guess_a_layout() {
    for length in 0..24 {
        let mut input = source();
        input.extra_data = Some(vec![0; length].into_boxed_slice());
        assert!(parameters(&input).is_err());
    }
    for (offset, value) in [(4, 1), (5, 0), (9, 0), (9, 6)] {
        let mut input = source();
        input.extra_data.as_mut().unwrap()[offset] = value;
        assert!(parameters(&input).is_err());
    }
    for range in [0..4, 20..24] {
        let mut input = source();
        input.extra_data.as_mut().unwrap()[range].fill(0);
        assert!(parameters(&input).is_err());
    }
    let mut input = source();
    input.extra_data.as_mut().unwrap()[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(parameters(&input).is_err());
}

#[test]
fn optional_layout_must_agree_and_existing_demux_layouts_are_preserved() {
    let mut input = source();
    let mut cookie = input.extra_data.take().unwrap().into_vec();
    cookie.extend_from_slice(&24u32.to_be_bytes());
    cookie.extend_from_slice(b"chan");
    cookie.extend_from_slice(&0u32.to_be_bytes());
    cookie.extend_from_slice(&((101u32 << 16) | 2).to_be_bytes());
    cookie.extend_from_slice(&[0; 8]);
    input.extra_data = Some(cookie.into_boxed_slice());
    assert!(parameters(&input).is_ok());
    input.extra_data.as_mut().unwrap()[39] = 1;
    assert!(parameters(&input).is_err());
    let mut known = source();
    known.channels = Some(Channels::FRONT_LEFT | Channels::FRONT_RIGHT);
    assert_eq!(parameters(&known).unwrap().sample_rate, known.sample_rate);
}

#[test]
fn header_only_inspection_keeps_unqualified_and_partial_properties_separate() {
    let mut input = source();
    let cookie = input.extra_data.as_mut().unwrap();
    cookie[..4].fill(0);
    cookie[9] = 6;
    let metadata = read_alac_configuration(cookie).unwrap();
    assert_eq!(
        (metadata.channels, metadata.bit_depth, metadata.sample_rate),
        (6, 24, Some(96000))
    );
    let partial = read_alac_configuration(&cookie[..10]).unwrap();
    assert_eq!((partial.channels, partial.bit_depth), (6, 24));
    assert_eq!(partial.sample_rate, None);
    assert!(parameters(&input).is_err());
}

#[test]
fn exact_media_clocks_count_pcm_frames_without_rounding_coarse_durations() {
    use symphonia::core::units::TimeBase;
    let mut input = parameters(&source()).unwrap();
    input.time_base = Some(TimeBase::new(1, 96000));
    assert_eq!(declared_pcm_frames(&input).unwrap(), Some(48013));
    input.time_base = Some(TimeBase::new(1, 192000));
    input.n_frames = Some(96026);
    assert_eq!(declared_pcm_frames(&input).unwrap(), Some(48013));
    input.time_base = Some(TimeBase::new(2, 384000));
    assert_eq!(declared_pcm_frames(&input).unwrap(), Some(48013));
    input.n_frames = Some(96025);
    assert_eq!(declared_pcm_frames(&input).unwrap(), None);
    input.time_base = Some(TimeBase::new(1, 1000));
    input.n_frames = Some(500);
    assert_eq!(declared_pcm_frames(&input).unwrap(), None);
    for clock in [
        TimeBase { numer: 0, denom: 1 },
        TimeBase { numer: 1, denom: 0 },
    ] {
        input.time_base = Some(clock);
        assert!(declared_pcm_frames(&input).is_err());
    }
}

#[test]
fn packet_pcm_duration_keeps_only_the_uncertainty_of_its_media_clock() {
    use symphonia::core::units::TimeBase;
    for (numer, denom, ticks, frames, valid) in [
        (1, 96000, 4096, 4096, true),
        (1, 96000, 4096, 4095, false),
        (1, 96000, 4096, 4097, false),
        (1, 192000, 8192, 4096, true),
        (1, 192000, 8192, 4095, false),
        (2, 192000, 4096, 4096, true),
        (1, 48000, 1479, 2957, true),
        (1, 48000, 1479, 2959, true),
        (1, 48000, 1479, 2955, false),
        (1, 1000, 43, 4096, true),
        (1, 1000, 43, 4031, false),
        (1, 1000, 43, 4225, false),
    ] {
        let mut params = parameters(&source()).unwrap();
        params.time_base = Some(TimeBase::new(numer, denom));
        let timeline = PacketTimeline::from_parameters(&params).unwrap().unwrap();
        let packet = Packet::new_from_boxed_slice(0, 0, ticks, Box::default());
        assert_eq!(
            timeline.check_pcm_duration(&packet, frames).is_ok(),
            valid,
            "{numer}/{denom}, {ticks} ticks, {frames} PCM frames"
        );
    }
    let mut params = parameters(&source()).unwrap();
    params.sample_rate = Some(u32::MAX);
    params.time_base = Some(TimeBase::new(u32::MAX, u32::MAX));
    let timeline = PacketTimeline::from_parameters(&params).unwrap().unwrap();
    let packet = Packet::new_from_boxed_slice(0, 0, u64::MAX, Box::default());
    assert!(timeline.check_pcm_duration(&packet, u64::MAX).is_err());
}
