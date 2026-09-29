//! The `track` command: one page per track, found by title.
//!
//! Same information as `file`, reached by the name of the music instead of the
//! path of a file. It reads the catalog, never the disk, and therefore knows
//! things a file cannot tell on its own: the release the track belongs to, its
//! position, who is credited on it.
//!
//! A title is not an identifier. Every track carrying it is shown, in full,
//! because the album version, the single and the live rendition are different
//! recordings and the difference is exactly what one wants to see.

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id, TitleMatch, Track};
use aede_core::sources::{self, SourcedCreditLink};
use aede_core::{lyrics, text};

use super::{
    Res, announce_window, data_dir, load, properties_table, role_label, selection_output,
    tags_table,
};
use crate::args::Args;
use crate::ui::{self, Table};

/// Default number of pages printed, so that a title as common as "Intro" does
/// not fill the terminal. Raised with `--limit`.
const DEFAULT_LIMIT: usize = 10;

pub fn show_track(args: &Args) -> Res {
    let catalog = load(args)?;
    let held = sources::load(&sources::sources_path(&data_dir(args)))?.unwrap_or_default();
    let sourced_credits = held.credit_links(&catalog);
    let edition_credits = held.edition_credit_links(&catalog);
    let parent_works = held.work_parent_links(&catalog);
    let title = args.positionals.join(" ");
    if title.trim().is_empty() {
        return Err("give a title: aede track \"Patient Number 9\"".into());
    }

    let (found, kind) = catalog.find_tracks(&title);
    let before_filters = found.len();

    // The options are shorthand for the grammar here too: three hand-written
    // filters used to answer questions the one evaluator already answers.
    let expression = track_query(args);
    let parsed = aede_core::query::parse(&expression)?;
    let data = super::user_data(args, &catalog)?;
    let context = aede_core::query::Context::new(&catalog, &data, aede_core::user::LOCAL_USER)
        .with_sources(&held);
    super::ensure_query_values(&parsed, &context)?;
    let matches: Vec<&Track> = found
        .into_iter()
        .filter(|t| aede_core::query::matches(&parsed, &context, t.id))
        .collect();

    if matches.is_empty() {
        // Saying "no such title" when the title exists and it is the filter
        // that excluded everything would send the user looking in the wrong
        // place.
        return Err(match before_filters {
            0 => format!("no track matches \"{title}\""),
            n => format!(
                "{} titled \"{title}\", none matching the filters given",
                ui::plural(n, "track")
            ),
        }
        .into());
    }
    let total = matches.len();
    let window = args.window(DEFAULT_LIMIT)?;
    let matches: Vec<&Track> = matches
        .into_iter()
        .skip(window.offset)
        .take(window.limit)
        .collect();

    let ids: Vec<Id> = matches.iter().map(|t| t.id).collect();
    // Caught before either branch below can silently win: the JSON branch
    // returns on its own, before ever reaching the shared gate inside
    // `selection_output`, so `--json --m3u` would otherwise print the JSON
    // and never mention the playlist that was also asked for.
    if let Some(message) = args.output_conflict(&["m3u", "csv", "json"]) {
        return Err(message.into());
    }
    // Its own JSON shape answers first, for the same reason as `search`: this
    // one carries the credits and the technical detail, which no flat table of
    // a selection can.
    if args.has("json") {
        let json = Json::Arr(
            matches
                .iter()
                .map(|t| {
                    as_json(
                        &catalog,
                        t,
                        &sourced_credits,
                        &edition_credits,
                        &parent_works,
                    )
                })
                .collect(),
        );
        println!("{}", json.to_string_pretty());
        return Ok(());
    }
    if let Some(result) = selection_output(&catalog, &ids, args) {
        return result;
    }

    if kind == TitleMatch::Partial {
        println!(
            "  {}",
            ui::dim(&format!(
                "no track is titled \"{title}\"; showing the titles containing it"
            ))
        );
    }
    let words = args.has("lyrics");
    for track in &matches {
        print_track(&catalog, track);
        let navigation = print_graph_links(&catalog, track, &held);
        super::print_sourced_credits(
            &catalog,
            sourced_credits
                .iter()
                .filter(|link| link.recording_id == track.recording_id)
                .cloned()
                .collect(),
        );
        if let Some(release_id) = track.release_id {
            super::print_sourced_edition_credits(
                &catalog,
                edition_credits
                    .iter()
                    .filter(|link| link.release_id == release_id)
                    .cloned()
                    .collect(),
            );
        }
        if words {
            print_lyrics(&catalog, track);
        }
        // Right under the track it is about, in the same pass that printed
        // it — not gathered into a second loop after every track has already
        // been shown. Two matches means two "Yours"/"Notes" sections can
        // appear back to back with nothing between them but a blank line;
        // printed afterwards, the second one reads as belonging to whichever
        // track happened to print last, which is only sometimes the truth.
        super::panel_for(args, &catalog, EntityKind::Track, track.id);
        navigation.print();
    }

    // A truncated list must say so: a silent cut reads as "that is all there
    // is", which is the one thing it is not.
    println!();
    if total > matches.len() {
        announce_window(window, total, "track");
        println!(
            "  {}",
            ui::dim("or narrow it down with --artist, --album or --comment")
        );
    } else if total > 1 {
        println!("  {}", ui::dim(&ui::plural(total, "track")));
    }
    Ok(())
}

