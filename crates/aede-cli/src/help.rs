//! Command help: the concise index and one page per command.
//!
//! Keeping presentation separate from dispatch lets the command runner stay
//! focused on parsing, validation, and execution.

use crate::{COMMANDS, OPTION_SCOPE, VERSION, args::Args, canonical, commands, ui};

/// The short, command-specific part of every help page.
///
/// The detailed prose lives in the command where it earns it; this shared
/// shape makes `aede help album`, `aede help albums`, and every other command
/// discoverable in the same way without duplicating the option-scope table.
pub(crate) struct CommandPage {
    pub(crate) usage: &'static str,
    pub(crate) summary: &'static str,
}

pub(crate) fn command_page(command: &str) -> CommandPage {
    match command {
        "accounts" => CommandPage {
            usage: "aede accounts [list | init <name> | create <name> <admin|user|auditor> | password <name> | role <name> <admin|user|auditor> | rename <name> <new-name> | enable <name> | disable <name> | revoke <name> | keys <name> [create <label> | revoke <id>]] [--password-stdin] [--json]",
            summary: "Manage local accounts; init, create and password ask for a masked password and confirmation. Use --password-stdin with redirected input for scripts. The first administrator retains existing personal data. Client API keys are shown once at creation, remain valid until revoked and are cleared by account changes or credential restoration.",
        },
        "scan" => CommandPage {
            usage: "aede scan [folder…]",
            summary: "Read watched music folders into the local catalog.",
        },
        "serve" => CommandPage {
            usage: "aede serve [--port N] [--bind IP] [--tls-cert PATH --tls-key PATH --authority HOST:PORT]",
            summary: "Serve the catalog API locally or through explicit HTTPS, and coordinate CLI writes.",
        },
        "cancel" => CommandPage {
            usage: "aede cancel <task-id>",
            summary: "Request cancellation of a delegated scan or fetch on the local server.",
        },
        "roots" => CommandPage {
            usage: "aede roots [folder…]",
            summary: "List, add, remove, or exclude watched folders.",
        },
        "stats" => CommandPage {
            usage: "aede stats",
            summary: "Show the library's size, quality, and completeness.",
        },
        "doctor" => CommandPage {
            usage: "aede doctor",
            summary: "Find missing metadata, duplicates, and other catalog issues.",
        },
        "credits" => CommandPage {
            usage: "aede credits [album title|MusicBrainz ID|folder] [--json]",
            summary: "Measure MusicBrainz recording, work and edition credit coverage.",
        },
        "credit" => CommandPage {
            usage: "aede credit --add recording:<ID>|work:<ID>|release:<ID> --artist=<name> --role=<role> [--artist-id=<MBID>] [--instrument=<name>] | aede credit --exclude=<credit ID> | --undo=<credit ID>",
            summary: "Add a manual credit or exclude/restore one sourced credit without touching tags.",
        },
        "check" => CommandPage {
            usage: "aede check [folder…]",
            summary: "Verify the checksums stored in audio containers.",
        },
        "copy" => CommandPage {
            usage: "aede copy <destination>",
            summary: "Copy a selection to a player, card, or other destination.",
        },
        "spectrum" => CommandPage {
            usage: "aede spectrum [folder…]",
            summary: "Create spectrograms beside the selected music.",
        },
        "analyze" => CommandPage {
            usage: "aede analyze [folder…] [--json | --json-layout album|artist] [--force] [--show-results] [--threads N]",
            summary: "Analyze albums with FlacCompagnon and optionally save one JSON report per album.",
        },
        "playlist" => CommandPage {
            usage: "aede playlist [folder…]",
            summary: "Write portable playlists in album and artist folders.",
        },
        "play" => CommandPage {
            usage: "aede play <file|folder|m3u|collection|artist|album|track> [--seek TIME] [--repeat off|one|all] [--shuffle off|random|smart] [--seed U64] [--lyrics] [--normalize off|track|album] [--bass DB] [--treble DB]",
            summary: "Play local audio, an M3U, a saved collection or a catalogued name, with an initial seek position, repeat modes and seeded random or smart order. Shows album and numbered filename with keyboard controls, a 24-band visualizer, and listening history. --lyrics replaces the terminal spectrum with local timed lyric cues or a short untimed preview; it never downloads lyrics. Normalization defaults to album gain for catalogued album selections and track gain otherwise; --normalize overrides it. Current FlacCompagnon analyses or fresh loudness measurements supply gain when matching tags are absent. Optional broad bass/treble shelves each accept -12 to +12 dB; zero is flat. Positive tone boosts reserve headroom. Unexpected over-full-scale samples are reported and hard-clamped. Known multichannel layouts are safely downmixed to stereo, omitting LFE; unknown layouts are refused. CPAL provides native output when available, with ffplay fallback.",
        },
        "reset" => CommandPage {
            usage: "aede reset",
            summary: "Remove catalog data while keeping watched folders.",
        },
        "backup" => CommandPage {
            usage: "aede backup <file>",
            summary: "Save the catalog, sources, and personal data together.",
        },
        "restore" => CommandPage {
            usage: "aede restore <file>",
            summary: "Restore a previously saved Aède backup.",
        },
        "import" => CommandPage {
            usage: "aede import <report…>",
            summary: "Import acoustic analyses from FlacCompagnon.",
        },
        "sources" => CommandPage {
            usage: "aede sources",
            summary: "Inspect, export, import, or remove externally sourced data.",
        },
        "review" => CommandPage {
            usage: "aede review [name] [--interactive] | aede review --accept=<ID> | --reject=<ID> | --undo=<ID>",
            summary: "Resolve uncertain or conflicting source identities without changing tags.",
        },
        "relations" => CommandPage {
            usage: "aede relations [name]",
            summary: "List local and sourced graph relationships with stable IDs.",
        },
        "relation" => CommandPage {
            usage: "aede relation <ID>",
            summary: "Inspect, annotate, or label one graph relationship.",
        },
        "rules" => CommandPage {
            usage: "aede rules",
            summary: "List, export, or import reproducible personal decisions.",
        },
        "fetch" => CommandPage {
            usage: "aede fetch [name… | folder…] [options]",
            summary: "Enrich artists, albums, tracks, and artwork from chosen sources.",
        },
        "recording" => CommandPage {
            usage: "aede recording <title|MusicBrainz ID>",
            summary: "Show one recording, its local placements, and sourced work links.",
        },
        "work" => CommandPage {
            usage: "aede work <title|MusicBrainz ID>",
            summary: "Show one composition and the recordings that realize it.",
        },
        "release-group" => CommandPage {
            usage: "aede release-group <title|MusicBrainz ID>",
            summary: "Show the album identity shared by every local edition.",
        },
        "missing" => CommandPage {
            usage: "aede missing [name…]",
            summary: "List credited studio albums that the local shelf does not hold.",
        },
        "merge" => CommandPage {
            usage: "aede merge <first> <second>",
            summary: "Record that two local artist spellings name one person.",
        },
        "extract" => CommandPage {
            usage: "aede extract [folder…]",
            summary: "Write artwork already embedded in audio files beside the music.",
        },
        "fingerprint" => CommandPage {
            usage: "aede fingerprint [folder…]",
            summary: "Compute acoustic fingerprints without using the network.",
        },
        "query" => CommandPage {
            usage: "aede query <expression>",
            summary: "Select tracks with the relational query language.",
        },
        "collection" => CommandPage {
            usage: "aede collection <name>",
            summary: "Save, run, or remove a named dynamic collection.",
        },
        "collections" => CommandPage {
            usage: "aede collections",
            summary: "List the saved dynamic collections.",
        },
        "love" => CommandPage {
            usage: "aede love <kind> <name>",
            summary: "Mark an artist, album, or track as a favourite.",
        },
        "rate" => CommandPage {
            usage: "aede rate <kind> <name>",
            summary: "Give an artist, album, or track a personal rating.",
        },
        "note" => CommandPage {
            usage: "aede note <kind> <name>",
            summary: "Read or write a personal note.",
        },
        "tag" => CommandPage {
            usage: "aede tag <kind> <name> <label[,label…]>",
            summary: "Attach or remove personal labels.",
        },
        "played" => CommandPage {
            usage: "aede played <track>",
            summary: "Manually record a listen made outside Aède, or undo its latest record with --remove.",
        },
        "favourites" => CommandPage {
            usage: "aede favourites",
            summary: "List everything marked as a favourite.",
        },
        "notes" => CommandPage {
            usage: "aede notes",
            summary: "List, export, import, or safely relink personal records.",
        },
        "history" => CommandPage {
            usage: "aede history",
            summary: "Show listening history, newest first.",
        },
        "artists" => CommandPage {
            usage: "aede artists",
            summary: "List artists in the catalog.",
        },
        "countries" => CommandPage {
            usage: "aede countries",
            summary: "Summarize artists by their MusicBrainz origin.",
        },
        "albums" => CommandPage {
            usage: "aede albums",
            summary: "List albums in the catalog.",
        },
        "genres" => CommandPage {
            usage: "aede genres",
            summary: "List genres in the catalog.",
        },
        "genre" => CommandPage {
            usage: "aede genre <name>",
            summary: "Show albums and artists carrying one genre.",
        },
        "labels" => CommandPage {
            usage: "aede labels",
            summary: "List record labels in the catalog.",
        },
        "label" => CommandPage {
            usage: "aede label <name> [--offline]",
            summary: "Show a label's catalog and artists; check a Discogs profile when needed.",
        },
        "years" => CommandPage {
            usage: "aede years",
            summary: "Break the catalog down by release year.",
        },
        "artist" => CommandPage {
            usage: "aede artist <name|MusicBrainz artist ID>",
            summary: "Show a local artist or a source-only credited contributor and their links.",
        },
        "album" => CommandPage {
            usage: "aede album <title|MusicBrainz release ID>",
            summary: "Show one album's tracks, credits, and sources.",
        },
        "track" => CommandPage {
            usage: "aede track <title>",
            summary: "Show a track's album, credits, tags, and technical facts.",
        },
        "search" => CommandPage {
            usage: "aede search <text>",
            summary: "Search graph objects and source-only contributors, plus optional notes, comments, or lyrics.",
        },
        "file" => CommandPage {
            usage: "aede file <path>",
            summary: "Inspect one audio file directly, without the catalog.",
        },
        "export" => CommandPage {
            usage: "aede export",
            summary: "Export the catalog as JSON or a table.",
        },
        "help" => CommandPage {
            usage: "aede help [command]",
            summary: "Show the command index or the page for one command.",
        },
        _ => unreachable!("every command is declared in COMMANDS"),
    }
}

