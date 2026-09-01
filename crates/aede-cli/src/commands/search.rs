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

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id};

use super::{Res, announce_window, load, selection_output};
use crate::args::{Args, Window};
use crate::ui::{self, Table};

pub fn search(args: &Args) -> Res {
    let catalog = load(args)?;
    let query = args.positionals.join(" ");
    if query.trim().is_empty() {
        return Err("give some text to search for".into());
    }
    let window = args.window(30)?;
    // The search ranks by how well a name matched, so a window over the hits
    // is a window over that ranking: page two is the next thirty best, not a
    // second search.
    let hits: Vec<aede_core::model::SearchHit> = catalog
        .search(&query, window.offset.saturating_add(window.limit))
        .into_iter()
        .skip(window.offset)
        .collect();

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
    for &id in in_comments.iter().chain(in_lyrics.iter().map(|(id, _)| id)) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    // A command with a JSON shape of its own answers first: `search --json`
    // reports the hits — artists and albums included — which is a better answer
    // than the flat track table the shared selection path would give.
    if args.has("json") {
        return print_json(&catalog, &hits, &in_comments, &in_lyrics, window);
    }
    if let Some(result) = selection_output(&catalog, &ids, args) {
        return result;
    }

    println!("{}", ui::section(&format!("Results for \"{query}\"")));
    let mut t = Table::new(&["Type", "Name", "Context"])
        .limit(1, 45)
        .limit(2, 35);
    for hit in &hits {
        // "release" is the model's word; "album" is the user's. On screen the
        // user's wins — the JSON keeps the model's, for a client that has to
        // map it back onto a table.
        let kind = match hit.kind {
            EntityKind::Artist => "artist",
            EntityKind::Release => "album",
            EntityKind::Track => "track",
            EntityKind::Label => "label",
            EntityKind::Genre => "genre",
        };
        t.push(vec![kind.to_string(), hit.name.clone(), hit.detail.clone()]);
    }
    print!("{}", t.render());

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
    in_comments: &[Id],
    in_lyrics: &[(Id, String)],
    window: Window,
) -> Res {
    let mut rows: Vec<Json> = hits
        .iter()
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
    println!("{}", Json::Arr(rows).to_string_pretty());
    Ok(())
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
