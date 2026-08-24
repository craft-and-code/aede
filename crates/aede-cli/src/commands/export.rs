//! Getting the catalog out: JSON, CSV, and playlists.
//!
//! Three formats for three different questions.
//!
//! **JSON** is the faithful dump: nine linked tables, one per table of the
//! model, which is what allows the catalog to be rebuilt or fed to another
//! program.
//!
//! **CSV** is flat by nature, so it cannot hold that graph. It answers another
//! question — "let me sort my library in a spreadsheet" — and for that a single
//! denormalized table beats nine faithful ones. Its values are therefore raw:
//! milliseconds and bytes, not `4:20` and `31.2 MB`, because a column one
//! cannot add up is a column one cannot use.
//!
//! **M3U** is not an export of the catalog but of a *selection*: whatever is on
//! screen, handed to a player.

use std::collections::BTreeSet;

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::store;

use super::{Res, load};
use crate::args::Args;
use crate::ui;

pub fn export(args: &Args) -> Res {
    // `export` is about the whole catalog and takes no argument. Ignoring one
    // would answer a question that was not asked, with a file that looks right.
    if let Some(stray) = args.positionals.first() {
        return Err(format!(
            "export covers the whole catalog and takes no argument.\n\
             To export one album or one artist, ask the command that knows it:\n\
             \taede album \"{stray}\" --csv\n\
             \taede artist \"{stray}\" --csv"
        )
        .into());
    }

    let catalog = load(args)?;
    // `--csv` flattens the graph into one table; without it the dump is the
    // faithful one, and `--json` on `export` asks for exactly that.
    if args.has("csv") {
        return if args.has("tracks") {
            let all: Vec<Id> = catalog.tracks.iter().map(|t| t.id).collect();
            tracks_table(&catalog, &all, args)
        } else {
            let all: Vec<Id> = catalog.releases.iter().map(|r| r.id).collect();
            albums_table(&catalog, &all, args)
        };
    }
    emit(args, &store::to_json(&catalog).to_string_pretty())
}

/// Writes the tracks on screen as a table, in whichever format was asked for.
///
/// The counterpart of [`m3u`] for a spreadsheet or a program: same selection,
/// other shape.
pub fn tracks_table(catalog: &Catalog, tracks: &[Id], args: &Args) -> Res {
    let (header, rows) = track_rows(catalog, tracks);
    rows_table(&header, &rows, args)
}

/// Writes to the file given by `--output`, or to standard output.
pub fn emit(args: &Args, text: &str) -> Res {
    match args.value("output") {
        Some(path) => {
            std::fs::write(path, text)?;
            println!(
                "{} {}",
                ui::green("→"),
                format_args!(
                    "written to {path} ({})",
                    aede_core::text::format_size(text.len() as u64)
                )
            );
        }
        None => print!("{text}"),
    }
    Ok(())
}

/// Field separator, `,` unless asked otherwise.
///
/// Excel in a French or German locale splits on `;` and would show a one-column
/// sheet otherwise. The alternative — a `sep=;` line at the top of the file —
/// is understood by Excel alone and corrupts the file for every other reader.
fn separator(args: &Args) -> Result<char, Box<dyn std::error::Error>> {
    match args.value("separator") {
        None => Ok(','),
        Some("tab") => Ok('\t'),
        Some(value) => {
            let mut chars = value.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(c),
                _ => Err(
                    format!("--separator takes one character, or \"tab\": got \"{value}\"").into(),
                ),
            }
        }
    }
}

/// Writes the albums on screen as a table, in whichever format was asked for.
pub fn albums_table(catalog: &Catalog, releases: &[Id], args: &Args) -> Res {
    let (header, rows) = album_rows(catalog, releases);
    rows_table(&header, &rows, args)
}

