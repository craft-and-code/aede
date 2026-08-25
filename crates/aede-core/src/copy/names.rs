//! Making a path acceptable to the filesystem it is about to be written to.
//!
//! A music library is full of names a general-purpose filesystem takes without
//! blinking and a portable player refuses: `Where Is My Mind?`, `Symphony No. 5:
//! Allegro`, `AC/DC`. Copying to a FAT32 or exFAT card — which is what a player
//! or a cheap USB stick almost always is — fails on those, one file at a time,
//! in the middle of a run that has already taken twenty minutes.
//!
//! **Nothing here is silent.** Every component this module changes is reported
//! back so the run can say what it renamed and why. A copy whose file names
//! quietly differ from the library's is a copy nobody can compare against the
//! original afterwards.

use std::collections::BTreeSet;

/// Characters no FAT/exFAT/NTFS volume accepts in a name.
///
/// `/` is absent on purpose: it never reaches this function, being the
/// separator that split the path into the components given to it.
const FORBIDDEN: &[char] = &['?', '*', ':', '"', '<', '>', '|', '\\'];

/// What replaces a character that cannot be written.
///
/// One visible character rather than removal: `Where Is My Mind_` still reads
/// as the title, where `Where Is My Mind` claims to be a name that was never
/// there — and two titles differing only by punctuation would silently become
/// one.
const REPLACEMENT: char = '_';

/// Names DOS reserved for devices, which FAT still refuses whatever the
/// extension: `NUL.flac` cannot be created.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Longest component the destination is assumed to accept.
///
/// 255 is what FAT32 with long file names, exFAT, ext4, APFS and NTFS all
/// allow. The unit is **bytes**, not characters: an accented title is longer
/// on disk than it looks on screen, and a limit counted in characters would
/// pass here and fail at the write.
const MAX_COMPONENT: usize = 255;

/// Splits a name into its stem and its extension, where there is one.
///
/// **Not every dot ends a name.** `Vol. 1: Live` has two, and neither of them
/// introduces an extension — taking the last one would make the stem `Vol` and
/// the "extension" ` 1_ Live`, which is how a disambiguating counter ends up
/// inside the title as `Vol (2). 1_ Live`. A test caught exactly that. So an
/// extension is recognised rather than assumed: a short run of letters and
/// digits, no spaces, after the final dot.
///
/// The extension comes back with its dot, so `stem + extension` is always the
/// name that went in.
fn split_extension(name: &str) -> (&str, &str) {
    let Some(dot) = name.rfind('.') else {
        return (name, "");
    };
    // A leading dot makes a hidden file, not an extension on an empty stem.
    if dot == 0 {
        return (name, "");
    }
    let candidate = &name[dot + 1..];
    let plausible = !candidate.is_empty()
        && candidate.len() <= 12
        && candidate.chars().all(|c| c.is_ascii_alphanumeric());
    match plausible {
        true => (&name[..dot], &name[dot..]),
        false => (name, ""),
    }
}

/// A component made acceptable, or left exactly as it was.
///
/// Returns `None` when nothing had to change, which is what lets the caller
/// report only the names it actually touched.
pub fn adapt(component: &str) -> Option<String> {
    let adapted = rewrite(component);
    (adapted != component).then_some(adapted)
}

fn rewrite(component: &str) -> String {
    let mut out: String = component
        .chars()
        // Control characters are as unwritable as the punctuation, and far
        // more likely to come from a damaged tag than from a title.
        .map(|c| match FORBIDDEN.contains(&c) || (c as u32) < 0x20 {
            true => REPLACEMENT,
            false => c,
        })
        .collect();

    // A name ending in a dot or a space is accepted by the call and then read
    // back without it, which turns "Vol. 2 " into a file the copy can never
    // find again. Trimmed rather than replaced: the trailing space carries no
    // meaning anybody wants to keep.
    let trimmed = out.trim_end_matches([' ', '.']);
    if trimmed.len() != out.len() {
        out = match trimmed.is_empty() {
            true => REPLACEMENT.to_string(),
            false => trimmed.to_string(),
        };
    }

    // The reserved word is the *stem*, so `NUL.flac` is refused too.
    let (stem, extension) = split_extension(&out);
    if RESERVED.iter().any(|word| stem.eq_ignore_ascii_case(word)) {
        out = format!("{stem}{REPLACEMENT}{extension}");
    }

    if out.len() > MAX_COMPONENT {
        out = shorten(&out);
    }
    if out.is_empty() {
        out = REPLACEMENT.to_string();
    }
    out
}

/// Cuts an overlong component down to size, keeping its extension.
///
/// The extension is what says what the file *is*, and a player that sorts by
/// it would lose the track entirely; the middle of a long title is what nobody
/// misses. Cut on a character boundary, never in the middle of one, or the
/// result is not a string at all.
fn shorten(component: &str) -> String {
    let (stem, extension) = split_extension(component);
    let room = MAX_COMPONENT.saturating_sub(extension.len());
    let mut end = room.min(stem.len());
    while end > 0 && !stem.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{extension}", &stem[..end])
}

