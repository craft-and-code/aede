//! Aède — command-line interface.
//!
//! Milestone M0.6: scan folders, build the catalog, query it, record what the
//! user makes of it, and copy a selection out to a player or a card.

mod args;
mod commands;
mod help;
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
        let command = canonical(&args.command);
        if command.is_empty() || !help::is_command(command) {
            help::print_index();
        } else {
            help::print_command(command);
        }
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
            help::print_index();
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
    for (option, commands, what) in OPTION_SCOPE {
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
            // A command whose whole subject is a file, refused the option
            // that writes to one, needs the form that does work — not a list
            // of other commands. The same reason `--role` carries a hint.
            if *option == "output" && matches!(command, "backup" | "restore") {
                eprintln!("The file is not an option here: aede {command} <file>");
            }
            if *option == "role" {
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
    // argument, because the folder is what is being removed. That is only
    // true when `--remove` stands alone: `roots --exclude <folder> --remove`
    // already has its folder from `--exclude`, so a stray word there (a
    // mistyped `-no-scan`, say) is not the argument being read, and letting
    // it through here was letting it through everywhere — silently, the one
    // thing an argument must never be.
    // `sources --template <name>` is the other: the name narrows which empty
    // records the template covers, so a positional there is the argument
    // being read, not one slipping past unnoticed.
    let asks_for_command_help = command == "help" && args.positionals.len() == 1;
    let reads_an_argument = asks_for_command_help
        || (command == "roots" && args.has("remove") && args.value("exclude").is_none())
        || (command == "sources" && args.has("template"));
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

/// Every option this program recognises at all.
///
/// **Before anything answers, including `--help` and `--version`**: an option
/// nobody recognises makes the whole command line untrustworthy, and
/// `aede --fegioregj` printing a cheerful help page is the same silence in a
/// friendlier costume. [`OPTION_SCOPE`] then says which commands honour which
/// of these; being on this list only means the word exists.
///
/// At module level rather than inside `main` so that a test can read it, for
/// the reason written on [`OPTION_SCOPE`].
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
    "members",
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
    "template",
    "summaries",
    "discography",
    "covers",
    "size",
    "images",
    "country",
    "identify",
    "credits",
    "recordings",
    "lang",
    "portraits",
    "logos",
    "fanart",
    "no-logo",
    "no-label-logo",
    "no-portrait",
    "no-background",
    "no-banner",
    "no-album-cover",
    "no-cdart",
    "banners",
    "labels",
    "accept",
    "reject",
    "undo",
    "interactive",
    "graph",
];

/// Where each restricted option means something.
///
/// **One table, and it is the only place this is written down.** The global
/// option list above says an option exists; this says which commands can
/// honour it, which is what stops `aede stats --csv` from printing a table that
/// is not one. Lifted out of `main` so that a test can read it: a message that
/// advises `aede fetch --artists` when `--artists` belongs to `playlist` is an
/// option nobody can type, and only something comparing the two can notice.
const OPTION_SCOPE: &[(&str, &[&str], &str)] = &[
    ("csv", CSV_COMMANDS, "produce a table"),
    ("m3u", M3U_COMMANDS, "produce a playlist"),
    ("output", OUTPUT_COMMANDS, "write to a file"),
    ("forget", SAID_ELSEWHERE_COMMANDS, "forget what was stored"),
    (
        "pending",
        IMPORT_COMMANDS,
        "list or restrict to what is waiting",
    ),
    ("list", LIST_COMMANDS, "list what is held"),
    ("source", SOURCE_COMMANDS, "select a source"),
    ("accept", &["review"], "accept one source claim by ID"),
    ("reject", &["review"], "reject one source claim by ID"),
    ("undo", &["review"], "undo one source-review decision by ID"),
    (
        "interactive",
        &["review"],
        "review claims one at a time with their context",
    ),
    ("graph", &["export"], "include every attributed graph layer"),
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
    ("members", MEMBER_COMMANDS, "show who played in it"),
    ("country", &["artists"], "filter by where an artist is from"),
    ("artist", ARTIST_COMMANDS, "narrow to one artist"),
    ("year", YEAR_COMMANDS, "narrow to one year"),
    ("genre", GENRE_COMMANDS, "filter by genre"),
    ("label", LABEL_COMMANDS, "filter by label"),
    ("comment", COMMENT_COMMANDS, "filter on the comments"),
    ("comments", &["search"], "search the comments"),
    ("notes", &["search"], "search what you wrote"),
    ("limit", PAGING_COMMANDS, "show a window of its result"),
    ("offset", PAGING_COMMANDS, "start further down its result"),
    ("all", PAGING_COMMANDS, "hold nothing back"),
    (
        "json",
        JSON_COMMANDS,
        "answer in JSON, or save album reports with analyze",
    ),
    ("separator", CSV_COMMANDS, "choose a separator"),
    ("sort", SORT_COMMANDS, "be sorted"),
    ("severity", DOCTOR_COMMANDS, "filter by severity"),
    ("album", &["track"], "narrow to one album"),
    ("with", &["artist"], "cross two artists"),
    ("tracks", &["export"], "switch to one row per track"),
    (
        "yes",
        &["reset", "history", "fetch", "backup", "restore"],
        "skip the confirmation",
    ),
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
            "missing",
            "relation",
        ],
        "take something back",
    ),
    (
        "full",
        &["scan", "check", "spectrum", "fetch", "fingerprint"],
        "ignore what was already done",
    ),
    (
        "summaries",
        &["fetch"],
        "follow the wikidata link to an article",
    ),
    (
        "discography",
        &["fetch"],
        "browse everything credited to an artist",
    ),
    ("covers", &["fetch"], "look for missing cover art"),
    (
        "identify",
        &["fetch"],
        "ask what the fingerprinted files sound like",
    ),
    (
        "credits",
        &["fetch"],
        "ask MusicBrainz for recording and work credits",
    ),
    ("recordings", &["fetch"], "legacy name for --credits"),
    ("portraits", &["fetch"], "look for a picture of the artist"),
    (
        "logos",
        &["fetch"],
        "look for artist and identified-label logos",
    ),
    (
        "fanart",
        &["fetch"],
        "keep all available Fanart.tv music artwork",
    ),
    ("no-logo", &["fetch"], "exclude artist logos from --fanart"),
    (
        "no-label-logo",
        &["fetch"],
        "exclude label logos from --fanart",
    ),
    (
        "no-portrait",
        &["fetch"],
        "exclude artist portraits from --fanart",
    ),
    (
        "no-background",
        &["fetch"],
        "exclude artist backgrounds from --fanart",
    ),
    (
        "no-banner",
        &["fetch"],
        "exclude artist banners from --fanart",
    ),
    (
        "no-album-cover",
        &["fetch"],
        "exclude album covers from --fanart",
    ),
    ("no-cdart", &["fetch"], "exclude disc artwork from --fanart"),
    (
        "banners",
        &["fetch"],
        "keep a wide banner too, with --logos",
    ),
    (
        "labels",
        &["fetch"],
        "ask MusicBrainz for a label's own identifier",
    ),
    (
        "size",
        &["fetch", "spectrum"],
        "choose how large a picture is",
    ),
    ("lang", &["fetch"], "choose the language of the prose"),
    (
        "images",
        &["fetch", "extract"],
        "keep the back and the booklet too, in artwork/",
    ),
    (
        "threads",
        &["scan", "check", "spectrum", "copy", "analyze"],
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
    ("text", &["note", "relation"], "carry a note"),
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
        &[
            "copy",
            "spectrum",
            "playlist",
            "fetch",
            "extract",
            "fingerprint",
        ],
        "say what it would do without doing it",
    ),
    (
        "lyrics",
        &["track", "search", "fetch"],
        "show the words, look in them, or go and get them",
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
    (
        "export",
        &["notes", "sources", "rules"],
        "write what is held to a file",
    ),
    (
        "template",
        &["sources"],
        "write a document with the keys and nothing filled in",
    ),
    (
        "import",
        &["notes", "sources", "rules"],
        "take back in what was exported",
    ),
    ("from", &["note"], "copy what was said elsewhere"),
    (
        "tag",
        &["notes", "relation", "relations"],
        "write, remove, or filter on a tag",
    ),
];

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
    "countries",
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

