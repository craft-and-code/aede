//! Reproducible developer profiling of synthetic PCM; no audio device is used.
//!
//! Run `cargo run -p aede-dsp --example profile --release --offline --
//! --seconds 10 --repeats 3 > /tmp/aede-dsp-profile.csv`.
//! To isolate a strong sinc ratio, add `--rates 384000 --channels 2
//! --blocks 4096 --sinc-output-rate 8001` (only that stage runs).
//! Setup, individual block calls and finalization are timed separately. Input
//! generation/cloning, processor destruction and CSV output are excluded from
//! stage timings. Processing includes black-box barriers and small checksum
//! probes. Analysis-only stages report zero output PCM frames. These timings
//! describe this host, build and synthetic workload; they are not CI gates,
//! audio conformance results, callback timing or target-NAS guarantees.

use std::error::Error;
use std::hint::black_box;
use std::time::{Duration, Instant};

use aede_dsp::loudness::LoudnessProgramme;
use aede_dsp::{
    Dsp, OutputMeter, PcmFormat, ProcessStats, RateConverter, Spectrum, ToneControls, TpdfQuantizer,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Config {
    seconds: u32,
    repeats: usize,
    rates: Vec<u32>,
    channels: Vec<u16>,
    blocks: Vec<usize>,
    sinc_output_rate: Option<u32>,
}

impl Config {
    fn read() -> Result<Option<Self>> {
        let mut config = Self {
            seconds: 10,
            repeats: 3,
            rates: vec![44_100, 48_000, 96_000, 192_000],
            channels: vec![1, 2],
            blocks: vec![256, 1024, 4096],
            sinc_output_rate: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(option) = args.next() {
            if option == "--help" {
                eprintln!(
                    "Usage: profile [--seconds N] [--repeats N] [--rates 44100,48000,96000,192000] [--channels 1,2] [--blocks 256,1024,4096] [--sinc-output-rate RATE]\n\
                     Rates must be within 8000..=384000; durations, repeats and blocks must be positive.\n\
                     --sinc-output-rate runs only the sinc stage at that rate; the ratio must select sinc for every input.\n\
                     Above 192kHz the full matrix requires even input rates to retain an FFT case.\n\
                     CSV goes to stdout. Use --release; no sound or files are produced."
                );
                return Ok(None);
            }
            if !matches!(
                option.as_str(),
                "--seconds"
                    | "--repeats"
                    | "--rates"
                    | "--channels"
                    | "--blocks"
                    | "--sinc-output-rate"
            ) {
                return Err(format!("unknown option {option:?}").into());
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{option} requires a value"))?;
            match option.as_str() {
                "--seconds" => config.seconds = value.parse()?,
                "--repeats" => config.repeats = value.parse()?,
                "--rates" => config.rates = parse_list(&value)?,
                "--channels" => config.channels = parse_list(&value)?,
                "--blocks" => config.blocks = parse_list(&value)?,
                "--sinc-output-rate" => config.sinc_output_rate = Some(value.parse()?),
                _ => unreachable!("validated profiling option"),
            }
        }
        if config.seconds == 0
            || config.repeats == 0
            || config.blocks.contains(&0)
            || config
                .rates
                .iter()
                .any(|rate| !(8_000..=384_000).contains(rate))
            || config
                .channels
                .iter()
                .any(|channels| !matches!(channels, 1 | 2))
        {
            return Err("invalid profiling configuration; see --help".into());
        }
        if let Some(output) = config.sinc_output_rate {
            if !(8_000..=384_000).contains(&output)
                || config.rates.iter().any(|&input| {
                    let common = gcd(input, output);
                    input == output || (input / common <= 1024 && output / common <= 1024)
                })
            {
                return Err("--sinc-output-rate must select the unusual-ratio sinc path for every input rate".into());
            }
        } else if config
            .rates
            .iter()
            .any(|&rate| rate > 192_000 && !rate.is_multiple_of(2))
        {
            return Err("the full FFT matrix requires even input rates above 192000; use --sinc-output-rate for unusual high-rate ratios".into());
        }
        Ok(Some(config))
    }
}

fn parse_list<T>(raw: &str) -> Result<Vec<T>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
{
    raw.split(',')
        .map(|value| value.parse().map_err(Into::into))
        .collect()
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

#[derive(Clone, Copy)]
enum Stage {
    Gain,
    Eq,
    Loudness,
    OutputMeter,
    Spectrum,
    Dither,
    Fft,
    Sinc,
    Pipeline,
}

impl Stage {
    const ALL: [Self; 9] = [
        Self::Gain,
        Self::Eq,
        Self::Loudness,
        Self::OutputMeter,
        Self::Spectrum,
        Self::Dither,
        Self::Fft,
        Self::Sinc,
        Self::Pipeline,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Gain => "gain_guard",
            Self::Eq => "gain_eq_guard",
            Self::Loudness => "source_loudness",
            Self::OutputMeter => "output_meter",
            Self::Spectrum => "spectrum",
            Self::Dither => "tpdf_i16",
            Self::Fft => "resample_fft",
            Self::Sinc => "resample_sinc",
            Self::Pipeline => "loudness_gain_eq_guard_meter_spectrum",
        }
    }

    fn output_rate(self, input_rate: u32, sinc_output_rate: Option<u32>) -> u32 {
        match self {
            Self::Fft => match input_rate {
                44_100 => 48_000,
                48_000 => 44_100,
                96_000 | 192_000 | 384_000 => 48_000,
                rate if rate > 192_000 => rate / 2,
                // The configurable range makes this exact 2:1 FFT case valid.
                _ => input_rate * 2,
            },
            // Consecutive rates are coprime, forcing the unusual-ratio path.
            Self::Sinc => sinc_output_rate.unwrap_or_else(|| {
                if input_rate < 384_000 {
                    input_rate + 1
                } else {
                    input_rate - 1
                }
            }),
            _ => input_rate,
        }
    }

    fn configuration(self, explicit_sinc: bool) -> &'static str {
        match self {
            Self::Gain => "gain_minus6_ramp0_sample_guard",
            Self::Eq => "gain_minus12_bass_plus6_treble_minus3_sample_guard",
            Self::Loudness => "LoudnessProgramme_default_integrated_and_true_peak",
            Self::OutputMeter => "OutputMeter_default_peak_and_guard_stats",
            Self::Spectrum => "Spectrum_default_24_bands",
            Self::Dither => "TpdfQuantizer_default_signed16",
            Self::Fft => "RateConverter_new_preferred_ratio",
            Self::Sinc if explicit_sinc => "RateConverter_new_explicit_sinc_rate",
            Self::Sinc => "RateConverter_new_coprime_rates",
            Self::Pipeline => {
                "source_loudness_then_gain_minus12_eq_plus6_minus3_guard_meter_spectrum"
            }
        }
    }

    fn gains(self) -> (f32, f32, f32) {
        match self {
            Self::Gain => (-6.0, 0.0, 0.0),
            Self::Eq | Self::Pipeline => (-12.0, 6.0, -3.0),
            _ => (0.0, 0.0, 0.0),
        }
    }

    fn lazy_first_call(self) -> bool {
        matches!(self, Self::Loudness | Self::Pipeline)
    }

    fn produces_pcm(self) -> bool {
        !matches!(self, Self::Loudness | Self::OutputMeter | Self::Spectrum)
    }
}

struct Pipeline {
    source: LoudnessProgramme,
    dsp: Dsp,
    meter: OutputMeter,
    spectrum: Spectrum,
}

enum Processor {
    Dsp(Box<Dsp>),
    Loudness(Box<LoudnessProgramme>),
    Meter(Box<OutputMeter>),
    Spectrum(Box<Spectrum>),
    Dither(TpdfQuantizer, Vec<i16>),
    Rate(Box<RateConverter>),
    Pipeline(Box<Pipeline>),
}

#[derive(Default)]
struct Work {
    frames: u64,
    checksum: f64,
}

impl Processor {
    fn new(
        stage: Stage,
        format: PcmFormat,
        block_samples: usize,
        output_rate: u32,
    ) -> Result<Self> {
        let (gain, bass, treble) = stage.gains();
        let dsp = || -> Result<Dsp> {
            let mut dsp = Dsp::new(format);
            dsp.set_gain_db(gain, 0)?;
            dsp.set_tone(ToneControls::new(bass, treble)?)?;
            Ok(dsp)
        };
        Ok(match stage {
            Stage::Gain | Stage::Eq => Self::Dsp(Box::new(dsp()?)),
            Stage::Loudness => Self::Loudness(Box::new(LoudnessProgramme::new())),
            Stage::OutputMeter => Self::Meter(Box::new(OutputMeter::new(format)?)),
            Stage::Spectrum => Self::Spectrum(Box::new(Spectrum::new(format))),
            Stage::Dither => Self::Dither(TpdfQuantizer::new(), vec![0; block_samples]),
            Stage::Fft | Stage::Sinc => {
                Self::Rate(Box::new(RateConverter::new(format, output_rate)?))
            }
            Stage::Pipeline => Self::Pipeline(Box::new(Pipeline {
                source: LoudnessProgramme::new(),
                dsp: dsp()?,
                meter: OutputMeter::new(format)?,
                spectrum: Spectrum::new(format),
            })),
        })
    }

    fn process(
        &mut self,
        format: PcmFormat,
        source_peak: f32,
        samples: &mut [f32],
    ) -> Result<Work> {
        let channels = usize::from(format.channels());
        let frames = (samples.len() / channels) as u64;
        let checksum = match self {
            Self::Dsp(dsp) => {
                black_box(dsp.process_for_output(black_box(samples))?);
                probe(black_box(samples))
            }
            Self::Loudness(meter) => {
                meter.push(format, black_box(samples))?;
                return Ok(Work::default());
            }
            Self::Meter(meter) => {
                meter.observe(
                    black_box(samples),
                    ProcessStats {
                        sample_peak: source_peak,
                        overfull_samples: 0,
                    },
                )?;
                return Ok(Work::default());
            }
            Self::Spectrum(spectrum) => {
                let levels = black_box(spectrum.push(black_box(samples))?);
                return Ok(Work {
                    frames: 0,
                    checksum: levels
                        .map_or(0.0, |levels| levels.iter().copied().map(f64::from).sum()),
                });
            }
            Self::Dither(quantizer, output) => {
                output.resize(samples.len(), 0);
                for (sample, target) in black_box(samples).iter().zip(output.iter_mut()) {
                    *target = quantizer.i16(*sample);
                }
                let output = black_box(output.as_slice());
                output.first().map_or(0.0, |&value| f64::from(value))
                    + output
                        .get(output.len() / 2)
                        .map_or(0.0, |&value| 2.0 * f64::from(value))
                    + output.last().map_or(0.0, |&value| 3.0 * f64::from(value))
            }
            Self::Rate(converter) => {
                let output = black_box(converter.push(black_box(samples), false)?);
                return Ok(Work {
                    frames: (output.len() / channels) as u64,
                    checksum: probe(output),
                });
            }
            Self::Pipeline(pipeline) => {
                pipeline.source.push(format, black_box(samples))?;
                let stats = pipeline.dsp.process_for_output(samples)?;
                pipeline.meter.observe(samples, stats)?;
                black_box(pipeline.spectrum.push(samples)?);
                probe(black_box(samples))
            }
        };
        Ok(Work { frames, checksum })
    }

    fn finish(&mut self, channels: u16) -> Result<Work> {
        let checksum = match self {
            Self::Loudness(meter) => loudness_checksum(meter)?,
            Self::Meter(meter) => meter_checksum(meter)?,
            Self::Pipeline(pipeline) => {
                loudness_checksum(&pipeline.source)? + meter_checksum(&pipeline.meter)?
            }
            Self::Rate(converter) => {
                let output = black_box(converter.push(&[], true)?);
                return Ok(Work {
                    frames: (output.len() / usize::from(channels)) as u64,
                    checksum: probe(output),
                });
            }
            _ => 0.0,
        };
        Ok(Work {
            frames: 0,
            checksum,
        })
    }
}

fn probe(samples: &[f32]) -> f64 {
    samples.first().map_or(0.0, |&value| f64::from(value))
        + samples
            .get(samples.len() / 2)
            .map_or(0.0, |&value| 2.0 * f64::from(value))
        + samples.last().map_or(0.0, |&value| 3.0 * f64::from(value))
}

fn loudness_checksum(meter: &LoudnessProgramme) -> Result<f64> {
    Ok(black_box(meter.measurement()?).map_or(0.0, |result| {
        f64::from(result.integrated_lufs) + result.true_peak.map_or(0.0, f64::from)
    }))
}

fn meter_checksum(meter: &OutputMeter) -> Result<f64> {
    let result = black_box(meter.snapshot()?);
    Ok(result.frames as f64
        + f64::from(result.output_sample_peak)
        + result.output_true_peak.map_or(0.0, f64::from)
        + f64::from(result.pre_guard_sample_peak)
        + result.guarded_samples as f64)
}

fn synthetic(format: PcmFormat, seconds: u32) -> Result<Vec<f32>> {
    let frames = usize::try_from(format.sample_rate())?
        .checked_mul(usize::try_from(seconds)?)
        .ok_or("source frame count overflow")?;
    let sample_count = frames
        .checked_mul(usize::from(format.channels()))
        .ok_or("source sample count overflow")?;
    let mut source = Vec::new();
    source.try_reserve_exact(sample_count)?;
    let high = 6_419.0_f64.min(f64::from(format.sample_rate()) * 0.31);
    for frame in 0..frames {
        let time = frame as f64 / f64::from(format.sample_rate());
        for channel in 0..format.channels() {
            let phase = f64::from(channel) * 0.37;
            let signal = 0.08 * (std::f64::consts::TAU * 120.0 * time + phase).sin()
                + 0.10 * (std::f64::consts::TAU * 997.0 * time + phase).sin()
                + 0.04 * (std::f64::consts::TAU * high * time + phase).sin();
            source.push(signal as f32);
        }
    }
    Ok(source)
}

struct Timings {
    setup: Duration,
    first_call: Duration,
    processing: Duration,
    finalize: Duration,
    wall: Duration,
    max_call: Duration,
    max_call_audio_ms: f64,
    max_call_audio_ratio: f64,
    calls: usize,
}

fn profile(
    stage: Stage,
    format: PcmFormat,
    block_frames: usize,
    source: &[f32],
    output_rate: u32,
) -> Result<(Timings, Work)> {
    // Each stage and trial starts from the same source and fresh processing
    // state. The copy is outside the measured processing/construction interval.
    let mut samples = source.to_vec();
    let source_peak = source
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let block_samples = block_frames
        .checked_mul(usize::from(format.channels()))
        .ok_or("block size overflow")?;
    let wall_start = Instant::now();
    let setup_start = Instant::now();
    let mut processor =
        Processor::new(stage, format, block_samples.min(source.len()), output_rate)?;
    let setup = setup_start.elapsed();
    let mut timings = Timings {
        setup,
        first_call: Duration::ZERO,
        processing: Duration::ZERO,
        finalize: Duration::ZERO,
        wall: Duration::ZERO,
        max_call: Duration::ZERO,
        max_call_audio_ms: 0.0,
        max_call_audio_ratio: 0.0,
        calls: 0,
    };
    let mut work = Work::default();
    for block in samples.chunks_mut(block_samples) {
        let audio_ms =
            block.len() as f64 / f64::from(format.channels()) / f64::from(format.sample_rate())
                * 1000.0;
        let start = Instant::now();
        let observed = processor.process(format, source_peak, block)?;
        let elapsed = start.elapsed();
        if timings.calls == 0 {
            timings.first_call = elapsed;
        }
        if elapsed > timings.max_call {
            timings.max_call = elapsed;
            timings.max_call_audio_ms = audio_ms;
        }
        timings.max_call_audio_ratio = timings
            .max_call_audio_ratio
            .max(elapsed.as_secs_f64() * 1000.0 / audio_ms);
        timings.processing += elapsed;
        timings.calls += 1;
        work.frames += observed.frames;
        work.checksum += observed.checksum;
    }
    let start = Instant::now();
    let tail = processor.finish(format.channels())?;
    timings.finalize = start.elapsed();
    work.frames += tail.frames;
    work.checksum += tail.checksum;
    black_box(&work);
    timings.wall = wall_start.elapsed();
    let input_frames = (source.len() / usize::from(format.channels())) as u64;
    let expected_frames = if stage.produces_pcm() {
        input_frames
            .checked_mul(u64::from(output_rate))
            .ok_or("output frame count overflow")?
            .div_ceil(u64::from(format.sample_rate()))
    } else {
        0
    };
    if work.frames != expected_frames || !work.checksum.is_finite() {
        return Err(format!("{} produced inconsistent frames/checksum", stage.name()).into());
    }
    Ok((timings, work))
}

fn main() -> Result<()> {
    let Some(config) = Config::read()? else {
        return Ok(());
    };
    if cfg!(debug_assertions) {
        eprintln!("Warning: this is a debug build; use --release for representative profiling.");
    }
    eprintln!(
        "Synthetic PCM only. Timings exclude generation/copy/destruction; output_frames is zero for analysis-only stages. The 2x speed margin and setup+first-call <=100ms columns are provisional developer observations, not acceptance gates."
    );
    println!(
        "stage,configuration,trial,os,arch,debug_build,input_rate,output_rate,channels,seconds,block_frames,block_audio_ms,gain_db,bass_db,treble_db,lazy_first_call,input_frames,output_frames,calls,setup_ms,first_call_ms,processing_ms,finalize_ms,stage_wall_ms,max_call_ms,max_call_audio_ms,worst_call_audio_ratio,realtime_factor,provisional_margin_2x,provisional_startup_100ms,checksum"
    );
    for rate in config.rates {
        for &channels in &config.channels {
            let format = PcmFormat::new(rate, channels)?;
            let source = synthetic(format, config.seconds)?;
            for &block_frames in &config.blocks {
                for stage in Stage::ALL {
                    if config.sinc_output_rate.is_some() && !matches!(stage, Stage::Sinc) {
                        continue;
                    }
                    for trial in 1..=config.repeats {
                        let output_rate = stage.output_rate(rate, config.sinc_output_rate);
                        let (timings, work) =
                            profile(stage, format, block_frames, &source, output_rate)?;
                        let (gain, bass, treble) = stage.gains();
                        let realtime = f64::from(config.seconds)
                            / (timings.processing + timings.finalize).as_secs_f64();
                        let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
                        println!(
                            "{},{},{},{},{},{},{},{},{},{},{},{:.6},{},{},{},{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{:.12}",
                            stage.name(),
                            stage.configuration(config.sinc_output_rate.is_some()),
                            trial,
                            std::env::consts::OS,
                            std::env::consts::ARCH,
                            cfg!(debug_assertions),
                            rate,
                            output_rate,
                            channels,
                            config.seconds,
                            block_frames,
                            block_frames as f64 / f64::from(rate) * 1000.0,
                            gain,
                            bass,
                            treble,
                            stage.lazy_first_call(),
                            source.len() / usize::from(channels),
                            work.frames,
                            timings.calls,
                            ms(timings.setup),
                            ms(timings.first_call),
                            ms(timings.processing),
                            ms(timings.finalize),
                            ms(timings.wall),
                            ms(timings.max_call),
                            timings.max_call_audio_ms,
                            timings.max_call_audio_ratio,
                            realtime,
                            realtime >= 2.0,
                            timings.setup + timings.first_call <= Duration::from_millis(100),
                            work.checksum,
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
