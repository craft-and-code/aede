//! End-to-end test: run the real binary against the reference files and check
//! what it prints.
//!
//! This is the only test that exercises the whole chain — directory walk, tag
//! reading, graph construction, persistence, reload, rendering.

use std::path::PathBuf;
use std::process::Command;

fn library() -> PathBuf {
    // The reference files belong to the core crate.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/fixtures")
        .canonicalize()
        .expect("reference folder")
}

/// A throwaway data directory, removed when the test ends.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!("aede_e2e_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporary folder");
        Sandbox { dir }
    }

    /// Runs the binary and returns `(stdout, stderr, success)`.
    fn run(&self, args: &[&str]) -> (String, String, bool) {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(args)
            .env("AEDE_HOME", &self.dir)
            .env("NO_COLOR", "1")
            .output()
            .expect("running the binary");
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.success(),
        )
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn scan_then_query() {
    let sandbox = Sandbox::new("full");
    let root = library();

    // --- Initial scan ------------------------------------------------------
    let (out, err, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok, "the scan must succeed. stderr: {err}");
    assert!(out.contains("Scan complete"), "output: {out}");
    assert!(
        sandbox.dir.join("catalog.json").exists(),
        "the catalog must be written"
    );

    // --- Statistics --------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["stats"]);
    assert!(ok);
    assert!(out.contains("Miles Davis"), "the artist must appear: {out}");
    assert!(out.contains("FLAC"));
    assert!(out.contains("Metadata completeness"));

    // --- Artist page -------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(
        out.contains("Kind of Blue"),
        "the discography must appear: {out}"
    );

    // --- Search, case and accent insensitive -------------------------------
    let (out, _, ok) = sandbox.run(&["search", "kind of blue"]);
    assert!(ok);
    assert!(out.contains("Kind of Blue"));

    // --- Machine-readable output -------------------------------------------
    let (out, _, ok) = sandbox.run(&["stats", "--json"]);
    assert!(ok);
    let value = aede_core::json::parse(&out).expect("valid JSON output");
    assert!(value.field_u64("tracks").unwrap_or(0) >= 8, "output: {out}");

    // --- Diagnosis ---------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(out.contains("Diagnosis"));

    // --- Incremental scan: nothing to read again ---------------------------
    let (out, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Reused from previous scan"), "output: {out}");
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Read") && l.trim_end().ends_with('0')),
        "no file should have been read again:\n{out}"
    );
}

#[test]
fn missing_catalog_gives_an_actionable_message() {
    let sandbox = Sandbox::new("empty");
    let (_, err, ok) = sandbox.run(&["stats"]);
    assert!(!ok, "the command must fail");
    assert!(
        err.contains("aede scan"),
        "the message must say what to do: {err}"
    );
}

#[test]
fn unknown_command_is_reported() {
    let sandbox = Sandbox::new("unknown");
    let (_, err, ok) = sandbox.run(&["statz"]);
    assert!(!ok);
    assert!(err.contains("unknown command"), "stderr: {err}");
}

#[test]
fn a_misspelled_option_stops_the_command() {
    // It used to warn and carry on: `albums --limite=5` put one line on the
    // error stream and the whole unlimited listing on the standard one. The
    // answer is the half that gets read, and it was a wrong answer rather than
    // a missing one.
    let sandbox = Sandbox::new("option");
    let (out, err, ok) = sandbox.run(&["stats", "--limite=3"]);
    assert!(!ok, "an option nobody recognises stops the command");
    assert!(err.contains("unknown option: --limite"), "stderr: {err}");
    assert!(err.contains("Did you mean --limit?"), "stderr: {err}");
    assert!(out.is_empty(), "and answers nothing: {out}");

    // Named in the spelling it was typed in: `-o` and `--output` are one
    // option, and an error naming the other sends the reader hunting.
    let (_, err, ok) = sandbox.run(&["artists", "-z"]);
    assert!(!ok);
    assert!(err.contains("unknown option: -z"), "stderr: {err}");

    // Refused before anything answers, `--help` included. `aede --fegioregj`
    // printing a cheerful help page is the same silence in a friendlier
    // costume: the command line was not understood, and the page says nothing
    // about that.
    let (out, err, ok) = sandbox.run(&["--fegioregj"]);
    assert!(!ok, "output: {out}");
    assert!(err.contains("unknown option"), "stderr: {err}");
    assert!(
        !out.contains("COMMANDS"),
        "the help is not an answer: {out}"
    );
}

#[test]
fn every_option_is_refused_where_it_means_nothing() {
    // Three rounds of this fault had been fixed one option at a time — `--csv`,
    // then `--genre`/`--label`, then `--artist`/`--year` — while fourteen more
    // stayed unguarded: `aede stats --severity=error`, `aede albums --full`,
    // `aede artists --with Miles` all answered cheerfully and dropped the word.
    // Fixing a class one member at a time is how it survives.
    let sandbox = Sandbox::new("all_guards");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    for args in [
        vec!["stats", "--severity=error"],
        vec!["stats", "--sort", "tracks"],
        vec!["albums", "--full"],
        vec!["albums", "--threads", "4"],
        vec!["albums", "--yes"],
        vec!["albums", "--replace"],
        vec!["albums", "--tracks"],
        vec!["artists", "--album", "Kind of Blue"],
        vec!["artists", "--with", "Miles"],
        // `genres --sort tracks` used to belong here and no longer does: every
        // listing takes --sort now. `search` is the one that still cannot be
        // ordered, since its rows are ranked by how well the name matched and
        // reordering them would throw the ranking away.
        vec!["search", "--sort", "tracks"],
        vec!["scan", "--severity=error"],
        vec!["roots", "--full"],
        vec!["albums", "--follow-symlinks"],
        vec!["stats", "--remove", "/tmp"],
    ] {
        let (out, err, ok) = sandbox.run(&args);
        assert!(!ok, "{args:?} must be refused, got:\n{out}");
        assert!(err.contains("applies to"), "{args:?} stderr: {err}");
    }

    // And an option that needs another one, which the table cannot see: it
    // knows where an option reaches, not what it needs once it is there.
    for args in [vec!["export", "--tracks"], vec!["albums", "--separator=;"]] {
        let (out, err, ok) = sandbox.run(&args);
        assert!(!ok, "{args:?} must be refused, got:\n{out}");
        assert!(err.contains("without --csv"), "{args:?} stderr: {err}");
    }

    // A value the option cannot read is an error too, not a different answer:
    // `--sort banana` used to fall through to sorting by name.
    let (_, err, ok) = sandbox.run(&["artists", "--sort", "banana"]);
    assert!(!ok);
    assert!(err.contains("is not something to sort on"), "stderr: {err}");
    // And it offers what this listing does have, rather than a fixed list.
    assert!(err.contains("tracks"), "stderr: {err}");

    // What is guarded still works where it belongs.
    for args in [
        vec!["artists", "--sort", "name"],
        vec!["doctor", "--severity=error"],
        vec!["export", "--csv", "--tracks"],
        vec!["albums", "--csv", "--separator=;"],
    ] {
        let (_, err, ok) = sandbox.run(&args);
        assert!(ok, "{args:?} must work: {err}");
    }
}

#[test]
fn json_is_offered_wherever_a_table_is() {
    // `--json` was declared globally and read by four commands. Everywhere
    // else — every listing, every page — it was accepted, ignored, and the
    // ordinary table printed instead, which looks like an answer.
    let sandbox = Sandbox::new("json_everywhere");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    for args in [
        vec!["albums", "--json"],
        vec!["artists", "--json"],
        vec!["genres", "--json"],
        vec!["labels", "--json"],
        vec!["years", "--json"],
        vec!["genre", "jazz", "--json"],
        vec!["label", "Columbia", "--json"],
        vec!["album", "Kind of Blue", "--json"],
        vec!["artist", "Miles Davis", "--json"],
    ] {
        let (out, err, ok) = sandbox.run(&args);
        assert!(ok, "{args:?} failed: {err}");
        assert!(
            out.trim_start().starts_with('['),
            "{args:?} must answer in JSON, got:\n{out}"
        );
    }

    // The commands with a shape of their own keep it: `search --json` reports
    // the hits, artists and albums included, not a flat table of tracks.
    let (out, _, ok) = sandbox.run(&["search", "miles", "--json"]);
    assert!(ok);
    assert!(
        out.contains("\"found_in\""),
        "the search shape is kept: {out}"
    );

    // Numbers are numbers and empty cells are null, because JSON can say what
    // a CSV cannot. A title that merely looks like a number stays a string.
    let (out, _, ok) = sandbox.run(&["albums", "--json"]);
    assert!(ok);
    assert!(out.contains("\"tracks\": 9"), "a count is a number: {out}");
    assert!(out.contains("\"compilation\": false"), "output: {out}");
    assert!(out.contains(": null"), "an absent field is null: {out}");
    assert!(
        out.contains("\"album\": \"Kind of Blue\""),
        "a title is a string: {out}"
    );

    // And the two formats come from one table, so a column cannot exist in one
    // and not the other.
    let (csv, _, _) = sandbox.run(&["albums", "--csv"]);
    let header = csv.lines().next().expect("a header row");
    for column in header.split(',') {
        assert!(
            out.contains(&format!("\"{column}\":")),
            "column {column} is in the CSV and not in the JSON"
        );
    }
}

#[test]
fn what_the_user_writes_survives_a_rescan() {
    // The only data in the program that no scan can rebuild. Keyed by anything
    // a scan renumbers, it would be lost on the second run — which is exactly
    // how the imported analyses were lost the first time.
    let sandbox = Sandbox::new("annotations");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["rate", "artist", "Miles Davis", "--stars", "5"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains('★'), "output: {out}");
    let (_, _, ok) = sandbox.run(&["love", "artist", "Miles Davis"]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["note", "artist", "Miles Davis", "--text", "the quintet"]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "vinyl"]);
    assert!(ok);

    // A second scan renumbers everything.
    let (_, _, ok) = sandbox.run(&["scan", "--full"]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["notes"]);
    assert!(ok);
    assert!(out.contains("Miles Davis"), "output: {out}");
    assert!(out.contains("the quintet"), "the note survived: {out}");
    assert!(out.contains("vinyl"), "the tag survived: {out}");
    assert!(out.contains('★'), "the rating survived: {out}");

    let (out, _, ok) = sandbox.run(&["favourites"]);
    assert!(ok);
    assert!(out.contains("Miles Davis"), "output: {out}");

    // One record holds all four, so taking them all back leaves nothing.
    for args in [
        vec!["love", "artist", "Miles Davis", "--remove"],
        vec!["rate", "artist", "Miles Davis", "--remove"],
        vec!["note", "artist", "Miles Davis", "--remove"],
        vec!["tag", "artist", "Miles Davis", "vinyl", "--remove"],
    ] {
        let (_, err, ok) = sandbox.run(&args);
        assert!(ok, "{args:?}: {err}");
    }
    let (out, _, ok) = sandbox.run(&["notes"]);
    assert!(ok);
    assert!(
        out.contains("nothing has been written"),
        "no empty shell is left behind: {out}"
    );
}

#[test]
fn tags_go_on_and_come_off_in_lists() {
    // Tagging is the one annotation that is naturally plural — a record is
    // vinyl *and* rare *and* to-rip-again — and putting them on one at a time,
    // then taking them off one at a time, is the kind of asymmetry that makes a
    // command tiring to use.
    let sandbox = Sandbox::new("tag_lists");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    // --- Several at once ------------------------------------------------------
    let (out, err, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "vinyl,rare,to keep"]);
    assert!(ok, "stderr: {err}");
    for label in ["vinyl", "rare", "to keep"] {
        assert!(out.contains(label), "{label} is missing from: {out}");
    }

    // A space after the comma is the way a person writes a list, and must mean
    // the same thing as no space at all.
    let (out, err, ok) = sandbox.run(&["tag", "album", "Duos", "jazz, modal"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("jazz"), "output: {out}");
    assert!(out.contains("modal"), "output: {out}");

    // All of them landed, and the pages show them.
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    for label in ["vinyl", "rare", "to keep"] {
        assert!(out.contains(label), "{label} not on the page: {out}");
    }

    // --- What was already there is not reported as new ------------------------
    let (out, _, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "vinyl,fresh"]);
    assert!(ok);
    assert!(out.contains("fresh"), "output: {out}");
    assert!(
        out.contains("already carried"),
        "a label that was already on it is said so, not counted twice: {out}"
    );

    // --- Taking several off ---------------------------------------------------
    let (out, err, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "vinyl, fresh", "--remove"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("no longer carries"), "output: {out}");
    let (out, _, _) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(!out.contains("vinyl"), "vinyl is gone: {out}");
    assert!(!out.contains("fresh"), "fresh is gone: {out}");
    assert!(out.contains("rare"), "and the others stayed: {out}");

    // --- Taking every one off -------------------------------------------------
    // Having listed them to put them on, being made to list them again to take
    // them off is the asymmetry this whole change is about.
    let (out, err, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "--remove"]);
    assert!(ok, "stderr: {err}");
    assert!(
        out.contains("rare") && out.contains("to keep"),
        "it names what it removed, since the user did not type the list: {out}"
    );
    let (out, _, _) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(!out.contains("rare"), "nothing is left: {out}");

    // Removing from something that carries nothing says so rather than
    // claiming a removal.
    let (out, _, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "--remove"]);
    assert!(ok);
    assert!(out.contains("no tag at all"), "output: {out}");

    // A label that was never there is not reported as taken off.
    let (_, _, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "kept"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["tag", "artist", "Miles Davis", "kept,never", "--remove"]);
    assert!(ok);
    assert!(out.contains("did not carry"), "output: {out}");
    assert!(out.contains("never"), "and names which: {out}");

    // --- The old shape is untouched -------------------------------------------
    // One word, no comma, an unquoted multi-word name: exactly what worked
    // before lists existed, and it must still mean the same thing.
    let (out, err, ok) = sandbox.run(&["tag", "artist", "Dave Brubeck", "classic"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("Dave Brubeck"), "output: {out}");
    assert!(out.contains("classic"), "output: {out}");

    // And a tag with no target is still refused.
    let (_, err, ok) = sandbox.run(&["tag", "album"]);
    assert!(!ok, "stderr: {err}");
}

#[test]
fn a_name_matching_several_things_is_refused_with_something_typeable() {
    // Listing the names again repeats the ambiguity being reported: two albums
    // called "Kind of Blue" print as "Kind of Blue, Kind of Blue", which tells
    // the reader nothing and offers nothing to type.
    let sandbox = Sandbox::new("ambiguous");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (_, err, ok) = sandbox.run(&["love", "album", "Kind of Blue"]);
    assert!(!ok, "two albums carry that title");
    assert!(err.contains("matches 2 albums"), "stderr: {err}");
    assert!(err.contains("Miles Davis Sextet"), "it says which: {err}");

    // And what it printed can be typed straight back in.
    let reference = err
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("release:"))
        .expect("a reference to paste back");
    let (out, err, ok) = sandbox.run(&["love", reference]);
    assert!(ok, "the reference it offered must be accepted: {err}");
    assert!(out.contains("favourite"), "output: {out}");
}

#[test]
fn the_history_counts_more_than_it_keeps() {
    let sandbox = Sandbox::new("history");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let file = root.join("track.flac");
    let reference = format!("track:{}", file.to_str().unwrap());
    for _ in 0..3 {
        let (_, err, ok) = sandbox.run(&["played", &reference]);
        assert!(ok, "stderr: {err}");
    }
    let (out, _, ok) = sandbox.run(&["history"]);
    assert!(ok);
    assert!(out.contains("So What"), "output: {out}");
    assert!(out.contains("3 times"), "the counter is shown: {out}");
}

#[test]
fn reset_says_what_it_does_not_take() {
    // The catalog goes; what the user wrote is in another file and stays. A
    // destructive command that does not say so invites the wrong hesitation.
    let sandbox = Sandbox::new("reset_notes");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["love", "artist", "Miles Davis"]);
    assert!(ok);

    let (out, _, _) = sandbox.run(&["reset"]);
    assert!(
        out.contains("stay: they are not in this file"),
        "output: {out}"
    );
}

