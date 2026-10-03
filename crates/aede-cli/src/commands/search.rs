//! The `search` command: one query across every entity.
//!
//! Names only, by default. `--comments` widens it to the **comment** tag —
//! where a rip came from, which pressing this is — `--notes` to what the user
//! wrote themselves, and `--lyrics` to the words of the songs. All three are
//! free prose, and a common word in any of them would bury the entity that
//! actually bears the name, which is why none joins an ordinary search and why
//! each keeps a section of its own: a hit found in a note was found by another
//! route, and the reader has to be able to tell which.
//!
//! The three are not the same field and must not be folded together. A comment
//! lives **inside the audio file**, put there by whoever tagged it; a note
//! lives in `user.json`, put there by the person using Aède; the words are the
//! song itself, and belong to nobody here. Searching one is searching the
//! library, searching another is searching yourself.

use aede_core::contributors::{self, SourcedContributor};
use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::{sources, text};
use std::collections::{BTreeMap, BTreeSet};

use super::{Res, announce_window, load, selection_output, sources_held};
use crate::args::{Args, Window};
use crate::ui::{self, Table};

struct ParentWorkHit {
    mbid: String,
    title: String,
    part_ids: BTreeSet<String>,
}

fn sourced_parent_works(
    catalog: &Catalog,
    held: &sources::Sources,
    query: &str,
) -> Vec<ParentWorkHit> {
    let wanted = text::normalize(query);
    let mut grouped: BTreeMap<String, ParentWorkHit> = BTreeMap::new();
    for link in held
        .work_parent_links(catalog)
        .into_iter()
        .filter(|link| link.trusted)
    {
        if link.parent.mbid != query
            && (wanted.is_empty() || !text::normalize(&link.parent.title).contains(&wanted))
        {
            continue;
        }
        if catalog
            .works
            .iter()
            .any(|work| work.mbid == link.parent.mbid)
        {
            continue;
        }
        let hit = grouped
            .entry(link.parent.mbid.clone())
            .or_insert_with(|| ParentWorkHit {
                mbid: link.parent.mbid.clone(),
                title: link.parent.title.clone(),
                part_ids: BTreeSet::new(),
            });
        if hit.title.is_empty() && !link.parent.title.is_empty() {
            hit.title = link.parent.title;
        }
        hit.part_ids.insert(link.child_mbid);
    }
    let mut hits: Vec<_> = grouped.into_values().collect();
    hits.sort_by(|left, right| {
        (left.mbid != query, text::normalize(&left.title), &left.mbid).cmp(&(
            right.mbid != query,
            text::normalize(&right.title),
            &right.mbid,
        ))
    });
    hits
}

