//! Explicit USB DAC selection and verified HAL lease. No default-device fallback.
use super::{
    api::{self, Property},
    format::{Description, Plan, Ranged, plan},
    lease::{Backend, Lease},
};
use crate::dop::Format;
use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

// Serialize the read/free -> take -> read/owned sequence within this process.
// HAL ownership is PID-scoped; this also refuses a second session in the PID.
static PREPARATION: Mutex<()> = Mutex::new(());
static RETIRED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
pub(super) fn control_guard() -> Result<MutexGuard<'static, ()>, String> {
    PREPARATION
        .lock()
        .map_err(|_| "CoreAudio DoP preparation is poisoned".into())
}
pub(super) fn retire(device: u32) -> Result<(), String> {
    let mut retired = RETIRED
        .lock()
        .map_err(|_| "DoP retired-device state is poisoned")?;
    if retired.len() >= 1024 {
        return Err("the DoP retired-device budget is exhausted".into());
    }
    if !retired.contains(&device) {
        retired.push(device);
    }
    Ok(())
}

pub(super) struct HalDevice {
    pub device: u32,
    pub stream: u32,
    uid: String,
}
pub(super) struct Prepared {
    pub lease: Lease<HalDevice>,
    pub plan: Plan,
    pub frame_limit: usize,
    pub tail: Duration,
    pub watched: Vec<(u32, Property)>,
}

fn streams(device: u32, scope: u32) -> Result<Vec<u32>, String> {
    api::list(
        device,
        Property {
            selector: u32::from_be_bytes(*b"stm#"),
            scope,
            element: 0,
        },
    )
}

pub(super) fn prepare(
    uid: &str,
    format: Format,
    current: &impl Fn() -> Result<(), String>,
) -> Result<Prepared, String> {
    let _guard = control_guard()?;
    current()?;
    let device = api::resolve(uid)?;
    let retired = RETIRED
        .lock()
        .map_err(|_| "DoP retired-device state is poisoned")?;
    if retired.len() >= 1024 || retired.contains(&device) {
        return Err(
            "this DAC had a failed DoP deregistration; restart the process before reuse".into(),
        );
    }
    drop(retired);
    let output = streams(device, api::OUTPUT)?;
    let [stream] = output.as_slice() else {
        return Err("DoP requires one explicitly ordered output stream".into());
    };
    let backend = HalDevice {
        device,
        stream: *stream,
        uid: uid.into(),
    };
    backend.health()?;
    let physical: Vec<Ranged> = api::list(*stream, Property::global(b"pfta"))?;
    let virtual_formats: Vec<Ranged> = api::list(*stream, Property::global(b"sfma"))?;
    let plan = plan(&physical, &virtual_formats, format)?;
    let pid = i32::try_from(std::process::id())
        .map_err(|_| "process ID is outside CoreAudio's PID type")?;
    let lease = Lease::acquire(backend, plan, pid, current)?;
    let geometry: Result<(usize, Duration), String> = (|| {
        let frames = api::get::<u32>(device, Property::global(b"fsiz"))?;
        if frames == 0 || frames > 16384 {
            return Err("DoP hardware buffer size is outside the bounded profile".into());
        }
        let safety = api::get::<u32>(device, Property::output(b"saft", 0))?;
        let device_latency = api::get::<u32>(device, Property::output(b"ltnc", 0))?;
        let stream_latency = api::get::<u32>(*stream, Property::global(b"ltnc"))?;
        let tail = u64::from(frames) * 2
            + u64::from(safety)
            + u64::from(device_latency)
            + u64::from(stream_latency);
        if tail > u64::from(format.carrier_rate()) * 2 {
            return Err("DoP hardware latency exceeds its bounded drain profile".into());
        }
        Ok((
            frames as usize,
            Duration::from_secs_f64(tail as f64 / f64::from(format.carrier_rate())),
        ))
    })();
    let (lease, (frame_limit, tail)) = lease.checked(geometry)?;
    let mut watched = vec![
        (device, Property::global(b"oink")),
        (device, Property::global(b"nsrt")),
        (device, Property::global(b"livn")),
        (device, Property::global(b"uid ")),
        (device, Property::global(b"tran")),
        (device, Property::global(b"fsiz")),
        (device, Property::output(b"stm#", 0)),
        (device, Property::output(b"dch2", 0)),
        (*stream, Property::global(b"pft ")),
        (*stream, Property::global(b"sfmt")),
        (*stream, Property::global(b"schn")),
    ];
    for element in 0..=2 {
        for selector in [b"volm", b"mute"] {
            let address = Property::output(selector, element);
            if api::has(device, address) {
                watched.push((device, address));
            }
        }
    }
    Ok(Prepared {
        lease,
        plan,
        frame_limit,
        tail,
        watched,
    })
}

