use super::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
struct Fake(Rc<RefCell<State>>);
struct State {
    hog: i32,
    rate: f64,
    physical: Description,
    virtual_format: Description,
    fail: Option<&'static str>,
    events: Vec<&'static str>,
}
impl Fake {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(State {
            hog: -1,
            rate: 48000.0,
            physical: description(48000.0),
            virtual_format: description(48000.0),
            fail: None,
            events: Vec::new(),
        })))
    }
    fn action(&self, name: &'static str) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state.events.push(name);
        if state.fail == Some(name) {
            state.fail = None;
            Err(format!("injected {name}"))
        } else {
            Ok(())
        }
    }
}
fn description(rate: f64) -> Description {
    Description {
        rate,
        id: u32::from_be_bytes(*b"lpcm"),
        flags: 4 | 8 | 64,
        bytes_per_packet: 6,
        frames_per_packet: 1,
        bytes_per_frame: 6,
        channels: 2,
        bits: 24,
        reserved: 0,
    }
}
fn plan() -> Plan {
    Plan {
        physical: description(176400.0),
        virtual_format: description(176400.0),
        carrier: crate::dop::Carrier::Packed24Le,
    }
}
impl Backend for Fake {
    fn hog(&self) -> Result<i32, String> {
        Ok(self.0.borrow().hog)
    }
    fn set_hog(&mut self, value: i32) -> Result<(), String> {
        self.action("hog")?;
        self.0.borrow_mut().hog = value;
        Ok(())
    }
    fn rate(&self) -> Result<f64, String> {
        Ok(self.0.borrow().rate)
    }
    fn set_rate(&mut self, value: f64) -> Result<(), String> {
        self.action("rate")?;
        let mut state = self.0.borrow_mut();
        state.rate = value;
        // HAL rate changes may implicitly reset both stream formats.
        state.physical = description(value);
        state.virtual_format = description(value);
        Ok(())
    }
    fn physical(&self) -> Result<Description, String> {
        Ok(self.0.borrow().physical)
    }
    fn set_physical(&mut self, value: Description) -> Result<(), String> {
        self.action("physical")?;
        self.0.borrow_mut().physical = value;
        Ok(())
    }
    fn virtual_format(&self) -> Result<Description, String> {
        Ok(self.0.borrow().virtual_format)
    }
    fn set_virtual(&mut self, value: Description) -> Result<(), String> {
        self.action("virtual")?;
        self.0.borrow_mut().virtual_format = value;
        Ok(())
    }
    fn health(&self) -> Result<(), String> {
        self.action("health")
    }
}

fn restored(fake: &Fake) {
    let s = fake.0.borrow();
    assert_eq!(s.hog, -1);
    assert_eq!(s.rate, 48000.0);
    assert_eq!(s.physical, description(48000.0));
    assert_eq!(s.virtual_format, description(48000.0));
}

#[test]
fn exclusive_preparation_and_close_restore_all_original_formats_once() {
    let fake = Fake::new();
    let mut lease = Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).unwrap();
    lease.verify().unwrap();
    assert_eq!(fake.0.borrow().hog, 77);
    lease.restore().unwrap();
    restored(&fake);
    let count = fake.0.borrow().events.len();
    lease.restore().unwrap();
    drop(lease);
    assert_eq!(fake.0.borrow().events.len(), count);
}

#[test]
fn every_partial_preparation_failure_rolls_back_implicit_rate_changes() {
    for failure in ["hog", "rate", "physical", "virtual"] {
        let fake = Fake::new();
        fake.0.borrow_mut().fail = Some(failure);
        assert!(Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).is_err());
        restored(&fake);
    }
}

#[test]
fn existing_or_lost_ownership_never_changes_another_sessions_settings() {
    for owner in [77, 91] {
        let fake = Fake::new();
        fake.0.borrow_mut().hog = owner;
        assert!(Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).is_err());
        assert_eq!(fake.0.borrow().hog, owner);
        assert!(!fake.0.borrow().events.contains(&"hog"));
    }
    let fake = Fake::new();
    let mut lease = Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).unwrap();
    fake.0.borrow_mut().hog = 91;
    let count = fake.0.borrow().events.len();
    assert!(lease.restore().is_err());
    assert_eq!(fake.0.borrow().events.len(), count);
}

#[test]
fn external_format_changes_are_preserved_and_owned_lease_is_released() {
    let fake = Fake::new();
    let mut lease = Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).unwrap();
    fake.0.borrow_mut().virtual_format = description(192000.0);
    assert!(lease.verify().is_err());
    assert!(lease.restore().is_err());
    assert_eq!(fake.0.borrow().virtual_format.rate, 192000.0);
    assert_eq!(fake.0.borrow().hog, -1);
}

#[test]
fn admission_retirement_during_preparation_still_restores_the_device() {
    let fake = Fake::new();
    let calls = std::cell::Cell::new(0);
    assert!(
        Lease::acquire(fake.clone(), plan(), 77, &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 4 {
                Err("retired".into())
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    restored(&fake);
}

#[test]
fn failed_post_acquisition_setup_returns_its_rollback_failure_to_the_caller() {
    for stage in ["geometry query failed", "callback capacity exhausted"] {
        let fake = Fake::new();
        let lease = Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).unwrap();
        fake.0.borrow_mut().fail = Some("hog");
        let failure = lease.checked::<()>(Err(stage.into()));
        let error = failure.err().expect("setup must fail");
        assert!(error.contains(stage), "{error}");
        assert!(error.contains("injected hog"), "{error}");
        assert!(error.contains("did not release"), "{error}");
        assert_eq!(fake.0.borrow().hog, 77);
    }
}

#[test]
fn post_acquisition_setup_keeps_ownership_on_success_and_restores_on_failure() {
    let fake = Fake::new();
    let lease = Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).unwrap();
    let (mut lease, geometry) = lease
        .checked(Ok(256))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(geometry, 256);
    assert_eq!(fake.0.borrow().hog, 77);
    lease.restore().unwrap();
    restored(&fake);
    let lease = Lease::acquire(fake.clone(), plan(), 77, &|| Ok(())).unwrap();
    assert_eq!(
        lease.checked::<()>(Err("geometry refused".into())).err(),
        Some("geometry refused".into())
    );
    restored(&fake);
}
