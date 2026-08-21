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

    if args.has("version") {
        println!("aede {VERSION}");
        return;
    }
    if args.has("help") || args.command.is_empty() {
        print_help();
        return;
    }

    // A misspelled option has to be visible: without this, `--limite=5`
    // instead of `--limit=5` would be silently ignored.
    const OPTIONS: &[&str] = &[
        "data",
        "replace",
        "remove",
        "limit",
        "sort",
        "type",
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
        "help",
        "version",
        "full",
        "follow-symlinks",
        "include-hidden",
    ];
    for unknown in args.unknown_flags(OPTIONS) {
        eprintln!(
            "{} unknown option ignored: --{unknown}",
            ui::yellow("Warning:")
        );
    }

    // An option that expects a value and was given none cannot be honoured.
    // Carrying on would answer as if the option had never been typed, which
    // is a wrong answer rather than a missing one.
    let missing = args.options_missing_a_value();
    if !missing.is_empty() {
        for name in &missing {
            eprintln!(
                "{} option --{name} expects a value: --{name}=…",
                ui::red("Error:")
            );
        }
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
        ("genre", GENRE_COMMANDS, "filter by genre"),
        ("label", LABEL_COMMANDS, "filter by label"),
        ("comment", COMMENT_COMMANDS, "filter on the comments"),
        ("comments", &["search"], "search the comments"),
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
        _ => return None,
    })
}

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
  check [folder…]      Verify the checksums the files carry, all of them or
                       only those under the folders given (--full re-verifies)

  artists              List of artists (--role composer, producer…)
  albums               List of albums (--compilations, --genre, --label)
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

{}
  --data <folder>      Catalog location
                       (default: $AEDE_HOME or ~/.local/share/aede)
  --limit <n>          Number of rows displayed
  --json               Machine-readable output (stats, doctor, search, track)
  --csv                Spreadsheet output: export, the listings, and any
                       selection (--separator=; or tab)
  --m3u                Playlist of the tracks shown (album, artist, track,
                       search); --output=<file> writes it instead of printing
  --output <file>      Write to a file rather than to standard output
  --no-color           Turn colours off
  -h, --help           Show this help
  -V, --version        Show the version

{}
  --full               Ignore the tag cache and re-read every file
  --replace            Forget the watched folders and keep only those given
  --threads <n>        Number of reader threads (default: available cores)
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
