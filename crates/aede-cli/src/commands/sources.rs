//! The `sources` command: what somebody else said about this library.
//!
//! The catalog holds what the files say, the annotation file holds what you
//! say, and `sources.json` holds what a source says — MusicBrainz first, a
//! plugin later. This is the command that shows it, takes it in, and drops it.
//!
//! It exists before anything fetches, on purpose. A store that can only be
//! filled over the network can only be tested over the network, and the layer
//! is worth more than that: `--import` reads a document in exactly the shape
//! the file has, so the whole of it — attachment, agreement, removal — can be
//! exercised offline, and so can a bug report.
//!
//! The shape follows `aede import`, which does the same job for another tool's
//! analyses: `--list` says what is held, `--forget` removes it, `--source`
//! narrows to one. Two stores of what somebody else said should not be learned
//! twice.

use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::sources::{self, Confidence, Facts, SourceRecord, Sources, Verdict};
use aede_core::user::EntityRef;

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn sources(args: &Args) -> Res {
    let dir = data_dir(args);
    let path = sources::sources_path(&dir);

    if args.has("import") {
        return import(args, &path);
    }
    if args.has("template") {
        return template(args);
    }
    if args.has("export") {
        return export(args, &path);
    }
    if args.has("forget") {
        return forget(args, &path);
    }

    let held = sources::load(&path)?.unwrap_or_default();
    if held.records.is_empty() {
        println!("{}", ui::section("Sources"));
        println!("  {}", ui::dim("nothing has been fetched yet"));
        // The store being empty is not the same as the feature being absent,
        // and a reader who cannot tell which will conclude the wrong one.
        println!(
            "  {}",
            ui::dim("aede sources --import <file> takes in a document by hand")
        );
        return Ok(());
    }

    let catalog = load(args)?;
    match args.has("list") {
        true => list(&held, &catalog, args),
        false => summary(&held, &catalog),
    }
}

/// One line per source: how much it said, and how much of it lands.
fn summary(held: &Sources, catalog: &Catalog) -> Res {
    println!("{}", ui::section("Sources"));

    let mut names: Vec<&str> = held.records.iter().map(|r| r.source.as_str()).collect();
    names.sort_unstable();
    names.dedup();

    let now = aede_core::clock::now_seconds();
    let mut table = Table::new(&["Source", "Records", "Attached", "Waiting", "Last fetch"])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right);

    for name in &names {
        let mine = Sources {
            records: held
                .records
                .iter()
                .filter(|r| r.source == *name)
                .cloned()
                .collect(),
        };
        let reach = sources::attachment(&mine, catalog);
        let last = mine.records.iter().map(|r| r.fetched_at).max().unwrap_or(0);
        table.push(vec![
            (*name).to_string(),
            mine.records.len().to_string(),
            reach.attached.to_string(),
            reach.waiting.to_string(),
            ui::ago(now.saturating_sub(last)),
        ]);
    }
    println!("{}", table.render());

    // A count of what is waiting means nothing without the reason, and the
    // reason is always the same one: the catalog does not hold that entity
    // yet. Saying so here is what stops it reading as a failure.
    let all = sources::attachment(held, catalog);
    if all.waiting > 0 {
        // Both verbs agree with the count, because `ui::plural` puts the count
        // in the sentence and the rest of the sentence has to follow it.
        let (name, kept) = match all.waiting {
            1 => ("names", "is"),
            _ => ("name", "are"),
        };
        println!(
            "  {}",
            ui::dim(&format!(
                "{} {name} nothing this catalog holds yet, and {kept} kept until it does",
                ui::plural(all.waiting, "record")
            ))
        );
    }
    Ok(())
}

