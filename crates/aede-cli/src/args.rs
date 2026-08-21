//! Command-line argument parsing.
//!
//! Deliberately minimal, and dependency-free: `--option=value`,
//! `--option value`, `--flag`, and everything else positional. `--` stops
//! option parsing, for paths that begin with a dash.

use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct Args {
    /// First positional value: the command.
    pub command: String,
    pub positionals: Vec<String>,
    flags: BTreeMap<String, Option<String>>,
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
            if let Some(rest) = item.strip_prefix("--") {
                match rest.split_once('=') {
                    Some((name, value)) => {
                        args.flags.insert(name.to_string(), Some(value.to_string()));
                    }
                    None => {
                        let value = if VALUED_NAME.contains(&rest) {
                            let mut words: Vec<String> = Vec::new();
                            while let Some(word) = iter.next_if(|next| !next.starts_with('-')) {
                                words.push(word);
                            }
                            (!words.is_empty()).then(|| words.join(" "))
                        } else if VALUED_WORD.contains(&rest) {
                            iter.next_if(|next| !next.starts_with('-'))
                        } else {
                            None
                        };
                        args.flags.insert(rest.to_string(), value);
                    }
                }
                continue;
            }
            if item.len() == 2 && item.starts_with('-') {
                let name = match item.as_str() {
                    "-h" => "help",
                    "-V" => "version",
                    "-j" => "json",
                    other => &other[1..],
                };
                args.flags.insert(name.to_string(), None);
                continue;
            }
            args.positionals.push(item);
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

    pub fn value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|v| v.as_deref())
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

    /// Options given but unknown to the command: better to warn than to
    /// silently ignore a typo.
    pub fn unknown_flags(&self, accepted: &[&str]) -> Vec<&str> {
        self.flags
            .keys()
            .map(|k| k.as_str())
            .filter(|k| !accepted.contains(k))
            .collect()
    }
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
    fn unknown_options_spotted() {
        let a = parse(&["stats", "--limite=3"]);
        assert_eq!(a.unknown_flags(&["limit", "json"]), ["limite"]);
    }

    #[test]
    fn an_option_does_not_eat_the_next_command() {
        // `--json` expects no value: `stats` must stay positional.
        let a = parse(&["--json", "stats"]);
        assert_eq!(a.command, "stats");
        assert!(a.has("json"));
    }
}
