//! Aède — command-line interface.
//!
//! Milestone M0: scan folders, build the catalog, query it.

mod args;
mod commands;
mod ui;

use args::Args;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Restores the default Unix behaviour for `SIGPIPE`.
///
/// Rust ignores that signal at startup, so an `aede stats | head` makes the
/// write fail and the program panic. We want the opposite: a quiet stop, like
/// any other command-line tool.
#[cfg(unix)]
fn restore_sigpipe() {
    const SIGPIPE: i32 = 13;
    const SIG_DFL: usize = 0;
    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    // Safe: we merely restore the default handler.
    unsafe {
        signal(SIGPIPE, SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() {
    restore_sigpipe();
    let args = Args::from_env();
    ui::init_color(args.has("no-color"));

    // Before anything answers, including `--help` and `--version`: an option
    // nobody recognises makes the whole command line untrustworthy, and
    // `aede --fegioregj` printing a cheerful help page is the same silence in
    // a friendlier costume.
    const OPTIONS: &[&str] = &[
        "data",
        "replace",
        "remove",
        "limit",
        "sort",
        "severity",
        "artist",
        "album",
        "with",
        "separator",
        "csv",
        "tracks",
        "m3u",
        "year",
        "output",
        "threads",
        "genre",
        "label",
        "json",
        "no-color",
        "yes",
        "forget",
        "source",
        "compilations",
        "no-compilations",
        "role",
        "comment",
        "comments",
        "offset",
        "all",
        "help",
        "version",
        "full",
        "follow-symlinks",
        "include-hidden",
    ];
    let unknown = args.unknown_flags(OPTIONS);
    if !unknown.is_empty() {
        for option in &unknown {
            eprintln!("{} unknown option: {option}", ui::red("Error:"));
            if let Some(near) = args::nearest(option, OPTIONS) {
                eprintln!("Did you mean {near}?");
            }
        }
        eprintln!("Run \"aede help\" for the list of options.");
        std::process::exit(2);
    }

    // An option that expects a value and was given none cannot be honoured.
    // Carrying on would answer as if the option had never been typed, which
    // is a wrong answer rather than a missing one. Checked before `--help`,
    // for the same reason the unknown options are: a command line that did not
    // parse gets an error, not a friendly page about something else.
    let missing = args.options_missing_a_value();
    if !missing.is_empty() {
        for name in &missing {
            eprintln!(
                "{} option --{name} expects a value: --{name}=…",
                ui::red("Error:")
            );
            // `aede --data` is how someone asks where their catalog is, so the
            // message answers that rather than only complaining about it.
            if *name == "data" {
                eprintln!(
                    "Its catalog is currently in {}",
                    commands::data_dir(&args).display()
                );
                // The location can be changed for good, and the variable that
                // does it is the least discoverable thing in the program.
                eprintln!("--data moves it for one command; AEDE_HOME moves it for good.");
            }
        }
        std::process::exit(2);
    }

    if args.has("version") {
        println!("aede {VERSION}");
        return;
    }
    if args.has("help") {
        print_help();
        return;
    }
    if args.command.is_empty() {
        // Running the program with nothing at all is a request for the help,
        // and that is worth keeping. Running it with options and no command is
        // not: `aede --data ~/music` named a catalog, did nothing with it,
        // printed the help and reported success — the options going into the
        // void exactly as a swallowed argument does.
        //
        // Except the ones that only shape what is printed: `--no-color` has
        // the help itself to act on, so it is not left with nothing.
        let idle = args.options_given_except(PRESENTATION_OPTIONS);
        if idle.is_empty() {
            print_help();
            return;
        }
        eprintln!(
            "{} no command to apply {} to.",
            ui::red("Error:"),
            idle.join(", ")
        );
        eprintln!("An option shapes what a command does; alone it does nothing.");
        // Naming the commands is not always enough — the same reason `--role`
        // carries a hint. `--data <folder>` is the one option in the program
        // that takes a folder without meaning "read the music in it", and
        // `aede --data ~/Music` is what someone types who reads it that way.
        // The error that only says "no command" leaves them exactly there.
        if let Some(folder) = args.value("data") {
            eprintln!("--data says where the catalog is kept, not what to read:");
            eprintln!("  aede scan {folder}              reads the music in that folder");
            eprintln!("  aede --data {folder} stats      uses a catalog kept there");
        }
        eprintln!("Run \"aede help\" for the list of commands.");
        std::process::exit(2);
    }

    // An option a command cannot honour is refused rather than ignored. The
    // global list above only says an option exists; this says where it means
    // something, which is what stops `aede stats --csv` from printing a table
    // that is not one.
    for (option, commands, what) in [
        ("csv", CSV_COMMANDS, "produce a table"),
        ("m3u", M3U_COMMANDS, "produce a playlist"),
        ("output", OUTPUT_COMMANDS, "write to a file"),
        ("forget", IMPORT_COMMANDS, "forget an analysis"),
        ("source", IMPORT_COMMANDS, "select a source"),
        (
            "compilations",
            ALBUM_LIST_COMMANDS,
            "single out compilations",
        ),
        (
            "no-compilations",
            ALBUM_LIST_COMMANDS,
            "leave compilations out",
        ),
        ("role", ROLE_COMMANDS, "filter by role"),
        ("artist", ARTIST_COMMANDS, "narrow to one artist"),
        ("year", YEAR_COMMANDS, "narrow to one year"),
        ("genre", GENRE_COMMANDS, "filter by genre"),
        ("label", LABEL_COMMANDS, "filter by label"),
        ("comment", COMMENT_COMMANDS, "filter on the comments"),
        ("comments", &["search"], "search the comments"),
        ("limit", PAGING_COMMANDS, "show a window of its result"),
        ("offset", PAGING_COMMANDS, "start further down its result"),
        ("all", PAGING_COMMANDS, "drop the row limit"),
        ("json", JSON_COMMANDS, "answer in JSON"),
        ("separator", CSV_COMMANDS, "choose a separator"),
        ("sort", SORT_COMMANDS, "be sorted"),
        ("severity", DOCTOR_COMMANDS, "filter by severity"),
        ("album", &["track"], "narrow to one album"),
        ("with", &["artist"], "cross two artists"),
        ("tracks", &["export"], "switch to one row per track"),
        ("yes", &["reset"], "skip the confirmation"),
        ("remove", &["roots"], "drop a folder"),
        ("full", &["scan", "check"], "ignore what was already done"),
        ("threads", &["scan", "check"], "read on several threads"),
        ("replace", &["scan"], "forget the watched folders"),
        ("follow-symlinks", &["scan"], "follow symbolic links"),
        ("include-hidden", &["scan"], "walk hidden files"),
    ] {
        if args.has(option) && !commands.contains(&args.command.as_str()) {
            eprintln!(
                "{} \"{}\" cannot {what}: --{option} applies to {}",
                ui::red("Error:"),
                args.command,
                commands.join(", ")
            );
            // Naming the commands is not always enough. A role means nothing
            // without a person, and someone typing `album "X" --role performer`
            // is after the people, not the album.
            if option == "role" {
                eprintln!(
                    "A role needs a person: aede artist \"<name>\" --role {}",
                    args.value("role").unwrap_or("<role>")
                );
            }
            std::process::exit(2);
        }
    }

    // Some options mean nothing on their own, only alongside another. The
    // command being right is not enough: `aede export --tracks` without `--csv`
    // writes the JSON dump and drops the option, and `aede albums --separator=;`
    // prints a table nobody asked to separate. The last corner of the same
    // fault, and the one the table above cannot see, since it only knows which
    // commands an option reaches, not what it needs once there.
    for (option, needs) in [("separator", "csv"), ("tracks", "csv")] {
        if args.has(option) && !args.has(needs) {
            eprintln!(
                "{} --{option} means nothing without --{needs}.",
                ui::red("Error:")
            );
            eprintln!("Add --{needs}, or drop --{option}.");
            std::process::exit(2);
        }
    }

    // A command that reads no positional must refuse one rather than answer as
    // though nothing had been typed. `aede artists ozzy --role producer` used
    // to list every producer in the library, "ozzy" going into the void — the
    // same fault as an option silently ignored, and the answer looks right,
    // which is what makes it worse.
    if let Some(hint) = takes_no_argument(&args.command)
        && !args.positionals.is_empty()
    {
        eprintln!(
            "{} \"{}\" takes no argument: \"{}\" was ignored.\n{hint}",
            ui::red("Error:"),
            args.command,
            args.positionals.join(" ")
        );
        std::process::exit(2);
    }

    let result = match args.command.as_str() {
        "scan" => commands::scan(&args),
        "roots" => commands::roots(&args),
        "stats" => commands::show_stats(&args),
        "doctor" => commands::show_doctor(&args),
        "check" => commands::check(&args),
        "reset" => commands::reset(&args),
        "import" => commands::import(&args),
        "artists" => commands::list_artists(&args),
        "albums" => commands::list_albums(&args),
        "genres" => commands::list_genres(&args),
        "genre" => commands::show_genre(&args),
        "labels" => commands::list_labels(&args),
        "label" => commands::show_label(&args),
        "years" => commands::list_years(&args),
        "artist" => commands::show_artist(&args),
        "album" => commands::show_album(&args),
        "track" => commands::show_track(&args),
        "search" => commands::search(&args),
        "file" => commands::inspect(&args),
        "export" => commands::export(&args),
        "help" => {
            print_help();
            Ok(())
        }
        other => {
            eprintln!("{} unknown command: \"{other}\"", ui::red("Error:"));
            eprintln!("Run \"aede help\" for the list of commands.");
            std::process::exit(2);
        }
    };

    if let Err(error) = result {
        eprintln!("{} {error}", ui::red("Error:"));
        std::process::exit(1);
    }
}

/// Commands that can render what they show as a CSV table.
const CSV_COMMANDS: &[&str] = &[
    "export", "album", "artist", "track", "genre", "label", "search", "albums", "artists",
    "genres", "labels", "years",
];

/// Commands that show tracks, and can therefore hand them to a player.
const M3U_COMMANDS: &[&str] = &["album", "artist", "track", "genre", "label", "search"];

/// Commands that act on what was imported from another tool.
const IMPORT_COMMANDS: &[&str] = &["import"];

/// The one command that lists releases and can therefore sort compilations
/// from the rest.
const ALBUM_LIST_COMMANDS: &[&str] = &["albums"];

/// Where a role means something: the listing narrows to the people who hold
/// one, the page narrows to what that person did in it. Two readings of the
/// same word, both useful, and neither of them makes sense without a person —
/// which is why `album` and `track` are not here: there, `--artist` is the
/// filter, and a role with nobody attached asks nothing.
const ROLE_COMMANDS: &[&str] = &["artists", "artist"];

/// Options that shape what is printed rather than what is answered, and so are
/// not left with nothing to do when no command follows them.
const PRESENTATION_OPTIONS: &[&str] = &["no-color"];

/// Commands that can answer in JSON.
///
/// Everything that can render a CSV can render a JSON of the same rows — they
/// go through one function — plus the two that have a shape of their own.
/// `--json` used to be declared globally and read by four commands, so
/// `aede albums --json` printed the ordinary table and dropped the word.
const JSON_COMMANDS: &[&str] = &[
    "export", "album", "artist", "track", "genre", "label", "search", "albums", "artists",
    "genres", "labels", "years", "stats", "doctor",
];

/// The one listing whose order can be chosen.
const SORT_COMMANDS: &[&str] = &["artists"];

/// The one command that reports issues, and can therefore filter them.
const DOCTOR_COMMANDS: &[&str] = &["doctor"];

/// Commands that can be narrowed to one artist, or to one year.
///
/// Both were declared among the options and guarded nowhere: `aede artists
/// --year=1969` answered about every year under a name that promised one. The
/// help says where a filter applies; this is what makes that true.
const ARTIST_COMMANDS: &[&str] = &["albums", "track"];
const YEAR_COMMANDS: &[&str] = &["albums"];

/// Commands that can be narrowed to one genre, or to one label.
const GENRE_COMMANDS: &[&str] = &["albums"];
const LABEL_COMMANDS: &[&str] = &["albums"];

/// Commands that can be narrowed to what a comment says.
const COMMENT_COMMANDS: &[&str] = &["albums", "track"];

/// Commands that read nothing but their options, and what to do instead.
///
/// `None` for the commands that do take an argument. The hint names the
/// singular command when there is one, since typing the plural with a name is
/// almost always a reach for the page rather than for the list.
fn takes_no_argument(command: &str) -> Option<&'static str> {
    Some(match command {
        "artists" => {
            "For one artist: aede artist \"<name>\"\n\
             To narrow the list: --role, --sort, --limit"
        }
        "albums" => {
            "For one album: aede album \"<title>\"\n\
             To narrow the list: --artist, --year, --genre, --label, --compilations"
        }
        "genres" => "For one genre: aede genre <name>",
        "labels" => "For one label: aede label \"<name>\"",
        "years" => "For one year: aede albums --year=<year>",
        "stats" | "doctor" | "roots" => "It describes the whole catalog.",
        // `aede help scan` reads like a request for one command's page, and
        // there is no such page: printing the whole help as though the word had
        // not been typed answers a question that was not asked.
        "help" => "It prints the whole help; there is no page per command.",
        _ => return None,
    })
}

/// Commands that show a bounded number of rows, and can therefore be paged.
///
/// `--limit` was global and honoured only here; on `scan` or `reset` it was
/// accepted and ignored like any other option nobody listed.
const PAGING_COMMANDS: &[&str] = &[
    "albums", "artists", "genres", "labels", "album", "artist", "track", "genre", "label",
    "search", "doctor", "stats",
];

/// Commands whose output can go to a file instead of the terminal.
const OUTPUT_COMMANDS: &[&str] = &[
    "export", "album", "artist", "track", "genre", "label", "search", "albums", "artists",
    "genres", "labels", "years",
];

fn print_help() {
    println!(
        "{}",
        ui::bold(&format!("aede {VERSION} — local music library"))
    );
    println!(
        "
{}
  aede <command> [options]

{}
  scan [folder…]       Scan the watched folders; any folder given is added to them
  roots                List the watched folders (--remove <folder> to drop one)
  stats                Library statistics
  doctor               Diagnosis: missing tags, duplicates, incomplete albums
                       (--severity=error|warning|info)
  check [folder…]      Verify the checksums the files carry, all of them or
                       only those under the folders given (--full re-verifies)

  artists              List of artists (--role composer, producer…,
                       --sort tracks|name)
  albums               List of albums (--artist, --year, --genre, --label,
                       --comment, --compilations, --no-compilations)
  genres               List of genres
  labels               List of labels
  years                Breakdown by year

  artist <name>        Artist card: discography, collaborations
                       (--with=<other> lists the tracks the two share)
  album <title>        Album card: tracks and credits
  track <title>        Track card: album, credits, technical details, tags
  genre <name>         Genre page: albums and artists carrying it
  label <name>         Label page: its catalogue and its artists
  search <text>        Search the whole catalog (--comments looks there too)
  file <path>          Inspect a single file, outside the catalog
  import <report…>     Take in FlacCompagnon reports (--forget removes them)
  reset                Remove the catalog, after confirmation (--yes skips it)
  export               Export the catalog as JSON, or as CSV with --csv
                       (one row per album; --tracks for one row per track)
  help                 This page, which is also what running aede alone shows

{}
  --data <folder>      Catalog location
                       (default: $AEDE_HOME or ~/.local/share/aede)
  --limit <n>          Number of rows displayed
  --offset <n>         Rows skipped first, to walk a result page by page
  --all                Every row, however many there are
  --json               Machine-readable output: the same rows as --csv,
                       plus stats, doctor, search and track
  --csv                Spreadsheet output: export, the listings, and any
                       selection (--separator=; or tab)
  --m3u                Playlist of the tracks shown (album, artist, track,
                       search); --output=<file> writes it instead of printing
  -o, --output <file>  Write to a file rather than to standard output
  --no-color           Turn colours off
  -h, --help           Show this help
  -V, --version        Show the version

{}
  --full               Ignore the tag cache and re-read every file
                       (scan, check)
  --replace            Forget the watched folders and keep only those given
  --threads <n>        Number of reader threads (scan, check;
                       default: available cores)
  --follow-symlinks    Follow symbolic links
  --include-hidden     Include hidden files and folders

{}
  Each says where it applies; a command that cannot honour one refuses it.
  --artist <name>      Of one artist (albums, track)
  --year <year>        Of one year (albums)
  --genre <name>       Carrying a genre (albums)
  --label <name>       Published under a label (albums)
  --compilations       Only what several artists share (albums)
  --no-compilations    Everything except those (albums)
  --comment <text>     Only what a comment mentions (albums, track)
  --comments           Search the comment tag as well (search)
  --role <role>        On artists: who is credited that way.
                       On artist <name>: what they did in that role.
  --album <title>      Of one album (track)
  --with <name>        The tracks two artists share (artist)
  --severity <level>   error, warning or info (doctor)
  --sort <order>       tracks or name (artists)

{}
  --forget             Remove the imported analyses instead of adding any
  --source <name>      Restrict --forget to one tool (default: all of them)

{}
  aede scan ~/Music
  aede stats
  aede doctor --severity=error --limit=50
  aede artist \"Miles Davis\"
  aede track \"So What\" --artist=\"Miles Davis\"
  aede albums --year=1969
  aede albums --compilations
  aede genre metal
  aede artists --role producer
  aede artist Ozzy --role performer --m3u
  aede search --comments \"vinyl rip\" --m3u
  aede search coltrane
  aede albums --limit 50 --offset 50
  aede albums --all -o everything.csv --csv
  aede import ~/Desktop/report.json",
        ui::cyan("USAGE"),
        ui::cyan("COMMANDS"),
        ui::cyan("GLOBAL OPTIONS"),
        ui::cyan("SCAN OPTIONS"),
        ui::cyan("FILTER OPTIONS"),
        ui::cyan("IMPORT OPTIONS"),
        ui::cyan("EXAMPLES")
    );
}
