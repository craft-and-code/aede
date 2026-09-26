//! `aede backup` and `aede restore`: one file out, one file in.
//!
//! Two commands rather than `backup --restore`, and the reason is the one that
//! renamed `artwork` to `extract`: **a command that writes is named for the
//! writing.** Restoring replaces up to four stores at once — the most destructive
//! thing this program can be asked to do to its own data — and hiding that
//! direction behind an option on a command called *backup* would put the
//! dangerous half under the reassuring name.
//!
//! # What a restore will and will not do
//!
//! It replaces a store the backup holds. It **never deletes** one the backup
//! does not: a backup made before anything was fetched carries no
//! `sources.json`, and treating that as "there should be none" would silently
//! throw away a layer that took twenty minutes of polite requests to build.
//! Left alone, and said out loud — the reader can then decide, which they
//! cannot do about a file that is already gone.
//!
//! Each store is also refused on its own. A backup whose *catalog* this build
//! cannot read still restores the notes, because losing irreplaceable data
//! in order to protect the rebuildable one would be the wrong trade twice over.

use aede_core::backup::{self, Backup, Part};
use aede_core::{clock, conclusions, sources, store, user};

use super::Res;
use crate::args::Args;
use crate::ui;

/// Where the file goes, or where it comes from.
///
/// A positional rather than `--output`, because the file is not a detail of
/// this command: it is what the command is about. `aede backup` with nothing
/// after it is refused rather than defaulted — a backup written somewhere the
/// reader did not choose is a backup they will not find.
fn named(args: &Args, form: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    match args.positionals.first() {
        Some(path) if !path.trim().is_empty() => Ok(std::path::PathBuf::from(path)),
        _ => Err(format!("name the file: {form}").into()),
    }
}

/// `aede backup <file>`: everything Aède knows, in one document.
pub fn backup(args: &Args) -> Res {
    let path = named(args, "aede backup <file>")?;
    let data = super::data_dir(args);

    println!("{}", ui::section("Backup"));

    // Overwriting a backup is the one mistake here that cannot be undone by
    // running the command again, so it asks — through the shared prompt, since
    // a second confirmation worded slightly differently is how a reader learns
    // to stop reading them.
    if path.exists() {
        println!(
            "  {} already exists",
            ui::yellow(&path.display().to_string())
        );
        if !super::confirmed(args, "overwrite it")? {
            return Err("nothing was written".into());
        }
    }

    // Read straight off the disk rather than through the loaders that refuse a
    // missing catalog: a data folder with notes and no catalog is a perfectly
    // ordinary thing to want backed up.
    let catalog = part(store::load(&store::catalog_path(&data)));
    let gathered = match part(conclusions::load(&conclusions::conclusions_path(&data))) {
        Part::Empty => catalog
            .held()
            .map(conclusions::Conclusions::from_catalog)
            .filter(|legacy| !legacy.files.is_empty() || !legacy.analyses.is_empty())
            .map(Part::Held)
            .unwrap_or(Part::Empty),
        other => other,
    };
    let made = Backup {
        made_at: clock::now_seconds(),
        made_by: env!("CARGO_PKG_VERSION").to_string(),
        catalog,
        conclusions: gathered,
        user: part(user::load(&user::user_path(&data))),
        sources: part(sources::load(&sources::sources_path(&data))),
    };

    // The same store summary a restore prints, from the same function, so the two
    // commands describe one store in one set of words. Written twice, they
    // would have drifted the first time a field was added to `user.json`.
    for (name, state) in summarise(&made, "nothing here to save") {
        println!("  {name:<18} {}", state.said());
    }

    if made.is_empty() {
        // Writing an empty document and reporting success would teach the
        // reader that the command works — which is precisely the belief that
        // costs them the library later.
        return Err(format!(
            "nothing to back up: {} holds no catalog, conclusions, notes or fetched facts",
            data.display()
        )
        .into());
    }

    backup::write(&made, &path)?;
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    println!(
        "{} {} ({})",
        ui::green("→"),
        path.display(),
        aede_core::text::format_size(size)
    );
    println!(
        "  {}",
        ui::dim("aede restore puts it back; nothing here reads or writes your audio")
    );
    Ok(())
}