/// Commands that act on a store of what somebody *else* said: another tool's
/// analyses, and the values fetched from a source. Two stores, one vocabulary
/// — `--list` says what is held, `--forget` drops it, `--source` narrows to
/// one — because a user who learned it on one should not have to learn it
/// again on the other.
const SAID_ELSEWHERE_COMMANDS: &[&str] = &["import", "sources", "missing", "merge"];

/// Commands where one external source can be selected. Review joins the
/// attributed-data commands here without inheriting their destructive
/// `--forget` option.
const SOURCE_COMMANDS: &[&str] = &[
    "import",
    "sources",
    "missing",
    "merge",
    "review",
    "relations",
];

/// Commands that can list what they hold rather than go and get more.
///
/// Not [`SAID_ELSEWHERE_COMMANDS`], although it started as a copy of it. Those
/// three options — `--forget`, `--list`, `--source` — happened to apply to the
/// same three commands, and sharing one list made that coincidence look like a
/// rule: `fingerprint` can list what it holds and has nothing to forget and no
/// source to select. **One list per question, not one list per set of commands
/// that happen to agree today.**
const LIST_COMMANDS: &[&str] = &["import", "sources", "missing", "fingerprint", "merge"];

/// The one command that lists releases and can therefore sort compilations
/// from the rest.
const ALBUM_LIST_COMMANDS: &[&str] = &["albums"];

