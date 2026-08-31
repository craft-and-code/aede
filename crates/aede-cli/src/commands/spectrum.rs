//! The `spectrum` command: a spectrogram beside every track.
//!
//! It reads the *catalog*, not the disk, even though it is given folders: the
//! catalog already knows which files are audio, what they claim to be, and
//! when they were last read. Walking the filesystem again would be a second
//! answer to a question already answered, and the two would drift.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use aede_core::model::Catalog;
use aede_core::{ffmpeg, spectrum};

use super::{Res, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn spectrum(args: &Args) -> Res {
    let catalog = load(args)?;
    let scope = super::scope_of(args)?;
    let redraw_everything = args.has("full");
    let dry_run = args.has("dry-run");

    let work = to_draw(&catalog, &scope, redraw_everything);
    println!("{}", ui::section("Spectrograms"));
    if work.is_empty() {
        let held = catalog
            .files
            .iter()
            .any(|f| super::in_scope(&f.path, &scope));
        println!(
            "  {}",
            match held {
                // The ordinary answer on the second run, and it has to read as
                // a result rather than as an empty screen: nothing was drawn
                // *because* nothing needed drawing.
                true => ui::green("every picture is already there and up to date"),
                false => ui::yellow("no track of the catalog is in that folder"),
            }
        );
        return Ok(());
    }

    if dry_run {
        let mut t = Table::new(&["Picture", "Track"]);
        for (audio, picture, _) in work.iter().take(20) {
            t.push(vec![
                picture.display().to_string(),
                aede_core::text::file_name(&audio.display().to_string()).to_string(),
            ]);
        }
        print!("{}", t.render());
        if work.len() > 20 {
            println!("{}", ui::dim(&format!("  … and {} more", work.len() - 20)));
        }
        println!("  {}", ui::dim(&format!("{} to draw", work.len())));
        return Ok(());
    }

    // Resolved once, before the first file: a thousand tracks would otherwise
    // mean a thousand failed lookups before the first picture.
    let program = ffmpeg::find().ok_or_else(|| ffmpeg::missing("spectrum"))?;

    // One ffmpeg per track, several at a time. Drawing a spectrogram decodes
    // the whole file and runs an FFT over it — seconds per track, hours over a
    // library — and the work is embarrassingly parallel because no two
    // pictures share anything. The same answer the scan gives to the same
    // question, down to `--threads` meaning the same thing.
    let total = work.len();
    let threads = aede_core::scan::resolve_threads(args.number_or("threads", 0)?).min(total.max(1));
    let queue = Mutex::new(work);
    let failures: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
    let done = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    // A poisoned lock must not abandon the rest of the work:
                    // one ffmpeg that panicked is not a reason to stop drawing.
                    let Some((audio, picture, caption)) =
                        queue.lock().unwrap_or_else(|e| e.into_inner()).pop()
                    else {
                        break;
                    };
                    if let Err(said) =
                        spectrum::render(&program, &audio, &picture, Some(caption.as_str()))
                    {
                        failures
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push((audio.display().to_string(), said));
                    }
                    done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        // The main thread only reports progress, so no picture waits on a
        // terminal write.
        let mut last = usize::MAX;
        while done.load(Ordering::Relaxed) < total {
            let current = done.load(Ordering::Relaxed);
            if current != last {
                last = current;
                print!("\r  drawing: {current}/{total}   ");
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    });
    println!("\r  drawing: {total}/{total}   ");

    let failures = failures.into_inner().unwrap_or_else(|e| e.into_inner());
    let drawn = total - failures.len();

    let mut t = Table::plain(2).align(1, Align::Right);
    t.push(vec!["Drawn".into(), drawn.to_string()]);
    if !failures.is_empty() {
        t.push(vec!["Failed".into(), failures.len().to_string()]);
    }
    print!("{}", t.render());

    if !failures.is_empty() {
        println!("{}", ui::section("What ffmpeg could not draw"));
        let mut t = Table::new(&["File", "Reason"]).path_limit(0, 60);
        for (path, reason) in failures.iter().take(20) {
            t.push(vec![path.clone(), reason.clone()]);
        }
        print!("{}", t.render());
        if failures.len() > 20 {
            println!(
                "{}",
                ui::dim(&format!("  … and {} more", failures.len() - 20))
            );
        }
    }
    Ok(())
}

/// The pictures that have to be drawn: `(track, picture, caption)`.
///
/// Everything already drawn and still current is left out here rather than
/// skipped later, so the count on screen is the work and not the library.
fn to_draw(
    catalog: &Catalog,
    scope: &[String],
    everything: bool,
) -> Vec<(PathBuf, PathBuf, String)> {
    catalog
        .files
        .iter()
        .filter(|file| super::in_scope(&file.path, scope))
        .filter_map(|file| {
            let audio = PathBuf::from(&file.path);
            let picture = spectrum::picture_for(&audio);
            let wanted = everything || spectrum::out_of_date(&audio, &picture);
            wanted.then(|| (audio, picture, spectrum::caption(&file.properties)))
        })
        .collect()
}
