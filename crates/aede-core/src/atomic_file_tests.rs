use super::*;

#[cfg(unix)]
#[test]
fn store_replacement_preserves_permissions_and_new_stores_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("aede_store_mode_{}_{nonce}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("user.json");
    std::fs::write(&path, b"old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    write(&path, b"new").unwrap();
    let old_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    let new_path = dir.join("conclusions.json");
    write(&new_path, b"new").unwrap();
    let new_mode = std::fs::metadata(&new_path).unwrap().permissions().mode() & 0o777;
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(
        old_mode, 0o600,
        "saving must not reopen a private store to other users"
    );
    assert_eq!(new_mode, 0o600, "new stores default to owner-only access");
}
