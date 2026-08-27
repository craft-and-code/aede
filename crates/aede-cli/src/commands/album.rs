//! The `album` command: one page per release.

use aede_core::model::{Catalog, EntityKind, Id, Release, TitleMatch};
use aede_core::text;

use super::{Res, load, role_label, selection_output};
use crate::args::Args;
use crate::ui::{self, Align, Table};
use aede_core::model::{DUPLICATE, OTHER_EDITION};

pub fn show_album(args: &Args) -> Res {
    let catalog = load(args)?;
    let title = args.positionals.join(" ");
    if title.trim().is_empty() {
        return Err("give a title: aede album \"Kind of Blue\"".into());
    }
    let (found, kind) = catalog.find_releases(&title);
    let mut matches: Vec<&Release> = found;
    if matches.is_empty()
        && let Some(hit) = catalog
            .search(&title, 1)
            .first()
            .filter(|h| h.kind == EntityKind::Release)
            .and_then(|h| catalog.release(h.id))
    {
        matches.push(hit);
    }

    if matches.is_empty() {
        // `album` describes one album; the words are joined so that a title can
        // be typed without quotes. Someone naming two of them is after a list,
        // which is what the plural command is for.
        if args.positionals.len() > 1 {
            return Err(format!(
                "no album matches \"{title}\".\n\
                 aede album takes one title; for several, filter the list:\n\
                 \taede albums --csv --artist=\"<name>\"\n\
                 \taede albums --csv --year=1990"
            )
            .into());
        }
        return Err(format!("no album matches \"{title}\"").into());
    }

    let total = matches.len();
    let window = args.window(DEFAULT_LIMIT)?;
    let matches: Vec<&Release> = matches
        .into_iter()
        .skip(window.offset)
        .take(window.limit)
        .collect();

    // A selection covers every album matched, not the first of them.
    let tracks: Vec<Id> = matches.iter().flat_map(|r| r.track_ids.clone()).collect();
    if let Some(result) = selection_output(&catalog, &tracks, args) {
        return result;
    }

    if kind == TitleMatch::Partial {
        println!(
            "  {}",
            ui::dim(&format!(
                "no album is titled \"{title}\"; showing the titles containing it"
            ))
        );
    }
    for release in &matches {
        print_album(&catalog, release);
        super::panel_for(args, &catalog, EntityKind::Release, release.id);
    }
    if total > matches.len() {
        println!(
            "\n  {}",
            ui::yellow(&format!(
                "{} of {} shown — raise --limit",
                matches.len(),
                total
            ))
        );
    } else if total > 1 {
        println!("\n  {}", ui::dim(&ui::plural(total, "album")));
    }
    Ok(())
}

/// Default number of album pages printed: an album page is long, and a prefix
/// that matches a whole discography should not scroll for a minute.
const DEFAULT_LIMIT: usize = 5;