pub fn is_command(command: &str) -> bool {
    COMMANDS
        .iter()
        .any(|(name, _, _)| *name == canonical(command))
}

/// `help` as a function, so that it sits in the table like every other command
/// rather than being a special case the table could forget.
pub fn run(args: &Args) -> commands::Res {
    let Some(topic) = args.positionals.first() else {
        print_index();
        return Ok(());
    };
    let command = canonical(topic);
    if !is_command(command) {
        return Err(
            format!("no command named \"{topic}\"; run aede help for the command list").into(),
        );
    }
    print_command(command);
    Ok(())
}

pub fn print_index() {
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
  serve [--bind IP]    Serve locally at 127.0.0.1:8787 by default; explicit
                       TLS options enable HTTPS. CLI writes delegate locally on Unix.
                       Run aede help serve for setup and access limits
  cancel <task-id>     Stop a delegated scan or fetch on the local server
  accounts            Manage logins, roles and sessions; run aede help accounts
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
  fetch [name… | folder…] Enrich artists, albums and tracks from chosen
                       sources. --summaries, --lyrics, --covers, --fanart and
                       the other passes can be combined. Run `aede help fetch`
                       for the complete guide and examples
  credits [album]      Recording/work and edition coverage by album, with
                       manual corrections separate from completed lookups
  credit               Add a manual recording, work or edition credit, or
                       exclude/restore one sourced credit by exact ID
  sources              What other sources say, beside your tags and never on
                       top of them. --list shows each record, --forget drops
                       them, --source narrows to one. --template writes a
                       document with the keys and nothing filled in, --import
                       <file> takes one back, --export writes out what is held
                       (both through --output, or to the terminal)
  review               Pending approximate matches and identity conflicts.
                       --accept=<ID> trusts one for graph navigation and
                       queries; --reject=<ID> keeps it as evidence only;
                       --undo=<ID> returns either decision to pending.
                       Decisions never rewrite tags, and --all shows resolved
                       claims alongside those still waiting
  relations [name]     Every local and sourced graph relationship, with a
                       stable ID and its provenance. --source narrows it;
                       --tag finds links carrying one of your labels
  relation <ID>        Open one relationship. --text writes a personal note,
                       --tag adds comma-separated labels, and --remove takes
                       the annotation back without changing either endpoint
  rules                Personal decisions that can be replayed: identity
                       reviews, artist filing, set-aside releases, manual
                       source records, saved queries and relation notes.
                       --export writes a portable bundle; --import takes one in
  fingerprint [folder…] Work out what each file's audio is, by decoding it.
                       The local half of identifying by sound: it touches no
                       network and stores what it computes in the catalog, so
                       it is never computed twice. By default only the files
                       your tags cannot identify — no title, or no artist —
                       since decoding a well-tagged library is hours of work
                       to confirm what the tags say. --full takes everything.
                       --list prints what is stored, whole and one value to a
                       line: the same string fpcalc prints, so two copies of an
                       album can be compared, and aede doctor reports the ones
                       that match.
                       Needs ffmpeg built with chromaprint, or fpcalc; it says
                       which, and how to install either
  extract [folder…]    Write the picture your files already carry into their
                       own folder, as cover.jpg. No network: it comes out of
                       the audio files, whatever the format — FLAC, MP3, MP4,
                       Ogg, AIFF, WAV. A folder that already holds an image is
                       left alone, and nothing is ever overwritten. Reach for
                       this before fetch --covers: an album whose artwork is
                       inside its files needs no download. --images writes out
                       the back, the booklet and the disc as well, into an
                       artwork/ subfolder (--dry-run says which folders, and
                       writes nothing). Also answers to: artwork
  spectrum [folder…]   Draw a spectrogram of every track into a spectrograms/
                       folder beside it, through ffmpeg, several at a time.
                       Only what is missing or older than its track is drawn,
                       so a second run over an unchanged library draws nothing
                       (--full redraws everything, --dry-run only says what it
                       would draw, --threads sets how many run at once).
                       --size half (the default) keeps a library's pictures in
                       the megabytes rather than the gigabytes; --size full
                       matches FlacCompagnon's own dimensions exactly, for
                       putting the two side by side. Changing --size does not
                       redraw what is already there on its own — --full does
  analyze [folder…]    Run FlacCompagnon's acoustic analysis on catalogued albums.
                       --json saves <album>.json in each album folder;
                       --threads sets how many tracks run at once.
  playlist [folder…]   Write an .m3u in every album folder, in album order and
                       with relative paths. --simple leaves out the #EXTINF
                       lines for players that choke on them, --artists adds one
                       per artist folder covering their whole discography,
                       --dry-run only says what it would write
  play <selection>     Play a file, a folder (recursively, in name order), an
                       .m3u/.m3u8 playlist, a saved collection, or a catalogued
                       artist, album or track by name. Use collection:<name>
                       when a collection shares a name with music. Albums play
                       in year order; duplicate album/track titles play all
                       matches. In a macOS/Linux terminal or Windows console: Space pauses or
                       resumes, n/→ skips forward, p/← goes back (or restarts
                       after three seconds), [/] seeks ten seconds back/forward,
                       r cycles repeat, z cycles shuffle, and q or Ctrl-C stops
                       orderly and returns to the shell. --seek TIME starts the first track at a
                       position in seconds, mm:ss or hh:mm:ss. --repeat
                       off|one|all controls natural completion; --shuffle
                       off|random|smart chooses the order. --seed U64 reproduces
                       an enabled shuffle. The 24 thin spectrum bars animate only in a
                       terminal. --lyrics replaces them with local timed words
                       or an untimed preview, without downloading anything.
                       The label shows album — numbered filename;
                       every played file enters history. --normalize
                       off|track|album overrides automatic ReplayGain or
                       Opus R128 selection. Album names use album gain;
                       files, folders, playlists, collections, artists and
                       tracks use track gain. Missing tags trigger a measured loudness fallback;
                       the target is -18 LUFS. Peak headroom may
                       reduce the gain; without a peak tag, Aède avoids a
                       positive boost. Unexpected over-full-scale samples
                       are hard-clamped and reported before output.
                       --bass DB and --treble DB apply optional broad tone
                       shelves from -12 to +12 dB. Both default to zero;
                       use zero to reset to flat bypass. Positive boosts
                       reserve preamp headroom before the final output guard.
                       Known multichannel sources are downmixed to stereo;
                       LFE is omitted and unknown layouts are refused.
                       Uses native audio
                       when available, otherwise ffplay. Set
                       AEDE_AUDIO_BACKEND=ffplay for the fallback, or
                       AEDE_AUDIO_BACKEND=native to require CPAL. Opus and
                       M4A also need ffmpeg
  artists              List of artists (--role composer, producer…,
                       --country france, --sort tracks|name)
  countries            Where the artists on the shelf are from. Not a tag:
                       there is no usable one for it, so the fact comes from
                       MusicBrainz and this reads what aede fetch stored. An
                       artist nobody has asked about has no country, and the
                       listing says how many those are rather than leaving
                       them out in silence
  missing [name…]      Studio albums MusicBrainz credits to your artists that
                       this catalog does not hold. Nothing is fetched here:
                       the answer is worked out from what fetch
                       --discography already stored, so an album stops being
                       listed the day you add it. Singles, live records and
                       compilations are left out; --all holds nothing back
                       and says of each row why it would not be there.
                       MusicBrainz is sometimes wrong about what an album is —
                       a demo or a compilation nobody has typed as one — so
                       --forget <title> sets a record aside and stops listing
                       it, --list shows what you set aside — narrowed by a
                       name, like the report itself — and --forget --remove
                       <title> puts it back. What the source said is
                       never altered: only what you are shown
  merge <a> <b>        Say that two spellings are one musician: the first
                       gives way to the second. Files tagged by Picard need
                       none of this — a shared MUSICBRAINZ_ARTISTID already
                       merges them — but nobody outside can know that your
                       O. Osbourne is Ozzy, so this is where you say it.
                       Nothing in your files changes: it is how the shelf is
                       read, and it takes effect on the next aede scan.
                       --list shows the statements, narrowed by a name;
                       --forget <spelling> takes one back. aede doctor names
                       the pairs worth looking at and merges none of them
  albums               List of albums (--artist, --year, --genre, --label,
                       --comment, --compilations, --no-compilations).
                       --query narrows it by anything the grammar can say:
                       aede albums --query \"album.rating:>=4\"
  genres               List of genres
  labels               List of labels
  years                Breakdown by year
  artist <name|id>     Artist card: discography, collaborations
                       (--with=<other> lists the tracks the two share).
                       --members is the dated line-up: who played in the band,
                       on what, and between which years, in MusicBrainz's own
                       words — and for a person, the bands they played in. It
                       comes from aede fetch, and the album pages use the same
                       dates to name the band as it stood the year each record
                       came out. A MusicBrainz credit-only artist has a
                       sourced card with local albums, recordings and works;
                       use its ID to disambiguate namesakes
  album <title|id>     Album edition card: tracks, credits and graph links
  track <title>        Track card: album, credits, technical details, tags
                       (--lyrics adds the words, from the tags or from a .lrc
                       file sitting beside the track)
  recording <title|id> Recorded performance: every local album placement and
                       each attributed work relationship. An ID removes title
                       ambiguity; without one, local:<file path> identifies
                       one recording precisely
  work <title|id>      Composition and the recordings that realize it. Works
                       fetched from MusicBrainz remain explicitly sourced and
                       never rewrite the file tags. Explicit parent/part
                       relations show symphonies and their movements
  release-group <id>   Album identity shared by its local editions. The
                       MusicBrainz ID removes ambiguity between equal titles
  genre <name>         Genre page: albums and artists carrying it
  label <name>         Label page: its catalogue, artists, and MusicBrainz
                       identity status (local, confirmed, proposed, conflict).
                       Checks Discogs on each visit when Wikipedia has no bio;
                       --offline reads only local data
  search <text>        Search artists, albums, tracks, recordings, works,
                       release groups, labels, trusted source-only credit
                       contributors and parent works. --comments also looks in the
                       comment tag, --notes in what you wrote yourself, and
                       --lyrics in the words of the songs
  file <path>          Read one file straight off disk: its technical
                       properties and raw tags, exactly as it carries them,
                       with no catalog involved. Handy to see why a scanned
                       file looks wrong, or to check one before adding it to
                       the library
  import <report…>     Take in FlacCompagnon reports. --list says what is
                       held and what became of it, --pending lists the
                       folders whose analyses match no file yet, --forget
                       removes analyses; --forget --pending [folder…] drops
                       only what is waiting, and keeps what did attach
  reset                Remove the catalog, after confirmation (--yes skips it)
  backup <file>        Everything Aède knows in one document: the catalog,
                       what you said and what sources said. The catalog can
                       be rebuilt by a scan; your notes, ratings and play
                       counts cannot be rebuilt by anything, and the fetched
                       layer costs twenty minutes of polite requests. An
                       existing file is overwritten only after confirmation
                       (--yes skips it)
  restore <file>       Put a backup back, after confirmation (--yes skips
                       it). It says what it will replace before asking. A
                       store the backup does not hold is left exactly as it
                       is and never deleted, and a store this build cannot
                       read does not stop the other two
  export               Export the catalog as JSON, or as CSV with --csv
                       (one row per album; --tracks for one row per track).
                       --graph exports the catalog, source evidence,
                       provenance, review decisions, personal data and the
                       materialized relation graph in one document
  query <expression>   (also: find) Every track an expression matches; the
                       result is a selection, so --csv, --json and --m3u apply
                         genre:metal year:1990..1999 -label:earache
                         (artist:ozzy OR artist:dio) album.rating:>=4 played:0
                       From the tags: title artist album albumartist genre
                       label comment path codec year duration size bitrate
                       samplerate lossless compilation
                       Classical tags: work (also an identified parent work),
                       movement. Titles in tags remain text, not identities
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
                       performer, conductor, orchestra, choir, ensemble,
                       soloist, remixer, featured, mainartist,
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
  played <track>       Record a listen made in another player manually
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
  help [command]       This index, or the detailed page for one command

{}
  --data <folder>      Catalog location
                       (default: $AEDE_HOME, then $XDG_DATA_HOME/aede,
                       then ~/.local/share/aede; .aede if HOME is unset)
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
  -v, -V, --version    Show the version

{}
  --full               Re-read every file (scan, check), or re-query stored
                       answers (fetch)
  --replace            Forget the watched folders and keep only those given
  --threads <n>        Number of reader threads (scan, check;
                       default: available cores)
  --follow-symlinks    Follow symbolic links
  --include-hidden     Include hidden files and folders
  --exclude <folder>   Never read this folder (roots). Kept in the catalog,
                       so a plain `aede scan` goes on honouring it

{}
  --identify           Identify fingerprinted files through AcoustID
  --credits            Retrieve rich MusicBrainz recording, work and exact-
                       edition credits, including work/part relationships
  --recordings         Compatibility alias for --credits
  --summaries          Retrieve the opening Wikipedia paragraph
  --discography        Browse an artist's MusicBrainz releases
  --lyrics             Retrieve missing lyrics as .lrc sidecars
  --labels             Identify record labels through MusicBrainz
  --lang <code>        Preferred language for --summaries
  --dry-run            List what fetch would ask, without a request
                       `aede help fetch` describes every pass and its files

{}
  --covers             Missing album cover from Cover Art Archive
  --size <what>        Image size: 250, 500, 1200 or original (--covers)
  --images             Back, booklet and disc images in artwork/ (extract,
                       fetch --covers)
  --portraits          Artist portrait, Wikidata first then Fanart.tv
  --logos              Artist and identified-label logos from Fanart.tv
  --banners            Wide artist banner with --logos
  --fanart             All Fanart.tv image families; 4K backgrounds win
  --no-logo, --no-label-logo, --no-portrait, --no-background,
  --no-banner, --no-album-cover, --no-cdart
                       Exclude a family from --fanart

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
  --country <name>     On artists: where they are from, as MusicBrainz says.
                       Not from a tag — there is no usable one — so it reads
                       what aede fetch stored, and an artist nobody has asked
                       about is not in the answer. A name, not a code:
                       --country france, --country 'united kingdom'. One word
                       reaching several countries covers all of them and says
                       so. Run aede countries for the list.
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
  --verify-existing    Compare existing files before resuming; conversion
                       compares against a fresh encode using this recipe
  --playlists          Write a selection playlist and remap copied M3U paths
  --dry-run            Say what would be copied, and write nothing
  --safe-names         Adapt names a destination refuses: ? : * < > and more
  --raw-names          Keep original characters; distinguish name collisions
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
  aede artist \"Miles Davis\"
  aede album \"Kind of Blue\"
  aede track \"So What\" --artist=\"Miles Davis\"
  aede search coltrane
  aede query \"genre:metal year:1990..1999\"
  aede fetch --fanart --no-background ~/Music/Jazz
  aede copy /Volumes/Player --query \"loved rating:>=4\" --verify
  aede help fetch",
        ui::cyan("USAGE"),
        ui::cyan("COMMANDS"),
        ui::cyan("GLOBAL OPTIONS"),
        ui::cyan("LIBRARY OPTIONS"),
        ui::cyan("FETCH OPTIONS"),
        ui::cyan("ARTWORK OPTIONS"),
        ui::cyan("FILTER OPTIONS"),
        ui::cyan("COPY OPTIONS"),
        ui::cyan("IMPORT OPTIONS"),
        ui::cyan("EXAMPLES")
    );
}

