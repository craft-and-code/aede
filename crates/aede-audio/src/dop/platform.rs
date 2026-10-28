//! Explicit native endpoint selection; no default route or backend fallback.
use super::driver::Factory;
use std::sync::Arc;

pub(super) fn factory(device: &str) -> Result<Factory, String> {
    #[cfg(target_os = "macos")]
    let prefix = "coreaudio:";
    #[cfg(target_os = "windows")]
    let prefix = "wasapi:";
    #[cfg(all(target_os = "linux", feature = "dop-alsa"))]
    let prefix = "alsa:";
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa")
    )))]
    let prefix = "unsupported:";
    let id = device
        .strip_prefix(prefix)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 4096
                && !id.chars().any(char::is_control)
                && id.trim() == *id
        })
        .ok_or("DoP needs an explicit compatible DAC ID for this native backend")?
        .to_owned();
    Ok(Arc::new(move |format, input, signals| {
        #[cfg(target_os = "macos")]
        {
            super::coreaudio::Session::open(&id, format, input, signals)
                .map(|native| Box::new(native) as Box<dyn super::driver::Driver>)
        }
        #[cfg(target_os = "windows")]
        {
            super::wasapi::open(&id, format, input, signals)
        }
        #[cfg(all(target_os = "linux", feature = "dop-alsa"))]
        {
            super::alsa::open(&id, format, input, signals)
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            all(target_os = "linux", feature = "dop-alsa")
        )))]
        {
            let _ = (&id, format, input, signals);
            Err("no native DoP backend in this build".into())
        }
    }))
}

pub(super) fn devices() -> Result<Vec<(String, String)>, String> {
    #[cfg(target_os = "macos")]
    let (prefix, devices) = ("coreaudio", super::coreaudio::inventory()?);
    #[cfg(target_os = "windows")]
    let (prefix, devices) = ("wasapi", super::wasapi::inventory()?);
    #[cfg(all(target_os = "linux", feature = "dop-alsa"))]
    let (prefix, devices) = ("alsa", super::alsa::inventory()?);
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa")
    )))]
    let (prefix, devices): (&str, Vec<(String, String)>) = ("unsupported", Vec::new());
    Ok(devices
        .into_iter()
        .map(|(id, name)| (format!("{prefix}:{id}"), name))
        .collect())
}
