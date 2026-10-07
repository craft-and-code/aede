use super::super::queue::{PcmProducer, pcm_queue};
use super::*;

fn source(bits: u32) -> IntegerPcmFormat {
    IntegerPcmFormat::new(44_100, ChannelLayout::STEREO, bits).unwrap()
}

fn readback(encoding: Encoding, effective_bits: u32) -> Readback {
    Readback {
        hardware: true,
        rate: (44_100, 1),
        channels: 2,
        layout: Some(ChannelLayout::STEREO),
        effective_bits,
        resampling: false,
        interleaved: true,
        encoding,
    }
}

#[test]
fn direct_alsa_selection_requires_an_explicit_bounded_hardware_name() {
    assert!(hardware_name("hw:CARD=0,DEV=0").is_ok());
    assert!(hardware_name("hw:0,0").is_ok());
    for name in [
        "default",
        "plughw:0,0",
        "dmix",
        "pipewire",
        "pulse",
        "hw:",
        "hw:0\0",
    ] {
        assert!(hardware_name(name).is_err());
    }
    assert!(hardware_name(&format!("hw:{}", "a".repeat(257))).is_err());
}

#[test]
fn hardware_identity_is_compared_after_numeric_or_symbolic_card_resolution() {
    let requested = selection("hw:CARD=USB_DAC,DEV=2,SUBDEV=0").unwrap();
    assert_eq!(requested.card, "USB_DAC");
    assert!(matches_identity(&requested, 4, (4, 2, 0), 1));
    for observed in [(3, 2, 0), (4, 3, 0), (4, 2, 1)] {
        assert!(!matches_identity(&requested, 4, observed, 1));
    }
    assert!(!matches_identity(&requested, 4, (4, 2, 0), 2));
    assert!(!matches_identity(&requested, -1, (-1, 2, 0), 1));
    let numeric = selection("hw:4,2,0").unwrap();
    assert_eq!(numeric.card, "4");
    assert!(matches_identity(&numeric, 4, (4, 2, 0), 1));
    for name in [
        "hw:4",
        "hw:CARD=4",
        "hw:4,+2",
        "hw:4,-1",
        "hw:4,4294967296",
        "hw:CARD=4,DEV=0,DEV=1",
        "hw:CARD=4,DEV=0,UNKNOWN=0",
        "hw:CARD=4,CARD=5",
        "hw:CARD=,DEV=0",
    ] {
        assert!(selection(name).is_err());
    }
}

#[test]
fn a_completed_nonblocking_drain_allows_setup_without_accepting_a_programme_reset() {
    assert!(healthy_state(HardwareState::Prepared, false, false, false).is_ok());
    assert!(healthy_state(HardwareState::Running, true, false, false).is_ok());
    assert!(healthy_state(HardwareState::Paused, true, true, false).is_ok());
    assert!(healthy_state(HardwareState::Draining, true, false, true).is_ok());
    assert!(healthy_state(HardwareState::Setup, true, false, true).is_ok());
    for state in [
        HardwareState::Setup,
        HardwareState::Prepared,
        HardwareState::Unknown,
    ] {
        assert_eq!(
            healthy_state(state, true, false, false),
            Err(Fault::RouteChanged)
        );
    }
    assert_eq!(
        healthy_state(HardwareState::Prepared, true, false, true),
        Err(Fault::RouteChanged)
    );
    assert_eq!(
        healthy_state(HardwareState::Paused, true, false, false),
        Err(Fault::RouteChanged)
    );
    assert_eq!(
        healthy_state(HardwareState::Running, false, false, false),
        Err(Fault::RouteChanged)
    );
    assert_eq!(
        healthy_state(HardwareState::Xrun, true, false, true),
        Err(Fault::Xrun)
    );
}