/// A few examples belong to the command they explain; the index keeps only a
/// compact cross-section so it remains a map rather than a second manual.
fn command_examples(command: &str) -> &'static [&'static str] {
    match command {
        "serve" => &[
            "aede scan ~/Music",
            "aede serve",
            "aede serve --port 0",
            "aede serve --bind 192.0.2.10 --port 8443 --tls-cert server.pem --tls-key server.key --authority music.example.test:8443",
        ],
        "cancel" => &["aede cancel <task-id> --data /path/to/aede-data"],
        "artist" => &[
            "aede artist \"Miles Davis\" --members",
            "aede artist <MusicBrainz-artist-ID>",
        ],
        "album" => &["aede album \"Kind of Blue\""],
        "track" => &["aede track \"So What\" --artist=\"Miles Davis\""],
        "recording" => &["aede recording <MusicBrainz-recording-ID>"],
        "credits" => &["aede credits", "aede credits \"Patient Number 9\""],
        "credit" => &[
            "aede credit --add recording:<MusicBrainz-recording-ID> --artist=\"Jane Doe\" --role=producer",
            "aede credit --exclude=<ID from aede relations>",
            "aede credit --undo=<ID>",
        ],
        "work" => &["aede work <MusicBrainz-work-ID>"],
        "release-group" => &["aede release-group <MusicBrainz-release-group-ID>"],
        "label" => &["aede label \"Blue Note\"", "aede label roadracer --offline"],
        "search" => &["aede search coltrane"],
        "query" => &[
            "aede query \"genre:metal year:1990..1999\"",
            r#"aede query "work:\"War Pigs\" instrument:guitar""#,
            r#"aede query "guest:\"Zakk Wylde\" -compilationartist""#,
        ],
        "review" => &[
            "aede review",
            "aede review --interactive",
            "aede review manson",
            "aede review --accept=<ID>",
            "aede review --all",
        ],
        "relations" => &["aede relations watt", "aede relations --tag=dubious"],
        "relation" => &[
            "aede relation <ID>",
            "aede relation <ID> --text=\"Needs verification\" --tag=dubious",
        ],
        "rules" => &[
            "aede rules",
            "aede rules --export --output=rules.json",
            "aede rules --import=rules.json",
        ],
        "export" => &["aede export --graph --output=graph.json"],
        "fetch" => &["aede fetch --fanart --no-background ~/Music/Jazz"],
        "copy" => &[
            "aede copy /Volumes/Player --query \"album.rating:>=4\" --verify --playlists",
            "aede copy /Volumes/Player --verify-existing",
        ],
        "scan" => &["aede scan ~/Music --dry-run", "aede scan --dry-run --json"],
        "notes" => &[
            "aede notes --waiting",
            "aede notes --relink=\"track:<old path>\" --to=\"track:<new path>\" --dry-run",
            "aede notes --relinks",
            "aede notes --undo-relink=<ID>",
        ],
        _ => &[],
    }
}

