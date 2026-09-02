//! Tests for [`super`], split out of `args.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

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
    assert_eq!(a.number_or("limit", 20), Ok(5));
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
fn a_lone_dash_is_a_value_and_not_an_option() {
    // `--file -` is how everyone spells "read it from the pipe", and a
    // parser that reads the dash as an option refuses the one form the
    // user already knows.
    let a = parse(&["note", "--file", "-"]);
    assert_eq!(a.value("file"), Some("-"));
    // A dash followed by anything else is still an option.
    let a = parse(&["albums", "--limit", "-5"]);
    assert_eq!(a.value("limit"), None);
}

#[test]
fn an_option_does_not_eat_the_next_command() {
    // `--json` expects no value: `stats` must stay positional.
    let a = parse(&["--json", "stats"]);
    assert_eq!(a.command, "stats");
    assert!(a.has("json"));
}