/// Makes a name unique among those already taken, and records it.
///
/// Shortening and character replacement can both map two different names onto
/// one — `Vol. 1: Live` and `Vol. 1? Live` both become `Vol. 1_ Live` — and a
/// copy that silently merges two albums into one folder is worse than a copy
/// that refuses. A counter is appended, before the extension, so the file
/// stays playable.
pub fn make_unique(name: &str, taken: &mut BTreeSet<String>) -> String {
    if taken.insert(name.to_string()) {
        return name.to_string();
    }
    let (stem, extension) = split_extension(name);
    for n in 2..10_000 {
        let candidate = format!("{stem} ({n}){extension}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    // Ten thousand names differing only by what was replaced is not a library,
    // it is a bug somewhere else; the name is left as it is rather than
    // looping for ever.
    name.to_string()
}

/// Asks the destination itself what it accepts, by trying.
///
/// Deliberately empirical. The alternative is to read the filesystem's *name*
/// and infer from it, which needs platform-specific code and is wrong exactly
/// where it matters: a FUSE mount, an SMB share of a Windows folder, or a card
/// reader all report something the table does not list, and the inference has
/// to guess. Creating one file settles the question for the volume actually
/// being written to.
///
/// A failure to probe reads as "restricted": a copy that adapts names it did
/// not have to costs a few odd characters, one that fails to adapt names it
/// should have costs the run.
pub fn restricts_names(destination: &std::path::Path) -> bool {
    let probe = destination.join("aede-name-probe?.tmp");
    let allowed = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    !allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_a_player_accepts_is_left_alone() {
        // Adapting a name that did not need it would make the copy differ from
        // the library for no reason, and `None` is what tells the caller there
        // is nothing to report.
        for name in [
            "01 Crazy Train.flac",
            "Blizzard of Ozz",
            "Café Bleu",
            "AC-DC",
            "folder.jpg",
        ] {
            assert_eq!(adapt(name), None, "{name} needed no change");
        }
    }

    #[test]
    fn the_punctuation_a_card_refuses_is_replaced() {
        assert_eq!(
            adapt("Where Is My Mind?.flac").as_deref(),
            Some("Where Is My Mind_.flac")
        );
        assert_eq!(
            adapt("Symphony No. 5: Allegro.flac").as_deref(),
            Some("Symphony No. 5_ Allegro.flac")
        );
        assert_eq!(adapt("AC/DC"), None, "the separator never reaches here");
        assert_eq!(
            adapt("a\"b<c>d|e*f.flac").as_deref(),
            Some("a_b_c_d_e_f.flac")
        );
    }

    #[test]
    fn a_trailing_dot_or_space_is_taken_off_rather_than_replaced() {
        // The call succeeds and the name comes back without it, so the copy
        // can never find the file it just wrote. Trimmed, because a trailing
        // space carries nothing anyone wants to keep.
        assert_eq!(adapt("Vol. 2 ").as_deref(), Some("Vol. 2"));
        assert_eq!(adapt("Greatest Hits.").as_deref(), Some("Greatest Hits"));
        assert_eq!(adapt("...").as_deref(), Some("_"));
    }

    #[test]
    fn a_device_name_is_refused_whatever_the_extension() {
        // `NUL.flac` cannot be created on a FAT volume: the reserved word is
        // the stem, not the whole name.
        assert_eq!(adapt("NUL.flac").as_deref(), Some("NUL_.flac"));
        assert_eq!(adapt("con.mp3").as_deref(), Some("con_.mp3"));
        assert_eq!(adapt("COM1").as_deref(), Some("COM1_"));
        assert_eq!(adapt("Nullify.flac"), None, "only the exact word");
    }

    #[test]
    fn an_overlong_name_keeps_its_extension() {
        // Cutting the tail would take the extension with it, and a player that
        // sorts by extension would lose the track entirely.
        let long = format!("{}.flac", "a".repeat(300));
        let adapted = adapt(&long).expect("too long to write");
        assert!(adapted.len() <= MAX_COMPONENT, "{}", adapted.len());
        assert!(adapted.ends_with(".flac"), "{adapted}");
    }

    #[test]
    fn a_name_is_cut_on_a_character_and_not_in_the_middle_of_one() {
        // Every "é" is two bytes: a limit counted in bytes lands inside one
        // every other time, and the result would not be a string at all.
        for extra in 0..4 {
            let long = format!("{}{}.flac", "é".repeat(200), "x".repeat(extra));
            let adapted = adapt(&long).expect("too long");
            assert!(adapted.len() <= MAX_COMPONENT);
            assert!(adapted.ends_with(".flac"));
        }
    }

    #[test]
    fn not_every_dot_ends_a_name() {
        // `Vol. 1: Live` holds a dot that introduces no extension. Taking the
        // last one regardless made the stem `Vol` and the extension ` 1_ Live`,
        // and the counter landed inside the title: `Vol (2). 1_ Live`.
        assert_eq!(split_extension("Vol. 1_ Live"), ("Vol. 1_ Live", ""));
        assert_eq!(
            split_extension("01 Crazy Train.flac"),
            ("01 Crazy Train", ".flac")
        );
        assert_eq!(split_extension("no dot at all"), ("no dot at all", ""));
        assert_eq!(split_extension(".hidden"), (".hidden", ""));
        // A long run after the dot is a sentence, not an extension.
        assert_eq!(
            split_extension("Symphony No. 5 in C minor"),
            ("Symphony No. 5 in C minor", "")
        );
    }

    #[test]
    fn two_names_that_became_one_are_told_apart() {
        // "Vol. 1: Live" and "Vol. 1? Live" both adapt to the same string, and
        // a copy that merged two albums into one folder would be worse than
        // one that refused.
        let mut taken = BTreeSet::new();
        assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live");
        assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live (2)");
        assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live (3)");
        // The counter goes before the extension, so the file stays playable.
        assert_eq!(make_unique("a.flac", &mut taken), "a.flac");
        assert_eq!(make_unique("a.flac", &mut taken), "a (2).flac");
        // And a title whose own dot is not an extension keeps its shape: the
        // counter goes at the end, not inside it. Three are already taken
        // above, so this one is the fourth.
        assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live (4)");
    }
}