#[test]
fn actual_hardware_rate_precision_and_mapping_are_required_independently() {
    let mut actual = readback(Encoding::Signed32, 24);
    assert!(admit(source(24), actual).is_ok());
    actual.rate = (88_200, 2);
    assert!(admit(source(24), actual).is_ok());
    for bad in [
        Readback {
            hardware: false,
            ..actual
        },
        Readback {
            rate: (44_101, 1),
            ..actual
        },
        Readback {
            rate: (44_100, 0),
            ..actual
        },
        Readback {
            effective_bits: 16,
            ..actual
        },
        Readback {
            effective_bits: 0,
            ..actual
        },
        Readback {
            channels: 1,
            ..actual
        },
        Readback {
            layout: None,
            ..actual
        },
        Readback {
            layout: Some(ChannelLayout::MONO),
            ..actual
        },
        Readback {
            resampling: true,
            ..actual
        },
    ] {
        assert!(admit(source(24), bad).is_err());
    }
    assert!(admit(source(24), readback(Encoding::Signed16, 16)).is_err());
    assert!(admit(source(24), readback(Encoding::Signed24In32, 32)).is_err());
    assert!(
        admit(
            source(24),
            Readback {
                interleaved: false,
                ..actual
            }
        )
        .is_err()
    );
}

#[test]
fn channel_map_admission_never_infers_positions_from_count() {
    assert_eq!(channel_layout("FL FR"), Some(ChannelLayout::STEREO));
    assert_eq!(channel_layout("MONO"), Some(ChannelLayout::MONO));
    assert_eq!(channel_layout("FC"), Some(ChannelLayout::MONO));
    for text in ["", "UNKNOWN", "FR FL", "FL", "FL FR LFE", "FL FR\nFC"] {
        assert_eq!(channel_layout(text), None);
    }
}

#[test]
fn configured_pcm_mapping_cannot_replace_the_selected_hardware_control() {
    let selected = (4, 2, 0);
    let actual = |positions| ControlMap {
        card: 4,
        device: 2,
        subdevice: 0,
        index: 0,
        integer: true,
        positions,
    };
    assert!(verify_control_map(selected, ChannelLayout::STEREO, Some(actual(&[3, 4]))).is_ok());
    for positions in [
        &[][..],
        &[4, 3],
        &[3],
        &[3, 4, 7],
        &[0, 0],
        &[3 | 0x10000, 4],
    ] {
        assert!(
            verify_control_map(selected, ChannelLayout::STEREO, Some(actual(positions))).is_err()
        );
    }
    assert!(verify_control_map(selected, ChannelLayout::STEREO, None).is_err());
    for wrong in [
        ControlMap {
            card: 3,
            ..actual(&[3, 4])
        },
        ControlMap {
            device: 1,
            ..actual(&[3, 4])
        },
        ControlMap {
            subdevice: 1,
            ..actual(&[3, 4])
        },
        ControlMap {
            index: 1,
            ..actual(&[3, 4])
        },
        ControlMap {
            integer: false,
            ..actual(&[3, 4])
        },
    ] {
        assert!(verify_control_map(selected, ChannelLayout::STEREO, Some(wrong)).is_err());
    }
    for mono in [&[2][..], &[7][..]] {
        assert!(verify_control_map(selected, ChannelLayout::MONO, Some(actual(mono))).is_ok());
        assert!(verify_control_map(selected, ChannelLayout::STEREO, Some(actual(mono))).is_err());
    }
    assert!(verify_control_map(selected, ChannelLayout::MONO, Some(actual(&[3, 4]))).is_err());
}

#[test]
fn controls_need_known_zero_decibel_gain_and_enabled_switches() {
    assert!(neutral_control(
        "PCM Playback Volume",
        ControlKind::Integer,
        &[12, 12],
        Some(&[0, 0])
    ));
    assert!(neutral_control(
        "Master Playback Switch",
        ControlKind::Boolean,
        &[1, 1],
        None
    ));
    for db in [
        None,
        Some(&[-100, 0][..]),
        Some(&[100, 0][..]),
        Some(&[0][..]),
    ] {
        assert!(!neutral_control(
            "PCM Playback Volume",
            ControlKind::Integer,
            &[12, 12],
            db
        ));
    }
    assert!(!neutral_control(
        "Master Playback Switch",
        ControlKind::Boolean,
        &[1, 0],
        None
    ));
    assert!(!neutral_control(
        "PCM Playback Volume",
        ControlKind::Integer,
        &[],
        Some(&[])
    ));
    assert!(!neutral_control(
        "DSP Mode",
        ControlKind::Unknown,
        &[0],
        None
    ));
    assert!(!neutral_control(
        "Vendor Gain",
        ControlKind::Integer,
        &[100],
        Some(&[0])
    ));
}