/// The detailed page for the one command that combines several independent
/// sources. Keeping it here leaves the front page useful as an index rather
/// than making every invocation of `aede help` a manual for `fetch`.
fn print_fetch_help() {
    println!("{}", ui::bold("aede fetch — enrich the catalog"));
    println!(
        "
{}
  aede fetch [name… | folder…] [options]

  With no extra option, asks MusicBrainz about artists and albums. A name
  narrows by artist or album title; an existing path narrows to that shelf.
  Names and folders can be combined. Nothing rewrites an audio file or tag.

{}
  --full               Ask again about answers already stored
  --dry-run            Show the planned requests without making any
  --summaries          Opening Wikipedia paragraph, with its source and licence
  --lang <code>        Preferred prose language; English remains the fallback
  --discography        All MusicBrainz releases credited to each artist
  --labels             Record-label identifiers from MusicBrainz
  --lyrics             Missing lyrics from LRCLIB, written as .lrc sidecars
  --identify           Identify fingerprinted files through AcoustID
  --credits            Recording performers, instruments, production credits,
                       work composers and lyricists, plus exact-edition roles,
                       all with relationship details and work/part links
  --recordings         Compatibility alias for --credits

{}
  --covers             Missing front cover from Cover Art Archive
  --size <what>        250, 500, 1200 or original (1200 by default)
  --images             Back, booklet and disc images in artwork/
  --portraits          Artist portrait: Wikidata first, Fanart.tv as fallback

  Run aede extract first when the music files already carry their artwork.
  Existing local images are never overwritten.

{}
  --logos              Artist and identified-label logos
  --banners            Wide artist banner, alongside --logos
  --fanart             Every supported Fanart.tv family in one metadata request:
                       artist and label logos, portrait, banner, background,
                       album cover and cdART. A 4K background wins over 1080p.

  Fanart.tv needs a free key in AEDE_FANARTTV_KEY. Artist images live beside
  the music when there is one shared artist folder, or under assets/ otherwise.
  Album cover and cdART images live in the album's artwork/ folder.

{}
  --no-logo            Skip artist logos
  --no-label-logo      Skip label logos
  --no-portrait        Skip artist portraits
  --no-background      Skip artist backgrounds
  --no-banner          Skip artist banners
  --no-album-cover     Skip Fanart.tv album covers
  --no-cdart           Skip cdART images

  These exclusions require --fanart. Each family is tracked separately, so a
  later run can fetch one previously excluded family without repeating the rest.

{}
  aede fetch
  aede fetch --summaries --lang=fr \"Miles Davis\"
  aede fetch --covers --images ~/Music/Jazz
  aede fetch --labels --logos
  aede fetch --fanart --no-cdart --no-portrait
  aede fetch --fanart --no-background ~/Music/Jazz",
        ui::cyan("USAGE"),
        ui::cyan("METADATA & TEXT"),
        ui::cyan("COVER & PORTRAIT ARTWORK"),
        ui::cyan("FANART.TV"),
        ui::cyan("FANART.TV EXCLUSIONS"),
        ui::cyan("EXAMPLES")
    );
    print_delegation_help("fetch");
    print_global_options(true);
}

