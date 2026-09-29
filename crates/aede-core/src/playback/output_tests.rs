use super::{OutputChange, OutputFormat, OutputSession};

#[cfg(unix)]
#[test]
fn output_keeps_one_process_for_matching_tracks_and_reopens_for_a_new_format() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aede_output_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let program = root.join("sink");
    std::fs::write(
        &program,
        "#!/bin/sh\nprintf x >> \"$0.count\"\ncat >> \"$0.data\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();

    let mut output = OutputSession::with_program(program.clone());
    let stereo = OutputFormat::new(44_100, 2).unwrap();
    let stereo_48k = OutputFormat::new(48_000, 2).unwrap();
    let mono = OutputFormat::new(48_000, 1).unwrap();
    assert_eq!(output.prepare(stereo).unwrap(), OutputChange::Opened);
    let first_id = output.process_id().unwrap();
    output.write_all(b"first track").unwrap();
    assert_eq!(output.prepare(stereo).unwrap(), OutputChange::Reused);
    assert_eq!(output.process_id(), Some(first_id));
    output.write_all(b"second track").unwrap();
    assert_eq!(
        output.prepare(stereo_48k).unwrap(),
        OutputChange::ReopenedForFormat
    );
    output.write_all(b"third track").unwrap();
    assert_eq!(
        output.prepare(mono).unwrap(),
        OutputChange::ReopenedForFormat
    );
    output.write_all(b"fourth track").unwrap();
    output.finish().unwrap();
    assert_eq!(output.format(), None);
    assert_eq!(
        std::fs::read(program.with_extension("count")).unwrap(),
        b"xxx"
    );
    assert_eq!(
        std::fs::read(program.with_extension("data")).unwrap(),
        b"first tracksecond trackthird trackfourth track"
    );
    std::fs::remove_dir_all(root).unwrap();
}