/// Every record, with what it says and whether the catalog can place it.
fn list(held: &Sources, catalog: &Catalog, args: &Args) -> Res {
    let only = args.value("source");
    let rows: Vec<&SourceRecord> = held
        .records
        .iter()
        .filter(|r| only.is_none_or(|s| r.source == s))
        .collect();

    if rows.is_empty() {
        println!("{}", ui::section("Sources"));
        println!(
            "  {}",
            ui::dim(&match only {
                Some(name) => format!("nothing was fetched from \"{name}\""),
                None => "nothing has been fetched yet".to_string(),
            })
        );
        return Ok(());
    }

    println!("{}", ui::section("What sources say"));
    let mut table = Table::new(&["Entity", "Source", "Confidence", "Says", "Where"])
        .limit(0, 42)
        .limit(3, 46);
    for record in &rows {
        let entity = record.entity();
        table.push(vec![
            format!("{} {}", entity.kind.as_str(), entity.key),
            record.source.clone(),
            confidence_label(record.confidence),
            says(&record.facts),
            match entity.resolve(catalog).is_some() {
                true => "in the catalog".to_string(),
                false => "waiting".to_string(),
            },
        ]);
    }
    println!("{}", table.render());
    println!("  {}", ui::dim(&ui::plural(rows.len(), "record")));
    Ok(())
}

/// `identified`, or a matched score — the distinction the roadmap insists on.
fn confidence_label(confidence: Confidence) -> String {
    match confidence {
        Confidence::Identified => "identified".to_string(),
        Confidence::Matched(score) => format!("matched {score}%"),
    }
}

/// What a record says, short enough for a column.
fn says(facts: &Facts) -> String {
    let mut parts: Vec<String> = Vec::new();
    match facts {
        Facts::Artist(a) => {
            if let Some(kind) = &a.kind {
                parts.push(kind.clone());
            }
            if let Some(known) = &a.disambiguation {
                parts.push(known.clone());
            }
            if let Some(area) = &a.area {
                parts.push(area.clone());
            }
            match (&a.began, &a.ended) {
                (Some(from), Some(to)) => parts.push(format!("{from}–{to}")),
                (Some(from), None) => parts.push(format!("{from}–")),
                _ => {}
            }
            if let Some(active) = a.active {
                parts.push(match active {
                    true => "still active".to_string(),
                    false => "ended".to_string(),
                });
            }
            // Last, and left to the column's own limit. A truncated opening
            // line says more about what was stored than the word "summary"
            // would, and the whole of it is on the artist's own card, where it
            // is wrapped and credited.
            if let Some(prose) = &a.summary {
                parts.push(prose.text.clone());
            }
        }
        Facts::Release(r) => {
            if let Some(primary) = &r.primary_type {
                parts.push(primary.clone());
            }
            parts.extend(r.secondary_types.iter().cloned());
            if let Some(date) = &r.first_released {
                parts.push(date.clone());
            }
            if let Some(label) = &r.label {
                parts.push(label.clone());
            }
        }
    }
    match parts.is_empty() {
        // An answer holding nothing is a real answer, and printing an empty
        // cell would make it look like a missing row instead.
        true => ui::dim("nothing").to_string(),
        false => parts.join(" · "),
    }
}

/// Takes in a document written in the shape of `sources.json`.
fn import(args: &Args, path: &std::path::Path) -> Res {
    let Some(file) = args.value("import") else {
        return Err("--import expects a file: --import=<file.json>".into());
    };
    let text = std::fs::read_to_string(file).map_err(|e| format!("cannot read \"{file}\": {e}"))?;
    let value =
        aede_core::json::parse(&text).map_err(|e| format!("\"{file}\" is not JSON: {e}"))?;
    let incoming = sources::from_json(&value)?;

    let mut held = sources::load(path)?.unwrap_or_default();
    let (mut updated, mut added) = (0usize, 0usize);
    for record in incoming.records {
        match held.set(record) {
            true => updated += 1,
            false => added += 1,
        }
    }
    sources::save(&held, path)?;

    println!("{}", ui::section("Imported"));
    println!(
        "{} {} added, {} updated, {} held",
        ui::green("→"),
        added,
        updated,
        held.records.len()
    );
    // Naming the file it wrote, for the same reason `import --forget` does: a
    // command that changed a store the user cannot see should say which one.
    println!("  {}", ui::dim(&path.display().to_string()));
    Ok(())
}