fn print_album(catalog: &Catalog, release: &Release) {
    let artist = release
        .album_artist_id
        .and_then(|id| catalog.artist(id))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Various Artists".into());

    println!("{}", ui::section(&release.title));
    println!("  {}", ui::bold(&artist));
    if let Some(year) = release.year {
        println!("  {year}");
    }
    let labels: Vec<String> = release
        .label_ids
        .iter()
        .filter_map(|&id| catalog.label(id))
        .map(|l| l.name.clone())
        .collect();
    if !labels.is_empty() {
        let mut line = labels.join(", ");
        if let Some(cat) = &release.catalog_number {
            line.push_str(&format!(" — {cat}"));
        }
        println!("  {}", ui::dim(&line));
    }
    let genres: Vec<String> = catalog
        .genres_of(EntityKind::Release, release.id)
        .into_iter()
        .map(|g| g.name.clone())
        .collect();
    if !genres.is_empty() {
        println!("  {}", ui::dim(&genres.join(", ")));
    }
    println!("  {}", ui::dim(&release.folder));

    // What is known about these files beyond their tags, in one line.
    //
    // Both answers lived on the track page only, which meant that verifying an
    // album — the unit anybody actually verifies — took one command per track,
    // and that a whole report imported for an artist could look like it had
    // done nothing at all. An album page that says nothing about integrity
    // reads as an album about which nothing is known.
    if let Some(line) = verification(catalog, release) {
        println!("  {}", ui::dim(&line));
    }

    // The same album elsewhere on disk: a copy to remove, or another encoding
    // kept on purpose. Either way the folder is what one needs.
    for (kind, wording) in [
        (DUPLICATE, "also present, same quality:"),
        (OTHER_EDITION, "also present, encoded differently:"),
    ] {
        for other in catalog.related_releases(release.id, kind) {
            if let Some(other) = catalog.release(other) {
                let line = format!("  {wording} {}", other.folder);
                println!(
                    "{}",
                    if kind == DUPLICATE {
                        ui::yellow(&line)
                    } else {
                        ui::dim(&line)
                    }
                );
            }
        }
    }

    println!("{}", ui::section("Tracks"));
    // A box set numbered 1, 2, 3, 1, 2, 3 with nothing saying which disc is a
    // page that cannot be read against the object on the shelf. The model has
    // carried `disc_no` since the first scan; only the page threw it away.
    //
    // The **column set stays the same** — a table that grows a column on some
    // albums and not others is a table nobody can learn, the same rule that
    // keeps `check` reporting in one shape. So the number itself carries the
    // disc, and only where there is more than one to carry.
    let discs = discs_spanned(catalog, release);
    let mut t = Table::new(&["#", "Title", "Duration", "Size", "Format", "Artists"])
        .align(0, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .limit(1, 45)
        .limit(5, 35);
    for &track_id in &release.track_ids {
        let Some(track) = catalog.track(track_id) else {
            continue;
        };
        let file = catalog.file(track.file_id);
        let performers: Vec<String> = catalog
            .credits_on(EntityKind::Track, track_id)
            .into_iter()
            .filter(|(_, role)| *role == "main")
            .map(|(a, _)| a.name.clone())
            .collect();
        t.push(vec![
            track_number(track, discs),
            track.title.clone(),
            track
                .duration_ms
                .map(text::format_duration)
                .unwrap_or_else(|| "—".into()),
            file.map(|f| text::format_size(f.size)).unwrap_or_default(),
            file.map(|f| f.properties.quality_label())
                .unwrap_or_default(),
            performers.join(", "),
        ]);
    }
    print!("{}", t.render());

    let duration: u64 = release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|t| t.duration_ms)
        .sum();
    let size: u64 = release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|t| catalog.file(t.file_id))
        .map(|f| f.size)
        .sum();
    println!(
        "  {}",
        ui::dim(&summary(discs, release.track_ids.len(), duration, size))
    );

    // Credits other than the main performance.
    let mut others: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    for &track_id in &release.track_ids {
        for (artist, role) in catalog.credits_on(EntityKind::Track, track_id) {
            if role != "main" {
                others
                    .entry(role.to_string())
                    .or_default()
                    .insert(artist.name.clone());
            }
        }
    }
    if !others.is_empty() {
        println!("{}", ui::section("Credits"));
        let mut t = Table::new(&["Role", "Artists"]).limit(1, 60);
        for (role, names) in others {
            t.push(vec![
                role_label(&role),
                names.into_iter().collect::<Vec<_>>().join(", "),
            ]);
        }
        print!("{}", t.render());
    }
}

/// How many discs the release actually spans.
///
/// A missing `disc_no` counts as the first, which is what the model does when
/// it orders tracks: a single-disc album whose tags omit the field must not
/// read as a two-disc set.
fn discs_spanned(catalog: &Catalog, release: &aede_core::model::Release) -> usize {
    release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .map(|t| t.disc_no.unwrap_or(1))
        .collect::<std::collections::BTreeSet<u32>>()
        .len()
}

/// The number as it should be read: `7` on one disc, `2-07` across several.
fn track_number(track: &aede_core::model::Track, discs: usize) -> String {
    let Some(number) = track.track_no else {
        return "—".to_string();
    };
    match discs > 1 {
        // Zero-padded on a multi-disc set so the column lines up: `2-7` beside
        // `2-11` reads as two different widths of the same thing.
        true => format!("{}-{number:02}", track.disc_no.unwrap_or(1)),
        false => number.to_string(),
    }
}

