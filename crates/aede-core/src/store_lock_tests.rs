use super::*;

#[test]
fn the_lock_is_shared_between_callers_and_released_on_drop() {
    let folder = std::env::temp_dir().join(format!(
        "aede-lock-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let first = StoreLock::acquire(&folder).expect("first lock");
    let busy = StoreLock::try_acquire(&folder);
    assert!(matches!(busy, Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    drop(first);
    StoreLock::try_acquire(&folder).expect("lock after release");
    std::fs::remove_file(folder.join(LOCK_FILE)).expect("remove test lock");
    std::fs::remove_dir(&folder).expect("remove test folder");
}
