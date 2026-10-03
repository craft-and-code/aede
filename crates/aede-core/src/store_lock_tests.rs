use super::*;

use super::test_support as persistence_test_support;

#[cfg(unix)]
#[test]
fn a_writer_lock_cannot_be_redirected_through_a_symbolic_link() {
    let directory = persistence_test_support::Directory::new("linked_lock");
    let original = directory.path().join("original.lock");
    std::fs::write(&original, b"preserved lock bytes").unwrap();
    std::os::unix::fs::symlink(&original, directory.path().join(LOCK_FILE)).unwrap();
    assert!(StoreLock::try_acquire(directory.path()).is_err());
    assert_eq!(std::fs::read(&original).unwrap(), b"preserved lock bytes");
}

#[cfg(unix)]
#[test]
fn newly_created_writer_locks_are_private_to_the_owner() {
    use std::os::unix::fs::PermissionsExt;
    let directory = persistence_test_support::Directory::new("private_lock");
    let lock = StoreLock::try_acquire(directory.path()).unwrap();
    let mode = std::fs::metadata(directory.path().join(LOCK_FILE))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0);
    drop(lock);
}

#[test]
fn the_lock_is_shared_between_callers_and_released_on_drop() {
    let directory = persistence_test_support::Directory::new("shared_lock");
    let folder = directory.path();
    let first = StoreLock::acquire(folder).expect("first lock");
    // A helper fork can briefly retain a duplicate until its exec closes it.
    // Keeping one open makes that lifetime deterministic without spawning.
    let inherited = first._file.try_clone().expect("duplicate lock handle");
    let busy = StoreLock::try_acquire(folder);
    assert!(matches!(busy, Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    drop(first);
    StoreLock::try_acquire(folder).expect("lock after release");
    drop(inherited);
    std::fs::remove_file(folder.join(LOCK_FILE)).expect("remove test lock");
}
