//! Explicit resolution of uncertain and conflicting source identities.
//!
//! Confidence belongs to the source; acceptance belongs to the user. This
//! command keeps those two statements separate and makes the latter durable,
//! reversible, and visible without ever rewriting an audio tag.

use std::io::{BufRead, Write};
use std::path::Path;

use aede_core::model::{Catalog, EntityKind};
use aede_core::sources::{
    self, Facts, ReviewDecision, ReviewItem, ReviewReason, SourceRecord, Sources,
};

use super::{Res, announce_window, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Table};

const DEFAULT_LIMIT: usize = 50;

pub fn review(args: &Args) -> Res {
    let catalog = load(args)?;
    let path = sources::sources_path(&data_dir(args));
    let mut held = sources::load(&path)?.unwrap_or_default();

    let actions = ["accept", "reject", "undo"]
        .into_iter()
        .filter(|action| args.has(action))
        .collect::<Vec<_>>();
    if actions.len() > 1 {
        return Err(format!(
            "{} ask for different decisions; choose one",
            actions
                .iter()
                .map(|action| format!("--{action}"))
                .collect::<Vec<_>>()
                .join(" and ")
        )
        .into());
    }

    if args.has("interactive") && !actions.is_empty() {
        return Err(
            "--interactive presents the decisions itself; drop --accept, --reject, or --undo"
                .into(),
        );
    }

    if let Some(action) = actions.first() {
        if !args.positionals.is_empty() {
            return Err(format!(
                "a name filters the review list and means nothing with --{action}; drop \"{}\"",
                args.positionals.join(" ")
            )
            .into());
        }
        let unused = ["all", "limit", "offset", "source"]
            .into_iter()
            .filter(|option| args.has(option))
            .map(|option| format!("--{option}"))
            .collect::<Vec<_>>();
        if !unused.is_empty() {
            return Err(format!(
                "{} only shape the review list and mean nothing with --{action}",
                unused.join(" and ")
            )
            .into());
        }
        let selector = args
            .value(action)
            .ok_or_else(|| format!("--{action} expects the ID printed by aede review"))?;
        let item = match *action {
            "accept" => held.decide(
                &catalog,
                selector,
                ReviewDecision::Accepted,
                aede_core::clock::now_seconds(),
            )?,
            "reject" => held.decide(
                &catalog,
                selector,
                ReviewDecision::Rejected,
                aede_core::clock::now_seconds(),
            )?,
            "undo" => held.clear_review(&catalog, selector)?,
            _ => unreachable!("the action list is closed"),
        };
        sources::save(&held, &path)?;
        print_decision(action, &item, &path);
        return Ok(());
    }

    let only_source = args.value("source");
    let wanted = aede_core::text::normalize(&args.positionals.join(" "));
    let all_items = held.review_items(&catalog);
    let selected = all_items
        .iter()
        .filter(|item| only_source.is_none_or(|source| item.source == source))
        .filter(|item| {
            wanted.is_empty()
                || aede_core::text::normalize(&item.entity.key).contains(&wanted)
                || aede_core::text::normalize(&item.entity.display_name(&catalog)).contains(&wanted)
                || item
                    .source_id
                    .as_deref()
                    .is_some_and(|id| aede_core::text::normalize(id).contains(&wanted))
        })
        .filter(|item| args.has("all") || item.decision.is_none())
        .cloned()
        .collect::<Vec<_>>();

    if selected.is_empty() {
        println!("{}", ui::section("Source review"));
        let resolved = all_items
            .iter()
            .filter(|item| item.decision.is_some())
            .count();
        if resolved > 0 && !args.has("all") && wanted.is_empty() && only_source.is_none() {
            println!("  {}", ui::green("Nothing is waiting for review."));
            println!(
                "  {}",
                ui::dim(&format!(
                    "{} already resolved — aede review --all shows them",
                    ui::plural(resolved, "claim")
                ))
            );
        } else {
            let message = match wanted.is_empty() {
                true => "Nothing needs review.".to_string(),
                false => format!(
                    "Nothing needing review matches \"{}\".",
                    args.positionals.join(" ")
                ),
            };
            println!("  {}", ui::green(&message));
        }
        return Ok(());
    }

    let window = args.window(DEFAULT_LIMIT)?;
    if args.has("interactive") {
        let items = selected
            .iter()
            .skip(window.offset)
            .take(window.limit)
            .cloned()
            .collect::<Vec<_>>();
        if items.is_empty() {
            announce_window(window, selected.len(), "claim");
            return Ok(());
        }
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        return interactive_review(
            &catalog,
            &mut held,
            &path,
            items,
            stdin.lock(),
            stdout.lock(),
        );
    }

    println!("{}", ui::section("Source review"));
    let mut table = Table::new(&["ID", "Entity", "Source", "Why", "Status"])
        .limit(1, 34)
        .limit(3, 46);
    for item in selected.iter().skip(window.offset).take(window.limit) {
        table.push(vec![
            item.id.clone(),
            format!(
                "{} {}",
                item.entity.kind.as_str(),
                item.entity.display_name(&catalog)
            ),
            item.source.clone(),
            reason(item),
            status(item),
        ]);
    }
    print!("{}", table.render());
    announce_window(window, selected.len(), "claim");
    println!(
        "  {}",
        ui::dim("aede review --accept=<ID> trusts it; --reject=<ID> keeps it as evidence only")
    );
    println!(
        "  {}",
        ui::dim("aede review --undo=<ID> makes either decision pending again; tags never change")
    );
    Ok(())
}