fn print_graph_links(
    catalog: &Catalog,
    track: &Track,
    held: &sources::Sources,
) -> super::navigation::Navigation {
    let Some(recording) = catalog.recording(track.recording_id) else {
        return super::navigation::Navigation::default();
    };
    let mut rows = Table::new(&["Relation", "Target", "Identity", "Evidence"])
        .limit(1, 42)
        .limit(2, 38)
        .limit(3, 32);
    let mut navigation = super::navigation::Navigation::default();
    navigation.entity(catalog, "Recording", EntityKind::Recording, recording.id);
    rows.push(vec![
        "recording".into(),
        recording.title.clone(),
        recording.mbid.clone().unwrap_or_else(|| "local".into()),
        format!("{} local placement(s)", recording.track_ids.len()),
    ]);
    for &work_id in &recording.work_ids {
        if let Some(work) = catalog.work(work_id) {
            rows.push(vec![
                "work".into(),
                work.title.clone(),
                work.mbid.clone(),
                "tags".into(),
            ]);
            navigation.entity(catalog, "Work", EntityKind::Work, work.id);
        }
    }
    let canonical: std::collections::BTreeSet<&str> = recording
        .work_ids
        .iter()
        .filter_map(|&id| catalog.work(id))
        .map(|work| work.mbid.as_str())
        .collect();
    for link in held
        .work_links(catalog)
        .into_iter()
        .filter(|link| link.recording_id == recording.id)
        .filter(|link| !canonical.contains(link.work.mbid.as_str()))
    {
        rows.push(vec![
            "source work".into(),
            link.work.title,
            link.work.mbid.clone(),
            format!(
                "{} · {} · {}",
                link.source,
                super::source_status(link.confidence, link.review, link.trusted),
                ui::since(link.fetched_at)
            ),
        ]);
        if link.trusted {
            navigation.add(
                "Source work",
                format!(
                    "aede work {}",
                    super::navigation::shell_arg(&link.work.mbid)
                ),
            );
        }
    }
    let mut seen_parents = std::collections::BTreeSet::new();
    for link in held
        .work_parent_links(catalog)
        .into_iter()
        .filter(|link| link.recording_id == recording.id)
    {
        if !seen_parents.insert((
            link.child_mbid.clone(),
            link.parent.mbid.clone(),
            link.source.clone(),
        )) {
            continue;
        }
        rows.push(vec![
            "parent work".into(),
            link.parent.title.clone(),
            link.parent.mbid.clone(),
            format!(
                "{} · {}",
                link.source,
                super::source_status(link.confidence, link.review, link.trusted)
            ),
        ]);
        if link.trusted {
            navigation.add(
                "Parent work",
                format!(
                    "aede work {}",
                    super::navigation::shell_arg(&link.parent.mbid)
                ),
            );
        }
    }
    if let Some(release) = track.release_id.and_then(|id| catalog.release(id)) {
        navigation.entity(catalog, "Album", EntityKind::Release, release.id);
        if let Some(artist_id) = release.album_artist_id {
            navigation.entity(catalog, "Artist", EntityKind::Artist, artist_id);
        }
        for &label_id in &release.label_ids {
            navigation.entity(catalog, "Label", EntityKind::Label, label_id);
        }
        if let Some(group_id) = release.release_group_id
            && let Some(group) = catalog.release_group(group_id)
        {
            rows.push(vec![
                "release group".into(),
                group.title.clone(),
                group.mbid.clone(),
                format!("{} local edition(s)", group.release_ids.len()),
            ]);
            navigation.entity(catalog, "Release group", EntityKind::ReleaseGroup, group.id);
        }
    }
    println!("{}", ui::section("Graph"));
    print!("{}", rows.render());
    for credit in catalog
        .credits
        .iter()
        .filter(|credit| credit.entity_kind == EntityKind::Track && credit.entity_id == track.id)
    {
        navigation.entity(
            catalog,
            "Credited artist",
            EntityKind::Artist,
            credit.artist_id,
        );
    }
    for link in held
        .credit_links(catalog)
        .into_iter()
        .filter(|link| link.trusted && link.recording_id == recording.id)
    {
        navigation.source_artist(&link.credit.artist_mbid);
    }
    if let Some(release_id) = track.release_id {
        for credit in catalog.credits.iter().filter(|credit| {
            credit.entity_kind == EntityKind::Release && credit.entity_id == release_id
        }) {
            navigation.entity(
                catalog,
                "Edition contributor",
                EntityKind::Artist,
                credit.artist_id,
            );
        }
        for link in held
            .edition_credit_links(catalog)
            .into_iter()
            .filter(|link| link.trusted && link.release_id == release_id)
        {
            navigation.source_artist(&link.credit.artist_mbid);
        }
    }
    navigation
}

