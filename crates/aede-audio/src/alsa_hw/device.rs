//! Read neutral controls on the same retained hardware card and monitor its events.
use super::*;
use crate::dsp::ChannelLayout;
use alsa::{
    Ctl,
    ctl::{ElemIface, ElemType, ElemValue},
    hctl::HCtl,
};
/// Verify neutral controls and retain event monitoring; never alter a mixer.
pub fn open_controls(
    card: i32,
    device: u32,
    subdevice: u32,
    layout: ChannelLayout,
) -> Result<Ctl, String> {
    if card < 0 {
        return Err("ALSA hardware card identity is unknown".to_owned());
    }
    let name = format!("hw:{card}");
    let ctl = Ctl::new(&name, true).map_err(|error| error.to_string())?;
    let actual_card = ctl
        .card_info()
        .map_err(|error| error.to_string())?
        .get_card()
        .get_index();
    if actual_card != card {
        return Err("ALSA controls resolved to another hardware card".to_owned());
    }
    ctl.subscribe_events(true)
        .map_err(|error| error.to_string())?;
    let inventory = ctl.elem_list().map_err(|error| error.to_string())?;
    if inventory.get_used() > 256 {
        return Err("ALSA control inventory exceeds the bounded strict profile".to_owned());
    }
    let hctl =
        HCtl::new(&name, true).map_err(|error| format!("cannot observe ALSA controls: {error}"))?;
    hctl.load().map_err(|error| error.to_string())?;
    let mut verified_map = false;
    for index in 0..inventory.get_used() {
        let id = inventory.get_id(index).map_err(|error| error.to_string())?;
        let name = id.get_name().map_err(|error| error.to_string())?;
        let display_name = format!("{name:?}");
        let is_map = inventory
            .get_interface(index)
            .map_err(|error| error.to_string())?
            == ElemIface::PCM
            && name == "Playback Channel Map";
        if is_map && (id.get_device() != device || id.get_index() != subdevice) {
            continue;
        }
        let element = hctl
            .find_elem(&id)
            .ok_or("ALSA control metadata changed during admission")?;
        let info = element.info().map_err(|error| error.to_string())?;
        if !(1..=2).contains(&info.get_count()) {
            return Err(format!("unknown ALSA control shape: {display_name}"));
        }
        let mut value = ElemValue::new(info.get_type()).map_err(|error| error.to_string())?;
        value.set_id(&id);
        // Read values and retain event monitoring on the same verified card
        // handle. PCM::get_chmap can instead return an ALSA config override.
        ctl.elem_read(&mut value)
            .map_err(|error| error.to_string())?;
        if is_map {
            if verified_map {
                return Err("ALSA selected channel-map control is ambiguous".to_owned());
            }
            let positions = (0..info.get_count())
                .map(|channel| {
                    value
                        .get_integer(channel)
                        .ok_or("invalid ALSA channel-map control")
                })
                .collect::<Result<Vec<_>, _>>()?;
            verify_control_map(
                (card, device, subdevice),
                layout,
                Some(ControlMap {
                    card: actual_card,
                    device: id.get_device(),
                    subdevice: id.get_subdevice(),
                    index: id.get_index(),
                    integer: info.get_type() == ElemType::Integer,
                    positions: &positions,
                }),
            )?;
            verified_map = true;
            continue;
        }
        let mut values = Vec::with_capacity(info.get_count() as usize);
        let mut decibels = Vec::with_capacity(info.get_count() as usize);
        for channel in 0..info.get_count() {
            match info.get_type() {
                ElemType::Integer => {
                    let integer = value
                        .get_integer(channel)
                        .ok_or("invalid ALSA integer control")?;
                    values.push(i64::from(integer));
                    decibels.push(
                        ctl.convert_to_db(&id, i64::from(integer))
                            .map_err(|error| {
                                format!("unknown ALSA gain for {display_name}: {error}")
                            })?
                            .0,
                    );
                }
                ElemType::Boolean => values.push(i64::from(
                    value
                        .get_boolean(channel)
                        .ok_or("invalid ALSA switch control")?,
                )),
                _ => {
                    return Err(format!(
                        "unknown ALSA enhancement or routing control: {display_name}"
                    ));
                }
            }
        }
        let kind = match info.get_type() {
            ElemType::Integer => ControlKind::Integer,
            ElemType::Boolean => ControlKind::Boolean,
            _ => ControlKind::Unknown,
        };
        if !neutral_control(
            name,
            kind,
            &values,
            (!decibels.is_empty()).then_some(decibels.as_slice()),
        ) {
            return Err(format!(
                "ALSA control is modifying or unverified: {display_name}; strict output does not change system controls"
            ));
        }
    }
    if !verified_map {
        verify_control_map((card, device, subdevice), layout, None)?;
    }
    Ok(ctl)
}