fn interactive_review<R: BufRead, W: Write>(
    catalog: &Catalog,
    held: &mut Sources,
    path: &Path,
    mut items: Vec<ReviewItem>,
    mut input: R,
    mut output: W,
) -> Res {
    let total = items.len();
    let mut index = 0usize;
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut undone = 0usize;
    let mut stopped = false;

    while index < items.len() {
        if ui::is_interactive() {
            write!(output, "\x1b[2J\x1b[H")?;
        }
        let item = &items[index];
        let record = record_for(held, item).ok_or_else(|| {
            format!(
                "review {} no longer has a source record; run aede review again",
                item.id
            )
        })?;
        render_card(&mut output, catalog, record, item, index + 1, total)?;

        loop {
            write!(
                output,
                "\n  [A]ccept  [R]eject  [S]kip  [P]revious  [U]ndo  [?] Help  [Q]uit\n  Choice: "
            )?;
            output.flush()?;

            let mut answer = String::new();
            if input.read_line(&mut answer)? == 0 {
                stopped = true;
                break;
            }
            match answer.trim().to_ascii_lowercase().as_str() {
                "a" | "accept" => {
                    let decided = held.decide(
                        catalog,
                        &items[index].id,
                        ReviewDecision::Accepted,
                        aede_core::clock::now_seconds(),
                    )?;
                    sources::save(held, path)?;
                    items[index] = decided;
                    accepted += 1;
                    index += 1;
                    break;
                }
                "r" | "reject" => {
                    let decided = held.decide(
                        catalog,
                        &items[index].id,
                        ReviewDecision::Rejected,
                        aede_core::clock::now_seconds(),
                    )?;
                    sources::save(held, path)?;
                    items[index] = decided;
                    rejected += 1;
                    index += 1;
                    break;
                }
                "u" | "undo" if items[index].decision.is_some() => {
                    let cleared = held.clear_review(catalog, &items[index].id)?;
                    sources::save(held, path)?;
                    items[index] = cleared;
                    undone += 1;
                    index += 1;
                    break;
                }
                "u" | "undo" => {
                    writeln!(
                        output,
                        "  {} This claim is already pending.",
                        ui::yellow("→")
                    )?;
                }
                "s" | "skip" | "n" | "next" | "" => {
                    index += 1;
                    break;
                }
                "p" | "previous" if index > 0 => {
                    index -= 1;
                    break;
                }
                "p" | "previous" => {
                    writeln!(output, "  {} This is the first claim.", ui::yellow("→"))?;
                }
                "?" | "h" | "help" => {
                    writeln!(
                        output,
                        "  Accept lets this source claim feed navigation and queries.\n  Reject keeps it as evidence only. Skip changes nothing.\n  Undo returns an accepted or rejected claim to pending."
                    )?;
                }
                "q" | "quit" => {
                    stopped = true;
                    break;
                }
                _ => {
                    writeln!(
                        output,
                        "  {} Choose A, R, S, P, U, ?, or Q.",
                        ui::yellow("→")
                    )?;
                }
            }
        }
        if stopped {
            break;
        }
    }

    writeln!(output, "\n{}", ui::section("Review summary"))?;
    writeln!(
        output,
        "  {} accepted · {} rejected · {} returned to pending",
        accepted, rejected, undone
    )?;
    let remaining = total.saturating_sub(index);
    if stopped && remaining > 0 {
        let ending = if accepted + rejected + undone == 0 {
            "nothing was changed"
        } else {
            "every decision made above was saved"
        };
        writeln!(
            output,
            "  {} {} left in this session; {ending}.",
            ui::yellow("→"),
            ui::plural(remaining, "claim")
        )?;
    } else {
        writeln!(output, "  {} End of this review window.", ui::green("✓"))?;
    }
    Ok(())
}