#[test]
fn hardware_packing_preserves_signed_endpoints_and_24_bit_low_bits() {
    for bits in [16, 24] {
        let edge = 1 << (bits - 1);
        let values = [-edge, edge - 1, -1, 1, 127, -128, 0, 129];
        for encoding in [
            Encoding::Signed16,
            Encoding::Signed24In32,
            Encoding::Packed24,
            Encoding::Signed32,
        ] {
            if bits > encoding.bits() {
                continue;
            }
            let mut bytes = [0; 32];
            let count = encode(source(bits), encoding, &values, &mut bytes).unwrap();
            assert_eq!(count, values.len() * encoding.width());
            for (index, value) in values.iter().copied().enumerate() {
                let start = index * encoding.width();
                let widened = match encoding {
                    Encoding::Signed16 => i32::from(i16::from_le_bytes(
                        bytes[start..start + 2].try_into().unwrap(),
                    )),
                    Encoding::Packed24 => {
                        let raw = i32::from_le_bytes([
                            bytes[start],
                            bytes[start + 1],
                            bytes[start + 2],
                            0,
                        ]);
                        (raw << 8) >> 8
                    }
                    _ => i32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()),
                };
                assert_eq!(widened >> (encoding.bits() - bits), value);
            }
        }
    }
}

#[test]
fn invalid_hardware_blocks_do_not_replace_a_valid_prefix() {
    let mut bytes = [99; 32];
    for values in [&[1][..], &[1, 8_388_608], &[-8_388_609, 1]] {
        assert_eq!(
            encode(source(24), Encoding::Signed32, values, &mut bytes),
            Err(Fault::InvalidFrames)
        );
        assert_eq!(bytes, [99; 32]);
    }
    assert_eq!(
        encode(source(24), Encoding::Signed16, &[1, 2], &mut bytes),
        Err(Fault::InvalidFrames)
    );
    assert_eq!(
        encode(source(24), Encoding::Signed32, &[1, 2], &mut bytes[..1]),
        Err(Fault::InvalidFrames)
    );
    assert_eq!(bytes, [99; 32]);
}

#[derive(Default)]
struct Mock {
    frame_bytes: usize,
    space: usize,
    limit: usize,
    delay: u64,
    submitted: Vec<u8>,
    starts: usize,
    pauses: Vec<bool>,
    stops: usize,
    finish_ready: bool,
    fault: Option<Fault>,
    start_fail: bool,
    invalid_write: bool,
    command_during_write: Option<(Arc<Shared>, u8)>,
    failure_during_write: Option<QueueFailure>,
    stop_fault: Option<Fault>,
}