/// Writes the layer out, in the shape `--import` reads.
///
/// The symmetry matters more than the feature: a store that can be filled and
/// not emptied out again is a store whose contents you have to trust somebody
/// else about. `aede notes --export` already draws the same round trip for
/// what the user writes.
fn export(args: &Args, path: &std::path::Path) -> Res {
    let held = sources::load(path)?.unwrap_or_default();
    if held.records.is_empty() {
        // Writing an empty document and reporting success would look like the
        // export worked and the layer was empty — two different things, and
        // the second is not what has just been established.
        return Err(
            "nothing has been fetched yet, so there is nothing to export.\n\
             aede sources --template=<file> writes a document you can fill in"
                .into(),
        );
    }
    super::export::emit(args, &sources::to_json(&held).to_string_pretty())
}

/// Writes a document with the right keys and nothing filled in.
///
/// The keys are the one thing a person cannot guess: they are how an entity
/// names itself — a normalised artist name, or an album's artist, title and
/// folder joined — and until something fetches, there is no way to see one.
/// Without this, the layer could be read and emptied but never filled by hand,
/// which makes it untestable by the person who most needs to test it.
///
/// Naming entities narrows it:
/// `aede sources --template --output=x.json "Kind of Blue"` writes that album
/// alone rather than a skeleton of the whole library.
fn template(args: &Args) -> Res {
    let catalog = load(args)?;
    let source = args.value("source").unwrap_or(sources::MUSICBRAINZ);
    let now = aede_core::clock::now_seconds();

    let wanted: Vec<String> = args
        .positionals
        .iter()
        .map(|name| aede_core::text::normalize(name))
        .filter(|name| !name.is_empty())
        .collect();
    let matches = |name: &str| -> bool {
        let key = aede_core::text::normalize(name);
        wanted.is_empty() || wanted.iter().any(|w| key.contains(w.as_str()))
    };

    let mut out = Sources::default();
    let blank = |key: String, facts: Facts| SourceRecord {
        key,
        source: source.to_string(),
        source_id: None,
        fetched_at: now,
        confidence: Confidence::Identified,
        facts,
    };
    for artist in &catalog.artists {
        if !matches(&artist.name) {
            continue;
        }
        if let Some(entity) = EntityRef::of(&catalog, EntityKind::Artist, artist.id) {
            out.set(blank(entity.key, Facts::Artist(Default::default())));
        }
    }
    for release in &catalog.releases {
        if !matches(&release.title) {
            continue;
        }
        if let Some(entity) = EntityRef::of(&catalog, EntityKind::Release, release.id) {
            out.set(blank(entity.key, Facts::Release(Default::default())));
        }
    }

    if out.records.is_empty() {
        return Err(match wanted.is_empty() {
            true => "this catalog holds nothing to write a template for".to_string(),
            false => format!(
                "nothing in this catalog matches \"{}\"",
                args.positionals.join(" ")
            ),
        }
        .into());
    }

    super::export::emit(args, &sources::to_json(&out).to_string_pretty())?;
    // Only when it went to a file: a template printed to the terminal is on
    // screen, and telling somebody to import what they can see is noise.
    if args.value("output").is_some() {
        println!(
            "  {} {}",
            ui::plural(out.records.len(), "empty record"),
            ui::dim("— fill in what you want, then: aede sources --import=<file>")
        );
    }
    Ok(())
}

