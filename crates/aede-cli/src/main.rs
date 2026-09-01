//! Aède — command-line interface.
//!
//! Milestone M0.6: scan folders, build the catalog, query it, record what the
//! user makes of it, and copy a selection out to a player or a card.

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
        "pending",
        "list",
        "no-scan",
        "lyrics",
        "simple",
        "artists",
        "extras",
        "dry-run",
        "verify",
        "safe-names",
        "raw-names",
        "collection",
        "compress",
        "quality",
        "source",
        "compilations",
        "no-compilations",
        "role",
        "comment",
        "comments",
        "notes",
        "offset",
        "all",
        "help",
        "version",
        "full",
        "follow-symlinks",
        "include-hidden",
        "exclude",
        "stars",
        "text",
        "from",
        "tag",
        "file",
        "append",
        "query",
        "export",
        "import",
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

    // An alias **is** the command, and every table below is written in terms of
    // the one name. Resolved once, here, rather than spelled twice in each of
    // the eight lists: `aede find year:1990..1994 --csv` was refused a table
    // that `aede query` produced happily, and the refusal helpfully listed
    // `query` among the commands that can — the program contradicting itself in
    // one breath, because only the dispatcher had been told the two are one
    // thing. Adding an alias must not mean revisiting eight tables to keep it
    // from becoming a second-class command.
    let command = canonical(&args.command);

    // An option a command cannot honour is refused rather than ignored. The
    // global list above only says an option exists; this says where it means
    // something, which is what stops `aede stats --csv` from printing a table
    // that is not one.
    for (option, commands, what) in [
        ("csv", CSV_COMMANDS, "produce a table"),
        ("m3u", M3U_COMMANDS, "produce a playlist"),
        ("output", OUTPUT_COMMANDS, "write to a file"),
        ("forget", IMPORT_COMMANDS, "forget an analysis"),
        (
            "pending",
            IMPORT_COMMANDS,
            "list or restrict to what is waiting",
        ),
        ("list", IMPORT_COMMANDS, "list what was imported"),
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
        ("notes", &["search"], "search what you wrote"),
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
        ("yes", &["reset", "history"], "skip the confirmation"),
        (
            "remove",
            &[
                "roots",
                "love",
                "rate",
                "note",
                "tag",
                "collection",
                "played",
                "history",
            ],
            "take something back",
        ),
        (
            "full",
            &["scan", "check", "spectrum"],
            "ignore what was already done",
        ),
        (
            "threads",
            &["scan", "check", "spectrum", "copy"],
            "read on several threads",
        ),
        ("replace", &["scan", "copy"], "forget the watched folders"),
        ("exclude", &["roots"], "keep a folder out of the catalog"),
        (
            "no-scan",
            &["roots"],
            "leave the catalog untouched until the next scan",
        ),
        ("follow-symlinks", &["scan"], "follow symbolic links"),
        ("include-hidden", &["scan"], "walk hidden files"),
        ("stars", &["rate"], "carry a rating"),
        ("text", &["note"], "carry a note"),
        ("file", &["note"], "read a note from a file"),
        ("append", &["note"], "add to a note"),
        (
            "query",
            &["collection", "copy", "albums", "artists"],
            "hold an expression",
        ),
        ("extras", &["copy"], "choose what travels beside the audio"),
        (
            "dry-run",
            &["copy", "spectrum", "playlist"],
            "say what it would do without doing it",
        ),
        (
            "lyrics",
            &["track", "search"],
            "show the words, or look in them",
        ),
        ("simple", &["playlist"], "leave out the #EXTINF lines"),
        (
            "artists",
            &["playlist"],
            "write one playlist per artist too",
        ),
        ("verify", &["copy"], "read back what it wrote"),
        ("safe-names", &["copy"], "adapt names to the destination"),
        ("raw-names", &["copy"], "leave names exactly as they are"),
        (
            "collection",
            &["copy"],
            "take its selection from a saved query",
        ),
        ("export", &["notes"], "write what you wrote to a file"),
        ("import", &["notes"], "take back in what was exported"),
        ("from", &["note"], "copy what was said elsewhere"),
        ("tag", &["notes"], "filter on a tag"),
    ] {
        if args.has(option) && !commands.contains(&command) {
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
    // `roots --remove <folder>` is the one place a listing does read an
    // argument, because the folder is what is being removed.
    let reads_an_argument = command == "roots" && args.has("remove");
    if let Some(hint) = takes_no_argument(command)
        && !args.positionals.is_empty()
        && !reads_an_argument
    {
        eprintln!(
            "{} \"{}\" takes no argument: \"{}\" was ignored.\n{hint}",
            ui::red("Error:"),
            args.command,
            args.positionals.join(" ")
        );
        std::process::exit(2);
    }

    let Some((_, _, run)) = COMMANDS
        .iter()
        .find(|(name, alias, _)| *name == args.command || alias.is_some_and(|a| a == args.command))
    else {
        eprintln!(
            "{} unknown command: \"{}\"",
            ui::red("Error:"),
            args.command
        );
        if let Some(near) = args::nearest(
            &args.command,
            &COMMANDS
                .iter()
                .map(|(name, _, _)| *name)
                .collect::<Vec<_>>(),
        ) {
            eprintln!("Did you mean {}?", near.trim_start_matches('-'));
        }
        eprintln!("Run \"aede help\" for the list of commands.");
        std::process::exit(2);
    };
    let result = run(&args);

    if let Err(error) = result {
        eprintln!("{} {error}", ui::red("Error:"));
        std::process::exit(1);
    }
}

/// Commands that can render what they show as a CSV table.
const CSV_COMMANDS: &[&str] = &[
    "export",
    "album",
    "artist",
    "track",
    "genre",
    "label",
    "search",
    "albums",
    "artists",
    "genres",
    "labels",
    "years",
    "favourites",
    "notes",
    "query",
    "collection",
];

/// Commands that show tracks, and can therefore hand them to a player.
const M3U_COMMANDS: &[&str] = &[
    "album",
    "artist",
    "track",
    "genre",
    "label",
    "search",
    "query",
    "collection",
];

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

/// Every command, its alias if it has one, and what it runs.
///
/// **One table, not a `match` and a help page that drift apart.** Two commands
/// worked for a week without appearing in the help — `find` and `favorites`,
/// both perfectly good, both invisible. The rule that caught `help` itself
/// ("a command that works is a command the help names") had no test behind it,
/// so nothing said. Now the dispatcher reads this list, the help is checked
/// against it, and a command added in one place cannot hide from the other.
type Command = fn(&Args) -> commands::Res;
const COMMANDS: &[(&str, Option<&str>, Command)] = &[
    ("scan", None, commands::scan),
    ("roots", None, commands::roots),
    ("stats", None, commands::show_stats),
    ("doctor", None, commands::show_doctor),
    ("check", None, commands::check),
    ("copy", None, commands::copy),
    ("spectrum", None, commands::spectrum),
    ("playlist", None, commands::playlist),
    ("reset", None, commands::reset),
    ("import", None, commands::import),
    ("query", Some("find"), commands::query),
    ("collection", None, commands::collection),
    ("collections", None, commands::collections),
    ("love", None, commands::love),
    ("rate", None, commands::rate),
    ("note", None, commands::note),
    ("tag", None, commands::tag),
    ("played", None, commands::played),
    ("favourites", Some("favorites"), commands::favourites),
    ("notes", None, commands::notes),
    ("history", None, commands::history),
    ("artists", None, commands::list_artists),
    ("albums", None, commands::list_albums),
    ("genres", None, commands::list_genres),
    ("genre", None, commands::show_genre),
    ("labels", None, commands::list_labels),
    ("label", None, commands::show_label),
    ("years", None, commands::list_years),
    ("artist", None, commands::show_artist),
    ("album", None, commands::show_album),
    ("track", None, commands::show_track),
    ("search", None, commands::search),
    ("file", None, commands::inspect),
    ("export", None, commands::export),
    ("help", None, run_help),
];

/// The one name a command is known by everywhere except on the command line.
///
/// [`COMMANDS`] is the only place an alias is written down, and this is what
/// keeps it that way. Every guard table below lists canonical names; a word
/// that is not an alias is already canonical and comes back unchanged, so the
/// caller never has to know which it was given.
///
/// Without it an alias dispatched correctly and was refused everything on the
/// way there — `aede find … --csv` answered that "find" cannot produce a table
/// and then listed `query`, which is the same command. An alias that is not the
/// command in *every* table is not an alias, it is a trap.
fn canonical(typed: &str) -> &str {
    COMMANDS
        .iter()
        .find(|(_, alias, _)| alias.is_some_and(|a| a == typed))
        .map(|(name, _, _)| *name)
        .unwrap_or(typed)
}

/// `help` as a function, so that it sits in the table like every other command
/// rather than being a special case the table could forget.
fn run_help(_args: &Args) -> commands::Res {
    print_help();
    Ok(())
}

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
    "export",
    "album",
    "artist",
    "track",
    "genre",
    "label",
    "search",
    "albums",
    "artists",
    "genres",
    "labels",
    "years",
    "stats",
    "doctor",
    "favourites",
    "notes",
    "query",
    "collection",
];

