//! Command-line argument parsing.
//!
//! Deliberately minimal, and dependency-free: `--option=value`,
//! `--option value`, `--flag`, and everything else positional. `--` stops
//! option parsing, for paths that begin with a dash.

use std::collections::BTreeMap;

/// Where a listing starts and how many rows it shows.
///
/// `limit` is `usize::MAX` when `--all` was given: "everything" is a very large
/// window rather than a special case every caller would have to remember.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// Rows skipped before the first one shown.
    pub offset: usize,
    /// Rows shown from there.
    pub limit: usize,
}

impl Window {
    /// Row numbers shown, counting from one, given how many there are in all.
    ///
    /// `None` when the window falls past the end — which is not an error but
    /// does need saying, or an empty screen reads as an empty library.
    pub fn shown(&self, total: usize) -> Option<(usize, usize)> {
        if self.offset >= total {
            return None;
        }
        let last = total.min(self.offset.saturating_add(self.limit));
        Some((self.offset + 1, last))
    }
}

#[derive(Debug, Default)]
pub struct Args {
    /// First positional value: the command.
    pub command: String,
    pub positionals: Vec<String>,
    flags: BTreeMap<String, Option<String>>,
    /// How each option was written, so an error can quote it back.
    spellings: BTreeMap<String, String>,
}

/// Options whose value is a single token: a number, a path, a keyword.
///
/// Exactly one word is taken, and anything after it goes on being parsed
/// normally — `--limit 10 coltrane` limits to ten and searches for Coltrane.
const VALUED_WORD: &[&str] = &[
    "remove",
    "data",
    "limit",
    "sort",
    "type",
    "severity",
    "year",
    "output",
    "threads",
    "separator",
    "source",
    "offset",
];

/// Options whose value is the **name of something**, and names have spaces in
/// them.
///
/// These take every word up to the next option, exactly as a title typed
/// without quotes is joined into one. `--with Jeff Beck` is the guitarist, not
/// the word "Jeff" followed by a stray "Beck" — and that stray word used to be
/// swallowed into the artist being asked about, which turned
/// `artist Ozzy --with Jeff Beck` into a search for "Ozzy Beck". Silently
/// building a name nobody typed is the worst of the three possible behaviours;
/// the other two are demanding quotes, and this.
///
/// The value still ends at the next `-`, so `--with Jeff Beck --json` works.
/// Putting such an option *before* the positional it competes with — `track
/// --artist Miles Davis So What` — makes it swallow the title too, and the
/// command then says it was given no title rather than answering the wrong
/// question.
const VALUED_NAME: &[&str] = &[
    "artist", "album", "with", "genre", "label", "comment", "role",
];

/// Short spellings, each standing for exactly one long option.
///
/// Deliberately few. A one-letter alias saves four keystrokes and costs a line
/// of documentation for ever, so it is worth it only where the option is typed
/// constantly.
const SHORT: &[(&str, &str)] = &[
    ("h", "help"),
    ("V", "version"),
    ("j", "json"),
    ("o", "output"),
];

/// Long name behind a short one; an unknown letter keeps its own spelling, and
/// [`Args::as_typed`] is what puts the dashes back on for a message.
fn long_name(short: &str) -> String {
    SHORT
        .iter()
        .find(|(letter, _)| *letter == short)
        .map(|(_, long)| (*long).to_string())
        .unwrap_or_else(|| short.to_string())
}

/// Splits `name=value` into its two halves; `value` is `None` without a `=`.
fn split_option(text: &str) -> (String, Option<String>) {
    match text.split_once('=') {
        Some((name, value)) => (name.to_string(), Some(value.to_string())),
        None => (text.to_string(), None),
    }
}

impl Args {
    pub fn parse(raw: impl IntoIterator<Item = String>) -> Args {
        let mut args = Args::default();
        let mut iter = raw.into_iter().peekable();
        let mut only_positionals = false;

        while let Some(item) = iter.next() {
            if only_positionals {
                args.positionals.push(item);
                continue;
            }
            if item == "--" {
                only_positionals = true;
                continue;
            }
            // A short option is resolved to its long name and then treated
            // exactly like it: `-o file`, `-o=file` and `--output file` are one
            // option written three ways, and only the spelling differs.
            let named = if let Some(rest) = item.strip_prefix("--") {
                let (name, value) = split_option(rest);
                args.spellings.insert(name.clone(), format!("--{name}"));
                Some((name, value))
            } else if item.len() >= 2
                && item.starts_with('-')
                && (item.len() == 2 || item.as_bytes().get(2) == Some(&b'='))
            {
                let (short, value) = split_option(&item[1..]);
                let name = long_name(&short);
                args.spellings.insert(name.clone(), format!("-{short}"));
                Some((name, value))
            } else {
                None
            };
            let Some((name, inline)) = named else {
                args.positionals.push(item);
                continue;
            };

            let value = match inline {
                Some(value) => Some(value),
                None if VALUED_NAME.contains(&name.as_str()) => {
                    let mut words: Vec<String> = Vec::new();
                    while let Some(word) = iter.next_if(|next| !next.starts_with('-')) {
                        words.push(word);
                    }
                    (!words.is_empty()).then(|| words.join(" "))
                }
                None if VALUED_WORD.contains(&name.as_str()) => {
                    iter.next_if(|next| !next.starts_with('-'))
                }
                None => None,
            };
            args.flags.insert(name, value);
        }

        if !args.positionals.is_empty() {
            args.command = args.positionals.remove(0);
        }
        args
    }