/// Drops what one source said, or everything.
fn forget(args: &Args, path: &std::path::Path) -> Res {
    let mut held = sources::load(path)?.unwrap_or_default();
    let before = held.records.len();
    if before == 0 {
        println!("{}", ui::section("Sources"));
        println!("  {}", ui::dim("there was nothing to forget"));
        return Ok(());
    }

    let removed = match args.value("source") {
        Some(name) => {
            let gone = held.forget(name);
            if gone == 0 {
                return Err(format!(
                    "no source named \"{name}\" — aede sources says which there are"
                )
                .into());
            }
            gone
        }
        None => std::mem::take(&mut held.records).len(),
    };

    sources::save(&held, path)?;
    println!(
        "{} {} removed, {} left",
        ui::green("→"),
        ui::plural(removed, "record"),
        held.records.len()
    );
    println!("  {}", ui::dim(&path.display().to_string()));
    Ok(())
}

// --------------------------------------------------------------------------
// Shown beside the tags, on the entity pages
// --------------------------------------------------------------------------

/// The block an entity page prints when a source has said something about it.
///
/// One function called from every page rather than one block written per page:
/// what a source says has to read the same way on an album and on an artist,
/// and a rule copied into four pages is a rule that will be right in three of
/// them.
///
/// Silent when nothing was fetched. A page that announced an empty section for
/// a feature nobody has used yet would be noise on every album in the library.
pub fn panel_for(args: &Args, catalog: &Catalog, kind: EntityKind, id: Id) {
    let Some(entity) = EntityRef::of(catalog, kind, id) else {
        return;
    };
    let Ok(held) = super::sources_held(args) else {
        return;
    };
    let records: Vec<&SourceRecord> = held.about(&entity).collect();
    if records.is_empty() {
        return;
    }

    println!("{}", ui::section("What sources say"));
    let now = aede_core::clock::now_seconds();

    // Prose first, and outside the table. A paragraph does not fit a column,
    // and it is the one thing here meant to be read rather than compared with
    // a tag — there is nothing in a file to compare it against.
    for record in &records {
        if let Facts::Artist(artist) = &record.facts
            && let Some(prose) = &artist.summary
        {
            for line in ui::wrap(&prose.text, 72) {
                println!("  {line}");
            }
            // The credit is not a nicety: this text is CC BY-SA, and reusing
            // it obliges naming where it came from and under what terms. It is
            // printed every time the text is, because that is what the licence
            // asks and because a reader deserves to know they are reading an
            // encyclopaedia rather than the program's own opinion.
            println!("  {}", ui::dim(&prose.credit()));
            println!();
        }
    }

    let mut table = Table::new(&["Source", "Field", "Says", "Your tags"]).limit(2, 34);

    for record in &records {
        let age = ui::ago(now.saturating_sub(record.fetched_at));
        let attribution = match record.confidence {
            Confidence::Identified => format!("{} · {age}", record.source),
            Confidence::Matched(score) => format!("{} {score}% · {age}", record.source),
        };
        for (field, theirs, verdict) in compared(catalog, &entity, &record.facts) {
            table.push(vec![
                attribution.clone(),
                field.to_string(),
                theirs,
                // The verdict is the point of the column: a value that matches
                // the tags is not the same news as one that contradicts them,
                // and neither is one the tags say nothing about.
                match verdict {
                    Some(Verdict::Agrees) => ui::green("matches your tags").to_string(),
                    Some(Verdict::Differs { yours, .. }) => ui::yellow(&yours).to_string(),
                    // The tags could have answered and did not.
                    Some(Verdict::NothingToCompare) => ui::dim("nothing in your tags").to_string(),
                    // There is no tag for this at all — an artist's country is
                    // not something the files have an opinion about. Saying
                    // "nothing in your tags" would suggest they were expected
                    // to, and send the reader looking for a field to fill.
                    None => String::new(),
                },
            ]);
        }
    }
    // A source that was asked and holds nothing is not an empty table. The
    // distinction is the one this whole layer exists to keep — "checked, and
    // it knows nothing" against "never checked" — and a table rendering
    // "(no results)" says neither of them.
    if table.is_empty() {
        for record in &records {
            // A record whose only content is the paragraph just printed has
            // no row here, and saying it "holds nothing" directly under its
            // own text would be plainly false.
            if matches!(&record.facts, Facts::Artist(a) if a.summary.is_some()) {
                continue;
            }
            println!(
                "  {}",
                ui::dim(&format!(
                    "{} was asked and holds nothing about this ({})",
                    record.source,
                    ui::ago(now.saturating_sub(record.fetched_at))
                ))
            );
        }
        whence(&records);
        return;
    }

    print!("{}", table.render());
    whence(&records);
    println!(
        "{}",
        ui::dim("  beside your tags, never on top of them — aede sources --forget removes them")
    );
}