/// Writes any listing as a table: a header, then the rows as given.
///
/// The listings differ too much to share a shape — a year is not an artist —
/// but they share the quoting, the separator, the JSON typing and where the
/// text ends up. **One function for both formats**, so that an option honoured
/// by one and not the other cannot happen again: `--json` was accepted by every
/// listing and read by none, which printed the ordinary table and dropped the
/// word.
pub fn rows_table(header: &[&str], rows: &[Vec<String>], args: &Args) -> Res {
    if args.has("json") {
        return emit(args, &rows_json(header, rows));
    }
    let separator = separator(args)?;
    let mut out = String::new();
    push_row(
        &mut out,
        &header.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
        separator,
    );
    for row in rows {
        push_row(&mut out, row, separator);
    }
    emit(args, &out)
}

/// Columns whose value is a number, and must reach JSON as one.
///
/// Named rather than guessed: a cell that merely *looks* like a number is not
/// one. An album called "1999" is a title, and turning it into an integer
/// because it happens to parse is a wrong answer no reader could detect.
const NUMERIC_COLUMNS: &[&str] = &[
    "tracks",
    "albums",
    "discs",
    "year",
    "duration_ms",
    "size_bytes",
    "track_no",
    "disc_no",
    "bitrate_kbps",
    "sample_rate_hz",
    "bit_depth",
    "channels",
];

/// Columns holding a yes/no, which JSON has a type for.
const BOOLEAN_COLUMNS: &[&str] = &["lossless", "compilation"];

/// The same rows as the CSV, as an array of objects.
///
/// An empty cell becomes `null` rather than `""`: "this album has no year" and
/// "this album has an empty year" are different claims, and only the first is
/// true. The CSV cannot make that distinction; JSON can, so it does.
pub fn rows_json(header: &[&str], rows: &[Vec<String>]) -> String {
    let list: Vec<Json> = rows
        .iter()
        .map(|row| {
            let mut o = Json::obj();
            for (name, cell) in header.iter().zip(row) {
                o.set(name, cell_json(name, cell));
            }
            o
        })
        .collect();
    Json::Arr(list).to_string_pretty()
}

fn cell_json(name: &str, cell: &str) -> Json {
    if cell.is_empty() {
        return Json::Null;
    }
    if BOOLEAN_COLUMNS.contains(&name) {
        match cell {
            "true" => return Json::Bool(true),
            "false" => return Json::Bool(false),
            _ => {}
        }
    }
    if NUMERIC_COLUMNS.contains(&name)
        && let Ok(number) = cell.parse::<f64>()
    {
        return Json::Num(number);
    }
    Json::Str(cell.to_string())
}