/// How many of an album's tracks were read by a method, and what came of it.
#[derive(Default, PartialEq, Debug)]
struct Reading {
    /// Files the method has an answer about.
    seen: usize,
    /// Files it found in order.
    good: usize,
    /// Files it found damaged, or whose checksum disagreed.
    bad: usize,
    /// Files whose answer no longer describes the bytes on disk.
    stale: usize,
}

/// One line saying what has been verified about this album, or nothing.
///
/// Two independent methods can speak here and both are named, because they do
/// not prove the same thing: Aède's own `check` reads the container checksums,
/// an imported report decoded the audio. When they disagree that is the most
/// interesting fact on the page, and it cannot show up at all if only one of
/// them is displayed.
fn verification(catalog: &Catalog, release: &Release) -> Option<String> {
    let files: Vec<&aede_core::model::AudioFile> = release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|t| catalog.file(t.file_id))
        .collect();
    let total = files.len();

    let mut checked = Reading::default();
    for record in files.iter().filter_map(|f| f.integrity.as_ref()) {
        use aede_core::audit::integrity::Verdict;
        match &record.verdict {
            // "Nothing to check" is not a reading: a WAV carries no checksum,
            // and counting it as verified would promise something the format
            // cannot give.
            Verdict::NothingToCheck => continue,
            Verdict::Intact => checked.good += 1,
            Verdict::Damaged { .. } => checked.bad += 1,
        }
        checked.seen += 1;
    }

    let mut by_source: std::collections::BTreeMap<&str, Reading> = Default::default();
    for file in &files {
        for record in catalog.analyses_of(file) {
            let reading = by_source.entry(record.source.as_str()).or_default();
            reading.seen += 1;
            if !record.still_applies(file.size, file.mtime) {
                reading.stale += 1;
            } else if record.md5_failed() {
                reading.bad += 1;
            } else if record.md5_state.is_some() {
                reading.good += 1;
            }
        }
    }

    let mut parts = Vec::new();
    if checked.seen > 0 {
        parts.push(format!(
            "checked: {}",
            wording(&checked, total, "intact", "damaged")
        ));
    }
    for (source, reading) in &by_source {
        parts.push(format!(
            "{source}: {}",
            wording(reading, total, "MD5 matches", "MD5 failed")
        ));
    }
    match parts.is_empty() {
        true => None,
        false => Some(parts.join(" · ")),
    }
}

/// `12 intact`, `9 of 12 intact`, `11 intact, 1 damaged`, `2 stale`.
///
/// The denominator appears only when the method did not cover the whole album,
/// because "12 of 12" on every page is a fraction nobody reads twice — and its
/// absence is what makes the one page saying "9 of 12" visible. A count of zero
/// is never printed either: "0 MD5 matches, 1 failed" says the same thing twice
/// and buries the half that matters.
fn wording(reading: &Reading, total: usize, good: &str, bad: &str) -> String {
    // A report that measured the spectrum but never opened the checksum has
    // said nothing about matching, and "0 of 12 MD5 matches" would read as
    // twelve failures.
    if reading.good == 0 && reading.bad == 0 && reading.stale == 0 {
        return format!("{} analysed", reading.seen);
    }
    let mut parts = Vec::new();
    if reading.good > 0 {
        parts.push(match reading.seen == total {
            true => format!("{} {good}", reading.good),
            false => format!("{} of {total} {good}", reading.good),
        });
    }
    if reading.bad > 0 {
        parts.push(format!("{} {bad}", reading.bad));
    }
    if reading.stale > 0 {
        parts.push(format!("{} stale", reading.stale));
    }
    parts.join(", ")
}