/// Where each of these answers came from, as an address a reader can follow.
///
/// **The identifier was stored and never shown**, which is the one thing a
/// reader needs to check anything by hand: to open the page, to ask the service
/// the same question, or to correct the data at its source. Asked for the
/// MusicBrainz identifier of an artist, there was nowhere in this program to
/// find it — and the nearest thing on screen was the Wikidata link, which is a
/// different identifier that looks enough like an answer to waste an afternoon.
///
/// Printed as the URL rather than the bare identifier: the identifier is inside
/// it, so it can still be copied for a query, and the page it opens is where a
/// wrong type or a missing date is actually fixed — for everybody, not just
/// here.
fn whence(records: &[&SourceRecord]) {
    for record in records {
        if let Some(address) = address_of(record) {
            println!("  {}", ui::dim(&format!("{}: {address}", record.source)));
        }
    }
}

/// Where one record's answer can be looked at, or its bare identifier.
///
/// Split from the printing so the decision is testable: which path a
/// MusicBrainz album takes is the sort of thing that is wrong once and then
/// wrong for a year, because a link that 404s looks like the service's fault.
fn address_of(record: &SourceRecord) -> Option<String> {
    let id = record.source_id.as_deref()?;
    let kind = match record.facts.kind() {
        EntityKind::Artist => "artist",
        // The identifier kept for an album is the release group's, not the
        // edition's — see `musicbrainz::release`. Naming the wrong path here
        // would produce a link that 404s on the one page a reader came to open.
        EntityKind::Release => "release-group",
        // `Facts` has only those two shapes today. A third would be one this
        // function has never seen, and guessing a path for it is how a link
        // that looks right leads nowhere.
        _ => return Some(id.to_string()),
    };
    Some(match record.source.as_str() {
        sources::MUSICBRAINZ => format!("https://musicbrainz.org/{kind}/{id}"),
        "wikipedia" => format!("https://www.wikidata.org/wiki/{id}"),
        // A source this build knows nothing about still has an identifier, and
        // the identifier is the useful half. Inventing an address for it would
        // be worse than showing none.
        _ => id.to_string(),
    })
}

/// The first value of a tag on any file credited to this artist.
///
/// A genre lives on the tracks, not on the artist, so "what do my files call
/// this artist" is answered by the files they appear on.
fn tags_of_artist(catalog: &Catalog, entity: &EntityRef, tag: &str) -> Vec<String> {
    let Some(id) = entity.resolve(catalog) else {
        return Vec::new();
    };
    // **Every** value, not the first: a genre tag holds a list, whether the
    // file writes it as several values or as one string with commas in it.
    // Reading only the first turned "Rock, Pop" into "Rock" on some files and
    // reported a disagreement with a genre the tags do carry.
    catalog
        .tracks_of_artist(id)
        .into_iter()
        .filter_map(|t| catalog.track(t))
        .find_map(|t| {
            catalog
                .file(t.file_id)
                .map(|f| f.tags.get(tag).cloned().unwrap_or_default())
                .filter(|values: &Vec<String>| !values.is_empty())
        })
        .unwrap_or_default()
}

