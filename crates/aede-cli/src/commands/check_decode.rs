//! Explicit full decoding without audio output or persisted container verdicts.
use super::*;
use aede_core::playback::decoder::{FileDecoder, FlacMd5Status};
use std::path::Path;
use std::process::{Command, Stdio};

struct Verification {
    method: &'static str,
    hash: bool,
}
struct FileResult<'a> {
    path: &'a str,
    outcome: Result<Verification, String>,
}

pub(super) fn run(catalog: &Catalog, scope: &[String], json: bool) -> Res {
    let rows: Vec<_> = catalog
        .files
        .iter()
        .filter(|file| super::super::in_scope(&file.path, scope))
        .map(|file| FileResult {
            path: &file.path,
            outcome: verify(
                Path::new(&file.path),
                file.properties.codec == "flac",
                Some(Path::new("flac")),
            ),
        })
        .collect();
    let failed = rows.iter().filter(|row| row.outcome.is_err()).count();
    if json {
        println!("{}", structured(&rows).to_string_compact());
    } else {
        let mut table = Table::new(&["File", "Result", "Verifier"]).path_limit(0, 60);
        for row in &rows {
            match &row.outcome {
                Ok(value) => {
                    table.push(vec![row.path.into(), "Decoded".into(), value.method.into()])
                }
                Err(error) => table.push(vec![row.path.into(), error.clone(), "Failed".into()]),
            }
        }
        ui::section("Full decode test");
        print!("{}", table.render());
    }
    if failed > 0 {
        return Err(format!("{failed} file(s) failed the complete decode test").into());
    }
    Ok(())
}

fn structured(rows: &[FileResult<'_>]) -> aede_core::json::Json {
    use aede_core::json::Json;
    let mut report = Json::obj();
    report.set("version", Json::Num(1.0));
    report.set(
        "files",
        Json::Arr(
            rows.iter()
                .map(|row| {
                    let mut value = Json::obj();
                    value.set("path", Json::Str(row.path.into()));
                    match &row.outcome {
                        Ok(result) => {
                            value.set(
                                "status",
                                Json::Str(if result.hash { "ok" } else { "no_hash" }.into()),
                            );
                            value.set("verifier", Json::Str(result.method.into()));
                        }
                        Err(error) => {
                            value.set("status", Json::Str("ko".into()));
                            value.set("verifier", Json::Str("Failed".into()));
                            value.set("message", Json::Str(error.clone()));
                        }
                    }
                    value
                })
                .collect(),
        ),
    );
    report
}

fn verify(path: &Path, flac: bool, program: Option<&Path>) -> Result<Verification, String> {
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
            Ok(status) if status.success() => {
                let status = FileDecoder::open(path)
                    .map_err(|error| error.to_string())?
                    .flac_md5_status();
                let hash = match status {
                    Some(FlacMd5Status::Pending | FlacMd5Status::Verified) => true,
                    Some(FlacMd5Status::NoSignature) => false,
                    _ => return Err("FLAC MD5 verification state is invalid".into()),
                };
                Verification {
                    method: if hash {
                        "flac -t"
                    } else {
                        "flac -t · decoded, no FLAC MD5"
                    },
                    hash,
                }
            }
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
fn native(path: &Path) -> Result<Verification, String> {
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
        Some(FlacMd5Status::Verified) => Ok(Verification {
            method: "Rust · FLAC MD5 verified",
            hash: true,
        }),
        Some(FlacMd5Status::NoSignature) => Ok(Verification {
            method: "Rust · decoded, no FLAC MD5",
            hash: false,
        }),
        Some(_) => Err("FLAC MD5 verification did not complete".into()),
        None => Ok(Verification {
            method: "Rust · decoded, no FLAC MD5",
            hash: false,
        }),
    }
}
#[cfg(test)]
#[path = "check_decode_tests.rs"]
mod tests;