fn record_for<'a>(held: &'a Sources, item: &ReviewItem) -> Option<&'a SourceRecord> {
    held.records.iter().find(|record| {
        record.entity() == item.entity
            && record.source == item.source
            && record.source_id == item.source_id
    })
}

fn render_card(
    output: &mut impl Write,
    catalog: &Catalog,
    record: &SourceRecord,
    item: &ReviewItem,
    position: usize,
    total: usize,
) -> std::io::Result<()> {
    let name = item.entity.display_name(catalog);
    writeln!(
        output,
        "┌─ {} {position}/{total} · {}",
        ui::bold("Source review"),
        ui::dim(&item.id)
    )?;
    writeln!(
        output,
        "│  {} · {}",
        ui::cyan(&item.entity.kind.as_str().replace('_', " ").to_uppercase()),
        ui::bold(&name)
    )?;
    writeln!(output, "├─ {}", ui::bold("Why Aède is asking"))?;
    card_field(output, "Reason", &reason(item))?;
    card_field(output, "Status", &plain_status(item))?;

    writeln!(output, "├─ {}", ui::bold("Your library"))?;
    for (label, value) in local_facts(catalog, item) {
        card_field(output, label, &value)?;
    }

    writeln!(output, "├─ {}", ui::bold("Source proposal"))?;
    card_field(output, "Source", &item.source)?;
    card_field(
        output,
        "Identity",
        item.source_id.as_deref().unwrap_or("not supplied"),
    )?;
    card_field(output, "Fetched", &ui::since(record.fetched_at))?;
    for (label, value) in source_facts(record) {
        card_field(output, label, &value)?;
    }
    if let Some(url) = source_url(item) {
        card_field(output, "Check", &url)?;
    }

    writeln!(output, "├─ {}", ui::bold("If accepted"))?;
    for line in ui::wrap(&impact(record), 68) {
        writeln!(output, "│  {line}")?;
    }
    writeln!(
        output,
        "└─ {}",
        ui::dim("Audio tags stay untouched whatever you choose.")
    )
}