/// Each field a record carries, with the tag it can be judged against.
///
/// `None` where the catalog holds no counterpart at all: an artist's country
/// is not something the tags have an opinion about, and printing "nothing in
/// your tags" against it would suggest the tags were expected to.
fn compared(
    catalog: &Catalog,
    entity: &EntityRef,
    facts: &Facts,
) -> Vec<(&'static str, String, Option<Verdict>)> {
    let mut rows: Vec<(&'static str, String, Option<Verdict>)> = Vec::new();
    match facts {
        Facts::Artist(a) => {
            // "country" was wrong: MusicBrainz calls this an *area*, and an
            // area is a country, a city or a region — "Seattle" is a valid
            // answer. "from" is true of all three.
            //
            // "known as" was wrong too, and worse: `disambiguation` is written
            // to separate two artists who share a name, not to describe one,
            // which is how a perfectly correct answer came out as
            // "known as: the band".
            for (field, value) in [
                ("type", &a.kind),
                ("from", &a.area),
                ("formed", &a.began),
                ("ended", &a.ended),
                ("note", &a.disambiguation),
                ("wikidata", &a.wikidata),
                ("discogs", &a.discogs),
                ("homepage", &a.homepage),
            ] {
                if let Some(value) = value {
                    rows.push((field, value.clone(), None));
                }
            }
            if !a.genres.is_empty() {
                // Beside the genre tag, never over it: the last column is what
                // your files say, and these two answers are both allowed to
                // exist.
                //
                // Compared as **sets**, which the first version did not do: it
                // took MusicBrainz's top genre and compared it, as a string, to
                // the whole tag. `pop` against `Rock, Pop` came out as a
                // disagreement, when the tag plainly says pop. Genres are not
                // exclusive and the two sides are not answering at the same
                // granularity — one is what a crowd voted, the other is what
                // one person typed — so a shared name is agreement.
                let theirs = a.genres.join(", ");
                let yours = tags_of_artist(catalog, entity, "genre");
                let verdict = aede_core::sources::verdict_set(&a.genres, &yours);
                rows.push(("genres", theirs, Some(verdict)));
            }
            if !a.aliases.is_empty() {
                rows.push(("also called", a.aliases.join(", "), None));
            }
            // Shown even when there is no end date, because that is the case
            // it answers: a band with no end may have never stopped, or may
            // be one nobody filled in, and the reader cannot tell otherwise.
            if let Some(active) = a.active {
                rows.push((
                    "status",
                    match active {
                        true => "still active".to_string(),
                        false => "ended".to_string(),
                    },
                    None,
                ));
            }
        }
        Facts::Release(r) => {
            let release = entity
                .resolve(catalog)
                .and_then(|id| catalog.release(id))
                .cloned();
            let tag = |name: &str| -> Option<String> {
                release.as_ref().and_then(|rel| {
                    rel.track_ids
                        .first()
                        .and_then(|&t| catalog.track(t))
                        .and_then(|t| catalog.file(t.file_id))
                        .and_then(|f| f.first_tag(name).map(str::to_string))
                })
            };

            if let Some(primary) = &r.primary_type {
                let mut shown = primary.clone();
                if !r.secondary_types.is_empty() {
                    shown = format!("{shown} · {}", r.secondary_types.join(" · "));
                }
                rows.push((
                    "release type",
                    shown,
                    Some(aede_core::sources::verdict(
                        primary,
                        tag("releasetype").as_deref(),
                    )),
                ));
            }
            if let Some(date) = &r.first_released {
                let yours = release.as_ref().and_then(|rel| rel.date.clone());
                rows.push((
                    "first released",
                    date.clone(),
                    Some(aede_core::sources::verdict_date(date, yours.as_deref())),
                ));
            }
            if let Some(label) = &r.label {
                let yours = release.as_ref().and_then(|rel| {
                    rel.label_ids
                        .first()
                        .and_then(|&id| catalog.label(id))
                        .map(|l| l.name.clone())
                });
                rows.push((
                    "label",
                    label.clone(),
                    Some(aede_core::sources::verdict(label, yours.as_deref())),
                ));
            }
        }
    }
    rows
}

// Its own file from the start: this module is long enough that the tests would
// double it. `#[path]` keeps them a child module, so they still see what is
// private here.
#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