/// `aede restore <file>`: put a backup back.
pub fn restore(args: &Args) -> Res {
    let path = named(args, "aede restore <file>")?;
    let data = super::data_dir(args);
    let held = backup::read(&path)?;

    println!("{}", ui::section("Restore"));
    println!(
        "  made {} by Aède {}",
        ui::since(held.made_at),
        match held.made_by.is_empty() {
            true => "of an unknown version".to_string(),
            false => held.made_by.clone(),
        }
    );
    println!("  into {}", ui::dim(&data.display().to_string()));

    // Said before the question is asked, not after it is answered: a reader
    // agreeing to "replace files" has agreed to nothing they can picture.
    let into = [
        store::catalog_path(&data),
        conclusions::conclusions_path(&data),
        user::user_path(&data),
        sources::sources_path(&data),
    ];
    let mut writing = 0usize;
    for ((name, state), path) in summarise(&held, "not in this backup").iter().zip(&into) {
        match state {
            Doing::Write(what) => {
                writing += 1;
                let here = match path.exists() {
                    true => "replaces what is there",
                    false => "nothing there yet",
                };
                println!("  {name:<18} {what} — {}", ui::dim(here));
            }
            // Nothing is written, so nothing is lost: the file on disk stays as
            // it is whether the backup is silent about it or holds something
            // this build refuses.
            Doing::Skip(why) => println!(
                "  {name:<18} {}",
                ui::dim(&format!("{why} — left as it is"))
            ),
        }
    }
    if writing == 0 {
        return Err("nothing in this backup can be restored by this build".into());
    }
    println!(
        "  {}",
        ui::dim("a store this backup does not hold is left exactly as it is, never deleted")
    );

    if !super::confirmed(args, "restore it")? {
        return Err("nothing was restored".into());
    }

    if let Some(catalog) = held.catalog.held() {
        store::save_catalog_only(catalog, &store::catalog_path(&data))?;
    }
    if let Some(gathered) = held.conclusions.held() {
        conclusions::save(gathered, &conclusions::conclusions_path(&data))?;
    }
    if let Some(data_of_user) = held.user.held() {
        user::save(data_of_user, &user::user_path(&data))?;
    }
    if let Some(layer) = held.sources.held() {
        sources::save(layer, &sources::sources_path(&data))?;
    }
    println!(
        "{} {} restored",
        ui::green("→"),
        ui::plural(writing, "store")
    );
    if let Some(catalog) = held.catalog.held() {
        catch_up(catalog);
    }
    println!("  {}", ui::dim("your audio was not touched"));
    Ok(())
}

/// Says that a restored catalog is a photograph, and what to do about it.
///
/// **A restore does not put the library back; it puts back what the library
/// looked like.** The first version said "aede scan checks the library against
/// it", which is both vague and half the truth: a scan does not check, it
/// reconciles, and it reconciles in *both* directions — files added since are
/// read in, files gone since are dropped. A reader told only that something is
/// "checked" has no reason to run it, and will go on browsing a catalog that
/// lists records they deleted a month ago.
///
/// The date makes it concrete. "Run a scan" is advice nobody weighs; "this
/// describes your library as it was twelve days ago" is a fact they can weigh
/// against what they have been doing for twelve days.
fn catch_up(catalog: &aede_core::model::Catalog) {
    println!(
        "  {}",
        ui::dim(&format!(
            "this catalog was scanned {} and describes the library as it was then",
            ui::since(catalog.scanned_at)
        ))
    );
    println!(
        "  {}",
        ui::dim(
            "aede scan brings it up to date: files added since are read in, files gone since are dropped"
        )
    );

    // The case this command exists for is a disk that failed, and the machine
    // that reads the backup is often not the machine that wrote it. A watched
    // folder that is not here is not an error — the drive may simply not be
    // mounted yet — but a scan run before mounting it would drop every file
    // under it, which is the one way a restore can lose more than it returned.
    let absent: Vec<&String> = catalog
        .roots
        .iter()
        .filter(|root| !std::path::Path::new(root).exists())
        .collect();
    if absent.is_empty() {
        return;
    }
    println!(
        "  {} {} on this machine:",
        ui::yellow("!"),
        match absent.len() {
            1 => "one watched folder is not".to_string(),
            n => format!("{n} watched folders are not"),
        }
    );
    for root in &absent {
        println!("      {root}");
    }
    println!(
        "  {}",
        ui::dim(
            "a scan now would drop every file under it — mount it first, or \
             aede roots --remove says you meant to let it go"
        )
    );
}

