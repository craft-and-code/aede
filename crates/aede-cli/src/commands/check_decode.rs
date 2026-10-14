//! Explicit full decoding without audio output or persisted container verdicts.
use super::*;
use aede_core::playback::decoder::{FileDecoder, FlacMd5Status};
use std::path::Path;
use std::process::{Command, Stdio};

pub(super) fn run(catalog: &Catalog, scope: &[String]) -> Res {
    let mut table = Table::new(&["File", "Result", "Verifier"]).path_limit(0, 60);
    let mut failed = 0;
    for file in catalog
        .files
        .iter()
        .filter(|file| super::super::in_scope(&file.path, scope))
    {
        match verify(
            Path::new(&file.path),
            file.properties.codec == "flac",
            Some(Path::new("flac")),
        ) {
            Ok(method) => table.push(vec![file.path.clone(), "Decoded".into(), method.into()]),
            Err(error) => {
                failed += 1;
                table.push(vec![file.path.clone(), error, "Failed".into()]);
            }
        }
    }
    ui::section("Full decode test");
    print!("{}", table.render());
    if failed > 0 {
        return Err(format!("{failed} file(s) failed the complete decode test").into());
    }
    Ok(())
}

fn verify(path: &Path, flac: bool, program: Option<&Path>) -> Result<&'static str, String> {
    let before = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !before.is_file() {
        return Err("source must be a regular file, not a link".into());
    }
    let method = if flac && let Some(program) = program {
        match Command::new(program)
            .args(["--test", "--silent", "--"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) if status.success() => "flac -t",
            Ok(_) => return Err("flac -t rejected the complete stream".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => native(path)?,
            Err(error) => return Err(error.to_string()),
        }
    } else {
        native(path)?
    };
    let after = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !after.is_file()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err("source changed during verification".into());
    }
    Ok(method)
}
fn native(path: &Path) -> Result<&'static str, String> {
    let mut decoder = FileDecoder::open(path).map_err(|e| e.to_string())?;
    let channels = usize::from(decoder.channels());
    if channels == 0 || channels > 64 {
        return Err("unsupported channel count".into());
    }
    let mut buffer = vec![0.0; 4096 * channels];
    while decoder
        .read_frames(&mut buffer)
        .map_err(|e| e.to_string())?
        != 0
    {}
    match decoder.flac_md5_status() {
        Some(FlacMd5Status::Verified) => Ok("Rust · FLAC MD5 verified"),
        Some(FlacMd5Status::NoSignature) => Ok("Rust · decoded, no FLAC MD5"),
        Some(_) => Err("FLAC MD5 verification did not complete".into()),
        None => Ok("Rust · decoded, no FLAC MD5"),
    }
}
#[cfg(test)]
#[path = "check_decode_tests.rs"]
mod tests;