pub fn search(args: &Args) -> Res {
    let catalog = load(args)?;
    let query = args.positionals.join(" ");
    if query.trim().is_empty() {
        return Err("give some text to search for".into());
    }
    let window = args.window(30)?;
    // Keep the complete ranking until the requested shape is known. A track
    // export has its own rows: paging mixed graph hits first could leave a
    // playlist empty merely because albums occupied the chosen page.
    let hits = catalog.search(&query, usize::MAX);
    let sources = sources_held(args)?;
    let sourced_parents = sourced_parent_works(&catalog, &sources, &query);
    let mut sourced_contributors: Vec<SourcedContributor> =
        contributors::sourced(&catalog, &sources)
            .into_iter()
            .filter(|person| {
                person.mbid == query || contributors::matches_name(person, &query, false)
            })
            .collect();
    sourced_contributors.sort_by(|left, right| {
        (
            left.mbid != query,
            aede_core::text::normalize(&left.name),
            &left.mbid,
        )
            .cmp(&(
                right.mbid != query,
                aede_core::text::normalize(&right.name),
                &right.mbid,
            ))
    });

    // Kept apart from the hits above rather than merged into them: a hit found
    // in a comment was found by another route, and the reader has to be able to
    // tell which. The same reason an imported analysis sits in its own panel.
    let in_comments: Vec<Id> = if args.has("comments") {
        catalog.tracks_with_comment(&query)
    } else {
        Vec::new()
    };

    // The words, with the line that carried the text rather than the song:
    // "that one that goes something about a train" is answered by showing the
    // line, in the shape it was half-remembered.
    let in_lyrics: Vec<(Id, String)> = if args.has("lyrics") {
        catalog.tracks_with_lyric(&query)
    } else {
        Vec::new()
    };

    // A note can be about anything — a label, a genre, an artist — so what
    // comes back is not a list of tracks the way a comment hit is. It is shown
    // as what it is.
    let in_notes: Vec<(aede_core::user::EntityRef, String)> = if args.has("notes") {
        notes_matching(args, &catalog, &query)?
    } else {
        Vec::new()
    };

    // Only the tracks: an artist or an album is not something to play, nor a
    // row in a table of tracks. Comment hits are tracks, so they join in.
    let mut ids: Vec<Id> = hits
        .iter()
        .filter(|h| h.kind == EntityKind::Track)
        .map(|h| h.id)
        .collect();
    let mut included: BTreeSet<Id> = ids.iter().copied().collect();
    for &id in in_comments.iter().chain(in_lyrics.iter().map(|(id, _)| id)) {
        if included.insert(id) {
            ids.push(id);
        }
    }
    // Same reason as `track`: the JSON branch below returns on its own,
    // before it would ever reach the shared gate inside `selection_output`,
    // so the conflict has to be caught here too or `--json --csv` would
    // print the JSON and drop the CSV without a word.
    if let Some(message) = args.output_conflict(&["m3u", "csv", "json"]) {
        return Err(message.into());
    }
    // A command with a JSON shape of its own answers first: `search --json`
    // reports the hits — artists and albums included — which is a better answer
    // than the flat track table the shared selection path would give.
    if args.has("json") {
        return print_json(
            &catalog,
            &hits,
            &sourced_contributors,
            &sourced_parents,
            &in_comments,
            &in_lyrics,
            &in_notes,
            args,
        );
    }
    let ids: Vec<Id> = ids
        .into_iter()
        .skip(window.offset)
        .take(window.limit)
        .collect();
    if let Some(result) = selection_output(&catalog, &ids, args) {
        return result;
    }
    super::refuse_output_without_a_format(args)?;

    println!("{}", ui::section(&format!("Results for \"{query}\"")));
    if hits.is_empty() && sourced_contributors.is_empty() && sourced_parents.is_empty() {
        // Symmetric with the three sections below it, each of which says what
        // it did not find rather than falling back to the generic "(no
        // results)" a reader could otherwise mistake for nothing at all
        // having matched anywhere.
        println!("  {}", ui::dim("nothing by name"));
    } else if !hits.is_empty() {
        let mut t = Table::new(&["Type", "Name", "Context", "Open"])
            .limit(1, 32)
            .limit(2, 22)
            .limit(3, 64);
        for hit in hits.iter().skip(window.offset).take(window.limit) {
            // "release" is the model's word; "album" is the user's. On screen
            // the user's wins — the JSON keeps the model's, for a client that
            // has to map it back onto a table.
            let kind = match hit.kind {
                EntityKind::Artist => "artist",
                EntityKind::Release => "album",
                EntityKind::Track => "track",
                EntityKind::Recording => "recording",
                EntityKind::Work => "work",
                EntityKind::ReleaseGroup => "release group",
                EntityKind::Label => "label",
                EntityKind::Genre => "genre",
            };
            t.push(vec![
                kind.to_string(),
                hit.name.clone(),
                hit.detail.clone(),
                super::navigation::open_command(&catalog, hit.kind, hit.id).unwrap_or_default(),
            ]);
        }
        print!("{}", t.render());
        announce_window(window, hits.len(), "name match");
    }

    if !sourced_contributors.is_empty() {
        println!(
            "{}",
            ui::section("Sourced contributors (not in local artist tags)")
        );
        let mut table = Table::new(&["Name", "MusicBrainz ID", "Recordings", "Open"])
            .align(2, crate::ui::Align::Right)
            .limit(0, 30)
            .limit(3, 65);
        for person in sourced_contributors
            .iter()
            .skip(window.offset)
            .take(window.limit)
        {
            table.push(vec![
                person.name.clone(),
                person.mbid.clone(),
                person.recording_ids.len().to_string(),
                format!("aede artist {}", super::navigation::shell_arg(&person.mbid)),
            ]);
        }
        print!("{}", table.render());
        announce_window(window, sourced_contributors.len(), "sourced contributor");
    }

    if !sourced_parents.is_empty() {
        println!(
            "{}",
            ui::section("Sourced parent works (locally held parts)")
        );
        let mut table = Table::new(&["Work", "MusicBrainz ID", "Parts", "Open"])
            .align(2, crate::ui::Align::Right)
            .limit(0, 34)
            .limit(3, 65);
        for work in sourced_parents
            .iter()
            .skip(window.offset)
            .take(window.limit)
        {
            table.push(vec![
                work.title.clone(),
                work.mbid.clone(),
                work.part_ids.len().to_string(),
                format!("aede work {}", super::navigation::shell_arg(&work.mbid)),
            ]);
        }
        print!("{}", table.render());
        announce_window(window, sourced_parents.len(), "sourced parent work");
    }

    if args.has("comments") {
        print_comment_hits(&catalog, &in_comments, window);
    }
    if args.has("lyrics") {
        print_lyric_hits(&catalog, &in_lyrics, window);
    }
    if args.has("notes") {
        print_note_hits(&catalog, &in_notes, window);
    }
    Ok(())
}