/// The one listing whose order can be chosen.
const SORT_COMMANDS: &[&str] = &[
    "artists",
    "albums",
    "genres",
    "labels",
    "years",
    "query",
    "collection",
];

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
        "collections" => {
            "It lists what you saved. To save one: aede collection <name> --query \"…\""
        }
        "favourites" | "notes" | "history" => {
            "It lists what you wrote. To write: aede love|rate|note <kind> \"<name>\""
        }
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
    "albums",
    "artists",
    "genres",
    "labels",
    "album",
    "artist",
    "track",
    "genre",
    "label",
    "search",
    "doctor",
    "stats",
    "favourites",
    "notes",
    "history",
    "query",
    "collection",
    "import",
];

/// Commands whose output can go to a file instead of the terminal.
const OUTPUT_COMMANDS: &[&str] = &[
    "export",
    "album",
    "artist",
    "track",
    "genre",
    "label",
    "search",
    "albums",
    "artists",
    "genres",
    "labels",
    "years",
    "favourites",
    "notes",
    "query",
    "collection",
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
  roots                List the watched folders and the ones never read
                       (--remove <folder> drops a watched folder;
                       --exclude <folder> keeps one out of the catalog for
                       good, --exclude <folder> --remove reads it again).
                       Any of the three rescans straight away so the change
                       takes effect; --no-scan leaves that for later
  stats                Library statistics
  doctor               Diagnosis: missing tags, duplicates, incomplete albums
                       (--severity=error|warning|info)
  copy <destination>   Copy a selection somewhere that is not a library — a
                       player, a card, a drive — keeping its folder tree.
                       --query or --collection choose what; without either,
                       the whole library. --extras none|cover|images|all
                       (default: cover), --verify reads back what it wrote,
                       --dry-run says what it would do and writes nothing.
                       --compress <format> encodes on the way out, through
                       ffmpeg, several files at a time; what is already
                       compressed is copied as it is. A plain copy writes one
                       file at a time — one card is one queue — and --threads
                       overrides that either way
  check [folder…]      Verify the checksums the files carry, all of them or
                       only those under the folders given (--full re-verifies).
                       Nothing left to check prints the current report instead
  spectrum [folder…]   Draw a spectrogram of every track into a spectrograms/
                       folder beside it, through ffmpeg, several at a time.
                       Only what is missing or older than its track is drawn,
                       so a second run over an unchanged library draws nothing
                       (--full redraws everything, --dry-run only says what it
                       would draw, --threads sets how many run at once)
  playlist [folder…]   Write an .m3u in every album folder, in album order and
                       with relative paths. --simple leaves out the #EXTINF
                       lines for players that choke on them, --artists adds one
                       per artist folder covering their whole discography,
                       --dry-run only says what it would write

  artists              List of artists (--role composer, producer…,
                       --sort tracks|name)
  albums               List of albums (--artist, --year, --genre, --label,
                       --comment, --compilations, --no-compilations).
                       --query narrows it by anything the grammar can say:
                       aede albums --query \"album.rating:>=4\"
  genres               List of genres
  labels               List of labels
  years                Breakdown by year

  artist <name>        Artist card: discography, collaborations
                       (--with=<other> lists the tracks the two share)
  album <title>        Album card: tracks and credits
  track <title>        Track card: album, credits, technical details, tags
                       (--lyrics adds the words, from the tags or from a .lrc
                       file sitting beside the track)
  genre <name>         Genre page: albums and artists carrying it
  label <name>         Label page: its catalogue and its artists
  search <text>        Search the whole catalog. --comments also looks in the
                       comment tag, --notes in what you wrote yourself,
                       --lyrics in the words of the songs
  file <path>          Inspect a single file, outside the catalog
  import <report…>     Take in FlacCompagnon reports. --list says what is
                       held and what became of it, --pending lists the
                       folders whose analyses match no file yet, --forget
                       removes analyses; --forget --pending [folder…] drops
                       only what is waiting, and keeps what did attach
  reset                Remove the catalog, after confirmation (--yes skips it)
  export               Export the catalog as JSON, or as CSV with --csv
                       (one row per album; --tracks for one row per track)
  query <expression>   (also: find) Every track an expression matches; the
                       result is a selection, so --csv, --json and --m3u apply
                         genre:metal year:1990..1999 -label:earache
                         (artist:ozzy OR artist:dio) album.rating:>=4 played:0
                       From the tags: title artist album albumartist genre
                       label comment path codec year duration size bitrate
                       samplerate lossless compilation
                       What you wrote: rating loved tag note played — each
                       also as album.<field> and artist.<field>, since stars
                       on a track and on its album are different claims
                       A scope is part of the question: a bare rating, loved,
                       tag or note asks about the **track**, and what you
                       wrote on an album is album.<field>. An answer that
                       finds nothing says where it actually is.
                         tag:vinyl                 the track carries it
                         album.tag:vinyl           its album does
                         note:remaster             the note says so
                         rating:>=4  album.rating:5  loved
                       A field alone asks whether there is one at all, and
                       -field asks the opposite:
                         note        what you have written a note on
                         -rating     what you have never rated
                       Who did what: composer, lyricist, producer, engineer,
                       performer, conductor, remixer, featured, mainartist,
                       performing
  love <kind> <name>   Mark a favourite (--remove takes it back)
  rate <kind> <name>   Give it 1 to 5 stars: --stars 4, or --remove
  note <kind> <name>   Write a note. One note per thing, kept as typed.
                       --text <words>, or --file <path> (- reads a pipe),
                       --append adds to it, --remove takes it away,
                       --from <reference> copies another one.
                       With none of those, it reads the note back
  tag <kind> <name> <label[,label…]>
                       Attach free labels, several at once: vinyl,rare
                       --remove takes off the ones named, or every one of
                       them when none is named
  played <track>       Record a listen, until playback records its own
                       (--remove takes back the most recent one)
  collection <name>    Save a query under a name (--query), run it, or
                       drop it with --remove. It keeps the question, not the
                       answer, so it says what the library holds now
  collections          The saved queries, and how much each one holds now
  favourites           (also: favorites) Everything marked a favourite
  notes                Everything written (--tag <label> narrows)
                       --export writes it all out, --import <file> merges it
                       back in — never replaces
  history              What was played, most recent first
                       (--remove forgets the lot, after confirmation)
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
  --exclude <folder>   Never read this folder (roots). Kept in the catalog,
                       so a plain `aede scan` goes on honouring it

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
  --notes              Search your own notes as well (search)
  --stars <1-5>        How many stars (rate)
  --text <words>       The note itself (note)
  --file <path>        Read the note from a file, or from a pipe with - (note)
  --append             Add to the note instead of replacing it (note)
  --query <expression> An expression from the query grammar (collection,
                       copy, albums). On albums it keeps those holding a
                       track it matches: --query \"album.rating:>=4\"
  --export             Write out everything you wrote (notes)
  --import <file>      Merge a previous export back in (notes)
  --from <reference>   Copy what was said about another thing (note)
  --tag <label>        Only what carries this label (notes)
  --remove             Take back what was set (love, rate, note, tag, roots,
                       played, history); on tag with no label named, takes
                       off every one
  --role <role>        On artists: who is credited that way.
                       On artist <name>: what they did in that role.
  --album <title>      Of one album (track)
  --with <name>        The tracks two artists share (artist)
  --severity <level>   error, warning or info (doctor)
  --sort <order>       On the listings: name, artist, tracks, albums,
                       duration, size, year — each listing accepts the ones
                       it has a column for. On query and collection: title,
                       artist, album, year, duration, size, rating, played,
                       catalog. A trailing - reverses it, everywhere

{}
  --extras <what>      What travels beside the audio: none, cover (default),
                       images, all. The cover is the one the catalog picked,
                       so it leaves spectrograms and booklet scans behind
  --collection <name>  Copy what a saved query holds
  --verify             Read each file back and compare it with the source
  --dry-run            Say what would be copied, and write nothing
  --safe-names         Adapt names a destination refuses: ? : * < > and more
  --raw-names          Leave names exactly as they are
  --replace            Write files again even when they are already there
  --compress <format>  Encode on the way out: mp3, opus, aac, vorbis, flac,
                       wav. Needs ffmpeg installed. Only lossless sources are
                       encoded — what is already compressed is copied as it
                       stands rather than losing a second time
  --quality <setting>  V0…V9 for MP3, q0…q10 for Vorbis, or a bitrate like
                       192k. Only for the formats that have one: flac and
                       wav keep every sample, so there is nothing to choose

{}
  --list               List every analysis held, by folder, and say what
                       became of each: attached, waiting for a scan, or
                       stale because the file changed since
  --forget             Remove the imported analyses instead of adding any
  --pending            List the folders whose analyses match no file yet;
                       with --forget, remove only those. Both accept
                       folders, to act on one rather than on all of them
  --source <name>      Restrict to one tool (--list, --forget, --pending)

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
  aede query \"genre:metal year:1990..1999 -label:earache\"
  aede query \"album.rating:>=4 played:0\" --m3u
  aede query \"loved\" --sort played- --limit 20
  aede albums --query \"album.rating:>=4\"
  aede albums --query \"album.tag:vinyl\"
  aede query \"note:remaster\"
  aede query \"album.tag:vinyl OR album.tag:rare\"
  aede query \"-rating loved\"
  aede collection wishlist --query \"loved played:0\"
  aede collection wishlist --m3u
  aede notes --export -o backup.json
  aede love album \"Kind of Blue\"
  aede rate artist \"Miles Davis\" --stars 5
  aede note album \"Legion\" --text \"the 1992 pressing\"
  aede tag album \"Legion\" vinyl,rare,to rip again
  aede tag album \"Legion\" rare --remove
  aede tag album \"Legion\" --remove
  aede notes --tag vinyl
  aede search vinyle --notes
  aede roots --exclude ~/Music/Audiobooks
  aede played \"So What\" --remove
  aede history --remove
  aede copy /Volumes/Player --query \"loved rating:>=4\" --verify
  aede copy /Volumes/Card --collection wishlist --extras none
  aede copy /Volumes/Phone --compress opus --quality 128k
  aede copy /Volumes/Phone --compress mp3 --quality V0 --query \"loved\"
  aede import ~/Desktop/report.json
  aede import --pending
  aede import --forget --pending \"/Volumes/OldDrive/Music\"",
        ui::cyan("USAGE"),
        ui::cyan("COMMANDS"),
        ui::cyan("GLOBAL OPTIONS"),
        ui::cyan("SCAN OPTIONS"),
        ui::cyan("FILTER OPTIONS"),
        ui::cyan("COPY OPTIONS"),
        ui::cyan("IMPORT OPTIONS"),
        ui::cyan("EXAMPLES")
    );
}
