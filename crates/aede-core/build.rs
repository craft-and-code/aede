fn main() {
    let lock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let text = std::fs::read_to_string(lock).expect("workspace lockfile");
    let package = text
        .split("[[package]]")
        .find(|entry| {
            entry
                .lines()
                .any(|line| line == r#"name = "flaccompagnon-core""#)
        })
        .expect("locked FlacCompagnon");
    let version = package
        .lines()
        .find_map(|line| {
            line.strip_prefix(r#"version = ""#)
                .and_then(|value| value.strip_suffix('"'))
        })
        .expect("locked dependency version");
    println!("cargo:rustc-env=FLACCOMPAGNON_VERSION={version}");
}