fn print_server_help() {
    println!(
        "
{}
  Run aede scan <folder> first: the server needs an existing catalog.
  Use the same data directory for the server and CLI: --data <folder> or
  AEDE_HOME. The server listens only on 127.0.0.1:8787 by default.
  --port accepts 0..=65535; --port 0 chooses and prints a free local port.
  --bind accepts one literal IP address. A non-loopback bind requires all of
  --tls-cert, --tls-key and --authority, and never starts plaintext HTTP.
  The certificate and key are PEM files. --authority is the exact HTTPS
  HOST:PORT clients use, without https://; it may differ from --bind through
  NAT. HTTPS needs a fixed port, so --port 0 is refused with TLS.
  GET /api/v1/status checks availability; /api/v1/events and /api/v1/activity
  provide WebSocket catalog changes and task activity. GET /api/v1/albums,
  /api/v1/album?name=<title>, /api/v1/artists, /api/v1/artist?name=<name>,
  /api/v1/tracks, /api/v1/track?name=<title>, /api/v1/from?name=<artist>,
  /api/v1/doctor and /api/v1/search?q=<text> return structured JSON.
  URL-encode query values. See crates/aede-server/README.md for all routes,
  parameters and examples; docs/api.md defines the compatibility contract.

{}
  Without accounts, catalog reads are anonymous. aede accounts init creates
  the first administrator and protects catalog access; personal data is scoped
  to each session. See docs/server/accounts.md. GET /api/me/v1/playback provides
  native audio playback as processed f32le PCM over WebSocket, with consumed
  frame acknowledgements for private history. See docs/server/playback.md.
  A browser/mobile player interface remains separate work.
  Explicit HTTPS mode provides the supported remote access boundary; do not
  publish the default local HTTP listener through a proxy, tunnel or router.
  Windows path handling is covered by regression tests; native Windows
  validation is still required before release. Local command delegation
  and aede cancel remain Unix-only. See docs/design/paths.md.

{}
  AEDE_ADMIN_TOKEN enables POST /api/admin/v1/scan and POST /api/admin/v1/fetch,
  plus authenticated local-owner annotations, history and smart collections.
  Set a private secret of at least 32 ASCII characters before starting the
  server; an administrator account session can also authorize these routes.
  Send Authorization: Bearer <token>, never the secret in a URL or browser
  page. This token does not enable remote access.
  A JSON body (even an empty object) starts a task and returns 202 + task_id.
  GET /api/admin/v1/tasks/<id> reads its status and bounded command output;
  POST /api/admin/v1/tasks/<id>/cancel requests cancellation with no body.
  These routes require the same token. Tasks survive client disconnection,
  but their status is not kept across server restarts. Fetch uses the server's
  credentials and contacts only the explicitly selected/default CLI services.
  For compatibility, scan with no body remains synchronous (200 or an error)
  and rescans existing watched roots; it has no HTTP cancellation handle.
  A normal local CLI scan needs no administrative token.
  Personal writes use PUT /api/admin/v1/annotation?ref=<token>, POST/GET
  /api/admin/v1/history and GET/PUT/DELETE /api/admin/v1/collection?name=<name>.
  They write user.json for the local owner, never audio tags. /api/me/v1 uses
  the authenticated account's owner; persistent playlists remain future work.

{}
  On Unix, write-capable CLI commands from the same account and data directory
  run under the server. Run aede help scan or aede help fetch for cancellation.
  Account commands instead take the same writer lock locally, without delegation.
  Keep .aede.lock in place: every writer uses it to coordinate saved data.
  Ctrl-C or SIGTERM stops new work and waits for accepted commands to finish;
  a long fetch can delay shutdown. See docs/operating.md for permissions,
  backups, recovery and NAS deployment limits.",
        ui::cyan("STARTUP"),
        ui::cyan("ACCESS"),
        ui::cyan("OPTIONAL HTTP ADMINISTRATION"),
        ui::cyan("CLI COEXISTENCE & SHUTDOWN")
    );
}