/// What a command is about to do with one store, and what it holds.
enum Doing {
    /// It goes in, or comes out. Carries what it amounts to, in a few words.
    Write(String),
    /// It does not, and why. Nothing on disk changes either way.
    Skip(String),
}

impl Doing {
    /// One line for a reader, whichever it is.
    fn said(&self) -> String {
        match self {
            Doing::Write(what) => what.clone(),
            Doing::Skip(why) => ui::dim(why).to_string(),
        }
    }
}

/// The four stores of a backup, named and described, in a fixed order.
///
/// **One function for both commands.** `backup` and `restore` talk about the
/// same four things, and two lists of wording would have drifted the first
/// time a field was added to one of them — the same reason the role vocabulary
/// is one table read in both directions. Only the words for "there is none"
/// differ, because they mean different things on the way out and on the way in,
/// so that one is passed in.
fn summarise(held: &Backup, nothing: &str) -> [(&'static str, Doing); 4] {
    [
        ("catalog", state(&held.catalog, catalog_of, nothing)),
        (
            "conclusions",
            state(&held.conclusions, conclusions_of, nothing),
        ),
        ("what you said", state(&held.user, user_of, nothing)),
        (
            "what sources said",
            state(&held.sources, sources_of, nothing),
        ),
    ]
}

/// One store: what it holds, or why there is nothing to do with it.
fn state<T>(part: &Part<T>, of: impl Fn(&T) -> String, nothing: &str) -> Doing {
    match part {
        Part::Held(store) => Doing::Write(of(store)),
        Part::Empty => Doing::Skip(nothing.to_string()),
        // Never silent. A store that will not read is the one thing here that
        // somebody must be told about now rather than discover at the moment
        // they most need the file.
        Part::Unreadable(why) => Doing::Skip(format!("{} {why}", ui::red("×"))),
    }
}

/// A store read from disk, as a part of a backup.
///
/// A store that is not there is [`Part::Empty`] and not an error: a data folder
/// with notes and no catalog is ordinary, and refusing to back it up because of
/// the missing half would refuse the half that matters.
fn part<T>(read: Result<Option<T>, store::StoreError>) -> Part<T> {
    match read {
        Ok(Some(store)) => Part::Held(store),
        Ok(None) => Part::Empty,
        // Kept rather than fatal, and for the reason the whole file is split
        // into independent parts: an unreadable catalog must not stop the notes being
        // saved. It is reported, so nobody discovers it at restore time.
        Err(why) => Part::Unreadable(why.to_string()),
    }
}

fn catalog_of(catalog: &aede_core::model::Catalog) -> String {
    format!(
        "{}, {}",
        ui::plural(catalog.tracks.len(), "track"),
        ui::plural(catalog.releases.len(), "album")
    )
}

fn conclusions_of(gathered: &conclusions::Conclusions) -> String {
    format!(
        "{}, {}",
        ui::plural(gathered.files.len(), "file result"),
        ui::plural(gathered.analyses.len(), "analysis")
    )
}

/// What is in `user.json`, counting only what is in it.
///
/// The zeros are left out: "2 annotations, 0 collection, 0 record set aside"
/// spends two thirds of a line saying nothing, and a reader checking that their
/// notes are in the file has to find the one number that is not a zero. A store
/// holding none of these does not exist on disk in the first place, so there
/// is always something to say.
fn user_of(data: &aede_core::user::UserData) -> String {
    let counts = [
        (data.annotations.len(), "annotation"),
        (data.relation_annotations.len(), "relation annotation"),
        (data.collections.len(), "collection"),
        (data.set_aside.len(), "record set aside"),
        (data.plays.len(), "play"),
    ];
    let said: Vec<String> = counts
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, what)| ui::plural(*count, what))
        .collect();
    match said.is_empty() {
        true => "nothing written yet".to_string(),
        false => said.join(", "),
    }
}

fn sources_of(layer: &aede_core::sources::Sources) -> String {
    ui::plural(layer.records.len(), "record")
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
