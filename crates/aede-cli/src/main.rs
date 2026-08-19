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
    ] {
        if args.has(option) && !commands.contains(&args.command.as_str()) {
            eprintln!(
                "{} \"{}\" cannot {what}: --{option} applies to {}",
                ui::red("Error:"),
                args.command,
                commands.join(", ")
            );
            std::process::exit(2);
        }
    }

    let result = match args.command.as_str() {
        "scan" => commands::scan(&args),
        "roots" => commands::roots(&args),
        "stats" => commands::show_stats(&args),
        "doctor" => commands::show_doctor(&args),
        "check" => commands::check(&args),
        "artists" => commands::list_artists(&args),
        "albums" => commands::list_albums(&args),
        "genres" => commands::list_genres(&args),
        "labels" => commands::list_labels(&args),
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
    "export", "album", "artist", "track", "search", "albums", "artists", "genres", "labels",
    "years",
];

/// Commands that show tracks, and can therefore hand them to a player.
const M3U_COMMANDS: &[&str] = &["album", "artist", "track", "search"];

/// Commands whose output can go to a file instead of the terminal.
const OUTPUT_COMMANDS: &[&str] = &[
    "export", "album", "artist", "track", "search", "albums", "artists", "genres", "labels",
    "years",
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

  artists              List of artists
  albums               List of albums
  genres               List of genres
  labels               List of labels
  years                Breakdown by year

  artist <name>        Artist card: discography, collaborations
                       (--with=<other> lists the tracks the two share)
  album <title>        Album card: tracks and credits
  track <title>        Track card: album, credits, technical details, tags
  search <text>        Search the whole catalog
  file <path>          Inspect a single file, outside the catalog
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
  aede scan ~/Music
  aede stats
  aede doctor --severity=error --limit=50
  aede artist \"Miles Davis\"
  aede track \"So What\" --artist=\"Miles Davis\"
  aede albums --year=1969
  aede search coltrane",
        ui::cyan("USAGE"),
        ui::cyan("COMMANDS"),
        ui::cyan("GLOBAL OPTIONS"),
        ui::cyan("SCAN OPTIONS"),
        ui::cyan("EXAMPLES")
    );
}