fn print_cancel_help() {
    println!(
        "
{}
  Delegated scan and fetch commands print a task ID when they start.
  Use the same account and the same data directory as that command:
  --data <folder> or AEDE_HOME. IDs expire when the server restarts.
  Closing the CLI or pressing Ctrl-C does not cancel the server task.

  Cancellation requests a stop and returns immediately. Once stopped, the
  original CLI exits with code 130; answers already saved are kept.
  Only a running delegated scan or fetch can be cancelled: completed tasks,
  administrative HTTP tasks and other commands are refused. JSON-submitted
  HTTP scan/fetch tasks use POST /api/admin/v1/tasks/<id>/cancel instead.
  This command requires a running local Unix server; with no server it refuses.
  Local cancellation is unavailable on Windows. See aede help serve.",
        ui::cyan("TASKS & CANCELLATION")
    );
}

fn print_delegation_help(command: &str) {
    println!(
        "
{}
  On Unix, a running server for the same account and same data directory
  executes this command. Use the same --data <folder> or AEDE_HOME.
  Closing this CLI or pressing Ctrl-C does not cancel the server task.
  With no server, the command runs locally and Ctrl-C stops the local process.",
        ui::cyan("WITH A LOCAL SERVER")
    );
    if matches!(command, "scan" | "fetch") {
        println!(
            "  The delegated command prints its task ID. To stop it explicitly, run
  aede cancel <task-id> with the same data directory. Saved work is kept.
  Run aede help cancel for limits, and aede help serve for server setup."
        );
    } else {
        println!(
            "  This command has no explicit server cancellation yet; only delegated
  scan and fetch support aede cancel. See aede help serve."
        );
    }
}