/// What the user wrote, wherever the text appears in it.
///
/// Accent- and case-insensitive through `text::normalize`, like every other
/// search in the program: somebody who wrote "pressage vinyle" must find it by
/// typing "Vinyle".
fn notes_matching(
    args: &Args,
    catalog: &Catalog,
    query: &str,
) -> Result<Vec<(aede_core::user::EntityRef, String)>, Box<dyn std::error::Error>> {
    let data = super::user_data(args, catalog)?;
    let wanted = aede_core::text::normalize(query);
    let mut found: Vec<(aede_core::user::EntityRef, String)> = data
        .annotations
        .iter()
        .filter(|a| a.owner == aede_core::user::LOCAL_USER)
        .filter_map(|a| a.note.as_ref().map(|note| (a.target.clone(), note.clone())))
        .filter(|(_, note)| aede_core::text::normalize(note).contains(&wanted))
        .collect();
    // Sorted so two runs agree, and so the kinds group together on screen.
    found.sort_by(|a, b| (a.0.kind, &a.0.key).cmp(&(b.0.kind, &b.0.key)));
    Ok(found)
}

/// The notes carrying the text, in their own section.
fn print_note_hits(
    catalog: &Catalog,
    notes: &[(aede_core::user::EntityRef, String)],
    window: Window,
) {
    if notes.is_empty() {
        println!("  {}", ui::dim("nothing in your notes"));
        return;
    }
    println!("{}", ui::section("In your notes"));
    let mut t = Table::new(&["Kind", "Name", "Note"])
        .limit(1, 30)
        .limit(2, 50);
    for (reference, note) in notes.iter().skip(window.offset).take(window.limit) {
        t.push(vec![
            match reference.kind {
                EntityKind::Release => "album".to_string(),
                other => other.as_str().to_string(),
            },
            reference.display_name(catalog),
            note.replace('\n', " "),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, notes.len(), "note");
}

/// Machine-readable form. Every row says **where** it was found, so a client
/// can tell a name match from a comment match without guessing.
fn print_json(
    catalog: &Catalog,
    hits: &[aede_core::model::SearchHit],
    sourced_contributors: &[SourcedContributor],
    sourced_parents: &[ParentWorkHit],
    in_comments: &[Id],
    in_lyrics: &[(Id, String)],
    in_notes: &[(aede_core::user::EntityRef, String)],
    args: &Args,
) -> Res {
    let window = args.window(30)?;
    let mut rows: Vec<Json> = hits
        .iter()
        .skip(window.offset)
        .take(window.limit)
        .map(|h| {
            let mut o = Json::obj();
            o.set("type", h.kind.as_str().into());
            o.set("id", h.id.into());
            o.set("name", h.name.clone().into());
            o.set("context", h.detail.clone().into());
            o.set("found_in", "name".to_string().into());
            o
        })
        .collect();
    for person in sourced_contributors
        .iter()
        .skip(window.offset)
        .take(window.limit)
    {
        let mut row = Json::obj();
        row.set("type", "artist".into());
        row.set("id", Json::Null);
        row.set("musicbrainz_id", person.mbid.clone().into());
        row.set("name", person.name.clone().into());
        row.set(
            "context",
            format!(
                "{} credited local recordings · sourced",
                person.recording_ids.len()
            )
            .into(),
        );
        row.set("found_in", "source_credit".into());
        row.set(
            "open",
            format!("aede artist {}", super::navigation::shell_arg(&person.mbid)).into(),
        );
        rows.push(row);
    }
    for work in sourced_parents
        .iter()
        .skip(window.offset)
        .take(window.limit)
    {
        let mut row = Json::obj();
        row.set("type", "work".into());
        row.set("id", Json::Null);
        row.set("musicbrainz_id", work.mbid.clone().into());
        row.set("name", work.title.clone().into());
        row.set(
            "context",
            format!("{} locally held parts · sourced", work.part_ids.len()).into(),
        );
        row.set("found_in", "source_work_part".into());
        row.set(
            "open",
            format!("aede work {}", super::navigation::shell_arg(&work.mbid)).into(),
        );
        rows.push(row);
    }
    for &id in in_comments.iter().skip(window.offset).take(window.limit) {
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let mut o = Json::obj();
        o.set("type", EntityKind::Track.as_str().into());
        o.set("id", id.into());
        o.set("name", track.title.clone().into());
        o.set(
            "context",
            catalog
                .comment_of_track(id)
                .unwrap_or_default()
                .to_string()
                .into(),
        );
        o.set("found_in", "comment".to_string().into());
        rows.push(o);
    }
    for (id, line) in in_lyrics.iter().skip(window.offset).take(window.limit) {
        let Some(track) = catalog.track(*id) else {
            continue;
        };
        let mut o = Json::obj();
        o.set("type", EntityKind::Track.as_str().into());
        o.set("id", (*id).into());
        o.set("name", track.title.clone().into());
        // The line that matched, as on screen: a client that wanted the whole
        // song would ask for the track, not for a search result.
        o.set("context", line.clone().into());
        o.set("found_in", "lyrics".to_string().into());
        rows.push(o);
    }
    // A note can land on anything the catalog holds, not only a track — the
    // JSON says what kind it landed on, same as every other row here.
    for (reference, note) in in_notes.iter().skip(window.offset).take(window.limit) {
        let Some(id) = reference.resolve(catalog) else {
            continue;
        };
        let mut o = Json::obj();
        o.set("type", reference.kind.as_str().into());
        o.set("id", id.into());
        o.set("name", reference.display_name(catalog).into());
        o.set("context", note.clone().into());
        o.set("found_in", "note".to_string().into());
        rows.push(o);
    }
    super::export::emit(args, &Json::Arr(rows).to_string_pretty())
}

/// The tracks whose comment carries the text, in their own section.
/// The tracks whose words carry the text, in their own section.
///
/// One line per hit, not one song: a cell holding four hundred lines is a table
/// nobody can read, and the line is what was being looked for.
fn print_lyric_hits(catalog: &Catalog, found: &[(Id, String)], window: Window) {
    if found.is_empty() {
        println!("  {}", ui::dim("nothing in the lyrics"));
        return;
    }
    println!("{}", ui::section("In lyrics"));
    let mut t = Table::new(&["Track", "Album", "Line"])
        .limit(0, 30)
        .limit(1, 25)
        .limit(2, 45);
    for (id, line) in found.iter().skip(window.offset).take(window.limit) {
        let Some(track) = catalog.track(*id) else {
            continue;
        };
        let album = track
            .release_id
            .and_then(|r| catalog.release(r))
            .map(|r| r.title.clone())
            .unwrap_or_default();
        t.push(vec![track.title.clone(), album, line.clone()]);
    }
    print!("{}", t.render());
    announce_window(window, found.len(), "line");
}

fn print_comment_hits(catalog: &Catalog, tracks: &[Id], window: Window) {
    if tracks.is_empty() {
        println!("  {}", ui::dim("nothing in the comments"));
        return;
    }
    println!("{}", ui::section("In comments"));
    let mut t = Table::new(&["Track", "Album", "Comment"])
        .limit(0, 30)
        .limit(1, 25)
        .limit(2, 45);
    for &id in tracks.iter().skip(window.offset).take(window.limit) {
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let album = track
            .release_id
            .and_then(|r| catalog.release(r))
            .map(|r| r.title.clone())
            .unwrap_or_default();
        t.push(vec![
            track.title.clone(),
            album,
            catalog.comment_of_track(id).unwrap_or_default().to_string(),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, tracks.len(), "comment");
}
