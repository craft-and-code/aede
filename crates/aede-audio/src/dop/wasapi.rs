//! Explicit WASAPI exclusive/raw integer DoP on one STA control thread.
//! This unit owns COM, MMCSS and the endpoint buffer together so release order
//! remains visible; no COM interface or writable buffer crosses a thread.
use super::{
    Carrier, Format, Payload,
    driver::Driver,
    push::{Push, Sink},
    state::Signals,
};
use crate::queue::PcmConsumer;
use std::{mem::size_of, ptr, sync::Arc, time::Duration};
use windows::{
    Win32::{
        Foundation::HANDLE,
        Media::Audio::{Endpoints::IAudioEndpointVolume, *},
        System::{
            Com::*,
            Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW},
        },
    },
    core::{GUID, PCWSTR, PWSTR, w},
};
fn error(e: impl std::fmt::Display) -> String {
    format!("WASAPI DoP: {e}")
}
struct Apartment;
impl Apartment {
    fn open() -> Result<Self, String> {
        // SAFETY: the factory runs on a newly owned control thread. The STA is
        // balanced only after all interfaces and MMCSS registration are gone.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(error)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: one successful initialization on this same thread.
        unsafe { CoUninitialize() };
    }
}
struct Task(Option<HANDLE>);
impl Task {
    fn open() -> Result<Self, String> {
        let mut index = 0;
        // SAFETY: static NUL-terminated SDK task name and one writable index.
        let handle =
            unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) }.map_err(error)?;
        Ok(Self(Some(handle)))
    }
    fn close(&mut self) -> Result<(), String> {
        if let Some(handle) = self.0.take() {
            // SAFETY: this registration belongs exclusively to the current thread.
            unsafe { AvRevertMmThreadCharacteristics(handle) }.map_err(error)?;
        }
        Ok(())
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("{e}");
        }
    }
}
struct Name(PWSTR);
impl Name {
    unsafe fn copy(raw: PWSTR) -> Result<String, String> {
        let owned = Self(raw);
        if raw.is_null() {
            return Err("WASAPI returned a null endpoint ID".into());
        }
        // SAFETY: GetId vends a live NUL-terminated CoTaskMem UTF-16 string.
        let text = unsafe { owned.0.to_string() }.map_err(error)?;
        if text.is_empty() || text.len() > 4096 || text.chars().any(char::is_control) {
            return Err("WASAPI endpoint ID exceeds its profile".into());
        }
        Ok(text)
    }
}
impl Drop for Name {
    fn drop(&mut self) {
        // SAFETY: GetId ownership is transferred once to this guard.
        unsafe { CoTaskMemFree(Some(self.0.0.cast())) };
    }
}
fn endpoint_id(device: &IMMDevice) -> Result<String, String> {
    // SAFETY: live interface; Name takes ownership of the returned allocation.
    unsafe { Name::copy(device.GetId().map_err(error)?) }
}
fn neutral(volume: &IAudioEndpointVolume) -> Result<(), String> {
    // SAFETY: all access is on the owning STA; these methods only read controls.
    unsafe {
        if volume.GetMute().map_err(error)?.as_bool()
            || volume.GetMasterVolumeLevelScalar().map_err(error)? != 1.0
            || volume.GetChannelCount().map_err(error)? != 2
        {
            return Err(
                "DoP requires an unmuted stereo endpoint at unity gain; controls were preserved"
                    .into(),
            );
        }
        for channel in 0..2 {
            if volume.GetChannelVolumeLevelScalar(channel).map_err(error)? != 1.0 {
                return Err(
                    "DoP requires unity endpoint channel gains; controls were preserved".into(),
                );
            }
        }
    }
    Ok(())
}
fn wave(format: Format, carrier: Carrier) -> WAVEFORMATEXTENSIBLE {
    let width = carrier.bytes() as u16;
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: 0xfffe,
            nChannels: 2,
            nSamplesPerSec: format.carrier_rate(),
            nAvgBytesPerSec: format.carrier_rate() * u32::from(width) * 2,
            nBlockAlign: width * 2,
            wBitsPerSample: width * 8,
            cbSize: 22,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: 24,
        },
        dwChannelMask: 3,
        SubFormat: GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71),
    }
}
// Public Windows SDK ABI, independently checked against its C declarations.
const _: () = {
    assert!(size_of::<WAVEFORMATEX>() == 18);
    assert!(size_of::<WAVEFORMATEXTENSIBLE>() == 40);
};
pub(super) fn open(
    uid: &str,
    format: Format,
    input: PcmConsumer<Payload>,
    signals: Arc<Signals>,
) -> Result<Box<dyn Driver>, String> {
    let apartment = Apartment::open()?;
    signals.current()?;
    let encoded = uid.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: interfaces stay on this STA; all pointers name initialized live
    // descriptors or NUL-terminated buffers retained through each SDK call.
    let (device, client, volume, carrier, capacity, latency) = unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(error)?;
        let device = enumerator
            .GetDevice(PCWSTR(encoded.as_ptr()))
            .map_err(error)?;
        if endpoint_id(&device)? != uid || device.GetState().map_err(error)? != DEVICE_STATE_ACTIVE
        {
            return Err("the exact WASAPI endpoint is no longer active".into());
        }
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).map_err(error)?;
        neutral(&volume)?;
        let client: IAudioClient2 = device.Activate(CLSCTX_ALL, None).map_err(error)?;
        client
            .SetClientProperties(&AudioClientProperties {
                cbSize: size_of::<AudioClientProperties>() as u32,
                bIsOffload: false.into(),
                eCategory: AudioCategory_Media,
                Options: AUDCLNT_STREAMOPTIONS_RAW,
            })
            .map_err(error)?;
        let mut chosen = None;
        for carrier in [Carrier::Packed24Le, Carrier::High24In32Le] {
            let descriptor = wave(format, carrier);
            let status =
                client.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &descriptor.Format, None);
            if status.0 == 0 {
                chosen = Some(carrier);
                break;
            }
            if status != AUDCLNT_E_UNSUPPORTED_FORMAT {
                return Err(error(status));
            }
        }
        let carrier = chosen.ok_or("the endpoint has no exact exclusive 24-bit DoP carrier")?;
        let mut minimum = 0;
        client
            .GetDevicePeriod(None, Some(&mut minimum))
            .map_err(error)?;
        if !(1..=200000).contains(&minimum) {
            return Err("WASAPI minimum period exceeds the bounded DoP profile".into());
        }
        let duration = (minimum * 2).max(200000);
        let descriptor = wave(format, carrier);
        client
            .Initialize(
                AUDCLNT_SHAREMODE_EXCLUSIVE,
                0,
                duration,
                minimum,
                &descriptor.Format,
                None,
            )
            .map_err(error)?;
        let capacity = client.GetBufferSize().map_err(error)? as usize;
        let latency = client.GetStreamLatency().map_err(error)?;
        if !(1..=16384).contains(&capacity) || !(0..=20_000_000).contains(&latency) {
            return Err("WASAPI buffer/latency exceeds the bounded DoP profile".into());
        }
        (device, client, volume, carrier, capacity, latency)
    };
    // SAFETY: GetService follows successful stream initialization on this STA.
    let render = unsafe { client.GetService::<IAudioRenderClient>() }.map_err(error)?;
    let task = Task::open()?;
    signals.current()?;
    let sink = Hardware {
        device: Some(device),
        client: Some(client),
        render: Some(render),
        volume: Some(volume),
        task: Some(task),
        apartment: Some(apartment),
        uid: uid.into(),
        capacity,
        frame_bytes: 2 * carrier.bytes(),
        tail: Duration::from_secs_f64(capacity as f64 * 2.0 / f64::from(format.carrier_rate()))
            + Duration::from_nanos(latency as u64 * 100),
        started: false,
    };
    sink.verify()?;
    Ok(Box::new(Push::new(
        sink, format, carrier, input, signals, capacity,
    )?))
}
struct Hardware {
    device: Option<IMMDevice>,
    client: Option<IAudioClient2>,
    render: Option<IAudioRenderClient>,
    volume: Option<IAudioEndpointVolume>,
    task: Option<Task>,
    apartment: Option<Apartment>,
    uid: String,
    capacity: usize,
    frame_bytes: usize,
    tail: Duration,
    started: bool,
}
impl Hardware {
    fn client(&self) -> Result<&IAudioClient2, String> {
        self.client
            .as_ref()
            .ok_or_else(|| "WASAPI DoP output is closed".into())
    }
}
impl Sink for Hardware {
    fn verify(&self) -> Result<(), String> {
        let device = self
            .device
            .as_ref()
            .ok_or("WASAPI DoP endpoint is closed")?;
        // SAFETY: read-only observations on retained interfaces on their STA.
        unsafe {
            if device.GetState().map_err(error)? != DEVICE_STATE_ACTIVE
                || endpoint_id(device)? != self.uid
                || self.client()?.GetBufferSize().map_err(error)? as usize != self.capacity
            {
                return Err("WASAPI DoP endpoint identity or buffer changed".into());
            }
        }
        neutral(
            self.volume
                .as_ref()
                .ok_or("WASAPI DoP controls are closed")?,
        )
    }
    fn space(&self) -> Result<usize, String> {
        self.verify()?;
        // SAFETY: initialized exclusive timer-driven render stream on this STA.
        let padding = unsafe { self.client()?.GetCurrentPadding() }.map_err(error)? as usize;
        if self.started && padding == 0 {
            return Err("WASAPI DoP buffer ran dry; output is retired".into());
        }
        self.capacity
            .checked_sub(padding)
            .ok_or_else(|| "WASAPI padding exceeds its buffer".into())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, String> {
        if bytes.is_empty()
            || !bytes.len().is_multiple_of(self.frame_bytes)
            || bytes.len() / self.frame_bytes > self.capacity
        {
            return Err("invalid WASAPI DoP frame transfer".into());
        }
        let frames = (bytes.len() / self.frame_bytes) as u32;
        let render = self
            .render
            .as_ref()
            .ok_or("WASAPI DoP renderer is closed")?;
        // SAFETY: GetBuffer supplies exactly frames*nBlockAlign writable bytes,
        // used on the same thread and released once with that identical count.
        unsafe {
            let target = render.GetBuffer(frames).map_err(error)?;
            if target.is_null() {
                return match render.ReleaseBuffer(0, 0) {
                    Ok(()) => Err("WASAPI returned null render storage".into()),
                    Err(e) => Err(format!(
                        "WASAPI returned null render storage; release failed: {e}"
                    )),
                };
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
            // DoP idle/mute are encoded DSD silence, never the PCM SILENT flag.
            render.ReleaseBuffer(frames, 0).map_err(error)?;
        }
        Ok(frames as usize)
    }
    fn start(&mut self) -> Result<(), String> {
        // SAFETY: the owned, initialized buffer is fully prefilled by Push.
        self.started = true;
        unsafe { self.client()?.Start() }.map_err(error)?;
        Ok(())
    }
    fn tail(&self) -> Duration {
        self.tail
    }
    fn close(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if let Some(client) = self.client.as_ref() {
            if self.started {
                // SAFETY: only this control thread starts/stops this initialized stream.
                if let Err(e) = unsafe { client.Stop() } {
                    errors.push(error(e));
                }
            }
        }
        self.render.take();
        self.client.take();
        self.volume.take();
        self.device.take();
        if let Some(mut task) = self.task.take() {
            if let Err(e) = task.close() {
                errors.push(e);
            }
        }
        self.apartment.take();
        self.started = false;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
impl Drop for Hardware {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("WASAPI DoP cleanup failed: {e}");
        }
    }
}
pub(super) fn inventory() -> Result<Vec<(String, String)>, String> {
    // Keep COM initialization independent of a caller's existing MTA/STA.
    std::thread::spawn(|| {
        let _apartment = Apartment::open()?;
        // SAFETY: all enumeration interfaces and owned IDs stay on this STA.
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(error)?;
            let endpoints = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(error)?;
            let count = endpoints.GetCount().map_err(error)?;
            if count > 1024 {
                return Err("WASAPI endpoint inventory exceeds its budget".into());
            }
            let mut result = Vec::new();
            for i in 0..count {
                let uid = endpoint_id(&endpoints.Item(i).map_err(error)?)?;
                result.push((uid.clone(), uid));
            }
            Ok(result)
        }
    })
    .join()
    .map_err(|_| "WASAPI enumeration worker panicked".to_owned())?
}