fn print_global_options(include_data: bool) {
    if !include_data {
        println!(
            "\n{}\n  --no-color           Turn colours off\n  -h, --help           Show this page\n  -v, -V, --version    Show the version",
            ui::cyan("GLOBAL OPTIONS")
        );
        return;
    }
    println!(
        "
{}
  --data <folder>      Catalog location for this command
                       Default: $AEDE_HOME, then $XDG_DATA_HOME/aede,
                       then ~/.local/share/aede; .aede if HOME is unset
  --no-color           Turn colours off
  -h, --help           Show this page
  -v, -V, --version    Show the version",
        ui::cyan("GLOBAL OPTIONS")
    );
}

/// Prints one command's options, operating context and examples.
pub fn print_command(command: &str) {
    if command == "fetch" {
        print_fetch_help();
        return;
    }
    let page = command_page(command);
    let alias = COMMANDS
        .iter()
        .find(|(name, _, _)| *name == command)
        .and_then(|(_, alias, _)| *alias);

    println!("{}", ui::bold(&format!("aede {command} — command help")));
    println!("\n{}\n  {}", ui::cyan("USAGE"), page.usage);
    println!("\n{}\n  {}", ui::cyan("DESCRIPTION"), page.summary);
    if let Some(alias) = alias {
        println!("\n  Also available as: aede {alias}");
    }
    match command {
        "analyze" => {
            print_analyze_help();
            print_delegation_help(command);
        }
        "play" => print_play_help(),
        "serve" => print_server_help(),
        "cancel" => print_cancel_help(),
        _ if crate::mutates_store(command) => print_delegation_help(command),
        _ => {}
    }

    let options = OPTION_SCOPE
        .iter()
        .filter(|(_, commands, _)| commands.contains(&command))
        .collect::<Vec<_>>();
    if !options.is_empty() {
        println!("\n{}", ui::cyan("OPTIONS"));
        for (option, _, what) in options {
            println!("  --{option:<19} {what}");
        }
    }
    let examples = command_examples(command);
    if !examples.is_empty() {
        println!("\n{}", ui::cyan("EXAMPLES"));
        for example in examples {
            println!("  {example}");
        }
    }
    print_global_options(command != "play");
}