/// Where a role means something: the listing narrows to the people who hold
/// one, the page narrows to what that person did in it. Two readings of the
/// same word, both useful, and neither of them makes sense without a person —
/// which is why `album` and `track` are not here: there, `--artist` is the
/// filter, and a role with nobody attached asks nothing.
const ROLE_COMMANDS: &[&str] = &["artists", "artist"];

/// The one command that can answer with a band's line-up.
///
/// `album` is deliberately not here: an album shows the line-up of its year on
/// its own page, because that is the whole reason the dates are worth fetching,
/// and an option to switch it off would be an option nobody types.
const MEMBER_COMMANDS: &[&str] = &["artist"];

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
    ("analyze", None, commands::analyze),
    ("roots", None, commands::roots),
    ("stats", None, commands::show_stats),
    ("doctor", None, commands::show_doctor),
    ("check", None, commands::check),
    ("copy", None, commands::copy),
    ("spectrum", None, commands::spectrum),
    ("playlist", None, commands::playlist),
    ("reset", None, commands::reset),
    ("backup", None, commands::backup),
    ("restore", None, commands::restore),
    ("import", None, commands::import),
    ("sources", None, commands::sources),
    ("review", None, commands::review),
    ("rules", None, commands::rules),
    ("relations", None, commands::relations),
    ("relation", None, commands::relation),
    ("fetch", None, commands::fetch),
    ("missing", None, commands::missing),
    ("merge", None, commands::merge),
    ("extract", Some("artwork"), commands::artwork),
    ("fingerprint", None, commands::fingerprint),
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
    ("countries", None, commands::list_countries),
    ("albums", None, commands::list_albums),
    ("genres", None, commands::list_genres),
    ("genre", None, commands::show_genre),
    ("labels", None, commands::list_labels),
    ("label", None, commands::show_label),
    ("years", None, commands::list_years),
    ("artist", None, commands::show_artist),
    ("album", None, commands::show_album),
    ("track", None, commands::show_track),
    ("recording", None, commands::show_recording),
    ("work", None, commands::show_work),
    ("release-group", None, commands::show_release_group),
    ("search", None, commands::search),
    ("file", None, commands::inspect),
    ("export", None, commands::export),
    ("help", None, help::run),
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
    "analyze",
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
    "countries",
    "years",
    "stats",
    "doctor",
    "favourites",
    "notes",
    "query",
    "collection",
    "relations",
];

/// The one listing whose order can be chosen.
const SORT_COMMANDS: &[&str] = &[
    "artists",
    "albums",
    "genres",
    "labels",
    "years",
    "countries",
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
        "sources" => {
            "For one artist, album or track: aede artist \"<name>\" | aede album \"<title>\" | aede track \"<name>\"\n\
             To build one instead: aede sources --template \"<name>\""
        }
        "stats" | "doctor" | "roots" => "It describes the whole catalog.",
        "rules" => "It lists, exports, or imports personal rules.",
        "collections" => {
            "It lists what you saved. To save one: aede collection <name> --query \"…\""
        }
        "favourites" | "notes" | "history" => {
            "It lists what you wrote. To write: aede love|rate|note <kind> \"<name>\""
        }
        // `aede help fetch` is deliberately the one detailed page: fetch has
        // several independent sources and image families, too much to make
        // the front page pleasant to scan.
        "help" => "For one command: aede help <command>",
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
    "countries",
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
    // `missing` pages like the rest, and had to: it was added here for
    // `--all` alone, which let `--limit` and `--offset` through to a command
    // that ignored them — the exact fault this table exists to prevent, made
    // by the table's own author. An option list is a promise that the command
    // honours every option on it.
    "missing",
    "review",
    "relations",
];

/// Commands whose output can go to a file instead of the terminal.
const OUTPUT_COMMANDS: &[&str] = &[
    "export",
    "sources",
    "rules",
    "relations",
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
    "countries",
    "years",
    "favourites",
    "notes",
    "query",
    "collection",
];

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