impl Transport for Mock {
    fn controls_unchanged(&self) -> Result<(), Fault> {
        self.fault.map_or(Ok(()), Err)
    }
    fn space(&self) -> Result<usize, Fault> {
        Ok(self.space)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, Fault> {
        if self.invalid_write {
            return Ok(bytes.len() / self.frame_bytes + 1);
        }
        let frames = (bytes.len() / self.frame_bytes)
            .min(self.limit)
            .min(self.space);
        self.submitted
            .extend_from_slice(&bytes[..frames * self.frame_bytes]);
        self.space -= frames;
        self.delay += frames as u64;
        if let Some((state, command)) = &self.command_during_write {
            state.command.store(*command, Ordering::Release);
        }
        if let Some(failure) = &self.failure_during_write {
            failure.fail();
        }
        Ok(frames)
    }
    fn start(&mut self) -> Result<(), Fault> {
        if self.start_fail {
            return Err(Fault::Host);
        }
        self.starts += 1;
        Ok(())
    }
    fn pause(&mut self, paused: bool) -> Result<(), Fault> {
        self.pauses.push(paused);
        Ok(())
    }
    fn delay(&self) -> Result<u64, Fault> {
        Ok(self.delay)
    }
    fn finish(&mut self) -> Result<bool, Fault> {
        Ok(self.finish_ready)
    }
    fn stop(&mut self) -> Result<(), Fault> {
        self.stops += 1;
        self.stop_fault.map_or(Ok(()), Err)
    }
}

fn worker() -> (PcmProducer<i32>, Worker<Mock>) {
    let source = source(24);
    let (producer, pending) = pcm_queue::<i32>(2, 8, true).unwrap();
    let worker = Worker {
        hardware: Mock {
            frame_bytes: 8,
            space: 8,
            limit: 8,
            ..Mock::default()
        },
        pending,
        failure: producer.failure_handle(),
        shared: Arc::new(Shared::default()),
        source,
        encoding: Encoding::Signed32,
        period: 2,
        samples: vec![0; 8],
        bytes: vec![0; 32],
        offset: 0,
        length: 0,
        written: 0,
        paused: false,
        started: false,
    };
    (producer, worker)
}

#[test]
fn hardware_prefill_is_not_reported_as_played_audio_or_automatic_start() {
    let (mut producer, mut worker) = worker();
    assert_eq!(
        producer
            .write_integer(&[1, 2, 3, 4], worker.source)
            .unwrap(),
        4
    );
    assert!(!worker.step().unwrap());
    assert_eq!(worker.hardware.starts, 0);
    assert_eq!(producer.snapshot().consumed_frames, 2);
    assert_eq!(worker.written, 2);
    assert_eq!(worker.shared.played.load(Ordering::Acquire), 0);
    worker.shared.start.store(true, Ordering::Release);
    assert!(!worker.step().unwrap());
    assert_eq!(worker.hardware.starts, 1);
    worker.hardware.delay -= 1;
    assert!(!worker.step().unwrap());
    assert_eq!(worker.shared.played.load(Ordering::Acquire), 1);
}

#[test]
fn startup_prefills_the_hardware_buffer_when_more_programme_is_queued() {
    let (mut producer, mut worker) = worker();
    worker.hardware.space = 4;
    let source = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    producer.write_integer(&source, worker.source).unwrap();
    worker.shared.start.store(true, Ordering::Release);
    assert!(!worker.step().unwrap());
    assert_eq!(worker.hardware.starts, 0);
    assert_eq!(worker.hardware.delay, 2);
    assert_eq!(worker.shared.played.load(Ordering::Acquire), 0);
    assert!(!worker.step().unwrap());
    assert_eq!(worker.hardware.starts, 1);
    assert_eq!(worker.hardware.delay, 4);
    assert_eq!(producer.snapshot().queued_frames, 2);
}

#[test]
fn partial_hardware_writes_keep_frame_order_without_repeating_a_prefix() {
    let (mut producer, mut worker) = worker();
    worker.hardware.limit = 1;
    let values = [-1, 1, 127, -128, 17, -19];
    producer.write_integer(&values, worker.source).unwrap();
    for _ in 0..3 {
        assert!(!worker.step().unwrap());
    }
    let mut expected = [0; 24];
    encode(worker.source, worker.encoding, &values, &mut expected).unwrap();
    assert_eq!(worker.hardware.submitted, expected);
    assert_eq!(worker.written, 3);
}

#[test]
fn zero_progress_keeps_the_same_staged_frames_for_the_next_hardware_write() {
    let (mut producer, mut worker) = worker();
    producer.write_integer(&[1, -1], worker.source).unwrap();
    worker.hardware.limit = 0;
    assert!(!worker.step().unwrap());
    assert_eq!(worker.written, 0);
    assert_eq!(worker.offset, 0);
    worker.hardware.limit = 1;
    assert!(!worker.step().unwrap());
    assert_eq!(worker.written, 1);
    assert_eq!(worker.hardware.submitted, [0, 1, 0, 0, 0, 255, 255, 255]);
}

#[test]
fn pause_before_start_does_not_activate_or_pause_the_pcm_device() {
    let (mut producer, mut worker) = worker();
    producer.write_integer(&[1, 2], worker.source).unwrap();
    worker.shared.command.store(PAUSED, Ordering::Release);
    worker.shared.start.store(true, Ordering::Release);
    assert!(!worker.step().unwrap());
    assert_eq!(worker.hardware.starts, 0);
    assert!(worker.hardware.pauses.is_empty());
    worker.shared.command.store(RUNNING, Ordering::Release);
    assert!(!worker.step().unwrap());
    assert_eq!(worker.hardware.starts, 1);
}

#[test]
fn an_active_pause_keeps_pending_source_frames_and_the_hardware_delay() {
    let (mut producer, mut worker) = worker();
    worker.hardware.space = 2;
    producer
        .write_integer(&[1, 2, 3, 4, 5, 6], worker.source)
        .unwrap();
    worker.shared.start.store(true, Ordering::Release);
    worker.step().unwrap();
    worker.shared.command.store(PAUSED, Ordering::Release);
    worker.step().unwrap();
    assert_eq!(worker.hardware.pauses, [true]);
    assert_eq!(producer.snapshot().queued_frames, 1);
    assert_eq!(worker.written, 2);
    worker.hardware.space = 1;
    worker.shared.command.store(RUNNING, Ordering::Release);
    worker.step().unwrap();
    assert_eq!(worker.hardware.pauses, [true, false]);
    assert_eq!(worker.written, 3);
}

#[test]
fn a_verified_short_tail_completes_only_after_hardware_drain() {
    let (mut producer, mut worker) = worker();
    producer.write_integer(&[1, 2], worker.source).unwrap();
    producer.close_input();
    worker.shared.start.store(true, Ordering::Release);
    assert!(!worker.step().unwrap());
    assert!(!worker.shared.complete.load(Ordering::Acquire));
    assert_eq!(worker.shared.played.load(Ordering::Acquire), 0);
    worker.hardware.finish_ready = true;
    assert!(worker.step().unwrap());
    assert!(worker.shared.complete.load(Ordering::Acquire));
    assert_eq!(worker.shared.played.load(Ordering::Acquire), 1);
}

#[test]
fn an_empty_verified_source_finishes_without_starting_hardware() {
    let (producer, worker) = worker();
    producer.close_input();
    let state = Arc::clone(&worker.shared);
    worker.run();
    assert!(state.complete.load(Ordering::Acquire));
    assert!(state.exited.load(Ordering::Acquire));
    assert!(!state.started.load(Ordering::Acquire));
}

#[test]
fn a_failed_start_retains_accepted_progress_and_latches_source_failure() {
    let (mut producer, mut worker) = worker();
    let accepted = producer.write_integer(&[1, 2], worker.source).unwrap();
    worker.hardware.start_fail = true;
    worker.shared.start.store(true, Ordering::Release);
    let state = Arc::clone(&worker.shared);
    worker.run();
    assert_eq!(accepted, 2);
    assert!(state.check().unwrap_err().contains("host operation failed"));
    assert_eq!(state.played.load(Ordering::Acquire), 0);
    assert!(producer.write_integer(&[3, 4], source(24)).is_err());
}

#[test]
fn host_xruns_and_changed_controls_stop_before_more_source_is_consumed() {
    for fault in [Fault::Xrun, Fault::RouteChanged] {
        let (mut producer, mut worker) = worker();
        producer.write_integer(&[1, 2], worker.source).unwrap();
        worker.hardware.fault = Some(fault);
        let state = Arc::clone(&worker.shared);
        worker.run();
        assert_eq!(producer.snapshot().consumed_frames, 0);
        assert_eq!(state.fatal.load(Ordering::Acquire), fault as u8);
        assert!(!state.complete.load(Ordering::Acquire));
        assert!(producer.write_integer(&[3, 4], source(24)).is_err());
    }
}

#[test]
fn invalid_host_counts_and_delays_are_errors_instead_of_clamped_progress() {
    let (mut producer, mut invalid) = worker();
    producer.write_integer(&[1, 2], invalid.source).unwrap();
    invalid.hardware.invalid_write = true;
    assert_eq!(invalid.step(), Err(Fault::InvalidFrames));
    let (mut producer, mut delay) = worker();
    producer.write_integer(&[1, 2], delay.source).unwrap();
    delay.shared.start.store(true, Ordering::Release);
    delay.step().unwrap();
    delay.hardware.delay = 2;
    assert_eq!(delay.step(), Err(Fault::InvalidFrames));
}

#[test]
fn explicit_stop_does_not_consume_queued_source_or_report_completion() {
    let (mut producer, mut worker) = worker();
    producer.write_integer(&[1, 2], worker.source).unwrap();
    worker.shared.command.store(STOPPED, Ordering::Release);
    assert!(worker.step().unwrap());
    assert_eq!(worker.hardware.stops, 1);
    assert_eq!(producer.snapshot().consumed_frames, 0);
    assert!(!worker.shared.complete.load(Ordering::Acquire));
}

#[test]
fn joined_shutdown_reports_host_stop_failure_without_rejecting_an_intentional_abort() {
    let (producer, normal) = worker();
    let shared = Arc::clone(&normal.shared);
    producer.failure_handle().fail();
    normal.run();
    assert!(shared.exited.load(Ordering::Acquire));
    assert!(producer.snapshot().failed);
    assert!(shared.joined(Ok(())).is_ok());
    assert_eq!(
        shared.joined(Err("join failed".to_owned())),
        Err("join failed".to_owned())
    );

    let (producer, mut failed) = worker();
    let shared = Arc::clone(&failed.shared);
    producer.failure_handle().fail();
    failed.hardware.stop_fault = Some(Fault::Host);
    failed.run();
    assert!(shared.exited.load(Ordering::Acquire));
    assert_eq!(shared.joined(Ok(())), Err(Fault::Host.message().to_owned()));

    let (producer, mut changed) = worker();
    let shared = Arc::clone(&changed.shared);
    shared.fail(Fault::RouteChanged, &producer.failure_handle());
    changed.hardware.stop_fault = Some(Fault::Host);
    changed.run();
    assert_eq!(
        shared.joined(Ok(())),
        Err(Fault::RouteChanged.message().to_owned())
    );
    assert_eq!(
        shared.joined(Err(Fault::StopTimeout.message().to_owned())),
        Err(Fault::RouteChanged.message().to_owned())
    );
}

#[test]
fn stop_pause_or_source_failure_during_a_write_does_not_start_the_device() {
    for command in [STOPPED, PAUSED] {
        let (mut producer, mut worker) = worker();
        producer.write_integer(&[1, 2], worker.source).unwrap();
        producer.close_input();
        worker.shared.start.store(true, Ordering::Release);
        worker.hardware.command_during_write = Some((Arc::clone(&worker.shared), command));
        assert_eq!(worker.step().unwrap(), command == STOPPED);
        assert_eq!(worker.hardware.starts, 0);
        assert!(!worker.shared.complete.load(Ordering::Acquire));
    }
    let (mut producer, mut worker) = worker();
    producer.write_integer(&[1, 2], worker.source).unwrap();
    producer.close_input();
    worker.shared.start.store(true, Ordering::Release);
    worker.hardware.failure_during_write = Some(producer.failure_handle());
    assert!(worker.step().unwrap());
    assert_eq!(worker.hardware.starts, 0);
    assert!(!worker.shared.complete.load(Ordering::Acquire));
}

#[test]
fn failure_diagnostics_keep_the_first_host_or_transport_cause() {
    let (producer, worker) = worker();
    for fault in [
        Fault::Host,
        Fault::PauseUnsupported,
        Fault::WorkerPanic,
        Fault::StopTimeout,
    ] {
        assert!(!fault.message().is_empty());
        assert_eq!(Fault::from_code(fault as u8), fault);
    }
    worker.shared.fail(Fault::Xrun, &producer.failure_handle());
    worker
        .shared
        .fail(Fault::RouteChanged, &producer.failure_handle());
    assert!(worker.shared.check().unwrap_err().contains("xrun"));
    assert!(producer.snapshot().failed);
}