fn print_analyze_help() {
    println!("\n{}", ui::cyan("RESULTS AND REPORTS"));
    for paragraph in [
        "Unchanged tracks with valid existing reports are reused. --force analyzes them again. Results are stored in Aede even without --json.",
        "--json saves <album>.json in its album folder. --json-layout album|artist selects album or parent artist folders and enables saving. --force replaces selected reports. --show-results prints track results; progress is always shown.",
        "An artist-level FlacCompagnon report may cover several albums. Scan the folder containing it, or use aede import, to attach each result to its audio file. Album-level reports work the same way.",
        "When dates are available, the newer report modification date wins for overlapping results; new in-process measurements use their completion date. Imported results stay in conclusions.json when a report is deleted, including after a full scan. Changed audio files make their old measurements stale.",
    ] {
        for line in ui::wrap(paragraph, 76) {
            println!("  {line}");
        }
        println!();
    }
}

fn print_play_help() {
    println!("\n{}", ui::cyan("SELECTIONS"));
    println!("  aede play ~/Music/album.m3u     Playlist paths resolve beside the M3U");
    println!("  aede play collection:Favorites   Select a saved collection explicitly");
    println!("\n{}", ui::cyan("POSITION AND ORDER"));
    println!("  --seek TIME       Start the first played track at seconds, mm:ss or hh:mm:ss");
    println!("                    Seconds accept up to three decimal places, such as 1:30.250");
    println!("  --repeat off|one|all      No repeat, repeat one track, or repeat the selection");
    println!(
        "  --shuffle off|random|smart   Original, uniform random, or catalog similarity order"
    );
    println!("  --seed U64        Reproduce an order; requires random or smart shuffle");
    println!("                    Repeat and shuffle default to off; seek defaults to zero");
    println!(
        "  --lyrics          Show local lyric cues instead of the spectrum; requires terminal output"
    );
    println!(
        "                    Native output follows consumed frames; ffplay timing is estimated"
    );
    println!(
        "\n{}",
        ui::cyan("KEYS (macOS/Linux terminal and Windows console)")
    );
    println!("  Space    Pause or resume");
    println!("  n or →   Next track");
    println!("  p or ←   Previous track, or restart after three seconds");
    println!("  [ or ]   Seek ten seconds backward or forward");
    println!("  r        Cycle repeat: off → one → all → off");
    println!("  z        Cycle shuffle: off → random → smart → off");
    println!("  q or s / Ctrl-C   Stop orderly, restore the terminal and return to the shell");
    println!("\n  Without a terminal on standard input, the selection plays automatically.");
}
