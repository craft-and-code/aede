use super::*;
use std::cell::Cell;
#[test]
fn cancellation_during_seek_preparation_cannot_replace_or_resume_the_old_stream() {
    let active = Cell::new(true);
    let committed = Cell::new(false);
    let result = replace_prepared(
        || {
            if active.get() {
                Ok(())
            } else {
                Err("stopped during prepare".into())
            }
        },
        || {
            active.set(false);
            Ok("prepared seek stream")
        },
        |_| {
            committed.set(true);
            Ok(())
        },
    );
    assert!(
        result.is_err(),
        "a Stop during preparation was reported as a successful seek"
    );
    assert!(
        !committed.get(),
        "the cancelled stream was replaced/resumed"
    );
}
#[test]
fn a_retired_seek_does_not_open_any_decoder_or_device() {
    let opened = Cell::new(false);
    let result = replace_prepared(
        || Err("retired".into()),
        || {
            opened.set(true);
            Ok(())
        },
        |_| Ok(()),
    );
    assert!(result.is_err());
    assert!(!opened.get());
}
#[test]
fn a_current_preparation_keeps_its_normal_commit() {
    let committed = Cell::new(0);
    replace_prepared(
        || Ok(()),
        || Ok(7),
        |value| {
            committed.set(value);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(committed.get(), 7);
}

#[test]
fn cancelled_prepared_resources_are_dropped_without_publication() {
    struct Prepared<'a>(&'a Cell<usize>);
    impl Drop for Prepared<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let active = Cell::new(true);
    let dropped = Cell::new(0);
    let result = replace_prepared(
        || {
            if active.get() {
                Ok(())
            } else {
                Err("retired".into())
            }
        },
        || {
            active.set(false);
            Ok(Prepared(&dropped))
        },
        |_| panic!("retired resource was published"),
    );
    assert!(result.is_err());
    assert_eq!(dropped.get(), 1);
}
#[test]
fn stop_after_the_real_file_header_prevents_any_device_preparation() {
    let wave = crate::test_support::Wave::new(&[0.25, -0.25], 2);
    let checks = Cell::new(0);
    let result = MinimalPlayer::open_admitted(&wave.0, Arc::new(Gain::default()), || {
        checks.set(checks.get() + 1);
        if checks.get() == 1 {
            Ok(())
        } else {
            Err("Stop after file header".into())
        }
    });
    assert_eq!(
        result.err().expect("cancelled open"),
        "Stop after file header"
    );
    assert_eq!(checks.get(), 2);
}

#[test]
fn stop_while_the_previous_worker_is_joined_cannot_resume_the_replacement() {
    struct Visit<'a> {
        active: &'a Cell<bool>,
        cancel_on_drop: bool,
        paused: Cell<bool>,
    }
    impl Drop for Visit<'_> {
        fn drop(&mut self) {
            // Deterministic completion of an old worker join after a concurrent Stop.
            if self.cancel_on_drop {
                self.active.set(false);
            }
        }
    }
    for playing in [true, false] {
        let active = Cell::new(true);
        let mut old = Visit {
            active: &active,
            cancel_on_drop: true,
            paused: Cell::new(!playing),
        };
        let next = Visit {
            active: &active,
            cancel_on_drop: false,
            paused: Cell::new(true),
        };
        let result = install_prepared(
            &mut old,
            next,
            &|| {
                if active.get() {
                    Ok(())
                } else {
                    Err("Stop during worker retirement".into())
                }
            },
            |v| v.paused.get(),
            |v| v.paused.set(true),
            |v| v.paused.set(false),
        );
        assert!(result.is_err(), "retired replacement reported success");
        assert!(
            old.paused.get(),
            "new visit resumed after Stop during old worker retirement"
        );
    }
}

#[test]
fn a_current_replacement_keeps_playing_or_paused_after_old_visit_retirement() {
    struct Visit(Cell<bool>);
    for playing in [true, false] {
        let mut old = Visit(Cell::new(!playing));
        let next = Visit(Cell::new(true));
        install_prepared(
            &mut old,
            next,
            &|| Ok(()),
            |v| v.0.get(),
            |v| v.0.set(true),
            |v| v.0.set(false),
        )
        .unwrap();
        assert_eq!(old.0.get(), !playing);
    }
}
