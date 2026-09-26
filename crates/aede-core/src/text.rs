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
///
/// **That last rule is why this is the fallback and not the answer.** A tag
/// reading `Rob Zombie & Ozzy Osbourne` is two artists and one that reads
/// `Simon & Garfunkel` is one, and no amount of looking at the string can tell
/// them apart. The tag that can is `ARTISTS`, which taggers write with **one
/// value per artist** for exactly this reason; see
/// [`crate::model::builder`], which prefers it wherever a file carries it and
/// falls back here only when none does.
pub fn split_artists(raw: &str) -> Vec<String> {
    const HARD_SEPARATORS: [&str; 4] = [";", " / ", "//", " ; "];
    // `" w/"` carries no trailing space on purpose: it is written `w/Therapy?`
    // as often as `w/ Therapy?`, and the space before it is what keeps it from
    // matching inside a name. Nothing loses by it — a band whose name contains
    // a space followed by `w/` does not exist as far as anyone has met one.
    const FEATURE_MARKERS: [&str; 9] = [
        " feat. ",
        " feat ",
        " featuring ",
        " ft. ",
        " ft ",
        " avec ",
        " with ",
        " vs. ",
        " w/",
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

/// Drops, from one tag's list of names, the ones that only say again what the
/// other entries of that same list already say.
///
/// Real tags do this constantly. A file of *War Pigs* carries
/// `PERFORMER=Ozzy Osbourne; Judas Priest; Judas Priest & Ozzy Osbourne`:
/// three values, two musicians. The third is the credit written out whole, and
/// nothing in the string alone tells it from a band name — `&` is never a
/// separator here, and rightly, or `Kool & the Gang` would be shattered.
///
/// **The list itself is what tells them apart, and it is not a heuristic.** A
/// value made of two or more of the *other* values in the same tag, with
/// nothing left over but the words that join names together, is the file
/// saying one thing twice. Keeping the granular form loses nobody: every
/// person it named is still named, and each is now an artist in their own
/// right rather than a third party who happens to exist on one track.
/// `Kool & the Gang` survives any list that does not also hold `Kool` and
/// `the Gang` on their own — and a list that did would have named them itself.
///
/// The comparison is on [`normalize`]d keys, so joining punctuation has
/// already fallen away and only joining *words* remain to be recognised.
pub fn without_restatements(names: Vec<String>) -> Vec<String> {
    // Words that join names and name nobody. `&` is not among them because
    // `normalize` has already turned it into a space.
    const JOINERS: [&str; 13] = [
        "and",
        "et",
        "with",
        "avec",
        "feat",
        "featuring",
        "ft",
        "vs",
        "versus",
        "w",
        "x",
        "meets",
        "presents",
    ];

    let keys: Vec<String> = names.iter().map(|n| normalize(n)).collect();
    let mut kept = Vec::with_capacity(names.len());
    for (me, name) in names.iter().enumerate() {
        // Longest first, so `Ozzy Osbourne` is consumed whole rather than
        // leaving `osbourne` stranded because a shorter `Ozzy` matched first.
        let mut others: Vec<usize> = (0..keys.len())
            .filter(|&j| j != me && !keys[j].is_empty() && keys[j] != keys[me])
            .collect();
        others.sort_by_key(|&j| std::cmp::Reverse(keys[j].len()));

        // A space at each end so a needle only ever matches whole words:
        // without them `Al` would be eaten out of `Alice`.
        let mut left = format!(" {} ", keys[me]);
        let mut eaten = 0;
        for j in others {
            let needle = format!(" {} ", keys[j]);
            if let Some(at) = left.find(&needle) {
                left.replace_range(at..at + needle.len(), " ");
                eaten += 1;
            }
        }
        // Two, not one. One would mean that any name containing another name
        // of the list is a restatement of it, and `Therapy?` sitting beside
        // `Ozzy Osbourne w/Therapy?` would delete the collaboration instead of
        // the other way round.
        let restated = eaten >= 2 && left.split_whitespace().all(|w| JOINERS.contains(&w));
        if !restated {
            kept.push(name.clone());
        }
    }
    kept
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

/// A count and its noun, agreeing: `1 track`, `2 tracks`, `3 analyses`.
///
/// **The count is included**, because a caller that had to write it itself
/// would sooner or later write it twice or not at all. English plurals are not
/// a solved problem and this does not pretend otherwise: the `-es` endings and
/// `analysis`/`analyses` are here because this program says those words, and
/// anything else gets an `s`.
pub fn plural(count: usize, singular: &str) -> String {
    if count <= 1 {
        return format!("{count} {singular}");
    }
    let lower = singular.to_ascii_lowercase();
    if lower.ends_with("is") {
        return format!("{count} {}es", &singular[..singular.len() - 2]);
    }
    if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|end| lower.ends_with(end))
    {
        format!("{count} {singular}es")
    } else {
        format!("{count} {singular}s")
    }
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
/// Recognizes native Windows paths even when reading their catalog on Unix.
/// The original spelling, including verbatim prefixes, is never rewritten.
pub fn file_name(path: &str) -> &str {
    match path.rfind(path_separator(path)) {
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
    match path.rfind(path_separator(path)) {
        // C: is drive-relative; retaining its root separator is essential.
        Some(i)
            if (i == 2 || (i == 6 && path.starts_with(r"\\?\")))
                && path.as_bytes()[i - 1] == b':'
                && path.as_bytes()[i - 2].is_ascii_alphabetic() =>
        {
            &path[..=i]
        }
        Some(i) => &path[..i],
        None => "",
    }
}

fn path_separator(path: &str) -> impl Fn(char) -> bool + Copy {
    let verbatim = path.starts_with(r"\\?\");
    let drive = path.as_bytes().get(1) == Some(&b':')
        && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    let windows = cfg!(windows) || drive || path.starts_with(r"\\");
    move |c| {
        if verbatim {
            c == '\\'
        } else {
            c == '/' || (windows && c == '\\')
        }
    }
}

fn strip_folder<'a>(path: &'a str, folder: &str) -> Option<&'a str> {
    if folder.is_empty() {
        return path.is_empty().then_some(path);
    }
    let separator = path_separator(path);
    let folder_separator = path_separator(folder);
    let prefix = folder.trim_end_matches(folder_separator);
    if prefix.is_empty() && !path.starts_with(separator) {
        return None;
    }
    let head = path.get(..prefix.len())?;
    let matches = head
        .chars()
        .zip(prefix.chars())
        .all(|(left, right)| left == right || (separator(left) && folder_separator(right)));
    let rest = path.get(prefix.len()..)?;
    if !matches || (!rest.is_empty() && !rest.starts_with(separator)) {
        return None;
    }
    Some(rest.trim_start_matches(separator))
}

/// A portable relative path under a catalog folder, or `None` outside it.
///
/// Only the relative output uses `/`: native absolute paths must retain their
/// Windows verbatim spelling when passed to the filesystem or external tools.
pub fn relative_under(path: &str, folder: &str) -> Option<String> {
    let rest = strip_folder(path, folder)?;
    Some(
        rest.split(path_separator(path))
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// The disc a folder name announces, when it announces one.
///
/// A box set is very often laid out as `Album/Disc 1`, `Album/CD2`,
/// `Album/Disque 3`. Those folders are **not** separate albums, and treating
/// them as such splits one release into three — which is what happened: a
/// Final Fantasy VII soundtrack came back as two albums of the same name, each
/// numbering its tracks from one, with nothing on screen saying which disc was
/// which except the path.
///
/// Recognised: `disc`, `disk`, `cd`, `disque`, followed by digits, with or
/// without a space or a dash. Anything else is a folder like any other — a
/// name that merely *contains* the word is left alone, because `CD Singles` is
/// a collection and not a disc of one.
pub fn disc_folder(name: &str) -> Option<u32> {
    let lowered = name.trim().to_lowercase();
    let rest = ["disque", "disc", "disk", "cd"]
        .iter()
        .find_map(|word| lowered.strip_prefix(word))?;
    let digits = rest.trim_start_matches([' ', '-', '_', '.', '#']);
    // The whole remainder must be the number: `cd2` yes, `cd2 bonus` no, and
    // `disc one` no — a word this cannot read is a folder it must not claim.
    match digits.parse::<u32>() {
        Ok(number) if number > 0 => Some(number),
        _ => None,
    }
}

/// `true` when a path is the folder itself or something inside it.
///
/// Written out because the obvious `path.starts_with(folder)` is wrong on
/// strings: `/music/Rock` would then claim every file of `/music/Rockabilly`.
/// The test is on a separator boundary, which is the difference between a
/// folder and a prefix of its name.
pub fn is_under(path: &str, folder: &str) -> bool {
    strip_folder(path, folder).is_some()
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
