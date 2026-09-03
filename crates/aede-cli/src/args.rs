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
    "data",
    "limit",
    "sort",
    "severity",
    "year",
    "output",
    "threads",
    "separator",
    "source",
    "offset",
    "stars",
    "file",
    "import",
    "extras",
    "exclude",
    "compress",
    "quality",
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
    "artist",
    "album",
    "with",
    "genre",
    "label",
    "comment",
    "role",
    // "united kingdom", "united states", "new zealand": a country is a name
    // like any other, and demanding quotes for half of them is exactly what
    // this list exists to avoid.
    "country",
    "text",
    "from",
    "tag",
    "query",
    "collection",
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

/// `true` when a word can be the value of the option before it.
///
/// A leading dash means an option, with one exception every command-line tool
/// shares: a lone `-` is the name of standard input, and `--file -` is how a
/// note gets piped in. Refusing it there would refuse the one spelling everyone
/// already knows.
fn is_value(word: &str) -> bool {
    word == "-" || !word.starts_with('-')
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
                    while let Some(word) = iter.next_if(|next| is_value(next)) {
                        words.push(word);
                    }
                    (!words.is_empty()).then(|| words.join(" "))
                }
                None if VALUED_WORD.contains(&name.as_str()) => iter.next_if(|next| is_value(next)),
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
    pub fn whole_number(&self, name: &str) -> Result<Option<usize>, String> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<usize>()
                .map(Some)
                .map_err(|_| format!("--{name} expects a whole number, not \"{raw}\"")),
        }
    }

    /// A whole number, or the default when the option was not given.
    ///
    /// Strict, like [`Args::window`]: `--threads abc` used to fall back on the
    /// default and read on however many threads it liked, which is a different
    /// answer to the question rather than a refusal to answer it.
    pub fn number_or(&self, name: &str, default: usize) -> Result<usize, String> {
        Ok(self.whole_number(name)?.unwrap_or(default))
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
#[path = "args_tests.rs"]
mod tests;