impl Backend for HalDevice {
    fn hog(&self) -> Result<i32, String> {
        api::get(self.device, Property::global(b"oink"))
    }
    fn set_hog(&mut self, value: i32) -> Result<(), String> {
        api::set(self.device, Property::global(b"oink"), value)
    }
    fn rate(&self) -> Result<f64, String> {
        api::get(self.device, Property::global(b"nsrt"))
    }
    fn set_rate(&mut self, value: f64) -> Result<(), String> {
        api::set(self.device, Property::global(b"nsrt"), value)
    }
    fn physical(&self) -> Result<Description, String> {
        api::get(self.stream, Property::global(b"pft "))
    }
    fn set_physical(&mut self, value: Description) -> Result<(), String> {
        api::set(self.stream, Property::global(b"pft "), value)
    }
    fn virtual_format(&self) -> Result<Description, String> {
        api::get(self.stream, Property::global(b"sfmt"))
    }
    fn set_virtual(&mut self, value: Description) -> Result<(), String> {
        api::set(self.stream, Property::global(b"sfmt"), value)
    }
    fn health(&self) -> Result<(), String> {
        if api::get::<u32>(self.device, Property::global(b"livn"))? != 1
            || api::string(self.device, Property::global(b"uid "))? != self.uid
        {
            return Err("the explicitly selected DAC disappeared or changed identity".into());
        }
        if api::get::<u32>(self.device, Property::global(b"tran"))? != u32::from_be_bytes(*b"usb ")
        {
            return Err("the initial CoreAudio DoP profile requires a physical USB DAC".into());
        }
        if streams(self.device, api::OUTPUT)? != [self.stream]
            || !streams(self.device, api::INPUT)?.is_empty()
            || api::get::<u32>(self.stream, Property::global(b"sdir"))? != 0
            || api::get::<u32>(self.stream, Property::global(b"schn"))? != 1
            || api::get::<u32>(self.stream, Property::global(b"sact"))? != 1
        {
            return Err("DoP requires a dedicated active output-only DAC stream".into());
        }
        if api::list::<u32>(self.device, Property::output(b"dch2", 0))? != [1, 2] {
            return Err(
                "DoP needs verified front-left/front-right hardware channel association".into(),
            );
        }
        for element in 0..=2 {
            let volume = Property::output(b"volm", element);
            if api::has(self.device, volume) && api::get::<f32>(self.device, volume)? != 1.0 {
                return Err(
                    "DoP requires unity device gain; the existing system control was preserved"
                        .into(),
                );
            }
            let mute = Property::output(b"mute", element);
            if api::has(self.device, mute) && api::get::<u32>(self.device, mute)? != 0 {
                return Err("DoP requires an unmuted hardware route; the existing system control was preserved".into());
            }
        }
        Ok(())
    }
}

/// Read-only inventory. A listed device is not a qualified DoP route.
pub(in crate::dop) fn inventory() -> Result<Vec<(String, String)>, String> {
    let mut result = Vec::new();
    for id in api::ids()? {
        if streams(id, api::OUTPUT)?.is_empty() {
            continue;
        }
        result.push((
            api::string(id, Property::global(b"uid "))?,
            api::string(id, Property::global(b"lnam"))?,
        ));
    }
    result.sort();
    Ok(result)
}
