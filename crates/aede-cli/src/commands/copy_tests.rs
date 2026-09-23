use super::*;

fn args(words: &[&str]) -> Args {
    Args::parse(words.iter().map(|w| w.to_string()))
}

fn recipe(convert: Option<Target>) -> Recipe {
    Recipe {
        extras: Extras::default(),
        restrict_names: false,
        convert,
        quality: None,
    }
}

#[test]
fn how_many_files_at_once_follows_the_work_and_not_the_machine() {
    // A plain copy is a queue at one device — one card, one stick, one slow
    // drive — and several writers on it seek against each other rather than
    // going faster. `--verify`, which reads back what it just wrote, makes
    // that worse still.
    assert_eq!(
        workers(&args(&["copy", "/out"]), &recipe(None), 900).unwrap(),
        1
    );

    // Encoding is arithmetic: each file is an ffmpeg run that no other file
    // waits on, and one at a time leaves most of the machine idle.
    let encoding = workers(&args(&["copy", "/out"]), &recipe(Some(Target::Mp3)), 900).unwrap();
    assert!(encoding >= 1, "at least one");
    assert_eq!(
        encoding,
        aede_core::scan::resolve_threads(0),
        "as many as the machine offers"
    );

    // And the person copying to an NVMe, or encoding on a laptop they still
    // want to use, overrides it in either direction.
    assert_eq!(
        workers(&args(&["copy", "/out", "--threads=6"]), &recipe(None), 900).unwrap(),
        6
    );
    assert_eq!(
        workers(
            &args(&["copy", "/out", "--threads=1"]),
            &recipe(Some(Target::Mp3)),
            900
        )
        .unwrap(),
        1
    );

    // Never more workers than there is work: eight threads over three files
    // is five threads spawned to find an empty queue.
    assert_eq!(
        workers(&args(&["copy", "/out", "--threads=8"]), &recipe(None), 3).unwrap(),
        3
    );
    // And an empty plan still asks for one, rather than for none.
    assert_eq!(
        workers(&args(&["copy", "/out", "--threads=8"]), &recipe(None), 0).unwrap(),
        1
    );
}