/// One row per album: what a library looks like from above.
///
/// Built once, so that the CSV and the JSON can never drift apart.
fn album_rows(catalog: &Catalog, releases: &[Id]) -> (Vec<&'static str>, Vec<Vec<String>>) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let header = vec![
        "album_artist",
        "album",
        "year",
        "date",
        "tracks",
        "discs",
        "duration_ms",
        "size_bytes",
        "formats",
        "sample_rates_hz",
        "bit_depths",
        "lossless",
        "compilation",
        "label",
        "catalog_number",
        "barcode",
        "media",
        "genres",
        "integrity",
        "musicbrainz_albumid",
        "folder",
    ];
    for release in releases.iter().filter_map(|&id| catalog.release(id)) {
        let tracks: Vec<&aede_core::model::Track> = release
            .track_ids
            .iter()
            .filter_map(|&id| catalog.track(id))
            .collect();
        let files: Vec<&aede_core::model::AudioFile> = tracks
            .iter()
            .filter_map(|t| catalog.file(t.file_id))
            .collect();

        let discs: BTreeSet<u32> = tracks.iter().map(|t| t.disc_no.unwrap_or(1)).collect();
        let duration: u64 = tracks.iter().filter_map(|t| t.duration_ms).sum();
        let size: u64 = files.iter().map(|f| f.size).sum();
        let formats: BTreeSet<&str> = files.iter().map(|f| f.properties.codec.as_str()).collect();
        let rates: BTreeSet<u32> = files
            .iter()
            .filter_map(|f| f.properties.sample_rate)
            .collect();
        let depths: BTreeSet<u16> = files
            .iter()
            .filter_map(|f| f.properties.bit_depth)
            .collect();
        let genres: Vec<String> = catalog
            .genres_of(EntityKind::Release, release.id)
            .into_iter()
            .map(|g| g.name.clone())
            .collect();
        let labels: Vec<String> = release
            .label_ids
            .iter()
            .filter_map(|&id| catalog.label(id))
            .map(|l| l.name.clone())
            .collect();

        let row = [
            release
                .album_artist_id
                .and_then(|id| catalog.artist(id))
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Various Artists".into()),
            release.title.clone(),
            release.year.map(|y| y.to_string()).unwrap_or_default(),
            release.date.clone().unwrap_or_default(),
            tracks.len().to_string(),
            discs.len().to_string(),
            duration.to_string(),
            size.to_string(),
            join(formats.iter().map(|c| c.to_string())),
            join(rates.iter().map(|r| r.to_string())),
            join(depths.iter().map(|d| d.to_string())),
            bool_cell(files.iter().all(|f| f.properties.lossless)),
            bool_cell(release.is_compilation),
            labels.join(" / "),
            release.catalog_number.clone().unwrap_or_default(),
            release.barcode.clone().unwrap_or_default(),
            release.media.clone().unwrap_or_default(),
            genres.join(" / "),
            integrity_of(&files).into(),
            release.mbid.clone().unwrap_or_default(),
            release.folder.clone(),
        ];
        rows.push(row.to_vec());
    }
    (header, rows)
}

/// One row per track, for when the album view is too coarse.
/// The track table, once, for the same reason.
fn track_rows(catalog: &Catalog, tracks: &[Id]) -> (Vec<&'static str>, Vec<Vec<String>>) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let header = vec![
        "artist",
        "album_artist",
        "album",
        "year",
        "disc_no",
        "track_no",
        "title",
        "duration_ms",
        "size_bytes",
        "codec",
        "container",
        "sample_rate_hz",
        "bit_depth",
        "channels",
        "bitrate_kbps",
        "lossless",
        "integrity",
        "isrc",
        "musicbrainz_recordingid",
        "path",
    ];
    for track in tracks.iter().filter_map(|&id| catalog.track(id)) {
        let release = track.release_id.and_then(|id| catalog.release(id));
        let file = catalog.file(track.file_id);
        let performers: Vec<String> = catalog
            .credits_on(EntityKind::Track, track.id)
            .into_iter()
            .filter(|(_, role)| *role == "main")
            .map(|(a, _)| a.name.clone())
            .collect();

        let row = [
            performers.join(" / "),
            release
                .and_then(|r| r.album_artist_id)
                .and_then(|id| catalog.artist(id))
                .map(|a| a.name.clone())
                .unwrap_or_default(),
            release.map(|r| r.title.clone()).unwrap_or_default(),
            release
                .and_then(|r| r.year)
                .map(|y| y.to_string())
                .unwrap_or_default(),
            track.disc_no.map(|d| d.to_string()).unwrap_or_default(),
            track.track_no.map(|n| n.to_string()).unwrap_or_default(),
            track.title.clone(),
            track.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
            file.map(|f| f.size.to_string()).unwrap_or_default(),
            file.map(|f| f.properties.codec.clone()).unwrap_or_default(),
            file.map(|f| f.properties.container.clone())
                .unwrap_or_default(),
            number(file.and_then(|f| f.properties.sample_rate)),
            number(file.and_then(|f| f.properties.bit_depth)),
            number(file.and_then(|f| f.properties.channels)),
            number(file.and_then(|f| f.properties.bitrate_kbps)),
            bool_cell(file.map(|f| f.properties.lossless).unwrap_or(false)),
            integrity_of(&file.into_iter().collect::<Vec<_>>()).into(),
            track.isrc.clone().unwrap_or_default(),
            track.mbid.clone().unwrap_or_default(),
            file.map(|f| f.path.clone()).unwrap_or_default(),
        ];
        rows.push(row.to_vec());
    }
    (header, rows)
}

