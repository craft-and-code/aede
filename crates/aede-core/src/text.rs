//! Text normalization: this is the piece that decides whether "The Beatles",
//! "Beatles, The" and "beatles" designate the same entity.
//!
//! All entity matching rests on [`normalize`], until M1 brings identifiers
//! that do not depend on spelling.
//! When MusicBrainz arrives (M1), these normalized keys will remain the safety
//! net for unidentified files.

/// Splits multiple artists out of a single tag field.
///
/// Real libraries mix every convention: `;`, `/`, ` feat. `, ` & `… We cut on
/// the safe separators and leave `&` and `and` alone, because "Simon &
/// Garfunkel" or "Earth, Wind & Fire" are band names, not lists.
pub fn split_artists(raw: &str) -> Vec<String> {
    const HARD_SEPARATORS: [&str; 4] = [";", " / ", "//", " ; "];
    const FEATURE_MARKERS: [&str; 8] = [
        " feat. ",
        " feat ",
        " featuring ",
        " ft. ",
        " ft ",
        " avec ",
        " with ",
        " vs. ",
    ];

    let mut parts: Vec<String> = vec![raw.trim().to_string()];

    for separator in HARD_SEPARATORS {
        parts = parts
            .iter()
            .flat_map(|p| {
                p.split(separator)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
    }

    // "feat." mentions are looked up without regard to case.
    let mut expanded = Vec::new();
    for part in parts {
        let lower = part.to_lowercase();
        let mut cut = None;
        for marker in FEATURE_MARKERS {
            if let Some(idx) = lower.find(marker) {
                cut = Some(match cut {
                    Some((prev, _)) if prev <= idx => (prev, marker.len()),
                    _ => (idx, marker.len()),
                });
            }
        }
        match cut {
            Some((idx, len)) => {
                expanded.push(part[..idx].to_string());
                expanded.push(part[idx + len..].to_string());
            }
            None => expanded.push(part),
        }
    }

    let mut out = Vec::new();
    for part in expanded {
        let cleaned = part
            .trim()
            .trim_matches(|c| c == ',' || c == '-')
            .trim()
            .to_string();
        if !cleaned.is_empty() && !out.contains(&cleaned) {
            out.push(cleaned);
        }
    }
    out
}

/// Matching key: lowercase, without diacritics, without punctuation,
/// with normalized spacing and the leading article moved.
///
/// ```
/// use aede_core::text::normalize;
/// assert_eq!(normalize("The Beatles"), normalize("Beatles, The"));
/// assert_eq!(normalize("Björk"), "bjork");
/// ```
pub fn normalize(input: &str) -> String {
    let folded: String = input
        .trim()
        .to_lowercase()
        .chars()
        .flat_map(fold_char)
        .collect();

    let mut cleaned = String::with_capacity(folded.len());
    let mut last_was_space = true;
    for ch in folded.chars() {
        if ch.is_alphanumeric() {
            cleaned.push(ch);
            last_was_space = false;
        } else if !last_was_space {
            cleaned.push(' ');
            last_was_space = true;
        }
    }
    let cleaned = cleaned.trim().to_string();
    strip_leading_article(&cleaned)
}

/// Sort name: "The Beatles" -> "Beatles, The", as discographic usage requires.
pub fn sort_name(input: &str) -> String {
    const ARTICLES: [&str; 6] = ["the ", "le ", "la ", "les ", "der ", "die "];
    let lower = input.to_lowercase();
    for article in ARTICLES {
        if lower.starts_with(article) {
            let rest = input[article.len()..].trim();
            let art = input[..article.len()].trim();
            return format!("{rest}, {art}");
        }
    }
    input.to_string()
}

fn strip_leading_article(s: &str) -> String {
    const ARTICLES: [&str; 6] = ["the ", "le ", "la ", "les ", "der ", "die "];
    // The "beatles the" form produced by normalizing "Beatles, The".
    for article in ARTICLES {
        let suffix = format!(" {}", article.trim());
        if let Some(base) = s.strip_suffix(&suffix)
            && !base.is_empty()
        {
            return base.to_string();
        }
    }
    for article in ARTICLES {
        if let Some(rest) = s.strip_prefix(article)
            && !rest.is_empty()
        {
            return rest.to_string();
        }
    }
    s.to_string()
}

/// ASCII folding of the accented characters most common in Western artist
/// names. Deliberately limited: no transliteration of Cyrillic or Greek,
/// which must stay distinct.
fn fold_char(c: char) -> std::vec::IntoIter<char> {
    let replacement: &str = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => "i",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => "u",
        'ý' | 'ÿ' => "y",
        'ž' | 'ź' | 'ż' => "z",
        'š' | 'ś' => "s",
        'ř' => "r",
        'ł' => "l",
        'đ' | 'ð' => "d",
        'ť' => "t",
        'æ' => "ae",
        'œ' => "oe",
        'ß' => "ss",
        'þ' => "th",
        other => return vec![other].into_iter(),
    };
    replacement.chars().collect::<Vec<_>>().into_iter()
}

/// Formats a duration as `h:mm:ss` or `m:ss`.
///
/// The count of seconds is **rounded**, not truncated: a track of 4 min 20.7 s
/// reads 4:21, as it does in every player. Cutting the fraction off would show
/// a second less on roughly half the tracks of a library, and the sum of the
/// displayed times would drift away from the announced total.
pub fn format_duration(ms: u64) -> String {
    let total = (ms + 500) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Formats a size in bytes in a readable way.
///
/// **Decimal** units, 1 kB being 1000 bytes: that is what macOS Finder and most
/// Linux file managers show, so the figure matches what the system says about
/// the same files. Dividing by 1024 while writing "MB" — the usual shortcut —
/// understates an album by about 5%, which is exactly the kind of discrepancy a
/// user reports as a bug.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
    const STEP: f64 = 1000.0;
    if bytes < STEP as u64 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / STEP;
    let mut unit = 0;
    while value >= STEP && unit < UNITS.len() - 1 {
        value /= STEP;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Extracts the first year from a tag date, whatever its shape
/// (`1986`, `1986-03-05`, `05/03/1986`, `1986.03`).
pub fn extract_year(raw: &str) -> Option<u32> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let window = &raw[i..i + 4];
        if window.bytes().all(|b| b.is_ascii_digit())
            && let Ok(year) = window.parse::<u32>()
            && (1500..=2200).contains(&year)
        {
            return Some(year);
        }
        i += 1;
    }
    None
}

/// Reads a track number in the `5` or `5/12` form, returning
/// `(number, total)`.
pub fn parse_track_number(raw: &str) -> (Option<u32>, Option<u32>) {
    let mut parts = raw.split('/');
    let number = parts.next().and_then(|s| s.trim().parse::<u32>().ok());
    let total = parts.next().and_then(|s| s.trim().parse::<u32>().ok());
    (number, total)
}

/// Last component of a path, extension included.
///
/// Written by hand rather than through `std::path`: catalog paths are stored as
/// `String`, and a round trip through `Path` would either allocate or force
/// every caller to deal with a name that is not valid UTF-8. The separator is
/// `/`, which is what the scanner produces on the systems this runs on.
pub fn file_name(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// Containing directory of a path, without its trailing separator.
///
/// Empty when the path has no directory part at all — a bare file name is in no
/// folder, and inventing `"."` here would group it with every other path that
/// has no folder either.
pub fn folder(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// `true` when a path is the folder itself or something inside it.
///
/// Written out because the obvious `path.starts_with(folder)` is wrong on
/// strings: `/music/Rock` would then claim every file of `/music/Rockabilly`.
/// The test is on a separator boundary, which is the difference between a
/// folder and a prefix of its name.
pub fn is_under(path: &str, folder: &str) -> bool {
    if path == folder {
        return true;
    }
    let Some(rest) = path.strip_prefix(folder) else {
        return false;
    };
    // A folder written with its trailing slash is still that folder.
    rest.starts_with('/') || folder.ends_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_not_a_prefix_of_its_name() {
        assert!(is_under("/music/Rock/01.flac", "/music/Rock"));
        assert!(is_under("/music/Rock", "/music/Rock"));
        assert!(is_under("/music/Rock/01.flac", "/music/Rock/"));
        // The trap: one name beginning with the other.
        assert!(!is_under("/music/Rockabilly/01.flac", "/music/Rock"));
        assert!(!is_under("/music", "/music/Rock"));
        assert!(!is_under("/other/Rock/01.flac", "/music/Rock"));
    }

    #[test]
    fn path_parts() {
        assert_eq!(file_name("/music/a/01.flac"), "01.flac");
        assert_eq!(folder("/music/a/01.flac"), "/music/a");
        // A bare name is its own file name, and is in no folder.
        assert_eq!(file_name("01.flac"), "01.flac");
        assert_eq!(folder("01.flac"), "");
        // A file sitting at the root has no folder either: the root is not a
        // grouping.
        assert_eq!(folder("/01.flac"), "");
    }

    #[test]
    fn article_normalization() {
        assert_eq!(normalize("The Beatles"), normalize("Beatles, The"));
        assert_eq!(normalize("The Beatles"), "beatles");
        assert_eq!(normalize("  the   ROLLING   Stones "), "rolling stones");
        // A lone article must not disappear.
        assert_eq!(normalize("The The"), "the");
    }

    #[test]
    fn accent_and_punctuation_normalization() {
        assert_eq!(normalize("Björk"), "bjork");
        assert_eq!(normalize("Sigur Rós"), "sigur ros");
        assert_eq!(normalize("AC/DC"), "ac dc");
        assert_eq!(normalize("Motörhead!"), "motorhead");
        assert_eq!(normalize("Émilie Simon"), "emilie simon");
    }

    #[test]
    fn artist_splitting() {
        assert_eq!(split_artists("Miles Davis"), vec!["Miles Davis"]);
        assert_eq!(
            split_artists("Miles Davis; John Coltrane"),
            vec!["Miles Davis", "John Coltrane"]
        );
        assert_eq!(
            split_artists("Daft Punk feat. Pharrell Williams"),
            vec!["Daft Punk", "Pharrell Williams"]
        );
        // Ampersands inside band names must NOT be cut.
        assert_eq!(
            split_artists("Simon & Garfunkel"),
            vec!["Simon & Garfunkel"]
        );
        assert_eq!(
            split_artists("Earth, Wind & Fire"),
            vec!["Earth, Wind & Fire"]
        );
    }

    #[test]
    fn sort_names() {
        assert_eq!(sort_name("The Beatles"), "Beatles, The");
        assert_eq!(sort_name("Miles Davis"), "Miles Davis");
        assert_eq!(sort_name("Les Rita Mitsouko"), "Rita Mitsouko, Les");
    }

    #[test]
    fn year_extraction() {
        assert_eq!(extract_year("1959"), Some(1959));
        assert_eq!(extract_year("1959-08-17"), Some(1959));
        assert_eq!(extract_year("17/08/1959"), Some(1959));
        assert_eq!(extract_year("unknown"), None);
        assert_eq!(extract_year("12"), None);
    }

    #[test]
    fn track_numbers() {
        assert_eq!(parse_track_number("5"), (Some(5), None));
        assert_eq!(parse_track_number("5/12"), (Some(5), Some(12)));
        assert_eq!(parse_track_number("noise"), (None, None));
    }

    #[test]
    fn formatting() {
        assert_eq!(format_duration(65_000), "1:05");
        assert_eq!(format_duration(3_725_000), "1:02:05");
        // Rounded, not truncated: 4 min 20.7 s is 4:21, as in any player.
        assert_eq!(format_duration(260_700), "4:21");
        assert_eq!(format_duration(260_400), "4:20");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1500), "1.5 kB");
        // Decimal units, like the Finder: 315.7 MB, not 301.1 "MB".
        assert_eq!(format_size(315_727_769), "315.7 MB");
    }
}
