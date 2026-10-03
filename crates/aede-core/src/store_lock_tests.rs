use super::*;

#[test]
fn the_lock_is_shared_between_callers_and_released_on_drop() {
    let folder = std::env::temp_dir().join(format!(
        "aede-lock-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let first = StoreLock::acquire(&folder).expect("first lock");
    // A helper fork can briefly retain a duplicate until its exec closes it.
    // Keeping one open makes that lifetime deterministic without spawning.
    let inherited = first._file.try_clone().expect("duplicate lock handle");
    let busy = StoreLock::try_acquire(&folder);
    assert!(matches!(busy, Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    drop(first);
    StoreLock::try_acquire(&folder).expect("lock after release");
    drop(inherited);
    std::fs::remove_file(folder.join(LOCK_FILE)).expect("remove test lock");
    std::fs::remove_dir(&folder).expect("remove test folder");
}