/// Integrity of a set of files, in one word.
///
/// The worst verdict wins: an album holding one damaged track is a damaged
/// album, and one unverified track is enough for the album not to be vouched
/// for. Anything else would let a problem hide behind an average.
fn integrity_of(files: &[&aede_core::model::AudioFile]) -> &'static str {
    use aede_core::audit::integrity::Verdict;
    if files.is_empty() {
        return "";
    }
    let mut all_nothing = true;
    let mut any_unverified = false;
    for file in files {
        match file.integrity.as_ref().map(|r| &r.verdict) {
            Some(Verdict::Damaged { .. }) => return "damaged",
            Some(Verdict::Intact) => all_nothing = false,
            Some(Verdict::NothingToCheck) => {}
            None => {
                any_unverified = true;
                all_nothing = false;
            }
        }
    }
    match (any_unverified, all_nothing) {
        (true, _) => "not_verified",
        (false, true) => "nothing_to_check",
        (false, false) => "intact",
    }
}

fn number<T: ToString>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// `true`/`false` rather than 1/0: a spreadsheet filters on words, and 1/0
/// invites arithmetic on something that is not a quantity.
fn bool_cell(value: bool) -> String {
    if value { "true" } else { "false" }.to_string()
}

fn join(values: impl Iterator<Item = String>) -> String {
    values.collect::<Vec<_>>().join(" / ")
}

fn push_row(out: &mut String, cells: &[String], separator: char) {
    let line: Vec<String> = cells.iter().map(|c| escape(c, separator)).collect();
    out.push_str(&line.join(&separator.to_string()));
    // CRLF, as RFC 4180 asks: it is what Excel expects, and every other reader
    // copes with it.
    out.push_str("\r\n");
}

/// Quotes a field when it has to be, doubling the quotes inside it.
///
/// Album titles carry commas and quotation marks, and a few carry line breaks
/// left by a tagger. Getting this wrong shifts every following column.
fn escape(value: &str, separator: char) -> String {
    let needs_quotes = value.contains(separator)
        || value.contains('"')
        || value.contains('\n')
        || value.contains('\r');
    if !needs_quotes {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Renders a track list as an extended M3U playlist.
///
/// Paths are absolute, which is what makes the file work wherever it is saved.
/// `#EXTM3U` and the `#EXTINF` lines are what a player reads to show a title
/// without opening every file.
pub fn m3u(catalog: &Catalog, tracks: &[Id]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for &id in tracks {
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let Some(file) = catalog.file(track.file_id) else {
            continue;
        };
        let artist = catalog
            .credits_on(EntityKind::Track, id)
            .into_iter()
            .find(|(_, role)| *role == "main")
            .map(|(a, _)| a.name.clone())
            .unwrap_or_default();
        // Seconds, rounded, and -1 when unknown: that is what the format says.
        let seconds = match track.duration_ms {
            Some(ms) => ((ms + 500) / 1000) as i64,
            None => -1,
        };
        if artist.is_empty() {
            out.push_str(&format!("#EXTINF:{seconds},{}\n", track.title));
        } else {
            out.push_str(&format!("#EXTINF:{seconds},{artist} - {}\n", track.title));
        }
        out.push_str(&file.path);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_is_quoted_only_when_it_has_to_be() {
        assert_eq!(escape("So What", ','), "So What");
        assert_eq!(escape("Freedom, Pt. 2", ','), "\"Freedom, Pt. 2\"");
        // The separator decides: the same title needs nothing under `;`.
        assert_eq!(escape("Freedom, Pt. 2", ';'), "Freedom, Pt. 2");
        // Quotes are doubled, not escaped with a backslash.
        assert_eq!(escape("Say \"Hello\"", ','), "\"Say \"\"Hello\"\"\"");
        assert_eq!(escape("Two\nlines", ','), "\"Two\nlines\"");
    }
}