/// The line under the tracks: what the object is, in one breath.
///
/// The disc count leads it, and only when there is more than one — the same
/// rule the `#` column follows, for the same reason: "1 disc" under every
/// album in the library is a word that never carries information, and the eye
/// stops reading a line that is always the same. On a box set it is the first
/// thing asked ("is my rip complete?"), and counting the `2-xx` rows by hand to
/// find out is exactly the work the page exists to save.
///
/// It says how many discs are **here**, not how many the tags claim: a set
/// missing its fourth disc must read as three, or the page reassures the user
/// about a hole it is looking straight at.
fn summary(discs: usize, tracks: usize, duration_ms: u64, size: u64) -> String {
    let mut parts = Vec::new();
    if discs > 1 {
        parts.push(ui::plural(discs, "disc"));
    }
    parts.push(ui::plural(tracks, "track"));
    parts.push(text::format_duration(duration_ms));
    parts.push(text::format_size(size));
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aede_core::model::Track;

    fn track(disc: Option<u32>, number: Option<u32>) -> Track {
        Track {
            disc_no: disc,
            track_no: number,
            ..Default::default()
        }
    }

    #[test]
    fn a_single_disc_album_shows_a_plain_number() {
        // A column of "1-01, 1-02" on every album in a library is noise: the
        // disc is worth saying only where there is more than one.
        assert_eq!(track_number(&track(Some(1), Some(7)), 1), "7");
        assert_eq!(track_number(&track(None, Some(7)), 1), "7");
        assert_eq!(track_number(&track(None, None), 1), "—");
    }

    #[test]
    fn a_box_set_says_which_disc() {
        // Numbered 1, 2, 3, 1, 2, 3 with nothing saying which disc, the page
        // cannot be read against the object on the shelf.
        assert_eq!(track_number(&track(Some(2), Some(7)), 3), "2-07");
        // Zero-padded so the column lines up: "2-7" beside "2-11" reads as two
        // different widths of the same thing.
        assert_eq!(track_number(&track(Some(2), Some(11)), 3), "2-11");
        // A track whose disc the tags forgot belongs to the first, which is
        // where the model orders it.
        assert_eq!(track_number(&track(None, Some(3)), 2), "1-03");
        assert_eq!(track_number(&track(Some(2), None), 2), "—");
    }

    #[test]
    fn what_was_verified_is_said_in_a_fraction_only_when_it_needs_one() {
        let reading = |seen, good, bad, stale| Reading {
            seen,
            good,
            bad,
            stale,
        };
        // The whole album, and nothing went wrong: no denominator, because
        // "12 of 12" on every page is a fraction nobody reads twice.
        assert_eq!(
            wording(&reading(12, 12, 0, 0), 12, "intact", "damaged"),
            "12 intact"
        );
        // Part of it, which is exactly the case the denominator exists for.
        assert_eq!(
            wording(&reading(9, 9, 0, 0), 12, "intact", "damaged"),
            "9 of 12 intact"
        );
        // A failure is never folded into the good count.
        assert_eq!(
            wording(&reading(12, 11, 1, 0), 12, "MD5 matches", "MD5 failed"),
            "11 MD5 matches, 1 MD5 failed"
        );
        // An answer about bytes that have changed since is neither good nor
        // bad — it is void, and says so.
        assert_eq!(
            wording(&reading(12, 10, 0, 2), 12, "MD5 matches", "MD5 failed"),
            "10 MD5 matches, 2 stale"
        );
        // A report that never opened the checksum has said nothing about
        // matching, and "0 of 12 MD5 matches" would read as twelve failures.
        assert_eq!(
            wording(&reading(12, 0, 0, 0), 12, "MD5 matches", "MD5 failed"),
            "12 analysed"
        );
        // And a count of zero is never printed: "0 MD5 matches, 1 failed" says
        // the same thing twice and buries the half that matters.
        assert_eq!(
            wording(&reading(1, 0, 1, 0), 1, "MD5 matches", "MD5 failed"),
            "1 MD5 failed"
        );
    }

    #[test]
    fn the_summary_counts_the_discs_of_a_box_set_and_only_of_a_box_set() {
        // 4 discs is the answer to "is my rip complete?", and counting the
        // "4-xx" rows by hand to get it is work the page should have done.
        assert_eq!(
            summary(4, 85, 16_451_000, 1_500_000_000),
            "4 discs · 85 tracks · 4:34:11 · 1.5 GB"
        );
        // On a single disc the word carries nothing, and a line that always
        // reads the same stops being read at all.
        assert_eq!(
            summary(1, 9, 2_700_000, 300_000_000),
            "9 tracks · 45:00 · 300.0 MB"
        );
    }
}