fn card_field(output: &mut impl Write, label: &str, value: &str) -> std::io::Result<()> {
    if matches!(label, "File" | "Folder") {
        // Paths are deliberately not wrapped or truncated: the point of this
        // row is to let the user identify the exact local file on disk.
        return writeln!(output, "│  {label:<11} {value}");
    }
    let mut lines = ui::wrap(value, 53).into_iter();
    let first = lines.next().unwrap_or_default();
    writeln!(output, "│  {label:<11} {first}")?;
    for line in lines {
        writeln!(output, "│  {:<11} {line}", "")?;
    }
    Ok(())
}

fn local_facts(catalog: &Catalog, item: &ReviewItem) -> Vec<(&'static str, String)> {
    let Some(id) = item.entity.resolve(catalog) else {
        return vec![("Entity", "no longer present in the catalog".to_string())];
    };
    match item.entity.kind {
        EntityKind::Artist => catalog.artist(id).map_or_else(Vec::new, |artist| {
            let mut rows = vec![
                ("Name", artist.name.clone()),
                ("MusicBrainz ID", tag_identity(&artist.mbid)),
            ];
            push_paths(&mut rows, artist_paths(catalog, artist.id));
            rows
        }),
        EntityKind::Release => catalog.release(id).map_or_else(Vec::new, |release| {
            let mut rows = vec![
                ("Title", release.title.clone()),
                ("MusicBrainz ID", tag_identity(&release.mbid)),
                (
                    "Date",
                    release.date.clone().unwrap_or_else(|| "unknown".into()),
                ),
                ("Folder", release.folder.clone()),
            ];
            push_paths(
                &mut rows,
                release
                    .track_ids
                    .iter()
                    .filter_map(|track| catalog.track(*track))
                    .filter_map(|track| catalog.file(track.file_id))
                    .map(|file| file.path.clone())
                    .collect(),
            );
            rows
        }),
        EntityKind::Track => catalog.track(id).map_or_else(Vec::new, |track| {
            let file = catalog
                .file(track.file_id)
                .map(|file| file.path.clone())
                .unwrap_or_else(|| item.entity.key.clone());
            let album = track
                .release_id
                .and_then(|release| catalog.release(release))
                .map(|release| release.title.clone())
                .unwrap_or_else(|| "none".into());
            vec![
                ("Title", track.title.clone()),
                ("MusicBrainz ID", tag_identity(&track.mbid)),
                ("Album", album),
                ("File", file),
            ]
        }),
        EntityKind::Recording => catalog.recording(id).map_or_else(Vec::new, |recording| {
            vec![
                ("Title", recording.title.clone()),
                ("MusicBrainz ID", tag_identity(&recording.mbid)),
            ]
        }),
        EntityKind::Work => catalog.work(id).map_or_else(Vec::new, |work| {
            vec![
                ("Title", work.title.clone()),
                ("MusicBrainz ID", work.mbid.clone()),
            ]
        }),
        EntityKind::ReleaseGroup => catalog.release_group(id).map_or_else(Vec::new, |group| {
            vec![
                ("Title", group.title.clone()),
                ("MusicBrainz ID", group.mbid.clone()),
            ]
        }),
        EntityKind::Label => catalog.label(id).map_or_else(Vec::new, |label| {
            let mut rows = vec![
                ("Name", label.name.clone()),
                ("MusicBrainz ID", tag_identity(&label.mbid)),
            ];
            let folders = catalog
                .releases
                .iter()
                .filter(|release| release.label_ids.contains(&label.id))
                .map(|release| release.folder.clone())
                .collect();
            push_paths(&mut rows, folders);
            rows
        }),
        EntityKind::Genre => catalog
            .genres
            .get(id as usize)
            .map_or_else(Vec::new, |genre| vec![("Name", genre.name.clone())]),
    }
}