    pub fn from_env() -> Args {
        Args::parse(std::env::args().skip(1))
    }

    pub fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    /// Options typed, minus those listed, each in the spelling it was typed in.
    ///
    /// What separates "run with nothing", which asks for the help, from "run
    /// with options and no command", which asks for something the program
    /// cannot do and has to be told so. The exceptions are the options that
    /// shape what is printed rather than what is answered: they have the help
    /// itself to act on.
    pub fn options_given_except(&self, ignored: &[&str]) -> Vec<&str> {
        self.flags
            .keys()
            .filter(|k| !ignored.contains(&k.as_str()))
            .map(|k| self.as_typed(k))
            .collect()
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|v| v.as_deref())
    }

    /// The slice of a result to show: where to start, and how much.
    ///
    /// One reading for the whole program, because paging is only meaningful if
    /// every command agrees on what a window is — and because the interface at
    /// M2 will ask for pages, not for "the first fifty of everything". The
    /// order is already deterministic on every listing, which is what makes a
    /// second page mean anything at all.
    ///
    /// Strict, unlike the old `--limit` reading: `--limit abc` used to fall
    /// back on the default and answer a question nobody asked.
    pub fn window(&self, default_limit: usize) -> Result<Window, String> {
        let offset = self.whole_number("offset")?.unwrap_or(0);
        if self.has("all") && self.value("limit").is_some() {
            return Err("--all and --limit ask for opposite things".into());
        }
        if self.has("all") {
            return Ok(Window {
                offset,
                limit: usize::MAX,
            });
        }
        let limit = match self.whole_number("limit")? {
            // Zero shows nothing, which nobody ever means; "everything" has a
            // name of its own rather than an encoding to remember.
            Some(0) => return Err("--limit=0 would show nothing; --all shows everything".into()),
            Some(n) => n,
            None => default_limit,
        };
        Ok(Window { offset, limit })
    }

    /// A whole number given to an option, refusing anything else.
    fn whole_number(&self, name: &str) -> Result<Option<usize>, String> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<usize>()
                .map(Some)
                .map_err(|_| format!("--{name} expects a whole number, not \"{raw}\"")),
        }
    }

    pub fn usize_value(&self, name: &str, default: usize) -> usize {
        self.value(name)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    /// Options that expect a value and were given none.
    ///
    /// `--album` on its own does not narrow anything, so the command would
    /// answer as if no filter had been asked for — the one case where being
    /// quiet gives a wrong answer rather than an incomplete one.
    pub fn options_missing_a_value(&self) -> Vec<&str> {
        self.flags
            .iter()
            .filter(|(name, value)| {
                value.is_none()
                    && (VALUED_WORD.contains(&name.as_str())
                        || VALUED_NAME.contains(&name.as_str()))
            })
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Options the program knows nothing about, each in the spelling it was
    /// typed in.
    ///
    /// The caller refuses them. It used to warn and carry on, so
    /// `aede albums --limite=5` put one line on the error stream and the whole
    /// unlimited listing on the standard one — a wrong answer rather than a
    /// missing one, and the answer is the half that gets read. Every other
    /// silent acceptance in this program has been closed the same way; this was
    /// the last one open, and the only one whose own comment claimed it was
    /// handled.
    pub fn unknown_flags(&self, accepted: &[&str]) -> Vec<&str> {
        self.flags
            .keys()
            .filter(|k| !accepted.contains(&k.as_str()))
            .map(|k| self.as_typed(k))
            .collect()
    }

    /// The option as the user wrote it, dashes included.
    ///
    /// `-o` and `--output` are one option, and an error naming the second when
    /// the first was typed sends the reader hunting for something they never
    /// wrote. Same rule as the role names: what is shown is what was accepted.
    fn as_typed<'a>(&'a self, name: &'a str) -> &'a str {
        self.spellings.get(name).map(|s| s.as_str()).unwrap_or(name)
    }
}

/// The known option closest to what was typed, when one is close enough.
///
/// Levenshtein distance, bounded by a third of the length: `--limite` is
/// `--limit` misspelled, `--json` is not `--label` misspelled. A suggestion
/// that fires on everything is noise; one that never fires costs a round trip
/// through the help for a dropped letter.
pub fn nearest(typed: &str, known: &[&str]) -> Option<String> {
    let typed = typed.trim_start_matches('-');
    let bound = (typed.chars().count() / 3).max(1);
    known
        .iter()
        .map(|option| (distance(typed, option), *option))
        .filter(|(d, _)| *d <= bound)
        .min_by_key(|(d, option)| (*d, option.len()))
        .map(|(_, option)| format!("--{option}"))
}

/// Levenshtein distance, two rows at a time.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let substitute = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitute.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(items: &[&str]) -> Args {
        Args::parse(items.iter().map(|s| s.to_string()))
    }

    #[test]
    fn a_valued_option_takes_the_next_word() {
        // `--album` used to be missing from the valued list: the title and the
        // album ended up glued together into one search string.
        let a = parse(&["track", "So What", "--album", "Kind of Blue"]);
        assert_eq!(a.positionals, ["So What"]);
        assert_eq!(a.value("album"), Some("Kind of Blue"));
    }

    #[test]
    fn a_name_option_takes_the_whole_name() {
        // `artist Ozzy --with Jeff Beck` used to give --with the word "Jeff"
        // and leave "Beck" to be joined onto the artist, which turned the
        // question into one about an "Ozzy Beck" nobody had ever typed.
        let a = parse(&["artist", "Ozzy", "--with", "Jeff", "Beck"]);
        assert_eq!(a.positionals, ["Ozzy"]);
        assert_eq!(a.value("with"), Some("Jeff Beck"));
    }

    #[test]
    fn a_name_ends_at_the_next_option() {
        let a = parse(&["artist", "Ozzy", "--with", "Zakk", "Wylde", "--json"]);
        assert_eq!(a.positionals, ["Ozzy"]);
        assert_eq!(a.value("with"), Some("Zakk Wylde"));
        assert!(a.has("json"));

        // Quoting still works, and gives the same thing.
        let quoted = parse(&["artist", "Ozzy", "--with", "Zakk Wylde"]);
        assert_eq!(quoted.value("with"), Some("Zakk Wylde"));
        assert_eq!(
            parse(&["artist", "Ozzy", "--with=Zakk Wylde"]).value("with"),
            Some("Zakk Wylde")
        );
    }

    #[test]
    fn a_short_option_is_the_long_one_written_shorter() {
        // -o carries a value, which the old short-flag branch could not do: it
        // inserted a valueless flag and left the file name as a positional.
        for form in [
            vec!["albums", "-o", "out.csv"],
            vec!["albums", "-o=out.csv"],
            vec!["albums", "--output", "out.csv"],
            vec!["albums", "--output=out.csv"],
        ] {
            let a = parse(&form);
            assert_eq!(a.value("output"), Some("out.csv"), "form: {form:?}");
            assert!(a.positionals.is_empty(), "form: {form:?}");
        }
        // The valueless ones keep working.
        assert!(parse(&["-h"]).has("help"));
        assert!(parse(&["-V"]).has("version"));
        assert!(parse(&["stats", "-j"]).has("json"));
    }

    #[test]
    fn a_window_is_read_strictly() {
        let w = parse(&["albums", "--limit", "10", "--offset", "20"])
            .window(50)
            .expect("a window");
        assert_eq!(w.offset, 20);
        assert_eq!(w.limit, 10);
        assert_eq!(parse(&["albums"]).window(50).unwrap().limit, 50);
        assert_eq!(
            parse(&["albums", "--all"]).window(50).unwrap().limit,
            usize::MAX
        );

        // A limit that cannot be read used to fall back on the default and
        // answer a question nobody asked.
        assert!(parse(&["albums", "--limit", "abc"]).window(50).is_err());
        // Written with `=`, since a bare `-1` is refused earlier as a missing
        // value: an option's value may not start with a dash.
        assert!(parse(&["albums", "--offset=-1"]).window(50).is_err());
        assert!(
            parse(&["albums", "--offset", "-1"])
                .options_missing_a_value()
                .contains(&"offset"),
            "a value that looks like an option is not a value"
        );
        // Zero shows nothing, which nobody means.
        assert!(parse(&["albums", "--limit", "0"]).window(50).is_err());
        // And the two ways of saying how much are opposites.
        assert!(
            parse(&["albums", "--all", "--limit", "5"])
                .window(50)
                .is_err()
        );
    }

    #[test]
    fn a_window_says_which_rows_it_shows() {
        let w = Window {
            offset: 50,
            limit: 50,
        };
        assert_eq!(w.shown(312), Some((51, 100)));
        // The last page is short, and stops at the last row rather than past it.
        assert_eq!(
            Window {
                offset: 300,
                limit: 50
            }
            .shown(312),
            Some((301, 312))
        );
        // Everything fits: the caller prints nothing.
        assert_eq!(
            Window {
                offset: 0,
                limit: 50
            }
            .shown(12),
            Some((1, 12))
        );
        // Past the end is not an error, but it has to be said.
        assert_eq!(
            Window {
                offset: 400,
                limit: 50
            }
            .shown(312),
            None
        );
        assert_eq!(
            Window {
                offset: 0,
                limit: usize::MAX
            }
            .shown(312),
            Some((1, 312))
        );
    }

    #[test]
    fn a_word_option_keeps_taking_exactly_one_word() {
        // A number, a path or a keyword must not swallow what follows it: only
        // names are greedy, because only names have spaces in them.
        let a = parse(&["search", "--limit", "10", "coltrane"]);
        assert_eq!(a.value("limit"), Some("10"));
        assert_eq!(a.positionals, ["coltrane"]);

        let b = parse(&["albums", "--year", "1969", "--artist", "Miles", "Davis"]);
        assert_eq!(b.value("year"), Some("1969"));
        assert_eq!(b.value("artist"), Some("Miles Davis"));
    }

    #[test]
    fn an_option_left_without_a_value_is_reported() {
        let a = parse(&["track", "So What", "--album"]);
        assert_eq!(a.options_missing_a_value(), ["album"]);
        // A flag that expects nothing is not concerned.
        let b = parse(&["stats", "--json"]);
        assert!(b.options_missing_a_value().is_empty());
    }

    #[test]
    fn command_and_positionals() {
        let a = parse(&["scan", "/music", "/other"]);
        assert_eq!(a.command, "scan");
        assert_eq!(a.positionals, ["/music", "/other"]);
    }

    #[test]
    fn options_with_equals_and_separate() {
        let a = parse(&["albums", "--limit=5", "--artist", "Miles Davis"]);
        assert_eq!(a.value("limit"), Some("5"));
        assert_eq!(a.value("artist"), Some("Miles Davis"));
        assert_eq!(a.usize_value("limit", 20), 5);
    }

    #[test]
    fn flags_without_value() {
        let a = parse(&["stats", "--json", "--no-color"]);
        assert!(a.has("json"));
        assert!(a.has("no-color"));
        assert_eq!(a.value("json"), None);
    }

    #[test]
    fn double_dash_stops_options() {
        let a = parse(&["scan", "--", "--odd-folder"]);
        assert_eq!(a.positionals, ["--odd-folder"]);
    }

    #[test]
    fn short_options() {
        let a = parse(&["-h"]);
        assert!(a.has("help"));
    }

    #[test]
    fn unknown_options_are_named_as_they_were_typed() {
        let a = parse(&["stats", "--limite=3"]);
        assert_eq!(a.unknown_flags(&["limit", "json"]), ["--limite"]);

        // `-o` and `--output` are one option, and a message naming the long
        // form when the short one was typed sends the reader hunting for
        // something they never wrote. The same rule as the role names.
        let a = parse(&["artists", "-z"]);
        assert_eq!(a.unknown_flags(&["limit", "json"]), ["-z"]);
        let a = parse(&["artists", "-o", "out.csv"]);
        assert!(a.unknown_flags(&["output"]).is_empty(), "-o is --output");
    }

    #[test]
    fn a_dropped_letter_gets_a_suggestion_and_a_distant_word_does_not() {
        let known = &["limit", "label", "json", "all", "compilations"];
        assert_eq!(nearest("--limite", known).as_deref(), Some("--limit"));
        assert_eq!(nearest("--lmit", known).as_deref(), Some("--limit"));
        assert_eq!(
            nearest("--compilation", known).as_deref(),
            Some("--compilations")
        );
        // Two real options one letter apart from each other must not be
        // proposed for something that is neither.
        assert_eq!(nearest("--fegioregj", known), None);
        assert_eq!(nearest("--x", known), None);
    }

    #[test]
    fn an_option_does_not_eat_the_next_command() {
        // `--json` expects no value: `stats` must stay positional.
        let a = parse(&["--json", "stats"]);
        assert_eq!(a.command, "stats");
        assert!(a.has("json"));
    }
}