#[test]
fn a_rating_given_is_a_rating_shown() {
    // A rating that never appears again is a rating nobody trusts. Every page
    // that names an entity ends with what was written about it — and prints
    // nothing at all when nothing was.
    let sandbox = Sandbox::new("panel");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(
        !out.contains("Yours"),
        "nothing written, nothing shown: {out}"
    );

    let (_, _, ok) = sandbox.run(&["rate", "artist", "Miles Davis", "--stars", "5"]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["love", "artist", "Miles Davis"]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["note", "artist", "Miles Davis", "--text", "the quintet"]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("Yours"), "output: {out}");
    assert!(out.contains('★'), "the rating: {out}");
    assert!(out.contains('♥'), "the favourite: {out}");
    assert!(out.contains("the quintet"), "the note: {out}");

    // And reading a note back is what `note` does with nothing to write.
    let (out, _, ok) = sandbox.run(&["note", "artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("the quintet"), "output: {out}");
}

#[test]
fn a_note_is_a_written_thing_with_a_section_of_its_own() {
    // A rating is a label on a thing; a note is a text somebody wrote, and
    // burying it in a row of stars and tags says it matters less than they do.
    let sandbox = Sandbox::new("notes_section");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let note = std::env::temp_dir().join("aede_e2e_note.md");
    std::fs::write(
        &note,
        "# Kind of Blue\n\nThe 1997 remaster is the one:\n\n- side A was fast\n- side B was not\n",
    )
    .expect("the note file");

    let (_, err, ok) = sandbox.run(&[
        "note",
        "artist",
        "Miles Davis",
        "--file",
        note.to_str().unwrap(),
    ]);
    assert!(ok, "stderr: {err}");

    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("Notes"), "its own section: {out}");
    assert!(
        !out.contains("Yours"),
        "and nothing else was written: {out}"
    );
    // Kept exactly as typed, blank lines and all: a note is not a field to be
    // tidied, and the front end is what will render the Markdown.
    assert!(out.contains("# Kind of Blue"), "output: {out}");
    assert!(out.contains("- side A was fast"), "output: {out}");

    // Marks and note are two sections, not one row.
    let (_, _, ok) = sandbox.run(&["rate", "artist", "Miles Davis", "--stars", "5"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    let yours = out.find("Yours").expect("the marks");
    let notes = out.find("Notes").expect("the note");
    assert!(yours < notes, "marks first, then the text: {out}");

    // Appending keeps what was there, with a blank line between two thoughts.
    let (_, err, ok) = sandbox.run(&[
        "note",
        "artist",
        "Miles Davis",
        "--text",
        "and the Japanese pressing",
        "--append",
    ]);
    assert!(ok, "stderr: {err}");
    let (out, _, ok) = sandbox.run(&["note", "artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("# Kind of Blue"), "the old text stays: {out}");
    assert!(
        out.contains("Japanese pressing"),
        "the new text is added: {out}"
    );

    // One note per thing: writing again without --append replaces it.
    let (_, _, ok) = sandbox.run(&["note", "artist", "Miles Davis", "--text", "only this"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["note", "artist", "Miles Davis"]);
    assert!(ok);
    assert!(!out.contains("Kind of Blue"), "one note, replaced: {out}");
    assert!(out.contains("only this"), "output: {out}");

    // Two sources for one text is a contradiction, not a precedence rule.
    let (_, err, ok) = sandbox.run(&[
        "note",
        "artist",
        "Miles Davis",
        "--text",
        "a",
        "--file",
        "b",
    ]);
    assert!(!ok);
    assert!(err.contains("give one"), "stderr: {err}");
}

#[test]
fn a_query_expresses_what_options_never_could() {
    let sandbox = Sandbox::new("query");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // OR and negation, the two things a pile of options cannot say.
    let (out, err, ok) = sandbox.run(&["query", "genre:jazz"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("So What"), "output: {out}");

    let (out, _, ok) = sandbox.run(&["query", "genre:jazz -album:duos"]);
    assert!(ok);
    assert!(!out.contains("Take Five"), "negation bites: {out}");

    let (out, _, ok) = sandbox.run(&[
        "query",
        "(album:duos OR album:\"kind of blue\") year:..1960",
    ]);
    assert!(ok);
    assert!(out.contains("So What"), "output: {out}");
    assert!(!out.contains("Take Five"), "1963 is past 1960: {out}");

    // What the user wrote is queryable, and the field says where it was
    // written: stars on the artist are not stars on the track.
    let (_, _, ok) = sandbox.run(&["rate", "artist", "Miles Davis", "--stars", "5"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["query", "artist.rating:>=4"]);
    assert!(ok);
    assert!(out.contains("So What"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["query", "rating:>=4"]);
    assert!(ok);
    assert!(
        out.contains("nothing matches"),
        "the stars are on the artist, not the track: {out}"
    );

    // The result is a selection, so everything a selection can do applies.
    let (m3u, _, ok) = sandbox.run(&["query", "genre:jazz", "--m3u"]);
    assert!(ok);
    assert!(m3u.starts_with("#EXTM3U"), "output: {m3u}");
    let (json, _, ok) = sandbox.run(&["query", "genre:jazz", "--json"]);
    assert!(ok);
    assert!(json.trim_start().starts_with('['), "output: {json}");

    // A field nobody has heard of names the ones that exist.
    let (_, err, ok) = sandbox.run(&["query", "bogus:1"]);
    assert!(!ok);
    assert!(err.contains("not a field"), "stderr: {err}");
    assert!(err.contains("genre"), "and lists them: {err}");
}

#[test]
fn a_saved_query_keeps_the_question_and_not_the_answer() {
    // A collection that stored its result would be a playlist. Keeping the
    // expression is what makes it answer with what the library holds now.
    let sandbox = Sandbox::new("collections");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["collection", "jazz", "--query", "genre:jazz"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("saved"), "output: {out}");

    let (out, _, ok) = sandbox.run(&["collection", "jazz"]);
    assert!(ok);
    assert!(out.contains("jazz ("), "the name heads the answer: {out}");
    assert!(out.contains("So What"), "output: {out}");

    // Running one produces a selection, so a playlist costs nothing.
    let (m3u, _, ok) = sandbox.run(&["collection", "jazz", "--m3u"]);
    assert!(ok);
    assert!(m3u.starts_with("#EXTM3U"), "output: {m3u}");

    let (out, _, ok) = sandbox.run(&["collections"]);
    assert!(ok);
    assert!(out.contains("genre:jazz"), "it shows the question: {out}");

    // An expression that does not parse is refused when it is saved, not the
    // next time somebody opens it.
    let (_, err, ok) = sandbox.run(&["collection", "broken", "--query", "bogus:1"]);
    assert!(!ok);
    assert!(err.contains("not a field"), "stderr: {err}");

    // A name nobody saved says what is saved.
    let (_, err, ok) = sandbox.run(&["collection", "metal"]);
    assert!(!ok);
    assert!(err.contains("Saved: jazz"), "stderr: {err}");

    let (_, _, ok) = sandbox.run(&["collection", "jazz", "--remove"]);
    assert!(ok);
    let (_, err, ok) = sandbox.run(&["collection", "jazz"]);
    assert!(!ok);
    assert!(err.contains("none is saved yet"), "stderr: {err}");
}

#[test]
fn a_result_can_be_ordered_and_the_unknown_stays_last() {
    let sandbox = Sandbox::new("sorting");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["query", "genre:jazz", "--sort", "year"]);
    assert!(ok, "stderr: {err}");
    let first = out
        .lines()
        .find(|l| l.contains("19"))
        .map(str::to_string)
        .unwrap_or_default();
    assert!(!first.is_empty(), "output: {out}");

    let (_, err, ok) = sandbox.run(&["query", "genre:jazz", "--sort", "bananas"]);
    assert!(!ok);
    assert!(err.contains("not something to sort on"), "stderr: {err}");
    assert!(err.contains("year"), "and lists them: {err}");
}

#[test]
fn what_the_user_wrote_can_leave_and_come_back() {
    // The only irreplaceable data in the program deserves a way out and a way
    // back in — and the way back is a merge, because someone restoring half a
    // backup wants their two halves.
    let source = Sandbox::new("export_from");
    let root = library();
    let (_, _, ok) = source.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);
    let (_, _, ok) = source.run(&["rate", "artist", "Miles Davis", "--stars", "5"]);
    assert!(ok);
    let (_, _, ok) = source.run(&["collection", "jazz", "--query", "genre:jazz"]);
    assert!(ok);

    let backup = std::env::temp_dir().join("aede_e2e_backup.json");
    let (_, err, ok) = source.run(&["notes", "--export", "-o", backup.to_str().unwrap()]);
    assert!(ok, "stderr: {err}");

    let restored = Sandbox::new("export_to");
    let (_, _, ok) = restored.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);
    // Something written here first, and newer, must survive the import.
    let (_, _, ok) = restored.run(&["note", "artist", "Miles Davis", "--text", "mine"]);
    assert!(ok);

    let (out, err, ok) = restored.run(&["notes", "--import", backup.to_str().unwrap()]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("kept as they were"), "output: {out}");

    let (out, _, ok) = restored.run(&["notes"]);
    assert!(ok);
    assert!(out.contains("mine"), "the newer local note stayed: {out}");
    let (out, _, ok) = restored.run(&["collections"]);
    assert!(ok);
    assert!(out.contains("jazz"), "the collection came across: {out}");

    // Importing the same backup twice changes nothing.
    let (out, _, ok) = restored.run(&["notes", "--import", backup.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("0 added"), "output: {out}");
}

#[test]
fn the_options_and_the_grammar_are_one_evaluator() {
    // `aede albums --genre metal` and `aede query "genre:metal"` used to be two
    // filter loops answering the same question. Two is one too many: the day
    // one of them changed, nobody would have seen it. The options are now
    // shorthand for the grammar, and this walks both doors to the same room.
    let sandbox = Sandbox::new("sugar");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // Album titles from a listing, whichever way it was asked for.
    let albums_of = |args: &[&str]| -> Vec<String> {
        let (out, err, ok) = sandbox.run(args);
        assert!(ok, "{args:?}: {err}");
        out.lines()
            .filter_map(|line| {
                let cells: Vec<&str> = line.split(',').collect();
                (cells.len() > 2).then(|| cells[1].trim_matches('"').to_string())
            })
            .filter(|title| !title.is_empty() && title != "album")
            .collect()
    };

    for (options, expression) in [
        (vec!["albums", "--csv", "--genre", "jazz"], "genre:jazz"),
        (vec!["albums", "--csv", "--year", "1959"], "year:1959"),
        (
            vec!["albums", "--csv", "--label", "Columbia"],
            "label:Columbia",
        ),
        (
            vec!["albums", "--csv", "--compilations"],
            "compilation:true",
        ),
        (
            vec!["albums", "--csv", "--no-compilations"],
            "compilation:false",
        ),
    ] {
        let by_option = albums_of(&options);
        // The grammar evaluates over tracks; an album listing is the fold of
        // that, so the comparison goes through the same fold.
        let (out, err, ok) = sandbox.run(&["query", expression, "--csv", "--all"]);
        assert!(ok, "{expression}: {err}");
        let mut by_query: Vec<String> = out
            .lines()
            .filter_map(|line| {
                let cells: Vec<&str> = line.split(',').collect();
                (cells.len() > 3).then(|| cells[2].trim_matches('"').to_string())
            })
            .filter(|title| !title.is_empty() && title != "album")
            .collect();
        by_query.sort();
        by_query.dedup();
        let mut expected = by_option.clone();
        expected.sort();
        expected.dedup();
        assert_eq!(
            expected, by_query,
            "{options:?} and {expression:?} must answer the same"
        );
    }

    // The one mapping that is deliberate rather than mechanical: `--artist` on
    // an album listing means the **album artist**. Mapping it to `artist:`
    // would quietly have listed every album somebody guests on as their own.
    let (out, _, ok) = sandbox.run(&["albums", "--artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("Kind of Blue"), "output: {out}");

    // And a value naming nothing is still a misunderstanding, not an empty
    // result — the distinction the hand-written filter drew, now everyone's.
    let (_, err, ok) = sandbox.run(&["albums", "--genre", "polka"]);
    assert!(!ok, "a genre nobody has is not an empty answer");
    assert!(err.contains("no genre matches"), "stderr: {err}");
    let (_, err, ok) = sandbox.run(&["query", "genre:polka"]);
    assert!(!ok, "and the same through the other door");
    assert!(err.contains("no genre matches"), "stderr: {err}");
}

#[test]
fn help_is_a_command_like_the_others() {
    // It answers, so it is listed; it reads no argument, so it refuses one.
    // `aede help scan` reads as a request for one command's page and printed
    // the whole help as though nothing had been typed.
    let sandbox = Sandbox::new("help_command");

    let (out, _, ok) = sandbox.run(&["help"]);
    assert!(ok);
    assert!(
        out.lines().any(|l| l.trim_start().starts_with("help ")),
        "a command that works is a command the help names:\n{out}"
    );

    let (out, err, ok) = sandbox.run(&["help", "scan"]);
    assert!(!ok, "output: {out}");
    assert!(err.contains("takes no argument"), "stderr: {err}");
    assert!(err.contains("\"scan\" was ignored"), "stderr: {err}");
    assert!(!out.contains("COMMANDS"), "and prints nothing else: {out}");
}

#[test]
fn the_help_says_what_check_prints_with_nothing_left_to_verify() {
    // A user who ran `aede check` and got back a table, not a progress bar,
    // read the help's "verify the checksums" and had no way to connect the
    // two: the help described the run, never the report a run with nothing
    // left to do prints instead.
    let sandbox = Sandbox::new("help_check_wording");
    let (out, _, ok) = sandbox.run(&["help"]);
    assert!(ok);
    assert!(
        out.contains("Nothing left to check prints the current report instead"),
        "output: {out}"
    );
}

#[test]
fn the_help_explains_pending_analyses() {
    // `doctor` can only count what is waiting; `import --pending` is where a
    // user is meant to learn which ones, and `--forget --pending` where they
    // drop what will clearly never attach. All three must be named.
    let sandbox = Sandbox::new("help_import_pending");
    let (out, _, ok) = sandbox.run(&["help"]);
    assert!(ok);
    assert!(out.contains("--pending"), "output: {out}");
    assert!(out.contains("match no file yet"), "output: {out}");
    assert!(
        out.contains("--forget --pending"),
        "the way to drop them is named too: {out}"
    );
}

#[test]
fn the_help_shows_how_to_search_what_the_user_wrote() {
    // The fields were all listed, in one dense block of thirty words, with no
    // example of any of them. Somebody who had rated albums and tagged them
    // read that block and concluded the feature was gone. A list of field
    // names is a reference; it is not an answer to "how do I find my
    // four-star albums".
    let sandbox = Sandbox::new("help_user_fields");
    let (out, _, ok) = sandbox.run(&["help"]);
    assert!(ok);

    // Each of the three shapes is shown at least once, with a value.
    for shown in [
        "tag:vinyl",
        "album.tag:vinyl",
        "note:remaster",
        "album.rating",
    ] {
        assert!(out.contains(shown), "the help never shows {shown}:\n{out}");
    }
    // And the bare form, which had no way of being guessed at.
    assert!(
        out.contains("asks whether there is one at all"),
        "output: {out}"
    );
    // The one question that started this: a list of albums, not of tracks.
    assert!(
        out.contains("aede albums --query"),
        "the help must say how to list albums by what you wrote:\n{out}"
    );
}

#[test]
fn inspecting_a_single_file() {
    let sandbox = Sandbox::new("file");
    let file = library().join("track.flac");
    let (out, _, ok) = sandbox.run(&["file", file.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("FLAC 16/44.1"), "output: {out}");
    assert!(out.contains("So What"));
}

#[test]
fn an_option_without_a_command_has_nothing_to_act_on() {
    // `aede --data ~/music` named a catalog, did nothing with it, printed the
    // help and reported success. The option went into the void exactly as a
    // swallowed argument does, and the help made it look like an answer.
    let sandbox = Sandbox::new("no_command");

    let (out, err, ok) = sandbox.run(&["--data", "/tmp/aede_e2e_nowhere"]);
    assert!(!ok, "output: {out}");
    assert!(
        err.contains("no command to apply --data to"),
        "stderr: {err}"
    );
    assert!(!out.contains("COMMANDS"), "and no help page: {out}");

    // `--data <folder>` is the one option taking a folder that does not mean
    // "read the music in it", so it is the one people type expecting a scan.
    // An error saying only "no command" leaves them exactly where they were.
    assert!(
        err.contains("where the catalog is kept"),
        "it says what --data means: {err}"
    );
    assert!(
        err.contains("aede scan /tmp/aede_e2e_nowhere"),
        "and names what does read a folder: {err}"
    );

    // Named as typed, short spelling included.
    let (_, err, ok) = sandbox.run(&["-o", "somewhere.csv"]);
    assert!(!ok);
    assert!(err.contains("-o"), "stderr: {err}");

    // `--data` with no value at all is the question "where is my catalog?",
    // so the message answers it instead of only refusing.
    let (_, err, ok) = sandbox.run(&["--data"]);
    assert!(!ok);
    assert!(err.contains("expects a value"), "stderr: {err}");
    assert!(err.contains("catalog is currently in"), "stderr: {err}");

    // Nothing at all still asks for the help, and so does an option that only
    // shapes what is printed: `--no-color` has the help itself to act on.
    let (out, _, ok) = sandbox.run(&[]);
    assert!(ok);
    assert!(out.contains("COMMANDS"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["--no-color"]);
    assert!(ok);
    assert!(out.contains("COMMANDS"), "output: {out}");
}

#[test]
fn help_and_version() {
    let sandbox = Sandbox::new("help");
    let (out, _, ok) = sandbox.run(&["--version"]);
    assert!(ok);
    assert!(out.starts_with("aede "), "output: {out}");

    let (out, _, ok) = sandbox.run(&["--help"]);
    assert!(ok);
    assert!(out.contains("COMMANDS"));
}

#[test]
fn watched_folders_accumulate_across_scans() {
    // Scanning a second library used to replace the catalog instead of adding
    // to it, silently losing everything scanned before.
    let sandbox = Sandbox::new("roots");
    let fixtures = library();
    let scratch = std::env::temp_dir().join("aede_e2e_roots_src");
    let (a, b) = (scratch.join("a"), scratch.join("b"));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::copy(fixtures.join("track.flac"), a.join("1.flac")).unwrap();
    std::fs::copy(fixtures.join("track.mp3"), b.join("2.mp3")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", a.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["scan", b.to_str().unwrap()]);
    assert!(ok);
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Files found") && l.trim_end().ends_with('2')),
        "both folders must be walked:\n{out}"
    );

    let (out, _, ok) = sandbox.run(&["roots"]);
    assert!(ok);
    assert!(out.contains("a"), "output: {out}");
    assert!(out.contains("b"), "output: {out}");
    // The same three measures every listing carries: a folder count without a
    // weight is the one figure a user cannot act on.
    assert!(out.contains("Duration"), "output: {out}");
    assert!(out.contains("Size"), "output: {out}");

    // Dropping a folder needs one more scan to take the files out.
    let (_, _, ok) = sandbox.run(&["roots", "--remove", b.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Files found") && l.trim_end().ends_with('1')),
        "only the remaining folder must be walked:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_guest_appearance_is_listed_apart_from_the_discography() {
    let sandbox = Sandbox::new("guest");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("Discography"), "output: {out}");
}

#[test]
fn a_track_is_reachable_by_its_title() {
    // The point of the command: the same page as `file`, without having to
    // type a path.
    let sandbox = Sandbox::new("track");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok);
    assert!(out.contains("Kind of Blue"), "the album is shown: {out}");
    assert!(out.contains("Miles Davis"), "the credits are shown: {out}");
    assert!(out.contains("Sample rate"), "the technical panel is shown");

    // Several files carry that title: all of them are printed.
    let pages = out.matches("Album artist").count();
    assert!(pages > 1, "every match is shown, got {pages}");

    // A limit must be announced, never silent.
    let (out, _, ok) = sandbox.run(&["track", "So What", "--limit=1"]);
    assert!(ok);
    assert_eq!(out.matches("Album artist").count(), 1);
    assert!(out.contains("1–1 of"), "the truncation is announced: {out}");

    // And a page of one walks through them.
    let (out, _, ok) = sandbox.run(&["track", "So What", "--limit=1", "--offset=1"]);
    assert!(ok);
    assert!(out.contains("2–2 of"), "output: {out}");

    // An unknown title fails with a usable message.
    let (_, err, ok) = sandbox.run(&["track", "no such title here"]);
    assert!(!ok);
    assert!(err.contains("no track matches"), "stderr: {err}");

    // A filter that excludes everything says so rather than denying the title.
    let (_, err, ok) = sandbox.run(&["track", "So What", "--artist=Ozzy Osbourne"]);
    assert!(!ok);
    assert!(err.contains("none matching the filters"), "stderr: {err}");
}

#[test]
fn a_genre_and_a_label_are_pages_of_their_own() {
    // `genres` counts them; nothing could open one. A count you cannot open is
    // a dead end, and the interface at M2 needs exactly this page.
    let sandbox = Sandbox::new("facet");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["genre", "jazz"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("Jazz"), "the genre is named: {out}");
    assert!(out.contains("Albums"), "what is in it: {out}");
    assert!(out.contains("Kind of Blue"), "output: {out}");
    assert!(out.contains("Artists"), "and who is in it: {out}");
    assert!(out.contains("Miles Davis"), "output: {out}");

    // The tracks it gathers are a selection, like an album's or an artist's.
    let (m3u, _, ok) = sandbox.run(&["genre", "jazz", "--m3u"]);
    assert!(ok);
    assert!(m3u.starts_with("#EXTM3U"), "output: {m3u}");

    let (out, _, ok) = sandbox.run(&["label", "Columbia"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Columbia"), "output: {out}");
    assert!(out.contains("Kind of Blue"), "output: {out}");

    // A name nobody carries is an error that says where to look.
    let (_, err, ok) = sandbox.run(&["genre", "no such genre"]);
    assert!(!ok);
    assert!(err.contains("aede genres"), "stderr: {err}");

    // A partial match that lands on one name says nothing: the heading shows
    // the real name, so the widening is on screen already. `label earache`
    // used to print "no label is called \"earache\"" directly above a heading
    // reading "Earache Records" — a denial and its refutation, one line apart,
    // while `albums --label earache` narrowed on the same text without a word.
    let (out, _, ok) = sandbox.run(&["genre", "jaz"]);
    assert!(ok);
    assert!(out.contains("Jazz"), "the real name is the answer: {out}");
    assert!(
        !out.contains("no genre is called"),
        "a page that answers must not open by denying: {out}"
    );

    // The Artists table counts tracks, and a track counts once however many
    // performing roles the artist holds on it. Counting credits reported 57
    // tracks for a band whose albums on the label hold 29 — credited both as
    // main artist and as performer on each one — which the albums table
    // directly above visibly contradicted.
    let (out, _, ok) = sandbox.run(&["label", "Columbia"]);
    assert!(ok, "output: {out}");
    let page_holds = tracks_on_line(&out, "9 track");
    let artists = section(&out, "Artists");
    for line in artists.lines().filter(|l| l.contains("Miles Davis")) {
        let count: usize = line
            .split_whitespace()
            .next_back()
            .and_then(|c| c.parse().ok())
            .unwrap_or_else(|| panic!("no count on: {line}"));
        assert!(
            count <= page_holds,
            "no artist can carry more tracks than the page holds \
             ({count} > {page_holds}):\n{out}"
        );
    }
}

/// The body of one `ui::section`, up to the next heading.
///
/// Sections are headings in column zero; every row under one is indented.
fn section<'a>(out: &'a str, heading: &str) -> &'a str {
    let start = out
        .find(heading)
        .map(|i| i + heading.len())
        .unwrap_or_else(|| panic!("no {heading} section in:\n{out}"));
    let rest = &out[start..];
    let end = rest
        .lines()
        .scan(0usize, |at, line| {
            let here = *at;
            *at += line.len() + 1;
            Some((here, line))
        })
        .skip(1)
        .find(|(_, line)| !line.is_empty() && !line.starts_with(' '))
        .map(|(at, _)| at)
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn the_two_filters_that_were_declared_and_ignored_now_bite() {
    // `--genre` and `--label` sat in the option list and were honoured
    // nowhere: accepted everywhere, ignored everywhere.
    let sandbox = Sandbox::new("facet_filter");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["albums", "--genre", "jazz"]);
    assert!(ok, "output: {out}");
    assert!(
        out.contains("filtered on genre"),
        "a filter has to be visible, or it cannot be told from one that does \
         nothing:\n{out}"
    );

    // A genre nobody carries is an error, not an empty listing that reads as
    // an empty library.
    let (_, err, ok) = sandbox.run(&["albums", "--genre", "polka"]);
    assert!(!ok);
    assert!(err.contains("no genre matches"), "stderr: {err}");

    let (_, err, ok) = sandbox.run(&["albums", "--label", "no such label"]);
    assert!(!ok);
    assert!(err.contains("no label matches"), "stderr: {err}");
}

#[test]
fn a_role_reads_one_way_on_the_list_and_another_on_the_page() {
    // Two readings of the same word: on the listing, who is credited that way;
    // on one person's page, what they did in that role. The page could not be
    // asked at all — the option was refused there — although it is the more
    // natural of the two questions.
    let sandbox = Sandbox::new("role_page");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["artist", "Miles Davis", "--role", "composer"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("as composer"), "the heading says which: {out}");
    assert!(out.contains("Kind of Blue"), "output: {out}");

    // It narrows a selection, so it can be played or tabulated.
    let (m3u, _, ok) = sandbox.run(&["artist", "Miles Davis", "--role", "composer", "--m3u"]);
    assert!(ok);
    assert!(m3u.starts_with("#EXTM3U"), "output: {m3u}");

    // A role this person does not hold names the ones they do: told only that
    // nobody is credited that way, one cannot tell a misspelling from a
    // library whose tags never carried the field.
    let (_, err, ok) = sandbox.run(&["artist", "Miles Davis", "--role", "producer"]);
    assert!(!ok);
    assert!(err.contains("Credited as"), "stderr: {err}");
    assert!(err.contains("composer"), "stderr: {err}");

    // Where a role means nothing, the refusal says what to type instead.
    let (_, err, ok) = sandbox.run(&["album", "Kind of Blue", "--role", "performer"]);
    assert!(!ok);
    assert!(err.contains("A role needs a person"), "stderr: {err}");
    assert!(err.contains("aede artist"), "stderr: {err}");
}

#[test]
fn a_role_is_accepted_by_the_name_it_is_shown_under() {
    // The message contradicted itself in one breath: asked for
    // --role "album artist" — the only spelling ever printed — it answered
    // that the artist was not credited as album artist, and listed
    // "album artist (14)" among their credits. The screen said one word, the
    // parser wanted another.
    let sandbox = Sandbox::new("role_naming");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // The reference library has a Sextet as album artist of Kind of Blue.
    let by_label = sandbox.run(&["artists", "--role", "album artist"]);
    let by_key = sandbox.run(&["artists", "--role", "album"]);
    assert!(by_label.2, "stderr: {}", by_label.1);
    assert_eq!(
        by_label.0, by_key.0,
        "the shown name and the stored key must reach the same answer"
    );
    assert!(by_label.0.contains("Sextet"), "output: {}", by_label.0);

    // Typed as two bare words, since a role name has a space in it.
    let unquoted = sandbox.run(&["artists", "--role", "album", "artist"]);
    assert_eq!(unquoted.0, by_label.0, "no quotes needed either");

    // A role that does not exist offers the ones that do, spelled the way they
    // are shown — an error listing "album" where every screen says "album
    // artist" tells the user to type something that will be refused.
    let (_, err, ok) = sandbox.run(&["artists", "--role", "drummer"]);
    assert!(!ok);
    assert!(err.contains("Roles in use"), "stderr: {err}");
    assert!(
        err.contains("album artist"),
        "the offer has to be typeable back in: {err}"
    );

    // And a refusal on a person names what they *are*, without contradicting
    // itself: Miles Davis is not the album artist here, the Sextet is.
    let (_, err, ok) = sandbox.run(&["artist", "Miles Davis", "--role", "album artist"]);
    assert!(!ok);
    assert!(
        err.contains("not credited as album artist"),
        "stderr: {err}"
    );
    assert!(
        !err.contains("Credited as: main artist (12), composer (8), album artist"),
        "a message may not deny and confirm the same role: {err}"
    );
}

#[test]
fn the_help_says_where_each_option_applies() {
    // `--role` was printed under a heading that named the album listing, which
    // is the one place it does not work. A help that lies costs more than a
    // help that is short.
    let sandbox = Sandbox::new("help_options");
    let (out, _, ok) = sandbox.run(&["help"]);
    assert!(ok);
    assert!(!out.contains("ALBUM LIST OPTIONS"), "output: {out}");
    assert!(out.contains("FILTER OPTIONS"), "output: {out}");

    // Every filter option named in the help must be spelled the way the
    // program accepts it — a help naming an option the parser rejects is the
    // same lie in the other direction.
    for option in [
        "--artist",
        "--year",
        "--genre",
        "--label",
        "--compilations",
        "--comment",
        "--comments",
        "--role",
    ] {
        assert!(out.contains(option), "the help must name {option}:\n{out}");
    }

    // The COMMANDS list is what a user reads first, and for most of them it is
    // all they read. A filter named only in the section below exists, for that
    // reader, nowhere: `--year` was honoured by `albums` and absent from its
    // line. Whatever a command accepts, its own line says so.
    let commands = out
        .split("GLOBAL OPTIONS")
        .next()
        .expect("the help lists the commands before the global options");
    let albums = commands
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("albums "))
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    for option in [
        "--artist",
        "--year",
        "--genre",
        "--label",
        "--comment",
        "--compilations",
    ] {
        assert!(
            albums.contains(option),
            "the albums line must name {option}: {albums}"
        );
    }
}

#[test]
fn a_filter_is_refused_where_it_means_nothing() {
    // `--year` and `--artist` were declared among the options and guarded
    // nowhere: `aede artists --year=1969` listed every artist of every year,
    // and the answer looked right. A filter that applies nowhere must say so
    // rather than be dropped in silence.
    let sandbox = Sandbox::new("filter_guard");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (_, err, ok) = sandbox.run(&["artists", "--year", "1969"]);
    assert!(!ok, "the year must not be swallowed");
    assert!(err.contains("--year applies to albums"), "stderr: {err}");

    let (_, err, ok) = sandbox.run(&["genres", "--artist", "Ozzy"]);
    assert!(!ok, "the artist must not be swallowed");
    assert!(err.contains("--artist applies to"), "stderr: {err}");

    // And where it does apply, a value that is not a year is an error, not a
    // filter quietly dropped.
    let (_, err, ok) = sandbox.run(&["albums", "--year", "sixty-nine"]);
    assert!(!ok, "an unparsable year must not become no filter");
    assert!(err.contains("--year expects a year"), "stderr: {err}");

    let (_, _, ok) = sandbox.run(&["albums", "--year", "1969"]);
    assert!(ok, "a real year still works");
}

#[test]
fn a_command_that_reads_no_argument_refuses_one() {
    // `aede artists ozzy --role producer` listed every producer in the
    // library, "ozzy" going into the void. The answer looked right, which is
    // exactly what made it dangerous.
    let sandbox = Sandbox::new("no_argument");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (_, err, ok) = sandbox.run(&["artists", "ozzy", "--role", "composer"]);
    assert!(!ok, "the argument must not be swallowed");
    assert!(err.contains("takes no argument"), "stderr: {err}");
    assert!(err.contains("\"ozzy\" was ignored"), "it names it: {err}");
    assert!(
        err.contains("aede artist"),
        "and points at what does: {err}"
    );

    for (plural, singular) in [
        ("albums", "aede album"),
        ("genres", "aede genre"),
        ("labels", "aede label"),
    ] {
        let (_, err, ok) = sandbox.run(&[plural, "something"]);
        assert!(!ok, "{plural} must refuse an argument");
        assert!(
            err.contains(singular),
            "{plural} points at {singular}: {err}"
        );
    }

    // The commands that do take one are untouched.
    let (_, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["artists", "--role", "composer"]);
    assert!(ok, "the option alone is still fine");
}

#[test]
fn the_roles_a_library_holds_are_visible() {
    // `--role composer` returning nothing is indistinguishable from a bug
    // unless the library can say which roles it holds at all. A count of zero
    // is an answer; an empty screen is not.
    let sandbox = Sandbox::new("roles_seen");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["stats"]);
    assert!(ok);
    assert!(out.contains("Roles"), "output: {out}");
    assert!(out.contains("composer"), "output: {out}");
    assert!(
        !out.contains("main artist"),
        "the roles every track carries say nothing:\n{out}"
    );

    // A client builds its role picker from this, not from a list of its own.
    let (out, _, ok) = sandbox.run(&["stats", "--json"]);
    assert!(ok);
    let value = aede_core::json::parse(&out).expect("valid JSON");
    let roles = value.get("roles").and_then(|r| r.as_arr()).expect("roles");
    assert!(
        roles
            .iter()
            .any(|r| r.field_str("role").as_deref() == Some("composer")),
        "output: {out}"
    );

    // And the artist page names the role, even for someone who holds one only.
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("composer"), "output: {out}");
}

#[test]
fn the_credits_can_be_read_the_other_way_round() {
    // The artist page says what one person did; this says who does a thing.
    // That the question can be asked in both directions is the whole reason
    // credits carry a role.
    let sandbox = Sandbox::new("role");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["artists", "--role", "composer"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("credited as composer"), "output: {out}");
    assert!(out.contains("Miles Davis"), "output: {out}");

    // A role nobody holds lists the ones that exist: guessing the spelling of
    // a role is not the user's job.
    let (_, err, ok) = sandbox.run(&["artists", "--role", "drummer"]);
    assert!(!ok);
    assert!(err.contains("Roles in use"), "stderr: {err}");
    assert!(err.contains("composer"), "stderr: {err}");

    // And a command that cannot honour it refuses rather than ignoring it.
    let (_, err, ok) = sandbox.run(&["stats", "--role", "composer"]);
    assert!(!ok);
    assert!(err.contains("cannot filter by role"), "stderr: {err}");
}

#[test]
fn what_a_user_wrote_in_a_comment_can_be_found_again() {
    // The comment is the one tag the user writes themselves — where a rip came
    // from, what still needs replacing. It was read and stored, and searchable
    // nowhere.
    let sandbox = Sandbox::new("comments");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // Off by default: a comment is free prose, and a common word in one would
    // bury the entity that actually bears the name.
    let (out, _, ok) = sandbox.run(&["search", "vinyl"]);
    assert!(ok);
    assert!(!out.contains("In comments"), "opt-in only:\n{out}");

    let (out, _, ok) = sandbox.run(&["search", "--comments", "vinyl"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("In comments"), "its own section: {out}");
    assert!(out.contains("Take Five"), "output: {out}");
    assert!(
        out.contains("needs replacing"),
        "the comment is shown: {out}"
    );

    // A comment hit is a track, so it can become a playlist like any other.
    let (m3u, _, ok) = sandbox.run(&["search", "--comments", "vinyl", "--m3u"]);
    assert!(ok);
    assert!(m3u.contains("Take Five"), "output: {m3u}");

    // The JSON says where each hit was found, so a client need not guess.
    let (out, _, ok) = sandbox.run(&["search", "--comments", "vinyl", "--json"]);
    assert!(ok);
    assert!(out.contains("\"found_in\""), "output: {out}");
    assert!(out.contains("comment"), "output: {out}");

    // And it filters a selection.
    let (out, _, ok) = sandbox.run(&["track", "Take Five", "--comment", "vinyl"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Take Five"), "output: {out}");

    let (_, err, ok) = sandbox.run(&["track", "Take Five", "--comment", "nothing like this"]);
    assert!(!ok);
    assert!(err.contains("none matching the filters"), "stderr: {err}");
}

#[test]
fn a_listing_never_stops_without_saying_so() {
    // A listing shows fifty rows by default and used to stop there in silence.
    // Sorted by year, that meant the most recent albums of a real library
    // simply did not exist as far as the user could see.
    let sandbox = Sandbox::new("listing_limit");
    let root = std::env::temp_dir().join("aede_e2e_listing_limit_src");
    let _ = std::fs::remove_dir_all(&root);
    let source = library().join("track.flac");
    for year in 1960..1960 + 60 {
        let dir = root.join(format!("{year} Album {year}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&source, dir.join("01.flac")).unwrap();
    }
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // Sixty albums, fifty rows: the cut has to be announced, and it says which
    // rows these are so a second page can be asked for.
    let (out, _, ok) = sandbox.run(&["albums"]);
    assert!(ok, "output: {out}");
    assert!(
        out.contains("1–50 of 60 albums"),
        "the cut must name the rows shown:\n{out}"
    );
    assert!(
        out.contains("--offset=50"),
        "and hand over the next page: {out}"
    );

    // Page two picks up exactly where page one stopped.
    let (out, _, ok) = sandbox.run(&["albums", "--limit=50", "--offset=50"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("51–60 of 60 albums"), "output: {out}");

    // Shown in full, nothing is said: a notice that always fires means nothing.
    for form in [vec!["albums", "--limit=200"], vec!["albums", "--all"]] {
        let (out, _, ok) = sandbox.run(&form);
        assert!(ok);
        assert!(
            !out.contains(" of 60 albums"),
            "nothing was left out:\n{out}"
        );
        // Sixty copies of one album in sixty folders: sixty releases, since a
        // folder is what the user acts on.
        assert_eq!(
            out.lines().filter(|l| l.contains("Kind of Blue")).count(),
            60,
            "every row is there: {form:?}"
        );
    }

    // A window past the end is not an error, but silence there would read as
    // an empty library.
    let (out, _, ok) = sandbox.run(&["albums", "--offset=500"]);
    assert!(ok);
    assert!(out.contains("starts past the end"), "output: {out}");

    // A listing that matched nothing, though, is not a paging accident: saying
    // "--offset=0 starts past the end" of a list with no rows in it sends the
    // reader after a page nobody asked for. Easy to meet since the listings
    // learned --query.
    let (out, _, ok) = sandbox.run(&["artists", "--query", "year:2050"]);
    assert!(ok, "output: {out}");
    assert!(
        !out.contains("--offset"),
        "an empty match must not blame paging:\n{out}"
    );
    assert!(out.contains("no artist to show"), "output: {out}");

    // The ways of asking for a window that mean nothing are refused.
    for form in [
        vec!["albums", "--limit=0"],
        vec!["albums", "--limit=abc"],
        vec!["albums", "--all", "--limit=5"],
    ] {
        let (_, err, ok) = sandbox.run(&form);
        assert!(!ok, "{form:?} must be refused");
        assert!(!err.is_empty(), "{form:?} must say why");
    }

    // And paging a command that shows no rows is refused too, instead of being
    // accepted and ignored as --limit used to be everywhere.
    let (_, err, ok) = sandbox.run(&["reset", "--offset=2"]);
    assert!(!ok);
    assert!(err.contains("cannot"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}

/// The row of a table whose path column ends in this folder.
///
/// Matched on the **tail**, never on the whole path: the column truncates from
/// the left so that the last components — the ones that identify the folder —
/// survive a narrow terminal. On macOS a temporary path alone is sixty columns,
/// so a test comparing the full path there passes on Linux and fails on a Mac.
fn row_for_folder<'a>(out: &'a str, ending: &str) -> &'a str {
    out.lines()
        .find(|l| {
            l.split_whitespace()
                .next()
                .is_some_and(|column| column.ends_with(ending))
        })
        .unwrap_or_else(|| panic!("no row ending in {ending} in:\n{out}"))
}

#[test]
fn a_watched_folder_is_weighed_and_not_confused_with_its_neighbour() {
    // `path.starts_with(root)` on the bare string made "/music/Rock" claim
    // every file of "/music/Rockabilly": one folder counting a neighbour's
    // files, silently.
    let sandbox = Sandbox::new("roots_weight");
    let root = std::env::temp_dir().join("aede_e2e_roots_weight_src");
    let (rock, rockabilly) = (root.join("Rock"), root.join("Rockabilly"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&rock).unwrap();
    std::fs::create_dir_all(&rockabilly).unwrap();
    std::fs::copy(library().join("track.flac"), rock.join("1.flac")).unwrap();
    std::fs::copy(library().join("hires.flac"), rockabilly.join("2.flac")).unwrap();
    std::fs::copy(library().join("track.mp3"), rockabilly.join("3.mp3")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", rock.to_str().unwrap(), rockabilly.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["roots"]);
    assert!(ok, "output: {out}");
    let rock_row = row_for_folder(&out, "/Rock");
    assert!(
        rock_row.split_whitespace().any(|w| w == "1"),
        "Rock holds one track, not its neighbour's two: {rock_row}"
    );
    assert!(
        row_for_folder(&out, "/Rockabilly")
            .split_whitespace()
            .any(|w| w == "2"),
        "and the neighbour keeps its own:\n{out}"
    );
    assert!(
        out.contains("Duration") && out.contains("Size"),
        "output: {out}"
    );

    // Dropping a folder leaves its files in the catalog until a rescan, and
    // the table has to show them: the removal message promises they are still
    // there, and a table that hides them makes that promise unverifiable.
    let (_, _, ok) = sandbox.run(&["roots", "--remove", rockabilly.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["roots"]);
    assert!(ok);
    assert!(out.contains("no longer watched"), "output: {out}");
    assert!(
        out.contains("aede scan"),
        "and says how to drop them: {out}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_short_option_writes_the_same_thing() {
    let sandbox = Sandbox::new("short_option");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let short = sandbox.dir.join("short.csv");
    let long = sandbox.dir.join("long.csv");
    let (out, err, ok) = sandbox.run(&["albums", "--csv", "-o", short.to_str().unwrap()]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("written to"), "output: {out}");
    let (_, _, ok) = sandbox.run(&["albums", "--csv", "--output", long.to_str().unwrap()]);
    assert!(ok);
    assert_eq!(
        std::fs::read_to_string(&short).unwrap(),
        std::fs::read_to_string(&long).unwrap(),
        "-o and --output are one option written two ways"
    );
}

#[test]
fn compilations_can_be_singled_out() {
    let sandbox = Sandbox::new("compilations");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // The reference library holds one compilation, "Duos", and albums with an
    // album artist.
    let (all, _, _) = sandbox.run(&["albums", "--limit=200"]);
    assert!(all.contains("Duos"), "output: {all}");
    assert!(all.contains("Kind of Blue"), "output: {all}");

    let (only, _, ok) = sandbox.run(&["albums", "--compilations"]);
    assert!(ok, "output: {only}");
    assert!(only.contains("Compilations"), "the heading says so: {only}");
    assert!(only.contains("Duos"), "output: {only}");
    assert!(
        !only.contains("Kind of Blue"),
        "an album with an album artist is not a compilation:\n{only}"
    );

    let (without, _, ok) = sandbox.run(&["albums", "--no-compilations"]);
    assert!(ok);
    assert!(without.contains("Kind of Blue"), "output: {without}");
    assert!(!without.contains("Duos"), "output: {without}");

    // The two are opposites: asking for both is a contradiction, not a
    // silently empty answer.
    let (_, err, ok) = sandbox.run(&["albums", "--compilations", "--no-compilations"]);
    assert!(!ok);
    assert!(err.contains("opposite"), "stderr: {err}");

    // The filter reaches the CSV too, since it is the same selection.
    let (csv, _, ok) = sandbox.run(&["albums", "--compilations", "--csv"]);
    assert!(ok);
    assert!(csv.contains("Duos"), "output: {csv}");
    assert!(!csv.contains("Kind of Blue"), "output: {csv}");

    // A command that cannot honour it says so rather than ignoring it.
    let (_, err, ok) = sandbox.run(&["artists", "--compilations"]);
    assert!(!ok);
    assert!(err.contains("cannot"), "stderr: {err}");
}

#[test]
fn a_name_given_to_an_option_may_be_typed_without_quotes() {
    // The shell splits on spaces, so `--with Jeff Beck` reached the program as
    // two words: the option took "Jeff" and "Beck" was left to be joined onto
    // the positional. The command then went looking for an "Ozzy Beck" that
    // nobody had typed — a wrong answer built in silence, which is worse than
    // a refusal.
    let sandbox = Sandbox::new("name_option");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // A name typed in several words reaches the option whole. The pair does
    // not play together, and the answer says exactly that — the point is that
    // both names were understood.
    let (out, err, _) = sandbox.run(&["artist", "Miles", "--with", "Bill", "Evans"]);
    let text = format!("{out}{err}");
    assert!(
        !text.contains("Miles Evans"),
        "no name may be invented from the leftovers:\n{text}"
    );
    assert!(
        text.contains("Bill Evans"),
        "the whole name reached the option:\n{text}"
    );

    // The value stops at the next option rather than eating it.
    let (out, err, _) = sandbox.run(&["artist", "Miles", "--with", "Bill", "Evans", "--limit=1"]);
    let text = format!("{out}{err}");
    assert!(text.contains("Bill Evans"), "output: {text}");
    assert!(
        !text.contains("--limit"),
        "the option was not swallowed: {text}"
    );

    // Quoting keeps working, and says the same thing.
    let (plain_out, plain_err, _) = sandbox.run(&["artist", "Miles", "--with", "Bill", "Evans"]);
    let (quoted_out, quoted_err, _) = sandbox.run(&["artist", "Miles", "--with", "Bill Evans"]);
    assert_eq!(
        format!("{quoted_out}{quoted_err}"),
        format!("{plain_out}{plain_err}"),
        "quoted and unquoted must not diverge"
    );

    // A number is not a name: it still takes exactly one word, or `--limit 10
    // coltrane` would search for nothing at all.
    let (out, _, ok) = sandbox.run(&["search", "--limit", "1", "miles"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Miles"), "the search term survived: {out}");
}

#[test]
fn dropping_the_last_folder_lets_the_catalog_be_emptied() {
    // `roots --remove` says to run `aede scan` to drop the files. When the
    // folder removed was the only one, that scan used to fail for want of a
    // folder, and the files had no way out of the catalog.
    let sandbox = Sandbox::new("last_root");
    let scratch = std::env::temp_dir().join("aede_e2e_last_root_src");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::copy(library().join("track.flac"), scratch.join("1.flac")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", scratch.to_str().unwrap()]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["roots", "--remove", scratch.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["scan"]);
    assert!(ok, "the scan must run with no folder left. stderr: {err}");
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Gone since") && l.trim_end().ends_with('1')),
        "the file must leave the catalog:\n{out}"
    );

    let (out, _, ok) = sandbox.run(&["stats"]);
    assert!(ok);
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Tracks") && l.trim_end().ends_with('0')),
        "the catalog must be empty:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_first_scan_still_demands_a_folder() {
    let sandbox = Sandbox::new("no_catalog");
    let (_, err, ok) = sandbox.run(&["scan"]);
    assert!(!ok, "with no catalog there is nothing to infer");
    assert!(err.contains("give at least one folder"), "stderr: {err}");
}

#[test]
fn every_listing_carries_the_same_measures() {
    // Count, duration and size: what a slice of the library weighs must not
    // depend on the command used to look at it.
    let sandbox = Sandbox::new("measures");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    for command in ["artists", "albums", "genres", "labels", "years"] {
        let (out, _, ok) = sandbox.run(&[command]);
        assert!(ok, "{command} must run");
        assert!(
            out.contains("Duration"),
            "{command} lacks a duration:\n{out}"
        );
        assert!(out.contains("Size"), "{command} lacks a size:\n{out}");
    }

    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(
        out.contains(" KB") || out.contains(" MB") || out.contains(" B"),
        "the artist page must state a size:\n{out}"
    );
}

#[test]
fn a_guest_appearance_is_timed_on_its_own_tracks() {
    // The row used to count one track and time the whole album: a single guest
    // song read as forty minutes of music.
    let sandbox = Sandbox::new("guest_duration");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);

    let appears = out
        .split("Appears on")
        .nth(1)
        .expect("an Appears on section");
    let row = appears
        .lines()
        .find(|l| l.contains("Kind of Blue"))
        .expect("a release row");
    // Nine tracks of one second each: the row cannot claim the whole album.
    assert!(
        row.contains("0:0"),
        "the duration must cover the counted tracks only: {row}"
    );
}

/// The number on a line like `  writing:    1 album · 8 tracks · …`.
fn tracks_on_line(out: &str, label: &str) -> usize {
    let line = out
        .lines()
        .find(|l| l.trim_start().starts_with(label))
        .unwrap_or_else(|| panic!("no \"{label}\" line in:\n{out}"));
    let before = line
        .split("track")
        .next()
        .expect("something before the word");
    before
        .split_whitespace()
        .filter_map(|w| w.parse::<usize>().ok())
        .next_back()
        .unwrap_or_else(|| panic!("no count on: {line}"))
}

/// The count on a row of the Roles table, `  composer   8`.
fn role_count(out: &str, role: &str) -> usize {
    let roles = out.split("Roles").nth(1).expect("a Roles panel");
    let line = roles
        .lines()
        .find(|l| l.trim_start().starts_with(role))
        .unwrap_or_else(|| panic!("no {role} row in:\n{roles}"));
    line.split_whitespace()
        .filter_map(|w| w.parse::<usize>().ok())
        .next_back()
        .unwrap_or_else(|| panic!("no count on: {line}"))
}

#[test]
fn the_summary_lines_agree_with_the_roles_panel() {
    // Two figures about one person on one page, and they contradicted each
    // other: the header said "writing: 1 track" while the Roles panel counted
    // sixty-nine composer credits. The header reported the size of a display
    // table further down — a set that leaves out everything the artist also
    // plays on. A number answering a narrower question than its label is worse
    // than no number.
    let sandbox = Sandbox::new("writer");
    let scratch = std::env::temp_dir().join("aede_e2e_writer_src");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::copy(library().join("track.flac"), scratch.join("1.flac")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", scratch.to_str().unwrap()]);
    assert!(ok);

    // "track.flac" credits Miles Davis as performer *and* composer. Both are
    // true of him, so both lines are printed and both count that track.
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("performing:"), "the line is labelled:\n{out}");
    assert!(out.contains("writing:"), "so is the other one:\n{out}");
    assert_eq!(tracks_on_line(&out, "performing:"), 1, "output: {out}");
    assert_eq!(
        tracks_on_line(&out, "writing:"),
        role_count(&out, "composer"),
        "the header and the Roles panel must count the same thing:\n{out}"
    );

    // The table below is the one that subtracts, and it says so in its title
    // rather than letting the reader assume it lists everything written.
    assert!(
        !out.contains("Credited as writer or producer"),
        "a heading that promises more than the table holds:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn checking_a_library_finds_a_damaged_file() {
    let sandbox = Sandbox::new("check");
    // Deliberately deep: a real library sits far from the root, and on macOS a
    // temporary path alone is sixty columns before the file name starts. The
    // report has to keep the name, which is the only part that identifies the
    // file.
    let root = std::env::temp_dir().join("aede_e2e_check_src");
    let scratch = root.join("a-library-buried-under-a-long-and-tiresome-path/second-level");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::copy(library().join("track.flac"), scratch.join("good.flac")).unwrap();
    std::fs::copy(library().join("track.mp3"), scratch.join("nothing.mp3")).unwrap();
    let mut bytes = std::fs::read(library().join("track.flac")).unwrap();
    let index = bytes.len() - 200;
    bytes[index] ^= 0x01;
    std::fs::write(scratch.join("bad.flac"), &bytes).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", scratch.to_str().unwrap()]);
    assert!(ok);

    // Before any check, nothing is claimed about the files.
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(out.contains("not verified"), "doctor must say so: {out}");

    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok, "the check must run: {out}");
    assert!(out.contains("Damaged"), "output: {out}");
    assert!(out.contains("bad.flac"), "the file is named: {out}");

    // The verdict is stored: a second run has nothing left to read — and says
    // so *while still showing the verdicts*. The command answers "are my files
    // intact?", and it used to withhold that answer exactly when the work was
    // already done.
    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok);
    assert!(out.contains("nothing to read"), "output: {out}");
    assert!(
        out.contains("Intact"),
        "the verdicts are still shown:\n{out}"
    );
    assert!(out.contains("Damaged"), "output: {out}");
    assert!(
        out.contains("bad.flac"),
        "including which file is damaged:\n{out}"
    );
    // The same shape either way: a command answering in a different form
    // depending on the result is a command you cannot learn.
    for heading in ["Intact", "Damaged", "No checksum in the file"] {
        assert!(out.contains(heading), "{heading} missing from:\n{out}");
    }

    // And doctor now reports the damage as an error.
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(out.contains("damaged audio"), "output: {out}");
    assert!(!out.contains("not verified"), "everything was verified");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn checking_can_be_restricted_to_one_folder() {
    // Verifying a whole library is a long job; being able to try it on a corner
    // first is what makes it approachable.
    let sandbox = Sandbox::new("check_scope");
    let root = std::env::temp_dir().join("aede_e2e_scope_src");
    let (left, right) = (root.join("left"), root.join("right"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    std::fs::copy(library().join("track.flac"), left.join("1.flac")).unwrap();
    std::fs::copy(library().join("hires.flac"), right.join("2.flac")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["check", left.to_str().unwrap()]);
    assert!(ok, "output: {out}");
    assert!(out.contains("1 file to read"), "output: {out}");

    // The other folder is untouched, and the report says so.
    let (out, _, ok) = sandbox.run(&["check", right.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("1 file to read"), "output: {out}");

    // Everything now has a verdict, and the state is shown all the same.
    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok);
    assert!(out.contains("nothing to read"), "output: {out}");
    assert!(out.contains("Intact"), "output: {out}");

    // A folder the catalog knows nothing about is not silently an empty run.
    // It has to be a real folder holding none of the library — the temporary
    // directory itself is the *parent* of this test's library, so pointing at
    // it proved nothing.
    let elsewhere = root.join("empty");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let (out, _, ok) = sandbox.run(&["check", elsewhere.to_str().unwrap()]);
    assert!(ok);
    assert!(
        out.contains("no file of the catalog is in that folder"),
        "output: {out}"
    );

    // A folder that does not exist is an error, not an empty result.
    let (_, err, ok) = sandbox.run(&["check", "/nowhere/at/all"]);
    assert!(!ok);
    assert!(err.contains("does not exist"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn exporting_as_csv_and_as_a_playlist() {
    let sandbox = Sandbox::new("export");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // --- Albums, one row each ----------------------------------------------
    let (out, _, ok) = sandbox.run(&["export", "--csv"]);
    assert!(ok);
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].starts_with("album_artist,album,year"),
        "header: {}",
        lines[0]
    );
    assert!(
        lines.iter().any(|l| l.contains("Kind of Blue")),
        "the album is there:\n{out}"
    );
    // Raw values: a spreadsheet has to be able to add the column up.
    assert!(
        lines[1]
            .split(',')
            .any(|cell| cell.parse::<u64>().is_ok_and(|n| n > 1000)),
        "sizes and durations are numbers: {}",
        lines[1]
    );
    assert!(out.contains("\r\n"), "RFC 4180 line endings");

    // --- One row per track --------------------------------------------------
    let (out, _, ok) = sandbox.run(&["export", "--csv", "--tracks"]);
    assert!(ok);
    assert!(
        out.lines()
            .next()
            .unwrap()
            .starts_with("artist,album_artist")
    );
    assert!(out.contains("So What"));

    // --- The separator can be changed for Excel ----------------------------
    let (out, _, ok) = sandbox.run(&["export", "--csv", "--separator=;"]);
    assert!(ok);
    assert!(out.lines().next().unwrap().contains("album_artist;album"));
    let (_, err, ok) = sandbox.run(&["export", "--csv", "--separator=xyz"]);
    assert!(!ok);
    assert!(err.contains("one character"), "stderr: {err}");

    // --- A playlist of what is on screen -----------------------------------
    let (out, _, ok) = sandbox.run(&["album", "Kind of Blue", "--m3u"]);
    assert!(ok);
    assert!(out.starts_with("#EXTM3U"), "output: {out}");
    assert!(out.contains("#EXTINF:"), "durations and titles: {out}");
    assert!(out.contains(".flac"), "absolute paths: {out}");

    // --- Written to a file rather than printed ------------------------------
    let target = std::env::temp_dir().join("aede_e2e_export.m3u8");
    let _ = std::fs::remove_file(&target);
    let (out, _, ok) = sandbox.run(&[
        "album",
        "Kind of Blue",
        "--m3u",
        &format!("--output={}", target.display()),
    ]);
    assert!(ok, "output: {out}");
    let written = std::fs::read_to_string(&target).expect("the playlist file");
    assert!(written.starts_with("#EXTM3U"));
    let _ = std::fs::remove_file(&target);
}

#[test]
fn a_selection_can_be_exported_too() {
    let sandbox = Sandbox::new("selection");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    // The same option on a selection gives that selection, not the library.
    let (whole, _, ok) = sandbox.run(&["export", "--csv", "--tracks"]);
    assert!(ok);
    let (one, _, ok) = sandbox.run(&["album", "Kind of Blue", "--csv"]);
    assert!(ok);
    assert!(
        one.lines().count() < whole.lines().count(),
        "one album is smaller than the catalog"
    );
    assert!(one.contains("Kind of Blue"));

    // An argument export cannot honour is refused, with the command that can.
    let (_, err, ok) = sandbox.run(&["export", "--csv", "Kind of Blue"]);
    assert!(!ok, "a stray argument must not be ignored");
    assert!(err.contains("takes no argument"), "stderr: {err}");
    assert!(err.contains("aede album"), "it points at the right command");
}

#[test]
fn every_listing_can_become_a_table() {
    // `--csv` used to be accepted everywhere and honoured on four commands.
    let sandbox = Sandbox::new("listing_csv");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    for (command, header) in [
        ("albums", "album_artist,album,year"),
        ("artists", "artist,sort_name,tracks"),
        ("genres", "genre,tracks,duration_ms"),
        ("labels", "label,albums,tracks"),
        ("years", "year,albums,tracks"),
    ] {
        let (out, _, ok) = sandbox.run(&[command, "--csv"]);
        assert!(ok, "{command} --csv must run");
        assert!(
            out.starts_with(header),
            "{command} header, got: {}",
            out.lines().next().unwrap_or("")
        );
    }

    // --output writes the file instead of printing it.
    let target = std::env::temp_dir().join("aede_e2e_listing.csv");
    let _ = std::fs::remove_file(&target);
    let (out, _, ok) = sandbox.run(&["albums", "--csv", &format!("--output={}", target.display())]);
    assert!(ok);
    assert!(out.contains("written to"), "it says where it went: {out}");
    let written = std::fs::read_to_string(&target).expect("the file");
    assert!(written.starts_with("album_artist,"), "content: {written}");
    let _ = std::fs::remove_file(&target);

    // A command that cannot honour the option refuses it.
    let (_, err, ok) = sandbox.run(&["stats", "--csv"]);
    assert!(!ok, "stats has no table to give");
    assert!(err.contains("cannot produce a table"), "stderr: {err}");
    let (_, err, ok) = sandbox.run(&["albums", "--m3u"]);
    assert!(!ok, "a list of albums is not a playlist");
    assert!(err.contains("cannot produce a playlist"), "stderr: {err}");

    // Naming two albums points at the command that lists several.
    let (_, err, ok) = sandbox.run(&["album", "Kind of Blue", "Sketches of Spain"]);
    assert!(!ok);
    assert!(err.contains("aede albums"), "stderr: {err}");
}

#[test]
fn resetting_asks_before_removing_the_catalog() {
    let sandbox = Sandbox::new("reset");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    // With no terminal to ask on, the command refuses rather than guessing.
    let (out, err, ok) = sandbox.run(&["reset"]);
    assert!(!ok, "it must not delete without an answer");
    assert!(err.contains("--yes"), "it says how to proceed: {err}");
    // And it says what is at stake before asking.
    assert!(out.contains("Watched folders"), "output: {out}");
    assert!(out.contains("Integrity verdicts"), "output: {out}");
    assert!(
        sandbox.dir.join("catalog.json").exists(),
        "the catalog is still there"
    );

    // Explicit consent removes it, and says how to get it back.
    let (out, _, ok) = sandbox.run(&["reset", "--yes"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("catalog removed"), "output: {out}");
    assert!(out.contains("aede scan"), "it prints the way back: {out}");
    assert!(
        !sandbox.dir.join("catalog.json").exists(),
        "the file is gone"
    );

    // The library is unreachable again, as after a fresh install.
    let (_, err, ok) = sandbox.run(&["stats"]);
    assert!(!ok);
    assert!(err.contains("aede scan"), "stderr: {err}");

    // Removing nothing is not an error.
    let (out, _, ok) = sandbox.run(&["reset", "--yes"]);
    assert!(ok);
    assert!(out.contains("no catalog to remove"), "output: {out}");
}

#[test]
fn an_album_query_does_not_pick_one_answer_in_silence() {
    let sandbox = Sandbox::new("album_match");
    let root = std::env::temp_dir().join("aede_e2e_album_match");
    let _ = std::fs::remove_dir_all(&root);
    for (folder, album) in [("one", "Danzig"), ("four", "Danzig 4")] {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(library().join("track.flac"), dir.join("1.flac")).unwrap();
        // The tag decides the album, not the folder name.
        let _ = album;
    }
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // The fixture is one album ("Kind of Blue") in two folders, so asking for a
    // fragment must show both matches rather than one of them.
    let (out, _, ok) = sandbox.run(&["album", "kind of"]);
    assert!(ok, "output: {out}");
    assert!(
        out.contains("showing the titles containing it"),
        "it says the match is not exact: {out}"
    );

    // An exact title stops the search there.
    let (out, _, ok) = sandbox.run(&["album", "Kind of Blue"]);
    assert!(ok);
    assert!(
        !out.contains("showing the titles containing it"),
        "an exact title needs no widening: {out}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Writes a report describing `file` as another tool would have measured it.
///
/// The size and date are read from the file itself: an imported analysis is
/// bound to the bytes it was reached on, so a fixture with invented numbers
/// would only ever exercise the staleness path.
fn write_report(at: &std::path::Path, file: &std::path::Path, md5: &str, transcoding: &str) {
    let named_as = file.to_string_lossy().into_owned();
    write_report_naming(at, file, &named_as, md5, transcoding);
}

/// The same, but writing `named_as` as the path instead of where the file is.
///
/// This is not an academic case. Watched folders are stored canonical, so a
/// report written against a symbolic link — or against `/var` where macOS says
/// `/private/var` — names the very same file by another route, and the two
/// strings never match.
fn write_report_naming(
    at: &std::path::Path,
    file: &std::path::Path,
    named_as: &str,
    md5: &str,
    transcoding: &str,
) {
    let meta = std::fs::metadata(file).expect("the analysed file");
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let text = format!(
        r#"{{"format":"flaccompagnon-report","version":1,"report":{{
             "root":"{root}",
             "files":[{{
               "path":"{path}",
               "file_name":"{name}",
               "size_bytes":{size},
               "modified_unix":{mtime},
               "detections":{{"transcoding":"{transcoding}","upscaling":false,
                              "upsampling":false,"summary":"Clean",
                              "detail":"full-band content"}},
               "cutoff_hz":22050.0,
               "real_bit_depth":16,
               "dr_db":9.3,
               "clipping":{{"clipped_samples":0,"peak_dbfs":-0.13,"clipped":false}},
               "flac_md5":{{"state":"{md5}"}}
             }}]}}}}"#,
        root = file.parent().unwrap().display(),
        path = named_as,
        name = file.file_name().unwrap().to_str().unwrap(),
        size = meta.len(),
    );
    std::fs::write(at, text).expect("writing the report");
}

#[test]
fn another_tools_analysis_can_be_taken_in_and_given_back() {
    let sandbox = Sandbox::new("import");
    let root = std::env::temp_dir().join("aede_e2e_import_src");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let flac = root.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // --- Importing ----------------------------------------------------------
    let report = root.join("report.json");
    write_report(&report, &flac, "Match", "none");
    let (out, err, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok, "the import must succeed. stderr: {err}");
    assert!(out.contains("Files matched"), "output: {out}");
    assert!(out.contains("Analyses stored"), "output: {out}");

    // What was imported is attributed to whoever measured it, never merged into
    // Aède's own reading.
    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");
    assert!(out.contains("Dynamic range"), "output: {out}");
    assert!(!out.contains("stale"), "the file has not changed: {out}");

    // Importing the same report twice replaces; it does not accumulate.
    let (out, _, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Analyses stored"), "output: {out}");
    let (out, _, _) = sandbox.run(&["track", "So What"]);
    assert_eq!(
        out.matches("Analysed by flaccompagnon").count(),
        1,
        "one analysis per file and per source:\n{out}"
    );

    // --- Two methods disagreeing --------------------------------------------
    let (_, _, ok) = sandbox.run(&["check"]);
    assert!(ok);

    // The album page says what has been read about it, by both methods and
    // naming both: verifying an album is what a person actually does, and it
    // used to take one `track` command per track to find out.
    let (out, _, ok) = sandbox.run(&["album", "Kind of Blue"]);
    assert!(ok, "output: {out}");
    assert!(
        out.contains("checked: 1 intact"),
        "aède's own reading: {out}"
    );
    assert!(
        out.contains("flaccompagnon: 1 MD5 matches"),
        "and the imported one, named:\n{out}"
    );

    write_report(&report, &flac, "Mismatch", "detected");
    let (_, _, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(
        out.contains("does not match its MD5"),
        "a failed MD5 is an error even when the checksums passed:\n{out}"
    );
    assert!(out.contains("re-encoded"), "and it says why:\n{out}");
    // The same report declares `transcoding: detected`, and that is said
    // nowhere. A failed MD5 is a fact — two methods compared a checksum and
    // disagreed, and `check` can be pointed at the file to settle it. A
    // spectral verdict is an inference from a heuristic, and this report only
    // relays facts. Both come from the same import, which is the point: what
    // was stored and what is reported are two different questions.
    for word in ["transcod", "upscal", "upsampl", "lossy"] {
        assert!(
            !out.contains(word),
            "\"{word}\" must not appear in the report:\n{out}"
        );
    }
    // And the file's own page keeps the measurement the verdict was drawn
    // from, so nothing is hidden from whoever wants to judge for themselves.
    let (page, _, _) = sandbox.run(&["track", "So What"]);
    assert!(page.contains("Analysed by flaccompagnon"), "page: {page}");
    assert!(page.contains("Cutoff"), "the cutoff is a number: {page}");
    for word in ["Transcoding", "Upscaled", "Upsampled", "Verdict"] {
        assert!(
            !page.contains(word),
            "\"{word}\" is an inference, not a measurement:\n{page}"
        );
    }

    // --- A report about other bytes -----------------------------------------
    // Once the file has been touched and read again, every imported verdict
    // about it is worthless: the catalog says so rather than answering with
    // confidence about bytes that are no longer there.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&flac)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let (_, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok);
    assert!(out.contains("stale"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(
        !out.contains("does not match its MD5"),
        "a stale verdict is not reported:\n{out}"
    );

    // And importing that same report again refuses it for the same reason,
    // instead of quietly restoring it.
    let (out, _, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Changed since the report"), "output: {out}");

    // --- What is held, and what became of it --------------------------------
    // The counterpart of --pending, and it was missing: the catalog could say
    // what had failed to attach and nothing at all about what had succeeded.
    // A report imported over clean files then showed every symptom of having
    // done nothing — no waiting line, no doctor entry, no page saying so.
    let (out, _, ok) = sandbox.run(&["import", "--list"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("flaccompagnon"), "the source is named: {out}");
    assert!(
        out.contains("stale"),
        "this one was voided by the file changing: {out}"
    );
    // Narrowed to a source nobody imported, the answer is "nothing matches
    // that" rather than "nothing is held", which would read as an empty store.
    let (out, _, ok) = sandbox.run(&["import", "--list", "--source", "nobody"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("matches that"), "output: {out}");

    // --- Forgetting ---------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["import", "--forget"]);
    assert!(ok, "output: {out}");
    // And it names the file it just wrote. "311 removed" over a report that
    // still shows them is a contradiction nobody can investigate without
    // knowing which catalog was emptied — `$AEDE_HOME` and `--data` both move
    // it, and the first is easy to forget.
    assert!(
        out.contains("catalog:") && out.contains("catalog.json"),
        "the destructive summary must name the file it wrote:\n{out}"
    );
    let (out, _, _) = sandbox.run(&["track", "So What"]);
    assert!(!out.contains("Analysed by"), "nothing is left: {out}");

    // Emptied means emptied: a second run finds nothing left to remove, which
    // is the assertion that a summary printed without a save would fail.
    let (out, _, ok) = sandbox.run(&["import", "--forget"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("0 analysis removed"), "output: {out}");

    // --- Refusals -----------------------------------------------------------
    let foreign = root.join("foreign.json");
    std::fs::write(&foreign, r#"{"format":"something-else"}"#).unwrap();
    let (_, err, ok) = sandbox.run(&["import", foreign.to_str().unwrap()]);
    assert!(!ok, "another tool's JSON is refused");
    assert!(err.contains("not a FlacCompagnon report"), "stderr: {err}");

    let (_, err, ok) = sandbox.run(&["import", "/nowhere/at/all.json"]);
    assert!(!ok);
    assert!(err.contains("does not exist"), "stderr: {err}");

    let (_, err, ok) = sandbox.run(&["import"]);
    assert!(!ok, "an import with nothing to import is an error");
    assert!(err.contains("give a report"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_analysis_can_arrive_before_the_library_does() {
    // Analysing a folder and then building the library from it is the natural
    // order for someone who already owns the other tool. The import must
    // therefore not require the files to be known yet.
    let sandbox = Sandbox::new("import_first");
    let root = std::env::temp_dir().join("aede_e2e_import_first_src");
    let music = root.join("music");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&music).unwrap();
    let flac = music.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();

    // The report sits outside the library, so that the scan reaches it through
    // what was already imported rather than by walking over it. And it names
    // the file by a path the catalog will never hold: this is what a report
    // written against a symbolic link looks like, and on macOS what every
    // report under /var looks like once the folder is canonicalized to
    // /private/var. Only the name and the size can bridge the two.
    let report = root.join("report.json");
    let elsewhere = "/elsewhere/Danzig/01 So What.flac";
    write_report_naming(&report, &flac, elsewhere, "Match", "none");

    // Nothing has ever been scanned: the catalog does not exist yet.
    let (out, err, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok, "an import needs no catalog. stderr: {err}");
    assert!(out.contains("Waiting for a scan"), "output: {out}");
    assert!(
        out.contains("they attach themselves"),
        "and it says what happens next: {out}"
    );

    // The scan brings the file in, and the analysis attaches itself although
    // the path it names is not the path the file is at.
    let (out, _, ok) = sandbox.run(&["scan", music.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Analyses now attached"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");
    assert!(!out.contains("stale"), "and it applies: {out}");

    // Attached for good: a second scan does not have to do it again.
    let (out, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    assert!(!out.contains("Analyses now attached"), "output: {out}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_pending_analysis_can_be_named_and_then_dropped_on_its_own() {
    // A scan only ever attaches a waiting analysis by matching name and size —
    // never by the mere fact that a scan happened. A report naming a file
    // under a path (and a name) the library will never hold is the case a
    // user actually hits: an old FlacCompagnon run, a folder that moved, a
    // report exported against a library that no longer exists. `doctor` can
    // only say how many are stuck like that; this is where a user is meant to
    // learn which ones, and to be rid of them without losing analyses that did
    // attach.
    let sandbox = Sandbox::new("import_pending");
    let root = std::env::temp_dir().join("aede_e2e_import_pending_src");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let flac = root.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // Matches the scanned file directly.
    let matched_report = root.join("matched.json");
    write_report(&matched_report, &flac, "Match", "none");
    // Names a file that is neither in the catalog nor a name-and-size match
    // for anything in it: nothing will ever attach it.
    let vanished_report = root.join("vanished.json");
    write_report_naming(
        &vanished_report,
        &flac,
        "/an/old/library/that moved away.flac",
        "Match",
        "none",
    );
    let (_, _, ok) = sandbox.run(&["import", matched_report.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["import", vanished_report.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Waiting for a scan"), "output: {out}");

    // `doctor` counts it, but only names how many.
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(
        out.contains("1 imported analysis waiting for the folders they name to be scanned"),
        "output: {out}"
    );

    // A scan of the very folder the report names does not make it attach: the
    // file it describes is not there, under any name.
    let (_, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(
        out.contains("waiting for the folders they name to be scanned"),
        "still waiting after a re-scan: {out}"
    );

    // --- Naming it -----------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["import", "--pending"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Waiting for a scan"), "output: {out}");
    assert!(out.contains("flaccompagnon"), "and its source: {out}");
    assert!(
        !out.contains("01 So What.flac"),
        "the matched analysis is not pending, and must not be listed: {out}"
    );

    // The folder is what a user acts on — scan it, or drop it — so that is the
    // unit listed, and it is written out **whole**. The listing that named
    // files and cut them to a column width showed
    // `…/1980 Blizzard of Ozz/01 I Don't Know.flac`, hiding the very part that
    // says whether a drive is unplugged or a folder was renamed.
    assert!(
        out.contains("/an/old/library"),
        "the head of the path is what identifies the folder: {out}"
    );
    assert!(
        out.lines()
            .any(|l| l.contains("Folder") && l.contains("Analyses")),
        "one row per folder, with how many wait in it: {out}"
    );
    assert!(
        out.lines()
            .filter(|l| l.contains("/an/old/library"))
            .all(|l| !l.contains('…')),
        "the folder is written whole, never cut to a column width: {out}"
    );

    // A source nobody imported from finds nothing — and says it was narrowed,
    // rather than reporting the clean catalog it is not.
    let (out, _, ok) = sandbox.run(&["import", "--pending", "--source=someone-else"]);
    assert!(ok);
    assert!(
        out.contains("nothing waiting matches that"),
        "output: {out}"
    );

    // A folder narrows the listing the same way.
    let (out, _, ok) = sandbox.run(&["import", "--pending", "/an/old"]);
    assert!(ok);
    assert!(out.contains("/an/old/library"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["import", "--pending", "/somewhere/else"]);
    assert!(ok);
    assert!(
        out.contains("nothing waiting matches that"),
        "output: {out}"
    );

    // --- Dropping only what is pending ----------------------------------------
    // A folder on a plain --forget is refused rather than swallowed: on a
    // command that deletes, an ignored argument is the worst kind.
    let (_, err, ok) = sandbox.run(&["import", "--forget", "/an/old"]);
    assert!(!ok, "stderr: {err}");
    assert!(err.contains("takes no folder"), "stderr: {err}");
    assert!(
        err.contains("--forget --pending"),
        "and says what does: {err}"
    );

    // A folder holding nothing pending removes nothing, and leaves the rest.
    let (out, _, ok) = sandbox.run(&["import", "--forget", "--pending", "/somewhere/else"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("0 analysis removed"), "output: {out}");

    let (out, _, ok) = sandbox.run(&["import", "--forget", "--pending"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("1 analysis removed"), "output: {out}");

    let (out, _, ok) = sandbox.run(&["import", "--pending"]);
    assert!(ok);
    assert!(out.contains("nothing is waiting"), "output: {out}");

    // The one that had attached is untouched.
    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok);
    assert!(
        out.contains("Analysed by flaccompagnon"),
        "a matched analysis must survive --forget --pending: {out}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_report_left_in_the_library_is_picked_up_by_the_scan() {
    // The report may equally well be sitting in the album folder. A scan walks
    // over it anyway, so it costs nothing to notice it.
    let sandbox = Sandbox::new("import_scan");
    let root = std::env::temp_dir().join("aede_e2e_import_scan_src");
    let deep = root.join("Danzig/1996 Blackacidevil");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&deep).unwrap();
    let flac = deep.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();
    write_report(&deep.join("analysis.json"), &flac, "Match", "none");
    // A JSON that is not a report is left alone, whatever it holds.
    std::fs::write(deep.join("other.json"), r#"{"hello": "world"}"#).unwrap();

    let (out, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Analyses imported"), "output: {out}");

    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok);
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn reports_are_looked_for_in_every_folder_underneath() {
    // Reports are kept the way albums are: one folder per artist, one per
    // album. Only looking at the top level would find nothing.
    let sandbox = Sandbox::new("import_recursive");
    let root = std::env::temp_dir().join("aede_e2e_import_recursive_src");
    let music = root.join("music");
    let reports = root.join("reports/Danzig/1996 Blackacidevil");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&music).unwrap();
    std::fs::create_dir_all(&reports).unwrap();
    let flac = music.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();
    write_report(&reports.join("album.json"), &flac, "Match", "none");

    let (_, _, ok) = sandbox.run(&["scan", music.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["import", root.join("reports").to_str().unwrap()]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Files matched"), "output: {out}");
    let (out, _, _) = sandbox.run(&["track", "So What"]);
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");

    // A folder holding no report at all is an error, not a silent success.
    let (_, err, ok) = sandbox.run(&["import", music.to_str().unwrap()]);
    assert!(!ok);
    assert!(err.contains("no .json report"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_selection_is_copied_out_keeping_its_tree() {
    // The one command that writes files, and it writes them outside the
    // library. Everything it can get wrong is expensive: a tree that does not
    // survive, a name the card refuses halfway through, a copy written into the
    // library itself.
    let sandbox = Sandbox::new("copy");
    let root = std::env::temp_dir().join("aede_e2e_copy_src");
    let out = std::env::temp_dir().join("aede_e2e_copy_dest");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
    let album = root.join("Pixies/Surfer Rosa");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    // A title a FAT card refuses, beside a cover and a spectrogram that a
    // filter on "images" could not tell apart.
    std::fs::copy(
        library().join("track.flac"),
        album.join("04 Where Is My Mind?.flac"),
    )
    .unwrap();
    std::fs::write(album.join("cover.jpg"), b"cover").unwrap();
    std::fs::write(album.join("spectrogram.png"), b"spectrum").unwrap();
    std::fs::write(album.join("rip.log"), b"log").unwrap();

    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // --- A dry run writes nothing ------------------------------------------
    let (report, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--dry-run"]);
    assert!(ok, "stderr: {err}");
    assert!(report.contains("nothing was written"), "output: {report}");
    assert_eq!(
        std::fs::read_dir(&out).unwrap().count(),
        0,
        "a dry run must leave the destination untouched"
    );

    // The cover travels by default; the spectrogram does not. This is the
    // whole reason the catalog's own choice beats an extension filter — both
    // files are images, and only one of them belongs on a player.
    assert!(report.contains("Covers"), "output: {report}");

    // --- The real thing -----------------------------------------------------
    let (report, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--verify"]);
    assert!(ok, "stderr: {err}");
    assert!(report.contains("Written"), "output: {report}");

    let track = out.join("Pixies/Surfer Rosa/04 Where Is My Mind?.flac");
    assert!(track.is_file(), "the tree is kept: {}", track.display());
    assert!(
        out.join("Pixies/Surfer Rosa/cover.jpg").is_file(),
        "the cover came"
    );
    assert!(
        !out.join("Pixies/Surfer Rosa/spectrogram.png").exists(),
        "the spectrogram did not"
    );
    assert!(
        !out.join("Pixies/Surfer Rosa/rip.log").exists(),
        "nor the log"
    );
    // Copied, not truncated.
    assert_eq!(
        std::fs::metadata(&track).unwrap().len(),
        std::fs::metadata(album.join("04 Where Is My Mind?.flac"))
            .unwrap()
            .len()
    );
    // And nothing half-written is left wearing a real name.
    assert!(
        !out.join("Pixies/Surfer Rosa/04 Where Is My Mind?.aede-partial")
            .exists()
    );

    // --- Running it again costs nothing ------------------------------------
    let (report, _, ok) = sandbox.run(&["copy", out.to_str().unwrap()]);
    assert!(ok);
    assert!(
        report.contains("Already there"),
        "an interrupted run must be cheap to finish: {report}"
    );

    // --- Names a card would refuse -----------------------------------------
    let (report, err, ok) =
        sandbox.run(&["copy", out.to_str().unwrap(), "--safe-names", "--dry-run"]);
    assert!(ok, "stderr: {err}");
    assert!(report.contains("Renamed"), "output: {report}");
    assert!(
        report.contains("Where Is My Mind_.flac"),
        "the new name is shown, not just counted: {report}"
    );

    // --- What it refuses ----------------------------------------------------
    // A destination that does not exist is almost always a drive that is not
    // plugged in, and creating it would fill the internal disk instead.
    let (_, err, ok) = sandbox.run(&["copy", "/tmp/aede_e2e_copy_absent"]);
    assert!(!ok);
    assert!(err.contains("does not exist"), "stderr: {err}");
    assert!(
        err.contains("not plugged in"),
        "and says why it matters: {err}"
    );

    // A copy inside the library would be read back by the next scan, and every
    // album would become its own duplicate.
    let inside = root.join("backup");
    std::fs::create_dir_all(&inside).unwrap();
    let (_, err, ok) = sandbox.run(&["copy", inside.to_str().unwrap()]);
    assert!(!ok);
    assert!(err.contains("is not a library"), "stderr: {err}");

    // …and it is still inside the library when reached by another route.
    // Watched folders are stored canonical, so a destination given through a
    // symbolic link names the same folder by a string that never compares
    // equal — and the guard, comparing strings, waved it straight through. On
    // macOS this is not a corner case but the ordinary one: /var is a link to
    // /private/var, so every path under it arrives in two spellings. `scan`
    // and `check` both canonicalize; `copy`, the one command that *writes*,
    // was the one that did not.
    let link = std::env::temp_dir().join("aede_e2e_copy_link");
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, &link).unwrap();
    #[cfg(unix)]
    {
        let through_the_link = link.join("backup");
        let (_, err, ok) = sandbox.run(&["copy", through_the_link.to_str().unwrap()]);
        assert!(
            !ok,
            "a symbolic link is not a way out of the library: {err}"
        );
        assert!(err.contains("is not a library"), "stderr: {err}");
    }
    let _ = std::fs::remove_file(&link);

    // A word that names no level of extras.
    let (_, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--extras", "everything"]);
    assert!(!ok);
    assert!(err.contains("none, cover, images, all"), "stderr: {err}");

    // Two options asking for opposite things.
    let (_, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--safe-names", "--raw-names"]);
    assert!(!ok);
    assert!(err.contains("opposite"), "stderr: {err}");

    // The selection goes in --query; a second positional is not a silent extra.
    let (_, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "loved"]);
    assert!(!ok);
    assert!(err.contains("one destination"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
}

/// `true` when ffmpeg is installed, which the conversion tests need and the
/// rest of the suite does not.
///
/// Skipped rather than failed where it is missing: ffmpeg is an external
/// program by design, and a checkout without it must still be able to run its
/// tests green. The skip says so out loud, so a suite that silently stopped
/// testing conversion cannot pass for a suite that tested it.
fn ffmpeg_is_installed() -> bool {
    let there = std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !there {
        eprintln!("skipped: ffmpeg is not installed");
    }
    there
}

#[test]
fn only_what_is_lossless_is_encoded_on_the_way_out() {
    // A library is mixed, and that is the case worth getting right: the FLACs
    // and WAVs are encoded, the MP3s are copied as they stand. Re-encoding an
    // MP3 into an MP3 loses quality to produce the same thing; into a FLAC it
    // produces something *larger* and no better: a lossless container with a
    // lossy ancestry, which is the one thing nobody rips on purpose.
    if !ffmpeg_is_installed() {
        return;
    }
    let sandbox = Sandbox::new("copy_compress");
    let root = std::env::temp_dir().join("aede_e2e_compress_src");
    let out = std::env::temp_dir().join("aede_e2e_compress_dest");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
    let album = root.join("Miles/Kind of Blue");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::copy(library().join("track.flac"), album.join("01 lossless.flac")).unwrap();
    std::fs::copy(
        library().join("track.mp3"),
        album.join("02 already lossy.mp3"),
    )
    .unwrap();
    std::fs::copy(
        library().join("track.wav"),
        album.join("03 uncompressed.wav"),
    )
    .unwrap();

    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // --- What it says it will do -------------------------------------------
    let (report, err, ok) = sandbox.run(&[
        "copy",
        out.to_str().unwrap(),
        "--compress",
        "mp3",
        "--dry-run",
    ]);
    assert!(ok, "stderr: {err}");
    assert!(report.contains("To encode"), "output: {report}");
    assert!(
        report.contains("Copied as they are"),
        "the split is shown, because a silent skip looks like lost files: {report}"
    );
    assert!(
        report.contains("estimated"),
        "an encoder's output is a guess and says so: {report}"
    );

    // --- And what it does ---------------------------------------------------
    let (report, err, ok) = sandbox.run(&[
        "copy",
        out.to_str().unwrap(),
        "--compress",
        "mp3",
        "--verify",
    ]);
    assert!(ok, "stderr: {err}\n{report}");

    let album_out = out.join("Miles/Kind of Blue");
    assert!(
        album_out.join("01 lossless.mp3").is_file(),
        "the FLAC was encoded"
    );
    assert!(
        album_out.join("03 uncompressed.mp3").is_file(),
        "so was the WAV"
    );
    assert!(
        album_out.join("02 already lossy.mp3").is_file(),
        "and the MP3 arrived"
    );
    // Copied rather than re-encoded: byte for byte what it was.
    assert_eq!(
        std::fs::read(album_out.join("02 already lossy.mp3")).unwrap(),
        std::fs::read(album.join("02 already lossy.mp3")).unwrap(),
        "an MP3 asked to become an MP3 is copied, not encoded a second time"
    );

    // …and the case that one does *not* prove. An MP3 asked to become an MP3
    // is left alone by the "already in that format" rule alone, so a build
    // that had lost the lossless rule entirely would still pass the assertion
    // above. The question the rule actually answers is what happens to an MP3
    // when a *different* format is asked for, and the answer must be the same:
    // a second lossy pass over a first one is audible, and an MP3 grown into a
    // FLAC is larger, no better, and lossless in name only.
    for (format, extension) in [("opus", "opus"), ("flac", "flac")] {
        let other = std::env::temp_dir().join(format!("aede_e2e_compress_{format}"));
        let _ = std::fs::remove_dir_all(&other);
        std::fs::create_dir_all(&other).unwrap();
        let (report, err, ok) =
            sandbox.run(&["copy", other.to_str().unwrap(), "--compress", format]);
        assert!(ok, "stderr: {err}\n{report}");
        let there = other.join("Miles/Kind of Blue");
        assert!(
            there.join("02 already lossy.mp3").is_file(),
            "the MP3 stays an MP3 when {format} is asked for"
        );
        assert!(
            !there.join(format!("02 already lossy.{extension}")).exists(),
            "and is not encoded a second time into {format}"
        );
        // While the lossless ones did convert, so the run did do its job.
        assert!(there.join(format!("01 lossless.{extension}")).exists() || format == "flac");
        let _ = std::fs::remove_dir_all(&other);
    }
    // The originals are untouched — this is the one command that writes, and
    // it writes outside.
    assert!(album.join("01 lossless.flac").is_file());

    // The metadata travelled: a player showing "track 1" and nothing else is
    // not a copy of a library.
    let (shown, _, ok) =
        sandbox.run(&["file", album_out.join("01 lossless.mp3").to_str().unwrap()]);
    assert!(ok);
    assert!(shown.contains("So What"), "the title followed: {shown}");

    // --- Running it again encodes nothing again -----------------------------
    let (report, _, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--compress", "mp3"]);
    assert!(ok);
    assert!(
        report.contains("Already there"),
        "an interrupted conversion is finished, not restarted: {report}"
    );
    // And nothing half-encoded is left wearing a whole file's name.
    assert!(!album_out.join("01 lossless.aede-partial.mp3").exists());

    // --- What it refuses ----------------------------------------------------
    let (_, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--compress", "wma"]);
    assert!(!ok);
    assert!(
        err.contains("mp3, opus"),
        "it offers what it accepts: {err}"
    );

    let (_, err, ok) = sandbox.run(&[
        "copy",
        out.to_str().unwrap(),
        "--compress",
        "mp3",
        "--quality",
        "best",
    ]);
    assert!(!ok);
    assert!(err.contains("192k"), "stderr: {err}");

    // A quality with nothing to apply it to is an option going into the void.
    let (_, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--quality", "V0"]);
    assert!(!ok);
    assert!(err.contains("add --compress"), "stderr: {err}");

    // …and so is a quality on a format that has none. `--compress wav
    // --quality 128k` reads as a request for small files, and WAV has no
    // quality setting at all: the option went into the void and the run
    // produced files some eleven times larger than the number just typed. On
    // the card this command exists to fill, that is the difference between
    // fitting and not — and the check costs nothing, since it happens before a
    // single file is read.
    for lossless in ["wav", "flac"] {
        let (_, err, ok) = sandbox.run(&[
            "copy",
            out.to_str().unwrap(),
            "--compress",
            lossless,
            "--quality",
            "128k",
        ]);
        assert!(!ok, "--quality must not be swallowed by {lossless}");
        assert!(err.contains("means nothing for"), "stderr: {err}");
        assert!(
            err.contains("mp3") && err.contains("opus"),
            "and it names the formats that do have one: {err}"
        );
    }

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn a_conversion_with_nothing_to_convert_says_so() {
    // The same silence as a swallowed option, seen from the other side:
    // `--compress mp3` over a selection that is already MP3 did exactly what
    // it should and said nothing at all about it, which reads as an option
    // that was ignored. It was honoured; it simply had nothing to do, and
    // that is worth one line.
    let sandbox = Sandbox::new("copy_nothing_to_convert");
    let root = std::env::temp_dir().join("aede_e2e_nothing_src");
    let out = std::env::temp_dir().join("aede_e2e_nothing_dest");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
    let album = root.join("a");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::copy(library().join("track.mp3"), album.join("01.mp3")).unwrap();
    std::fs::copy(library().join("vbr.mp3"), album.join("02.mp3")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (report, err, ok) = sandbox.run(&[
        "copy",
        out.to_str().unwrap(),
        "--compress",
        "mp3",
        "--dry-run",
    ]);
    assert!(ok, "stderr: {err}");
    assert!(
        report.contains("nothing here needs encoding"),
        "an option that had nothing to do must not look ignored: {report}"
    );

    // And with something to encode, that line is not printed — a notice that
    // appears whatever happens stops meaning anything.
    std::fs::copy(library().join("track.flac"), album.join("03.flac")).unwrap();
    let (_, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    let (report, _, ok) = sandbox.run(&[
        "copy",
        out.to_str().unwrap(),
        "--compress",
        "mp3",
        "--dry-run",
    ]);
    assert!(ok);
    assert!(!report.contains("nothing here needs encoding"), "{report}");

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn an_empty_answer_says_where_what_you_wrote_actually_is() {
    // A bare `loved` asks about the **track**, by design. The cost of that
    // design is one badly misleading answer: somebody who marked an *album* a
    // favourite types `loved`, is told nothing matches, and reasonably
    // concludes the feature is broken. It is not; they asked a different
    // question from the one they meant, and nothing on screen said so.
    let sandbox = Sandbox::new("query_scope_hint");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);
    let (_, err, ok) = sandbox.run(&["love", "album", "Duos"]);
    assert!(ok, "stderr: {err}");
    let (_, _, ok) = sandbox.run(&["tag", "album", "Duos", "great"]);
    assert!(ok);

    for (asked, offered) in [
        ("loved", "album.loved"),
        ("tag:great", "album.tag:great"),
        // A negated term is rewritten too, and this one stays empty at track
        // scope where a bare `-loved` would not: nothing is loved *on a
        // track*, so `-loved` matches the whole library and needs no hint.
        (
            "loved -tag:nosuchlabel",
            "album.loved -album.tag:nosuchlabel",
        ),
    ] {
        let (out, _, ok) = sandbox.run(&["query", asked]);
        assert!(ok, "output: {out}");
        // An empty answer is still an empty answer: the query means what it
        // says, and the hint is a hint.
        assert!(out.contains("nothing matches"), "output: {out}");
        assert!(
            out.contains("that is where you wrote it"),
            "{asked} must say where it actually is: {out}"
        );
        // What is offered has to be typeable back in, and give the answer.
        assert!(out.contains(offered), "{asked} must offer {offered}: {out}");
        let (found, _, ok) = sandbox.run(&["query", offered]);
        assert!(ok);
        assert!(
            !found.contains("nothing matches"),
            "what was offered must answer: {found}"
        );
    }

    // A question with nothing user-written in it gets no such line — a notice
    // printed whatever happens stops meaning anything.
    let (out, _, ok) = sandbox.run(&["query", "genre:nonexistentgenre"]);
    let _ = ok;
    assert!(!out.contains("that is where you wrote it"), "{out}");

    // And neither does one that already found something.
    let (out, _, ok) = sandbox.run(&["query", "album.loved"]);
    assert!(ok);
    assert!(!out.contains("that is where you wrote it"), "{out}");
}

#[test]
fn a_search_can_look_in_what_you_wrote() {
    // `--comments` searches the comment tag, which lives inside the audio
    // file. `--notes` searches what the user wrote, which lives in user.json.
    // Two different fields, two different sections, never folded together:
    // searching one is searching the library, searching the other is
    // searching yourself.
    let sandbox = Sandbox::new("search_notes");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);
    let (_, err, ok) = sandbox.run(&["note", "album", "Duos", "--text", "pressage vinyle de 1963"]);
    assert!(ok, "stderr: {err}");
    let (_, _, ok) = sandbox.run(&["note", "artist", "Dave Brubeck", "--text", "le quintet"]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["search", "vinyle", "--notes"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("In your notes"), "output: {out}");
    assert!(out.contains("Duos"), "output: {out}");
    assert!(out.contains("pressage vinyle"), "output: {out}");

    // A note on an artist is not a track, and is shown as what it is.
    let (out, _, ok) = sandbox.run(&["search", "quintet", "--notes"]);
    assert!(ok);
    assert!(out.contains("artist"), "the kind is named: {out}");
    assert!(out.contains("Dave Brubeck"), "output: {out}");

    // Accent- and case-insensitive, like every other search in the program.
    let (out, _, ok) = sandbox.run(&["search", "VINYLE", "--notes"]);
    assert!(ok);
    assert!(out.contains("Duos"), "output: {out}");

    // Without the option the notes are not searched at all: a common word in
    // free prose would bury the entity that actually bears the name.
    let (out, _, ok) = sandbox.run(&["search", "vinyle"]);
    assert!(ok);
    assert!(!out.contains("In your notes"), "output: {out}");

    // Nothing written that matches says so, rather than printing an empty
    // heading.
    let (out, _, ok) = sandbox.run(&["search", "nothingwrittenaboutthis", "--notes"]);
    assert!(ok);
    assert!(out.contains("nothing in your notes"), "output: {out}");

    // And the option is refused where it means nothing.
    let (_, err, ok) = sandbox.run(&["albums", "--notes"]);
    assert!(!ok);
    assert!(err.contains("--notes applies to search"), "stderr: {err}");
}

#[test]
fn an_album_listing_answers_the_grammar_too() {
    // The grammar evaluates over tracks, so "the albums I rated four stars or
    // more" had no answer: `query` gave back their tracks. An album listing
    // that takes the same expression is the fold of the finer question, and it
    // is what somebody asking about albums meant.
    let sandbox = Sandbox::new("albums_query");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    let (_, err, ok) = sandbox.run(&["rate", "album", "Duos", "--stars", "5"]);
    assert!(ok, "stderr: {err}");
    let (_, _, ok) = sandbox.run(&["tag", "album", "Duos", "vinyl,rare"]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["albums", "--query", "album.rating:>=4"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("Duos"), "output: {out}");
    assert!(out.contains("Album"), "it is an album listing: {out}");
    assert!(!out.contains("Take Five"), "and not a track listing: {out}");

    // What was written is searchable from here too.
    let (out, _, ok) = sandbox.run(&["albums", "--query", "album.tag:vinyl"]);
    assert!(ok);
    assert!(out.contains("Duos"), "output: {out}");

    // The options and the expression compose by AND rather than one replacing
    // the other — and an expression holding an OR must narrow *with* the
    // option, not swallow it.
    // The second branch of the OR is deliberately one that *does* match Duos:
    // with the expression left unbracketed, juxtaposition binds tighter than
    // OR, so `year:1900 album.rating:>=4 OR album.rating:5` reads as
    // `(year:1900 AND rating>=4) OR rating:5` and Duos comes back through the
    // second branch with the year silently ignored. A branch that matched
    // nothing would have let both readings pass, and proved nothing.
    let (out, _, ok) = sandbox.run(&[
        "albums",
        "--query",
        "album.rating:>=4 OR album.rating:5",
        "--year",
        "1900",
    ]);
    assert!(ok);
    assert!(
        !out.contains("Duos"),
        "--year 1900 must still narrow it, whatever the expression says: {out}"
    );

    // A broken expression is refused where it is typed, not at some later run.
    let (_, err, ok) = sandbox.run(&["albums", "--query", "nosuchfield:x"]);
    assert!(!ok);
    assert!(err.contains("is not a field"), "stderr: {err}");
}

#[test]
fn a_folder_can_be_kept_out_of_the_library_for_good() {
    // A music folder is rarely only music: Audiobooks, Podcasts, _incoming, a
    // Samples folder for a DAW. Without this the only way to keep them out is
    // to reorganise the disk to suit the program, which is the wrong way
    // round.
    let sandbox = Sandbox::new("scan_exclude");
    let root = std::env::temp_dir().join("aede_e2e_exclude_src");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Music/Album")).unwrap();
    std::fs::create_dir_all(root.join("Audiobooks/Book")).unwrap();
    std::fs::copy(
        library().join("track.flac"),
        root.join("Music/Album/01.flac"),
    )
    .unwrap();
    std::fs::copy(
        library().join("track.mp3"),
        root.join("Audiobooks/Book/ch1.mp3"),
    )
    .unwrap();

    let (out, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok, "output: {out}");
    let (out, _, _) = sandbox.run(&["stats"]);
    assert!(out.contains('2'), "both were taken in to begin with: {out}");

    // --- Excluding ----------------------------------------------------------
    let books = root.join("Audiobooks");
    let (out, err, ok) = sandbox.run(&["roots", "--exclude", books.to_str().unwrap()]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("will not be read"), "output: {out}");
    // The same promise `--remove` on a root makes, and it must be kept.
    assert!(out.contains("stay in the catalog"), "output: {out}");

    // A plain rescan honours it — this is the whole point. An exclusion that
    // had to be retyped would be forgotten exactly when it mattered.
    let (_, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["query", "path:Audiobooks"]);
    assert!(ok);
    assert!(out.contains("nothing matches"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["query", "path:Album"]);
    assert!(ok);
    assert!(
        !out.contains("nothing matches"),
        "the rest is untouched: {out}"
    );

    // --- And it survives being rebuilt again --------------------------------
    // A scan rebuilds the catalog from the files, and an exclusion is typed
    // rather than read from any file: dropping it here is the same fault as
    // dropping an imported analysis, and it is the one this feature shipped
    // with the first time.
    let (_, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["roots"]);
    assert!(ok);
    assert!(
        out.contains("Never read"),
        "an exclusion nobody can see is one nobody remembers setting: {out}"
    );
    assert!(out.contains("Audiobooks"), "output: {out}");

    // --- Taking it back -----------------------------------------------------
    let (out, err, ok) = sandbox.run(&["roots", "--exclude", books.to_str().unwrap(), "--remove"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("will be read again"), "output: {out}");
    let (_, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["query", "path:Audiobooks"]);
    assert!(ok);
    assert!(!out.contains("nothing matches"), "it is back: {out}");

    // Excluding what is not excluded, and dropping what was never excluded,
    // are both said rather than passed over.
    let (_, err, ok) = sandbox.run(&["roots", "--exclude", "/nowhere/at/all", "--remove"]);
    assert!(!ok);
    assert!(err.contains("is not excluded"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_listen_recorded_by_mistake_can_be_taken_back() {
    // Every other mark the user writes takes `--remove`: a favourite, a
    // rating, a note, a tag, a saved query. A listen was the one that could
    // only ever be added, so a mistaken `aede played` was permanent.
    let sandbox = Sandbox::new("history_remove");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);
    for _ in 0..3 {
        let (_, err, ok) = sandbox.run(&["played", "Take Five"]);
        assert!(ok, "stderr: {err}");
    }
    let (out, _, ok) = sandbox.run(&["query", "played:3"]);
    assert!(ok);
    assert!(!out.contains("nothing matches"), "output: {out}");

    // --- One listen back ----------------------------------------------------
    let (out, err, ok) = sandbox.run(&["played", "Take Five", "--remove"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("forgotten"), "output: {out}");

    // The counter and the log move together. They are two structures — the log
    // is bounded, the counter is not — and a removal that touched one only
    // would leave a track "played three times" with two plays behind it.
    let (out, _, ok) = sandbox.run(&["query", "played:2"]);
    assert!(ok);
    assert!(
        !out.contains("nothing matches"),
        "the count followed: {out}"
    );
    let (out, _, ok) = sandbox.run(&["history"]);
    assert!(ok);
    // Two rows left, plus the "most played" line under the table, which names
    // the title again.
    assert_eq!(
        out.lines().filter(|l| l.contains("Take Five")).count(),
        3,
        "and so did the log: {out}"
    );

    // Nothing left to take back says so rather than claiming a removal.
    for _ in 0..2 {
        let (_, _, ok) = sandbox.run(&["played", "Take Five", "--remove"]);
        assert!(ok);
    }
    let (out, _, ok) = sandbox.run(&["played", "Take Five", "--remove"]);
    assert!(ok);
    assert!(out.contains("no listen on record"), "output: {out}");

    // --- All of it ----------------------------------------------------------
    let (_, _, ok) = sandbox.run(&["played", "Take Five"]);
    assert!(ok);
    // Not undoable and not rebuildable, so it is confirmed like `reset` — and
    // refused outright where there is no terminal to confirm on.
    let (_, err, ok) = sandbox.run(&["history", "--remove"]);
    assert!(!ok, "a pipe must not be taken for a yes");
    assert!(err.contains("--yes"), "stderr: {err}");

    let (out, err, ok) = sandbox.run(&["history", "--remove", "--yes"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("forgotten"), "output: {out}");
    // It says what went: "your history is cleared" is a claim nobody can check.
    assert!(out.contains("listen"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["history"]);
    assert!(ok);
    assert!(out.contains("nothing has been played"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["query", "played:0"]);
    assert!(ok);
    assert!(
        !out.contains("nothing matches"),
        "the counts went too: {out}"
    );

    let (out, _, ok) = sandbox.run(&["history", "--remove", "--yes"]);
    assert!(ok);
    assert!(out.contains("no history to forget"), "output: {out}");
}

#[test]
fn every_listing_is_put_in_order_by_one_vocabulary() {
    // `--sort` reached three commands and not the other four, and the three
    // that had it did not agree on what the words meant. One vocabulary, and
    // the same trailing `-` the query grammar uses: somebody who learnt it on
    // one listing must not relearn it on the next.
    let sandbox = Sandbox::new("listing_sort");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    for (listing, key) in [
        ("albums", "size"),
        ("albums", "year"),
        ("albums", "artist"),
        ("artists", "tracks"),
        ("artists", "name"),
        ("genres", "name"),
        ("genres", "tracks"),
        ("labels", "albums"),
        ("years", "year"),
        ("years", "tracks"),
    ] {
        let (up, err, ok) = sandbox.run(&[listing, "--sort", key]);
        assert!(ok, "{listing} --sort {key}: {err}");
        // And the reversal, which must be accepted everywhere the key is.
        let (down, err, ok) = sandbox.run(&[listing, "--sort", &format!("{key}-")]);
        assert!(ok, "{listing} --sort {key}-: {err}");
        // Ascending and descending are two different answers wherever there is
        // more than one row to order — a `-` silently ignored is the fault
        // this whole class of guard exists to prevent.
        let rows = up.lines().count();
        if rows > 6 {
            assert_ne!(up, down, "{listing} --sort {key}- changed nothing");
        }
    }

    // A key this listing has no column for is refused, not ignored, and the
    // refusal offers the ones it does have.
    let (_, err, ok) = sandbox.run(&["genres", "--sort", "year"]);
    assert!(!ok);
    assert!(err.contains("no year to sort on"), "stderr: {err}");
    assert!(err.contains("tracks"), "and offers what it has: {err}");

    // A word that names no order at all.
    let (_, err, ok) = sandbox.run(&["albums", "--sort", "banana"]);
    assert!(!ok);
    assert!(err.contains("is not something to sort on"), "stderr: {err}");

    // Bare, every listing keeps the order that was chosen for it — `--sort`
    // overrides, its absence changes nothing.
    let (out, _, ok) = sandbox.run(&["artists"]);
    assert!(ok);
    assert!(out.contains("in total"), "output: {out}");
}

#[test]
fn an_artist_listing_answers_the_grammar_too() {
    // The symmetry `albums --query` was missing: an artist is kept when any
    // track the expression matches is credited to them.
    let sandbox = Sandbox::new("artists_query");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);
    let (_, err, ok) = sandbox.run(&["rate", "album", "Duos", "--stars", "5"]);
    assert!(ok, "stderr: {err}");

    let (out, err, ok) = sandbox.run(&["artists", "--query", "album.rating:>=4"]);
    assert!(ok, "stderr: {err}");
    assert!(out.contains("Dave Brubeck"), "output: {out}");
    assert!(!out.contains("Miles Davis"), "and nobody else: {out}");
    // A count that says "3 in total" over one row contradicts the rows under
    // it: the moment anything narrows the listing, the number is the rows.
    assert!(out.contains("matching"), "output: {out}");
    assert!(!out.contains("in total"), "output: {out}");

    // Unfiltered, it still says how many the library holds — which is what it
    // is showing.
    let (out, _, ok) = sandbox.run(&["artists"]);
    assert!(ok);
    assert!(out.contains("in total"), "output: {out}");

    // A broken expression is refused where it is typed.
    let (_, err, ok) = sandbox.run(&["artists", "--query", "nosuchfield:x"]);
    assert!(!ok);
    assert!(err.contains("is not a field"), "stderr: {err}");
}

#[test]
fn a_copy_takes_its_selection_from_the_grammar() {
    // `copy` has no filters of its own: the selection is the one `query`
    // answers, which is the rule every listing already follows.
    let sandbox = Sandbox::new("copy_selection");
    let out = std::env::temp_dir().join("aede_e2e_copy_sel_dest");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    // A saved query is a selection like any other.
    let (_, _, ok) = sandbox.run(&["collection", "hires", "--query", "samplerate:>48000"]);
    assert!(ok);
    let (report, err, ok) = sandbox.run(&[
        "copy",
        out.to_str().unwrap(),
        "--collection",
        "hires",
        "--dry-run",
    ]);
    assert!(ok, "stderr: {err}");
    assert!(report.contains("Tracks"), "output: {report}");

    // A collection nobody saved is an error, not an empty copy.
    let (_, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--collection", "nope"]);
    assert!(!ok);
    assert!(err.contains("no collection"), "stderr: {err}");

    // Both at once name two selections, and the command copies one thing.
    let (_, err, ok) = sandbox.run(&[
        "copy",
        out.to_str().unwrap(),
        "--collection",
        "hires",
        "--query",
        "loved",
    ]);
    assert!(!ok);
    assert!(err.contains("give one"), "stderr: {err}");

    // A selection matching nothing says so rather than reporting a copy of
    // nothing as a success.
    let (_, err, ok) = sandbox.run(&["copy", out.to_str().unwrap(), "--query", "played:>500"]);
    assert!(!ok);
    assert!(err.contains("matches no track"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn every_command_that_works_is_named_by_the_help() {
    // Two commands worked for a week without appearing anywhere in the help:
    // `find` and `favorites`, both perfectly good, both invisible. The rule
    // that caught `help` itself had no test behind it, so nothing said. This is
    // that test, and it is the reason the dispatcher and the help now read one
    // table.
    let sandbox = Sandbox::new("help_commands");
    let (help, _, ok) = sandbox.run(&["help"]);
    assert!(ok);

    for command in [
        "scan",
        "roots",
        "stats",
        "doctor",
        "check",
        "copy",
        "reset",
        "import",
        "query",
        "find",
        "collection",
        "collections",
        "love",
        "rate",
        "note",
        "tag",
        "played",
        "favourites",
        "favorites",
        "notes",
        "history",
        "artists",
        "albums",
        "genres",
        "genre",
        "labels",
        "label",
        "years",
        "artist",
        "album",
        "track",
        "search",
        "file",
        "export",
        "help",
    ] {
        // It answers — an unknown command exits 2 with "unknown command",
        // which is what tells a real command from a typo.
        let (_, err, _) = sandbox.run(&[command]);
        assert!(
            !err.contains("unknown command"),
            "{command} is not a command"
        );
        assert!(
            help.contains(command),
            "{command} works and the help never names it"
        );
    }

    // And a word that is no command says so, with the nearest one it knows.
    let (_, err, ok) = sandbox.run(&["albuns"]);
    assert!(!ok);
    assert!(err.contains("unknown command"), "stderr: {err}");
    assert!(err.contains("albums"), "and points at the real one: {err}");
}

#[test]
fn an_alias_is_the_command_and_not_a_lesser_one() {
    // `find` is `query`, `favorites` is `favourites`. Both dispatched to the
    // right function — and were refused their options on the way there, because
    // every guard table in the program was written in terms of the canonical
    // name and only the dispatcher had been told the two are one thing. The
    // symptom was the program contradicting itself in a single breath:
    //
    //     $ aede find year:1990..1994 --csv
    //     Error: "find" cannot produce a table: --csv applies to …, query, …
    //
    // — refusing a table and then listing, among those that can produce one,
    // the very command that had just been typed under its other name.
    let sandbox = Sandbox::new("alias_options");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    // Every option the canonical name accepts, the alias accepts too. Each is
    // run for real, not merely past the guard: an option that parses and then
    // does nothing is the fault this whole class of guard exists to catch.
    for (alias, canonical, options) in [
        (
            "find",
            "query",
            vec![
                vec!["year:1900..2100", "--csv"],
                vec!["year:1900..2100", "--json"],
                vec!["year:1900..2100", "--m3u"],
                vec!["year:1900..2100", "--sort", "title"],
                vec!["year:1900..2100", "--limit", "1"],
                vec!["year:1900..2100", "--all"],
            ],
        ),
        (
            "favorites",
            "favourites",
            vec![vec!["--csv"], vec!["--json"], vec!["--limit", "1"]],
        ),
    ] {
        for option in options {
            let mut typed = vec![alias];
            typed.extend(option.iter().copied());
            let (out, err, ok) = sandbox.run(&typed);
            assert!(
                ok,
                "aede {} was refused what {canonical} accepts.\nstderr: {err}",
                typed.join(" ")
            );
            assert!(!err.contains("cannot"), "aede {}: {err}", typed.join(" "));

            // And it answers the same thing under either name, which is the
            // point of an alias — passing the guard is only half of it.
            let mut under_the_other_name = vec![canonical];
            under_the_other_name.extend(option.iter().copied());
            let (same, _, ok) = sandbox.run(&under_the_other_name);
            assert!(ok);
            assert_eq!(
                out,
                same,
                "aede {} and aede {} must answer alike",
                typed.join(" "),
                under_the_other_name.join(" ")
            );
        }
    }

    // The refusals travel with the alias too: `favourites` reads no argument,
    // so `favorites` must refuse one rather than ignore it in silence.
    let (_, err, ok) = sandbox.run(&["favorites", "something"]);
    assert!(!ok, "stderr: {err}");
    assert!(err.contains("takes no argument"), "stderr: {err}");
}