/// Turns the filter options into one expression.
///
/// One mapping is a decision rather than a transcription, and the grammar is
/// what made it expressible: `--artist` on a track matches **either** a credit
/// **or** the album's own artist — asking for a track "by Miles Davis" should
/// find it on a Miles Davis album whether or not he is credited on that
/// particular piece. That is an `OR`, which is precisely what a pile of options
/// could never say and what the grammar says in four characters.
fn track_query(args: &Args) -> String {
    let mut terms: Vec<String> = Vec::new();
    if let Some(artist) = args.value("artist") {
        let value = super::quoted_query_value(artist);
        terms.push(format!("(artist:{value} OR albumartist:{value})"));
    }
    if let Some(album) = args.value("album") {
        terms.push(format!("album:{}", super::quoted_query_value(album)));
    }
    if let Some(comment) = args.value("comment") {
        terms.push(format!("comment:{}", super::quoted_query_value(comment)));
    }
    terms.join(" ")
}

fn print_track(catalog: &Catalog, track: &Track) {
    println!("{}", ui::section(&track.title));

    let release = track.release_id.and_then(|id| catalog.release(id));
    let mut context = Table::plain(2);
    if let Some(release) = release {
        let album = match release.year {
            Some(year) => format!("{} ({year})", release.title),
            None => release.title.clone(),
        };
        context.push(vec!["Album".into(), album]);
        let artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Various Artists".into());
        context.push(vec!["Album artist".into(), artist]);
    }
    if let Some(position) = position(track) {
        context.push(vec!["Position".into(), position]);
    }
    let genres: Vec<String> = catalog
        .genres_of(EntityKind::Track, track.id)
        .into_iter()
        .chain(
            release
                .map(|r| catalog.genres_of(EntityKind::Release, r.id))
                .unwrap_or_default(),
        )
        .map(|g| g.name.clone())
        .collect();
    if !genres.is_empty() {
        context.push(vec!["Genres".into(), dedupe(genres).join(", ")]);
    }
    if let Some(isrc) = &track.isrc {
        context.push(vec!["ISRC".into(), isrc.clone()]);
    }
    let file = catalog.file(track.file_id);
    if let Some(file) = file {
        if let Some(work) = file.first_tag("grouping") {
            context.push(vec!["Work/grouping tag".into(), work.into()]);
        }
        if let Some(movement) = movement_tag(file) {
            context.push(vec!["Movement tag".into(), movement]);
        }
        context.push(vec!["Path".into(), file.path.clone()]);
    }
    context.push(vec!["Integrity".into(), integrity_line(track, catalog)]);
    print!("{}", context.render());

    if let Some(file) = file {
        println!();
        print!(
            "{}",
            properties_table(&file.properties, file.has_embedded_art, file.size).render()
        );
    }

    let credit_rows: Vec<_> = catalog
        .credits
        .iter()
        .filter(|credit| credit.entity_kind == EntityKind::Track && credit.entity_id == track.id)
        .collect();
    if !credit_rows.is_empty() {
        println!("{}", ui::section("Credits"));
        let rich = credit_rows.iter().any(|credit| {
            credit.credited_as.is_some()
                || !credit.attributes.is_empty()
                || credit.began.is_some()
                || credit.ended.is_some()
        });
        if rich {
            let mut table = Table::new(&["Artist", "Role", "Credited as", "Details", "Source"])
                .limit(0, 30)
                .limit(2, 25)
                .limit(3, 40);
            for credit in credit_rows {
                let artist = catalog
                    .artist(credit.artist_id)
                    .map(|artist| artist.name.clone())
                    .unwrap_or_default();
                let details = credit
                    .attributes
                    .iter()
                    .map(|attribute| attribute.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                table.push(vec![
                    artist,
                    role_label(&credit.role),
                    credit.credited_as.clone().unwrap_or_default(),
                    details,
                    credit.source.clone(),
                ]);
            }
            print!("{}", table.render());
        } else {
            let mut by_role: std::collections::BTreeMap<String, Vec<String>> = Default::default();
            for credit in credit_rows {
                if let Some(artist) = catalog.artist(credit.artist_id) {
                    by_role
                        .entry(role_label(&credit.role))
                        .or_default()
                        .push(artist.name.clone());
                }
            }
            let mut table = Table::new(&["Role", "Artists"]).limit(1, 60);
            for (role, names) in by_role {
                table.push(vec![role, dedupe(names).join(", ")]);
            }
            print!("{}", table.render());
        }
    }

    if let Some(file) = file {
        print_analyses(catalog, file);
        println!("{}", ui::section("Tags"));
        if file.tags.is_empty() {
            println!("  {}", ui::yellow("no tag in this file"));
        } else {
            print!("{}", tags_table(&file.tags).render());
        }
    }
}

/// Shows the words, when they are asked for.
///
/// Behind `--lyrics` rather than on the page by default, and that is the whole
/// design decision here: a track page is read to learn what a file *is*, and
/// four hundred lines of text would bury the twelve that answer it. A song is
/// longer than everything else the page says put together.
///
/// The tag is preferred to the sidecar when a file has both — the tag travels
/// with the file, and whoever wrote it into the file meant it to.
fn print_lyrics(catalog: &Catalog, track: &Track) {
    let Some(found) = catalog.lyrics_of_track(track.id) else {
        println!("{}", ui::section("Lyrics"));
        println!(
            "  {}",
            ui::dim("none in the tags, and no .lrc beside the file")
        );
        // A command named only where nobody is looking is a command nobody
        // has, and this is the page somebody is on at the moment they want it.
        // Named, never run: the words are the song's copyright, and going and
        // getting them is a decision this program leaves to the reader.
        println!(
            "  {}",
            ui::dim(&format!(
                "aede fetch \"{}\" --lyrics asks LRCLIB for them",
                track.title
            ))
        );
        return;
    };

    // Named after where they came from, and whether they carry timings —
    // `.lrc` and plain text are two different things to whoever is about to
    // use them, and M3 will care about exactly this distinction.
    let origin = match found.source {
        lyrics::Source::Tag => "from the tags".to_string(),
        lyrics::Source::Sidecar => format!("from {}", text::file_name(&found.origin)),
    };
    let timing = match found.synced() {
        true => ", timed",
        false => "",
    };
    println!("{}", ui::section(&format!("Lyrics ({origin}{timing})")));
    for line in &found.lines {
        match line.at_ms {
            // Floored to the second rather than rounded: the line *starts*
            // at 12.5 s, and "0:13" would put it after a moment it precedes.
            Some(at) => println!(
                "  {}  {}",
                ui::dim(&format!("{}:{:02}", at / 60_000, at / 1000 % 60)),
                line.text
            ),
            None => println!("  {}", line.text),
        }
    }
}

/// Disc and track number, spelled out; `None` when the tags gave neither.
fn position(track: &Track) -> Option<String> {
    match (track.disc_no, track.track_no) {
        (Some(disc), Some(no)) => Some(format!("disc {disc}, track {no}")),
        (None, Some(no)) => Some(format!("track {no}")),
        (Some(disc), None) => Some(format!("disc {disc}")),
        (None, None) => None,
    }
}

fn dedupe(mut names: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

fn movement_tag(file: &aede_core::model::AudioFile) -> Option<String> {
    let number = file.first_tag("movementnumber");
    let total = file.first_tag("movementtotal");
    let title = file.first_tag("movement");
    if number.is_none() && title.is_none() {
        return None;
    }
    let position = match (number, total) {
        (Some(number), Some(total)) => format!("{number}/{total}"),
        (Some(number), None) => number.to_string(),
        _ => String::new(),
    };
    Some(match (position.is_empty(), title) {
        (true, Some(title)) => title.to_string(),
        (false, Some(title)) => format!("{position} · {title}"),
        (false, None) => position,
        (true, None) => String::new(),
    })
}

fn as_json(
    catalog: &Catalog,
    track: &Track,
    sourced: &[SourcedCreditLink],
    edition: &[sources::SourcedEditionCreditLink],
    parent_works: &[sources::SourcedWorkParentLink],
) -> Json {
    let mut o = Json::obj();
    o.set("id", track.id.into());
    o.set("title", track.title.clone().into());
    o.set("disc_no", track.disc_no.into());
    o.set("track_no", track.track_no.into());
    o.set("duration_ms", track.duration_ms.into());
    o.set("isrc", track.isrc.clone().into());

    let release = track.release_id.and_then(|id| catalog.release(id));
    o.set(
        "release",
        release.map(|r| r.title.clone()).unwrap_or_default().into(),
    );
    o.set("year", release.and_then(|r| r.year).into());
    o.set(
        "album_artist",
        release
            .and_then(|r| r.album_artist_id)
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_default()
            .into(),
    );

    let credits = Json::Arr(
        catalog
            .credits
            .iter()
            .filter(|credit| {
                credit.entity_kind == EntityKind::Track && credit.entity_id == track.id
            })
            .filter_map(|assertion| {
                let artist = catalog.artist(assertion.artist_id)?;
                let mut c = Json::obj();
                c.set("artist", artist.name.clone().into());
                c.set("role", assertion.role.clone().into());
                c.set("credited_as", assertion.credited_as.clone().into());
                c.set(
                    "attributes",
                    Json::Arr(assertion.attributes.iter().map(attribute_json).collect()),
                );
                c.set("began", assertion.began.clone().into());
                c.set("ended", assertion.ended.clone().into());
                c.set("order", assertion.order.into());
                c.set("source", assertion.source.clone().into());
                c.set("source_id", assertion.source_id.clone().into());
                Some(c)
            })
            .collect(),
    );
    o.set("credits", credits);
    o.set(
        "local_edition_credits",
        Json::Arr(
            catalog
                .credits
                .iter()
                .filter(|credit| {
                    credit.entity_kind == EntityKind::Release
                        && track.release_id == Some(credit.entity_id)
                })
                .filter_map(|assertion| {
                    let artist = catalog.artist(assertion.artist_id)?;
                    let mut credit = Json::obj();
                    credit.set("artist", artist.name.clone().into());
                    credit.set("role", assertion.role.clone().into());
                    credit.set("source", assertion.source.clone().into());
                    Some(credit)
                })
                .collect(),
        ),
    );
    o.set(
        "sourced_credits",
        Json::Arr(
            sourced
                .iter()
                .filter(|link| link.recording_id == track.recording_id)
                .map(|link| {
                    let mut credit = Json::obj();
                    credit.set("artist", link.credit.artist_name.clone().into());
                    credit.set("artist_mbid", link.credit.artist_mbid.clone().into());
                    credit.set("role", link.credit.role.clone().into());
                    credit.set("role_id", link.credit.role_id.clone().into());
                    credit.set("relation_id", link.credit.relation_id.clone().into());
                    credit.set("direction", link.credit.direction.clone().into());
                    credit.set("credited_as", link.credit.credited_as.clone().into());
                    credit.set(
                        "attributes",
                        Json::Arr(link.credit.attributes.iter().map(attribute_json).collect()),
                    );
                    credit.set("began", link.credit.began.clone().into());
                    credit.set("ended", link.credit.ended.clone().into());
                    credit.set("ended_explicitly", link.credit.over.into());
                    credit.set("order", link.credit.order.into());
                    credit.set(
                        "scope",
                        if link.work.is_some() {
                            "work"
                        } else {
                            "recording"
                        }
                        .into(),
                    );
                    credit.set("recording_id", link.recording_id.into());
                    credit.set(
                        "work_mbid",
                        link.work.as_ref().map(|work| work.mbid.clone()).into(),
                    );
                    credit.set(
                        "work_title",
                        link.work.as_ref().map(|work| work.title.clone()).into(),
                    );
                    credit.set("source", link.source.clone().into());
                    let (confidence, score) = match link.confidence {
                        sources::Confidence::Identified => ("identified", None),
                        sources::Confidence::Matched(score) => ("matched", Some(score as u32)),
                    };
                    credit.set("confidence", confidence.into());
                    credit.set("confidence_score", score.into());
                    credit.set("trusted", link.trusted.into());
                    credit.set("excluded", link.excluded.into());
                    credit.set(
                        "review",
                        link.review
                            .map(|decision| match decision {
                                sources::ReviewDecision::Accepted => "accepted".to_string(),
                                sources::ReviewDecision::Rejected => "rejected".to_string(),
                            })
                            .into(),
                    );
                    credit.set("fetched_at", link.fetched_at.into());
                    credit
                })
                .collect(),
        ),
    );

    o.set(
        "sourced_edition_credits",
        Json::Arr(
            edition
                .iter()
                .filter(|link| track.release_id == Some(link.release_id))
                .map(|link| {
                    let mut credit = Json::obj();
                    credit.set("artist", link.credit.artist_name.clone().into());
                    credit.set("artist_mbid", link.credit.artist_mbid.clone().into());
                    credit.set("role", link.credit.role.clone().into());
                    credit.set("relation_id", link.credit.relation_id.clone().into());
                    credit.set("credited_as", link.credit.credited_as.clone().into());
                    credit.set(
                        "attributes",
                        Json::Arr(link.credit.attributes.iter().map(attribute_json).collect()),
                    );
                    credit.set("scope", "edition".into());
                    credit.set("source", link.source.clone().into());
                    credit.set("trusted", link.trusted.into());
                    credit.set("excluded", link.excluded.into());
                    credit.set("fetched_at", link.fetched_at.into());
                    credit
                })
                .collect(),
        ),
    );
    o.set(
        "parent_works",
        Json::Arr(
            parent_works
                .iter()
                .filter(|link| link.recording_id == track.recording_id)
                .map(|link| {
                    let mut parent = Json::obj();
                    parent.set("child_mbid", link.child_mbid.clone().into());
                    parent.set("parent_mbid", link.parent.mbid.clone().into());
                    parent.set("parent_title", link.parent.title.clone().into());
                    parent.set("relation_id", link.parent.relation_id.clone().into());
                    parent.set("order", link.parent.order.into());
                    parent.set("source", link.source.clone().into());
                    parent.set("trusted", link.trusted.into());
                    parent.set("fetched_at", link.fetched_at.into());
                    parent.set(
                        "attributes",
                        Json::Arr(link.parent.attributes.iter().map(attribute_json).collect()),
                    );
                    parent
                })
                .collect(),
        ),
    );

    if let Some(file) = catalog.file(track.file_id) {
        o.set("work_tag", file.first_tag("grouping").into());
        o.set("movement_tag", file.first_tag("movement").into());
        o.set("movement_number", file.first_tag("movementnumber").into());
        o.set("movement_total", file.first_tag("movementtotal").into());
        o.set("path", file.path.clone().into());
        o.set("size", file.size.into());
        o.set("codec", file.properties.codec.clone().into());
        o.set("container", file.properties.container.clone().into());
        o.set("sample_rate", file.properties.sample_rate.into());
        o.set("bit_depth", file.properties.bit_depth.map(u32::from).into());
        o.set("channels", file.properties.channels.map(u32::from).into());
        o.set("bitrate_kbps", file.properties.bitrate_kbps.into());
        o.set("lossless", file.properties.lossless.into());
        let mut tags = Json::obj();
        for (key, values) in &file.tags {
            tags.set(key, values.join(" / ").into());
        }
        o.set("tags", tags);
        o.set(
            "analyses",
            Json::Arr(
                catalog
                    .analyses_of(file)
                    .map(|record| analysis_json(record, file))
                    .collect(),
            ),
        );
    }
    o
}

fn analysis_json(
    record: &aede_core::analysis::FileAnalysis,
    file: &aede_core::model::AudioFile,
) -> Json {
    let mut value = aede_core::store::analysis_to_json(record);
    value.set(
        "stale",
        (!record.still_applies(file.size, file.mtime)).into(),
    );
    value
}

fn attribute_json(attribute: &aede_core::model::CreditAttribute) -> Json {
    let mut value = Json::obj();
    value.set("id", attribute.id.clone().into());
    value.set("name", attribute.name.clone().into());
    value.set("value", attribute.value.clone().into());
    value.set("credited_as", attribute.credited_as.clone().into());
    value
}

/// What the last integrity check said about the file behind a track.
///
/// "not verified" is a state of its own, and saying so is the point: a blank
/// here would read as "fine".
fn integrity_line(track: &Track, catalog: &Catalog) -> String {
    use aede_core::audit::integrity::Verdict;
    let Some(record) = catalog
        .file(track.file_id)
        .and_then(|f| f.integrity.as_ref())
    else {
        return "not verified — run aede check".to_string();
    };
    match &record.verdict {
        Verdict::Intact => format!("intact ({})", record.method),
        Verdict::NothingToCheck => "the container carries no checksum".to_string(),
        Verdict::Damaged { detail } => format!("damaged — {detail}"),
    }
}

/// Shows what another tool measured on this file, when something was imported.
///
/// Attributed by name, and kept apart from Aède's own panel above: the reader
/// has to be able to tell which program said what, especially when the two
/// disagree.
fn print_analyses(catalog: &Catalog, file: &aede_core::model::AudioFile) {
    for record in catalog.analyses_of(file) {
        let stale = if record.still_applies(file.size, file.mtime) {
            String::new()
        } else {
            " — stale: the file changed since".to_string()
        };
        println!(
            "{}",
            ui::section(&format!("Analysed by {}{stale}", record.source))
        );
        print!("{}", analysis_table(record).render());
    }
}

fn analysis_table(record: &aede_core::analysis::FileAnalysis) -> Table {
    let source = record.source_data.as_ref();
    let nested = |key| source.and_then(|data| data.get(key));
    let mut table = Table::plain(2);
    let mut row = |label: &str, value: Option<String>| {
        if let Some(value) = value {
            table.push(vec![label.into(), value]);
        }
    };
    row("FLAC audio MD5", record.md5_state.clone());
    row("File MD5", record.file_md5.clone());
    row(
        "File CRC32",
        source.and_then(|data| data.field_str("file_crc32")),
    );
    row(
        "Real bit depth",
        record.real_bit_depth.map(|bits| format!("{bits} bits")),
    );
    row(
        "Bit-depth method",
        nested("bit_depth_evidence").and_then(|v| v.field_str("method")),
    );
    row(
        "Requantization rate",
        record.requant_rate.map(|v| format!("{v:.4}")),
    );
    row(
        "Codec lattice score",
        source
            .and_then(|v| v.field_f64("lattice_score"))
            .map(|v| format!("{v:.4}")),
    );
    row("Fake stereo", record.fake_stereo.map(yes_no));
    row(
        "Phase correlation",
        source
            .and_then(|v| v.field_f64("phase_correlation"))
            .map(|v| format!("{v:.3}")),
    );
    row(
        "Phase inverted",
        source
            .and_then(|v| v.field_optional_bool("phase_inverted"))
            .map(yes_no),
    );
    row(
        "Stereo balance",
        nested("stereo_balance").and_then(stereo_balance),
    );
    let high = nested("high_frequency_stereo");
    row(
        "High-band side/mid",
        high.and_then(|v| v.field_f64("side_to_mid_db"))
            .map(|v| format!("{v:.2} dB")),
    );
    row(
        "High-band reference",
        high.and_then(|v| v.field_f64("reference_side_to_mid_db"))
            .map(|v| format!("{v:.2} dB")),
    );
    row(
        "High-band narrowed",
        high.and_then(|v| v.field_optional_bool("narrowed"))
            .map(yes_no),
    );
    row(
        "High-band narrowed blocks",
        high.and_then(|v| v.field_f64("narrowed_block_fraction"))
            .map(|v| format!("{:.1}%", v * 100.0)),
    );
    let dc = nested("dc_offset");
    row(
        "DC offset (maximum)",
        dc.and_then(|v| v.field_f64("max_abs"))
            .map(|v| format!("{v:.6}")),
    );
    row(
        "DC offset by channel",
        dc.and_then(|v| v.get("channel_means"))
            .and_then(Json::as_arr)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Json::as_f64)
                    .map(|v| format!("{v:.6}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
    );
    let phase = nested("local_phase");
    row(
        "Local phase",
        phase
            .and_then(|v| v.get("broadband"))
            .and_then(phase_summary),
    );
    if let Some(bands) = phase.and_then(|v| v.get("bands")).and_then(Json::as_arr) {
        for band in bands {
            if let (Some(low), Some(high), Some(summary)) = (
                band.field_f64("low_hz"),
                band.field_f64("high_hz"),
                band.get("summary").and_then(phase_summary),
            ) {
                row(&format!("Phase {low:.0}–{high:.0} Hz"), Some(summary));
            }
        }
    }
    row(
        "Cutoff",
        record.cutoff_hz.map(|hz| format!("{:.1} kHz", hz / 1000.0)),
    );
    row(
        "Cutoff / Nyquist",
        record
            .cutoff_ratio
            .map(|ratio| format!("{:.1}%", ratio * 100.0)),
    );
    row(
        "Dynamic range",
        record.dr_db.map(|db| format!("{db:.1} dB")),
    );
    row(
        "Integrated loudness",
        record.integrated_lufs.map(|lufs| format!("{lufs:.2} LUFS")),
    );
    row(
        "Loudness range",
        source
            .and_then(|v| v.field_f64("loudness_range_lu"))
            .map(|v| format!("{v:.2} LU")),
    );
    let peaks = nested("loudness_peaks");
    row(
        "Momentary maximum",
        peaks
            .and_then(|v| v.get("momentary"))
            .and_then(loudness_peak),
    );
    row(
        "Short-term maximum",
        peaks
            .and_then(|v| v.get("short_term"))
            .and_then(loudness_peak),
    );
    row("Peak", record.peak_dbfs.map(|db| format!("{db:.2} dBFS")));
    row(
        "True peak",
        record.true_peak_dbtp.map(|db| format!("{db:.2} dBTP")),
    );
    row(
        "Clipped samples",
        record.clipped_samples.map(|n| n.to_string()),
    );
    row("Clipping events", record.clip_events.map(|n| n.to_string()));
    row("Clipped", record.clipped.map(yes_no));
    let discontinuities = nested("discontinuities");
    row(
        "Clicks",
        discontinuities
            .and_then(|v| v.get("clicks"))
            .and_then(event_summary),
    );
    row(
        "Dropouts",
        discontinuities
            .and_then(|v| v.get("dropouts"))
            .and_then(event_summary),
    );
    for (kind, label) in [("clicks", "Click"), ("dropouts", "Dropout")] {
        if let Some(events) = discontinuities
            .and_then(|v| v.get(kind))
            .and_then(|v| v.get("events"))
            .and_then(Json::as_arr)
        {
            for (index, event) in events.iter().enumerate() {
                row(&format!("{label} {}", index + 1), event_location(event));
            }
        }
    }
    row("Source badge", source.and_then(|v| v.field_str("badge")));
    row("Source verdict", record.summary.clone());
    row("Source detail", record.detail.clone());
    row("Transcoding flag", record.transcoding.clone());
    row("Upscaling flag", record.upscaling.map(yes_no));
    row("Upsampling flag", record.upsampling.map(yes_no));
    row("Error", record.error.clone());
    table
}

fn stereo_balance(value: &Json) -> Option<String> {
    let state = value.field_str("state")?;
    Some(match value.field_f64("right_minus_left_db") {
        Some(db) => format!("{state}, right − left {db:.2} dB"),
        None => state,
    })
}

fn phase_summary(value: &Json) -> Option<String> {
    let correlation = value.field_f64("correlation")?;
    let mut summary = format!("correlation {correlation:.3}");
    if let Some(minimum) = value.field_f64("minimum_correlation") {
        summary.push_str(&format!(", minimum {minimum:.3}"));
    }
    if let Some(at) = value.field_f64("minimum_start_secs") {
        summary.push_str(&format!(" at {at:.2} s"));
    }
    if let Some(opposed) = value.field_f64("opposed_fraction") {
        summary.push_str(&format!(", opposed {:.1}%", opposed * 100.0));
    }
    Some(summary)
}

fn loudness_peak(value: &Json) -> Option<String> {
    let lufs = value.field_f64("lufs")?;
    Some(match value.field_f64("start_secs") {
        Some(at) => format!("{lufs:.2} LUFS at {at:.2} s"),
        None => format!("{lufs:.2} LUFS"),
    })
}

fn event_summary(value: &Json) -> Option<String> {
    let count = value.field_u64("count")?;
    let events = value.get("events").and_then(Json::as_arr);
    Some(match events.and_then(|events| events.first()) {
        Some(first) => match first.field_f64("start_secs") {
            Some(at) => format!("{count} (first at {at:.3} s)"),
            None => count.to_string(),
        },
        None => count.to_string(),
    })
}

fn event_location(value: &Json) -> Option<String> {
    let start = value.field_f64("start_secs")?;
    let mut location = format!("at {start:.3} s");
    if let Some(channel) = value.field_u64("channel") {
        location.push_str(&format!(", channel {channel}"));
    }
    if let Some(duration) = value.field_f64("duration_secs") {
        location.push_str(&format!(", duration {duration:.6} s"));
    }
    Some(location)
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_string()
}

#[cfg(test)]
#[path = "track_tests.rs"]
mod tests;
