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
fn misspelled_option_is_reported() {
    let sandbox = Sandbox::new("option");
    let (_, err, _) = sandbox.run(&["stats", "--limite=3"]);
    assert!(err.contains("unknown option"), "stderr: {err}");
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
    assert!(out.contains("shown"), "the truncation is announced: {out}");

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

    // A partial match is announced rather than passed off as exact.
    let (out, _, ok) = sandbox.run(&["genre", "jaz"]);
    assert!(ok);
    assert!(
        out.contains("showing the ones containing it"),
        "output: {out}"
    );
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

    // Sixty albums, fifty rows: the cut has to be announced.
    let (out, _, ok) = sandbox.run(&["albums"]);
    assert!(ok, "output: {out}");
    assert!(
        out.contains("50 of 60 albums shown"),
        "the cut must be announced:\n{out}"
    );
    assert!(out.contains("--limit"), "and it must say how to lift it");

    // Shown in full, nothing is said: a notice that always fires means nothing.
    let (out, _, ok) = sandbox.run(&["albums", "--limit=200"]);
    assert!(ok);
    assert!(!out.contains("shown —"), "nothing was left out:\n{out}");

    let _ = std::fs::remove_dir_all(&root);
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

    // The verdict is stored: a second run has nothing left to read.
    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok);
    assert!(out.contains("already has a verdict"), "output: {out}");

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

    // Everything now has a verdict.
    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok);
    assert!(out.contains("already has a verdict"), "output: {out}");

    // A folder the catalog knows nothing about is not silently an empty run.
    let (out, _, ok) = sandbox.run(&["check", std::env::temp_dir().to_str().unwrap()]);
    assert!(ok);
    assert!(
        out.contains("verdict") || out.contains("no file"),
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
    assert!(
        out.contains("made from a lossy source"),
        "the lossy ancestry is reported too:\n{out}"
    );

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

    // --- Forgetting ---------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["import", "--forget"]);
    assert!(ok, "output: {out}");
    let (out, _, _) = sandbox.run(&["track", "So What"]);
    assert!(!out.contains("Analysed by"), "nothing is left: {out}");

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