fn source_facts(record: &SourceRecord) -> Vec<(&'static str, String)> {
    match &record.facts {
        Facts::Artist(facts) => {
            let mut rows = Vec::new();
            push_some(&mut rows, "Type", facts.kind.as_ref());
            push_some(&mut rows, "Area", facts.area.as_ref());
            if facts.began.is_some() || facts.ended.is_some() {
                rows.push((
                    "Dates",
                    format!(
                        "{}–{}",
                        facts.began.as_deref().unwrap_or("?"),
                        facts.ended.as_deref().unwrap_or("present or unknown")
                    ),
                ));
            }
            push_some(&mut rows, "Note", facts.disambiguation.as_ref());
            push_list(&mut rows, "Genres", &facts.genres);
            push_list(&mut rows, "Aliases", &facts.aliases);
            if !facts.members.is_empty() {
                rows.push(("Members", ui::plural(facts.members.len(), "relationship")));
            }
            if !facts.discography.is_empty() {
                rows.push((
                    "Releases",
                    ui::plural(facts.discography.len(), "credited release"),
                ));
            }
            rows
        }
        Facts::Release(facts) => {
            let mut rows = Vec::new();
            let mut release_type = facts.primary_type.clone().unwrap_or_default();
            if !facts.secondary_types.is_empty() {
                if !release_type.is_empty() {
                    release_type.push_str(" · ");
                }
                release_type.push_str(&facts.secondary_types.join(" · "));
            }
            if !release_type.is_empty() {
                rows.push(("Type", release_type));
            }
            push_some(&mut rows, "First date", facts.first_released.as_ref());
            push_some(&mut rows, "Label", facts.label.as_ref());
            push_some(&mut rows, "Label ID", facts.label_mbid.as_ref());
            rows
        }
        Facts::Track(facts) => {
            let mut rows = Vec::new();
            push_some(&mut rows, "Title", facts.title.as_ref());
            push_list(&mut rows, "Artists", &facts.artists);
            push_some(&mut rows, "Album", facts.album.as_ref());
            push_some(&mut rows, "Recording", facts.recording.as_ref());
            if !facts.works.is_empty() {
                let works = facts
                    .works
                    .iter()
                    .map(|work| {
                        if work.title.is_empty() {
                            work.mbid.as_str()
                        } else {
                            work.title.as_str()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                rows.push(("Works", works));
            }
            if !facts.credits.is_empty() {
                let credits = facts
                    .credits
                    .iter()
                    .map(|credit| format!("{} — {}", credit.artist_name, credit.role))
                    .collect::<Vec<_>>()
                    .join(", ");
                rows.push(("Credits", credits));
            }
            rows
        }
        Facts::Label(facts) => facts
            .logo
            .as_ref()
            .map(|logo| vec![("Logo", logo.url.clone())])
            .unwrap_or_default(),
    }
}

fn push_some(rows: &mut Vec<(&'static str, String)>, label: &'static str, value: Option<&String>) {
    if let Some(value) = value {
        rows.push((label, value.clone()));
    }
}

fn push_list(rows: &mut Vec<(&'static str, String)>, label: &'static str, values: &[String]) {
    if !values.is_empty() {
        rows.push((label, values.join(", ")));
    }
}

fn tag_identity(value: &Option<String>) -> String {
    value
        .clone()
        .unwrap_or_else(|| "not present in local metadata".into())
}

fn artist_paths(catalog: &Catalog, artist_id: aede_core::model::Id) -> Vec<String> {
    let mut paths = catalog
        .tracks_of_artist(artist_id)
        .into_iter()
        .filter_map(|track| catalog.track(track))
        .filter_map(|track| catalog.file(track.file_id))
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    for release_id in catalog.releases_as_album_artist(artist_id) {
        if let Some(release) = catalog.release(release_id) {
            paths.extend(
                release
                    .track_ids
                    .iter()
                    .filter_map(|track| catalog.track(*track))
                    .filter_map(|track| catalog.file(track.file_id))
                    .map(|file| file.path.clone()),
            );
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn push_paths(rows: &mut Vec<(&'static str, String)>, mut paths: Vec<String>) {
    paths.sort();
    paths.dedup();
    const MAX_PATHS: usize = 5;
    for path in paths.iter().take(MAX_PATHS) {
        rows.push(("File", path.clone()));
    }
    if paths.len() > MAX_PATHS {
        rows.push((
            "Files",
            format!("… and {} more local files", paths.len() - MAX_PATHS),
        ));
    }
}

fn plain_status(item: &ReviewItem) -> String {
    match item.decision {
        None => "pending".to_string(),
        Some(ReviewDecision::Accepted) => "accepted".to_string(),
        Some(ReviewDecision::Rejected) => "rejected".to_string(),
    }
}

fn source_url(item: &ReviewItem) -> Option<String> {
    let id = item.source_id.as_deref()?;
    match item.source.as_str() {
        sources::MUSICBRAINZ => {
            let kind = match item.entity.kind {
                EntityKind::Artist => "artist",
                EntityKind::Release => "release",
                EntityKind::Track | EntityKind::Recording => "recording",
                EntityKind::Work => "work",
                EntityKind::ReleaseGroup => "release-group",
                EntityKind::Label => "label",
                EntityKind::Genre => return None,
            };
            Some(format!("https://musicbrainz.org/{kind}/{id}"))
        }
        "acoustid" => Some(format!("https://acoustid.org/track/{id}")),
        _ => None,
    }
}

fn impact(record: &SourceRecord) -> String {
    match &record.facts {
        Facts::Artist(facts) if facts.members.is_empty() && facts.discography.is_empty() => {
            "This identity may connect the local artist to canonical source data and later relationships."
                .into()
        }
        Facts::Artist(facts) => format!(
            "This identity and its {} and {} may feed artist navigation and relational queries.",
            ui::plural(facts.members.len(), "membership"),
            ui::plural(facts.discography.len(), "release")
        ),
        Facts::Release(_) => {
            "This release identity may feed edition, label, and discography navigation.".into()
        }
        Facts::Track(facts) if facts.works.is_empty() && facts.credits.is_empty() => {
            "This recording identity may connect the local track to canonical recording data and later relationships."
                .into()
        }
        Facts::Track(facts) => format!(
            "This recording identity and its {} and {} may feed work, credit, and collaboration queries.",
            ui::plural(facts.works.len(), "work link"),
            ui::plural(facts.credits.len(), "credit")
        ),
        Facts::Label(_) => {
            "This label identity may connect local releases to the canonical label page and its artwork."
                .into()
        }
    }
}

fn reason(item: &ReviewItem) -> String {
    match &item.reason {
        ReviewReason::Approximate { score } => format!("approximate match ({score}%)"),
        ReviewReason::IdentityConflict {
            local_id,
            sourced_id,
        } => format!("local {local_id} ≠ source {sourced_id}"),
        ReviewReason::PriorDecision => "explicit decision retained".to_string(),
    }
}

fn status(item: &ReviewItem) -> String {
    match item.decision {
        None => ui::yellow("pending").to_string(),
        Some(ReviewDecision::Accepted) => format!(
            "accepted · {}",
            ui::since(item.reviewed_at.unwrap_or_default())
        ),
        Some(ReviewDecision::Rejected) => format!(
            "rejected · {}",
            ui::since(item.reviewed_at.unwrap_or_default())
        ),
    }
}

fn print_decision(action: &str, item: &ReviewItem, path: &std::path::Path) {
    println!("{}", ui::section("Source review"));
    let effect = match action {
        "accept" => "accepted — this claim may now participate in navigation and queries",
        "reject" => "rejected — it remains visible evidence but is not a graph link",
        "undo" => "returned to pending review",
        _ => unreachable!("the action list is closed"),
    };
    println!(
        "{} {} {} from {}: {effect}",
        ui::green("→"),
        item.entity.kind.as_str(),
        item.entity.key,
        item.source
    );
    println!("  {}", ui::dim(&path.display().to_string()));
}
