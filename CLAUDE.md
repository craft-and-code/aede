# Working conventions — Aède

This file is for Claude Code and for any contributor. It states the rules of the project: what is settled, what must not be broken, and how Rust is written here.

Where this file contradicts a general habit, **this file wins**. Where a rule is clearly wrong for a specific case, say so explicitly rather than quietly working around it.

---

## 1. The project on one page

Aède is a local music library: give it folders, it builds a navigable catalog. The long-term ambition is to cover what Roon does, locally, with no subscription and on open metadata.

**Current state: milestone M0.6.** Folder scanning, native tag reading, graph model, statistics, diagnostics, command-line navigation — what the user writes about it — favourites, ratings, notes, free tags, listening history — and `copy`, which puts a selection on a player or a card.

**Deliberately out of scope for now:** any audio playback, any network access, any external database. Do not introduce them "while passing by" — each is a milestone of its own (see the roadmap in the README).

Rust 1.89 or later (`rust-version` in the workspace manifest, `msrv` in `clippy.toml` — the two must stay in step).

Domain vocabulary follows MusicBrainz: a `release` is what a user calls an album, a `recording` is a recorded performance, a `track` is the position of a recording within a release. Sticking to this avoids expensive misunderstandings at M1.

## 2. Commands

```sh
cargo build                      # offline once the dependencies are fetched
cargo test                       # 349 tests
cargo doc --no-deps --open       # the API documentation
cargo fmt --all                  # rustfmt.toml
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps   # broken links are errors
tools/check.sh                   # all five at once, before committing
tools/demo-library.sh /tmp/demo-music   # test library (needs ffmpeg)
```

`tools/check.sh` must pass before every commit. No exceptions.

**`Cargo.lock` belongs to the machine that builds, and is never delivered.** The development sandbox resolves against a vendored mirror rather than crates.io, which makes the lock it writes wrong twice over: it carries no `checksum` lines at all — the one thing a lock exists to provide — and it can name a version the registry does not have. It did: `flate2 1.1.10` shipped in a delivery, crates.io stops at 1.1.9, and `cargo test` refused to resolve. The lock is regenerated locally, from the real registry, and `rust-version = "1.89"` with `resolver = "3"` is what keeps that resolution inside the MSRV. Deliveries exclude it.

More generally, **a delivery carries the files that changed, and nothing else.** Shipping the tree as an archive and unpacking it over the working copy overwrote files nobody had touched — `.gitignore` twice, then `Cargo.lock` — and left staging directories behind to be cleaned up by hand. The blast radius of a change should be the change.

`tools/check.sh` includes `cargo doc` because a broken documentation link is silent everywhere else — neither the build nor clippy reads them — and moving an item between modules is precisely what breaks one. In a codebase where the reasoning lives in the doc comments, a dead link is a real defect.

## 3. Invariants

These properties are covered by tests. Breaking them breaks the project — if a change requires it, raise it first.

**A dependency is a requirement, not a dogma.** Adding one is a decision to argue for, not a reflex. Three criteria, all three of them: it does something we could not do as well ourselves; it is maintained and widely used; its own dependency tree is small enough to read. Propose it, say what it replaces, and ask before adding it.

Current list: `lofty`, for the containers we have no parser of our own for. Planned: `serde` at M2, when the HTTP contract makes hand-written serialization the wrong trade.

**The hand-written parsers stay.** `lofty` is a fallback reached only when the signature matches none of them, and it must never become the primary path for FLAC, MP3, MP4, Ogg, WAV or AIFF. Those parsers extract things no general-purpose library exposes — the LAME encoder delay and padding, the ALAC magic cookie, the Opus pre-skip — and M3 needs them for gapless playback. A format one of them claims and then fails on keeps its own diagnosis: the fallback is for `UnrecognizedFormat`, nothing else.

**The model is a graph.** The `credit` table (who does what, on what) and the `relation` table (typed links between entities) are the heart of the system. Never "simplify" towards an album → artist hierarchy: that graph is precisely what will let a user click a drummer and see their forty appearances.

`model/` is divided by verb, and the division is load-bearing rather than cosmetic: `query.rs` takes `&self` throughout, so a lookup that tried to change something would not compile; `builder.rs` is the only place identifiers are handed out; `relations.rs` holds what is inferred rather than read, which is why it is the thing that carries `RELATION_RULES`. A new function goes to the file whose verb it is, not to whichever one is shortest. What the rest of the program calls is re-exported from `model/mod.rs`, so callers never name the sub-modules.

**A test asserts on what survives the display.** Path columns truncate from the **left**, so the last components — the ones that identify the thing — are what remains on a narrow terminal. A test matching a whole path therefore passes on Linux and fails on macOS, where a temporary path alone is sixty columns; this has now cost two rounds. Match the tail, or assert on something the renderer never shortens.

**A folder is not a prefix of its name.** `text::is_under` is the only way to ask whether a path sits in a folder. The obvious `path.starts_with(root)` is wrong on strings — `/music/Rock` claims every file of `/music/Rockabilly` — and `aede roots` counted a neighbour's files that way while `check` had already got it right inline, which is how one idea becomes two. The test is on a separator boundary, in one place.

**One idea, one implementation.** `text::file_name` and `text::folder` split a path; `clock::now_seconds` says what time it is. Both had grown three copies in three files, each a little different — which is how "the same thing" quietly becomes two things. A helper short enough to retype is exactly the one that gets retyped: put it where it belongs and use it.

**Construction is deterministic.** Two scans of the same library produce exactly the same identifiers. Consequences: sort before iterating, never let `HashMap` iteration order leak into output or into identifier assignment, and prefer `BTreeMap`/`BTreeSet` wherever order matters.

**Parsers never panic.** No `unwrap`, `expect`, `panic!`, direct indexing or unchecked slicing anywhere in `tags/` or `audit/`. A truncated or corrupt file yields an error or a partial result. Use the `Cursor` from `tags/bytes.rs`, whose reads all return `Option`. Use `checked_`/`saturating_` arithmetic on any value that comes from a file.

**Inferred data carries the version of the rules that inferred it.** `model::RELATION_RULES` is bumped when the way relations are derived changes; `store::from_json` then recomputes them, in memory, from the credits and tracks it just read. This is deliberately not `FORMAT_VERSION`: an out-of-date inference is stale, not invalid, and refusing to load would cost the user their integrity verdicts. The rule generalises — anything derived rather than read belongs here, not in the format version.

**The on-disk format is versioned.** Any incompatible change to the catalog bumps `store::FORMAT_VERSION`, and reading an older file must produce a clear message rather than a crash.

**A destructive command says what is lost, then asks.** `reset` lists what the catalog holds before removing it, and distinguishes what a rescan brings back from what it does not — the watched folders, the integrity verdicts and the imported analyses. With no terminal to ask on it refuses instead of assuming: assuming "no" makes a scripted reset fail without saying why, assuming "yes" removes something nobody agreed to lose. `--yes` is the explicit consent.

**A short option is the long one written shorter.** `args::SHORT` resolves it to a long name and everything downstream — values, guards, the missing-value check — sees one option. Aliases stay few: four saved keystrokes cost a documented line for ever, so they are worth it only where the option is typed constantly (`-o`, `-j`, `-h`, `-V`).

**The help is part of the contract, and one table proves it.** `main::COMMANDS` is the single list of every command, its alias and the function it runs: the dispatcher reads it, and a test walks it demanding that the help name each one. That test exists because the rule had been written for `help` itself and left untested, and two commands — `find` and `favorites` — then worked for a week without appearing anywhere in the help. A rule with no test behind it is a preference.

**The help is part of the contract.** An option printed under a heading that names the one command it does not work on is a lie that costs more than a missing line. Each option in `print_help` says where it applies, and a test asserts the help names every filter option the parser accepts — the same claim, checked from both ends. A command's own line in COMMANDS names the filters it honours, too: for most readers that line is the whole help, so `--year`, honoured by `albums` and named only in the section below, existed for them nowhere. It runs the other way as well — `help` answered for months without appearing in its own list of commands.

**An option a command cannot honour is refused.** `main.rs` holds `CSV_COMMANDS`, `M3U_COMMANDS` and `OUTPUT_COMMANDS`: the global option list only says an option exists, these say where it means something. Accepting `--csv` on `stats` and doing nothing is the same fault as swallowing a misspelled option — worse, in fact, since the command then reports success. Adding an option means adding it to a list here, or it will be silently ignored somewhere — **and the table has to be complete, not growing.** This fault was fixed three times one option at a time (`--csv`, then `--genre`/`--label`, then `--artist`/`--year`) while fourteen others stayed unguarded: `aede stats --severity=error`, `aede albums --full`, `aede artists --with Miles` all answered cheerfully and dropped the word. Fixing a class one member at a time is how the class survives. One end-to-end test now walks the whole list, and a new option that is not in the table fails it. Two corners the table cannot see, and which have their own checks: an option that needs *another option* rather than a command (`--separator` and `--tracks` are meaningless without `--csv`), and a **value** the option cannot read — `--sort banana`, `--severity=banana`, `--threads abc` and `--year=abc` all used to fall back on a default and answer a different question. Every option that takes a value reads it strictly, through `Args::whole_number`/`Args::number_or` for numbers or an explicit match for keywords; there is no such thing as a silent fallback.

The guard is only half of it, though, and the cheaper half. **Adding an option to the table where it is refused is not the same as deciding where it belongs.** `--json` was guarded to the four commands that read it, which was accurate and still wrong: every listing and every page could produce a CSV of exactly the rows a JSON would carry, and the option had simply never been wired to them. Refusing it there made a nine-month silence look like a decision. So the question a guard entry raises is *why not here* — and when the answer is "no reason", the fix is to implement it, not to forbid it. `export::rows_table` now renders both formats from one set of rows, which is what makes "a column exists in one and not the other" impossible rather than merely unlikely; a test walks the CSV header and demands every column in the JSON. `--artist` and `--year` were the proof: declared, documented, guarded nowhere, so `aede artists --year=1969` answered about every year. A filter whose value does not parse is refused for the same reason — `--year=abc` used to become no filter at all, which is the whole library returned under a name that promised one year.

An option the program has never heard of is refused too, and **before anything answers, `--help` and `--version` included**. It used to be a warning: `aede albums --limite=5` put one line on the error stream and the whole unlimited listing on the standard one, and the answer is the half that gets read. `aede --fegioregj` printed a cheerful help page — the same silence in a friendlier costume, since the page says nothing about the command line having failed to parse. `args::nearest` proposes the closest known option within a third of the typed length, and `args::as_typed` quotes the option back in the spelling it was typed in, `-z` and not `--z`. The rule that covers all of it: **a command either answers the question it was asked or refuses; it never answers a different one.** Warning and carrying on is answering a different one, and so is printing the help. `aede --data ~/music` named a catalog, did nothing with it, printed the whole help and reported success — the option going into the void exactly as a swallowed argument does, with a page of text making it look like an answer. Running the program with nothing at all still asks for the help, and so does an option that only shapes what is printed (`PRESENTATION_OPTIONS`), because `--no-color` has the help itself to act on. Anything else with no command is refused.

**Every entity deserves a page, and every page a filter.** The model is a graph; a listing that counts genres without letting you open one is a dead end. `artist`, `album`, `track`, `genre` and `label` each have a singular page, and what a page gathers is a selection — so `--csv` and `--m3u` work on it, through `commands::selection_output`, without the command knowing anything about them. Adding an entity kind means adding its page.

**What is shown is what is accepted.** `commands::ROLE_NAMES` is one table read in both directions — `role_label` for display, `role_key` for input — because a one-way `match` produced a message that denied a role and listed it in the same breath: the screen said "album artist", the parser wanted "album". Anything the interface prints as a name must be typeable back in, and an error offering alternatives offers them in the spelling it displays them in. The same applies to any vocabulary added later.

**A role is a question asked in both directions.** `Catalog::artists_in_role` answers "who does this here", `Catalog::tracks_of_artist_in_role` answers "what did this person do in that role", and `--role` carries both readings depending on whether it is attached to the listing or to a page. That inversion is the whole reason the `credit` table stores a role rather than being a bare artist column. `Catalog::roles_in_use` reads the vocabulary from the credits rather than from a fixed list — a role arriving from MusicBrainz at M1 must work without a line of code. A role needs a person, which is why `album` and `track` refuse it and say so: there, `--artist` is the filter.

**A column headed with a unit counts that unit, and two tables on one page must agree.** The Artists table of a facet page is headed *Tracks* and was counting **credits**: a band credited as main artist and as performer on each of its own tracks — the ordinary shape of a well-tagged file — showed 57 for the 29 tracks the albums table listed directly above it. Both numbers were on screen at once, which is what made it a defect rather than a curiosity: a page that contradicts itself teaches the user to distrust every figure on it. `facet::tracks_per_artist` folds the roles per track before counting, and its unit tests bound each count by the number of tracks the page holds. Any figure derived from the `credit` table is a count of credits until something makes it otherwise.

**A page that answers does not open by denying.** `aede label earache` printed *no label is called "earache"* immediately above a heading reading **Earache Records**, while `aede albums --label earache` narrowed on the same text without a word — the note was reporting the mechanism (exact lookup missed, substring lookup ran) to a user who had asked a question and got it answered. `facet::match_note` speaks only when the heading cannot: one name is its own explanation, several need saying, since the heading joins them with commas and reads as one. A note earns its line by carrying something not already on screen.

**The catalog is shared; what a person says is theirs.** There will be user accounts — the Subsonic surface at M2.5 has them by definition, and Aède's own front end will want them. The line to hold from now, before there is anything to migrate, is the one between the two kinds of data:

- **Facts about the files** — files, tracks, releases, artists, credits, relations, genres, labels, integrity verdicts, imported analyses. Read from the disk or measured on it, identical for everyone, and belonging to the catalog. Two people looking at the same library see the same facts.
- **What someone said or did** — favourites, ratings, notes, user tags, play history and counts, queues, saved queries. These belong to a person, always, even when there is exactly one.

The rule that follows: **no per-user field ever lands on a catalog entity.** A `rating` on `Release` or a `play_count` on `Track` reads as harmless while there is one user and becomes a question with no answer — *whose?* — the day there are two, and by then every read in the program assumes the single answer. As of M0 the boundary is intact: every table in `Catalog` is a fact. Keeping it that way costs nothing; repairing it later costs everything downstream of it.

And the shape that makes the migration never happen: **a per-user record carries an owner from the first version in which it exists**, and every read filters by it, even when the only owner is the local one. The single-user case is then the multi-user case with one user — the same code path, exercised on every run, rather than a second path written blind years later. Same reasoning as `args::Window` and `EntityRef`: one reading for the whole program, decided once.

What M0 deliberately does *not* decide: authentication (M2's problem, and Subsonic's legacy scheme must stay inside the compatibility layer rather than reaching the model) and authorization. The working assumption to argue against rather than from: the library is shared and only the annotations are private; scanning, importing and resetting are the owner's, not a listener's. The catalog itself has no owner, and that is a decision rather than an omission.

**What the user writes is the only irreplaceable thing here, and it is treated that way.** `user.rs` holds it: favourites, ratings, notes, free tags, plays. Four rules, each of which has already cost this project something once.

*Its own file.* `user.json`, with its own `USER_FORMAT_VERSION`, never inside the catalog. The catalog is derived from the disk and written whole; a rating that changes on a keystroke has no business rewriting a library, and `reset` says out loud that it is not taking this file with it.

*Never keyed by an identifier.* `EntityRef` names a thing the way the thing names itself — a path for a track, `artist|title|folder` for a release, the normalized key for the rest. Catalog identifiers are positions a scan renumbers, which is exactly how the imported analyses were lost the first time.

*Never dropped.* `user::reconcile` runs on every read. A target that no longer resolves is retried by file name, and rewritten only when **one** file matches — two candidates give no reason to prefer either, and moving somebody's note onto the wrong track is worse than leaving it waiting, because nothing on screen would ever say so. What still does not resolve is kept: the drive may simply be unplugged.

*A note is text, and text is kept as given.* One note per entity, stored byte for byte: no wrapping, no trimming, no reflowing, and no rendering. `--file` and `--file -` exist because a written thing does not fit between two quotation marks on a command line. Markdown is the intended format and is the **front end's** business at M2 — which makes the note untrusted input that must be escaped before it reaches any HTML, and makes any "helpful" rewriting on the way in a defect: the day Aède reformats a note is the day the note stops being the user's. On a page it gets a section of its own rather than a row among the stars and the tags, because a rating is a label on a thing and a note is something somebody wrote.

*One record, not four.* A favourite, a rating, a note and a set of tags are four ways of having an opinion about one thing. `Annotation` holds all four, which is what makes copying everything said about one album onto another (`note --from`) a record copy rather than a loop, and `forget_empty` is what stops an emptied record from surviving as a shell.

The bounded log and the unbounded counters are two structures on purpose: the log answers "what did I listen to last night", the counters answer "what have I never heard", and a truncated log cannot do the second — which is the question M3's `discover` shuffle asks.

**A query is an interface, not a storage engine.** `query.rs` defines the grammar on its own terms, so it works today over the vectors in memory and tomorrow over SQL; defined as "whatever the database makes easy" it would arrive late and shaped by the wrong concerns. Three rules hold it together.

*It evaluates over tracks, always.* A track is the finest grain and every coarser answer is a fold of it — the albums matching a query are the albums of its tracks. One evaluator, not five, and what comes out is a **selection**, which is already what `--csv`, `--m3u` and M3's queue consume. That is what makes a saved query a smart collection and a smart collection playable, with nothing new built.

*A field says where an opinion was written.* `rating` is the track's, `album.rating` the album's, `artist.rating` the artist's. Folding the three together would make "rated five stars" mean something different depending on where the user happened to put the stars, and no message could say which was meant.

*A value that cannot be compared is absent, not zero.* A track with no year does not satisfy `year:<2000`; counting the missing as zero would file every untagged file under "before 1970" — an answer, and a wrong one. Same reasoning as the strict option values: no silent substitution, ever.

*A saved query holds the question.* `user::Collection` keeps the expression as written, never the result — that is the whole difference between a smart collection and a playlist, and it is why running one produces a selection and costs nothing to play. It is parsed **when it is saved**, because a collection that only fails when somebody opens it is a trap left for later. And a listing of collections that cannot parse one shows it anyway, with `?` for its size: a grammar that drops a field must not take the whole screen down with it.

*An import merges; nothing here ever replaces.* `user::merge` is the only way in. Someone restoring half a backup wants their two halves, and a replacing import would be the one operation in this program able to lose everything at once. Last write wins per record, the loser is counted out loud, play counters take the larger of the two, and an event is identified by owner, track and time so that importing twice changes nothing.

*The options are shorthand for the grammar.* `browse::albums_query` turns `--genre`, `--year`, `--label`, `--comment`, `--artist` and the compilation flags into one expression, which the one evaluator answers. They used to be a second filter loop — two implementations of one question, and the day one changed nobody would have seen it. A test walks both doors and demands the same answer.

One mapping there is a **decision, not a transcription**: `--artist` on an album listing means the *album artist*, so it becomes `albumartist:` and not `artist:`. The obvious mapping would have quietly listed every album an artist guests on as one of their own. No end-to-end test could catch it — the reference library holds no such guest — so it is tested on the expression the options build, where the decision is taken. When the fixtures cannot express a difference, test the decision rather than the outcome.

`track` went the same way, and its mapping shows why the grammar had to exist first: `--artist` there means a credit **or** the album's own artist, which is an `OR` and therefore unsayable in options. What stays outside is stated rather than forgotten — `artists --role` answers about artists rather than tracks, and folding a track query into an artist answer loses the question; `artist --with` already calls one model function, so a query string there would add indirection without removing duplication.

*A value naming nothing is a misunderstanding, not an empty result.* `query::unknown_values` reports a genre, label or artist the library has never heard of, and the commands refuse. Without it the grammar would have been a step backwards from the options it replaced, which drew that distinction already — the same one `artists --role` draws between a role nobody holds and a word that is no role.

*Unknown sorts last, in both directions.* `query::sort` puts a track with no value for the key at the end whichever way round the sort was asked, which is why that test sits outside the reversal: "unknown" is not "smallest", and sorting by year must not open with everything nobody ever tagged. Ties fall back on catalog order, without which `--offset` would show one row twice and hide another.

Adding a field means one row in `FIELD_NAMES` and one arm in `texts_of`/`number_of`. An unknown field names the ones that exist rather than shrugging, and a flag is accepted both as `loved` and as `loved:false`, since offering one spelling and refusing the other makes the refused one a trap.

**A truncated list says so, and says where it stopped.** `args::Window` is the one reading of `--limit`, `--offset` and `--all` for the whole program, and `commands::announce_window` the one way of reporting it: `1–50 of 312 albums`, nothing at all when everything fit, and an explicit line when the window falls past the end. All five listings used to stop in silence, and sorted by year that made the most recent albums of a real library invisible — the header counting the matches is not enough, since nobody compares it against the rows they were handed.

Paging is only meaningful because construction is deterministic: page two is the rows after page one, and that holds only while every listing sorts. The window is read **strictly** — `--limit abc` is an error, not a fall-back on the default — and `--limit=0` is refused in favour of `--all`, because a size of zero is never what anyone means and "everything" deserves a name rather than an encoding. Any new listing goes through `Window` and that helper.

**A value that is a name may be typed in several words.** `args::VALUED_NAME` (`--artist`, `--album`, `--with`, `--genre`, `--label`) takes every word up to the next option; `args::VALUED_WORD` takes exactly one, because a number, a path or a keyword is one word. The bug this fixes was the worst kind: `artist Ozzy --with Jeff Beck` gave `--with` the word "Jeff", left "Beck" to be joined onto the positional, and went looking for an "Ozzy Beck" nobody had typed. A wrong answer built in silence is worse than a refusal — and here it is worse than being permissive too, since the shell has already split the words and only this program knows they were one name. Adding a name-valued option means adding it to the right list.

**An option means the same thing wherever it is typed.** `--m3u` and `--csv` on `album`, `artist`, `track` and `search` all describe the tracks on screen, through one helper (`commands::selection_output`); `export` describes the catalog. A command that cannot honour an argument says so — `aede export "an album"` is an error, never a full export under a name that promised a selection.

**Each export answers one question.** JSON is the faithful dump and mirrors the model; CSV is a flat table for a spreadsheet, denormalized on purpose and carrying raw values, since a formatted column cannot be summed; M3U exports a selection, not the catalog. Adding a format means saying which question it answers that the other three do not.

**An argument a command cannot read is refused.** The twin of the rule above, and it bit later: `main.rs::takes_no_argument` names the commands that read only their options, and a positional given to one of them is an error pointing at the command that does take it. `aede artists ozzy --role producer` listed every producer with "ozzy" going into the void — an answer that looks right is worse than one that fails.

**A count of zero is an answer; an empty screen is not.** `stats` prints the credit vocabulary the library actually holds, so `--role composer` returning nothing can be told apart from a bug. Wherever a filter can legitimately match nothing, something must let the user see the domain it filters over — otherwise every empty result reads as a defect, and the user is right to think so.

**A filter is visible or it is not a filter.** `albums` prints the facets it narrowed on, because a filter that leaves the count unchanged cannot be told from one that was ignored — and this project shipped `--genre` and `--label` declared in the option list and honoured nowhere for months. A filter matching nothing is an **error** naming where to look, never an empty listing: an empty listing reads as an empty library.

**A folder given to a command is walked whole.** `import` recurses, because reports are filed the way albums are — one folder per artist, one per album — and a walk that stopped at the top level would report "nothing found" on the folder the user actually meant. The same applies to any folder argument added later.

**The persisted JSON mirrors `schema.sql`.** One key in the file equals one table in the schema. Adding a field to the model means reflecting it in both. That is what will make the move to SQLite mechanical.

**The same album twice is two releases and one relation.** Never merge two folders into one release: they are two sets of files, and the folder is what the user acts on. `model::DUPLICATE` and `model::OTHER_EDITION` say which case it is — same track list and same encoding, or same track list and a different one. Reporting belongs to the album level: a copied album is one issue, not one per track, and an other edition is information rather than a warning.

**Roles are not interchangeable.** `model::is_performing_role` separates being audible on a recording from having written or produced it. The distinction drives the artist page, the performer rankings and the collaboration graph; ignoring it puts other people's albums in an artist's discography.

A figure derived from it carries the name of the role class it counts. An artist page shows `performing:` and `writing:`, never a bare total: tags credit the band, not its members, so a lyricist with forty albums has zero performing credits and the unlabelled zero read as an error.

The two lines are **not a partition**, and must not be made into one. Someone who writes what they sing belongs in both, because writing it and playing it are two facts about the same track, not two halves of one. The page also holds *display* sets that do subtract — `releases_written_without_performing`, `written_tracks_without_performing` — so a table does not repeat what the table above it already showed. Those exist to be printed, never to be counted: reporting one as `writing:` announced Ozzy Osbourne, sixty-nine composer credits, as writing a single track. A method whose result is a display set says so in its name, and a summary line reads from the measure, never from the size of a table.

**A command answers its question, whatever the work was.** `check` prints the same table whether it read a thousand files or none: the state of the library, then a line about this run. It used to withhold the verdicts precisely when everything was already verified, leaving "every file already has a verdict" and no way to learn *which*. A shape that changes with the outcome is a shape nobody can learn — the same rule that keeps a listing looking identical whether it matched one row or a thousand. State and run are separate lines because they count different things.

**"Not checked" is not "nothing to check".** An absent integrity verdict means no one has looked yet, and that can change; `Verdict::NothingToCheck` means the container carries no checksum, and that never will. Collapsing the two would have the user re-running `aede check` forever on their MP3s. The same distinction holds in the JSON, in `schema.sql` and in whatever the interface ends up drawing.

**A verdict belongs to the bytes it was reached on.** `integrity` travels with the file entry: a scan that reuses an unchanged file keeps it, a scan that re-reads a modified one drops it. Never carry a verdict across a change of size or date. An imported analysis follows the same rule from the other side: `FileAnalysis::still_applies` compares the size and date the other tool saw against the ones the catalog holds, and nothing acts on a record that fails it.

**A measurement carries who made it.** Imported analyses live in their own table, attributed to their source, and are never merged into Aède's own fields. A bit depth read from the wasted-bits counts and one obtained by decoding are two different claims; merging them loses the provenance and, worse, hides the case where they disagree. Noticing the disagreement is the whole reason to keep the data. `doctor` reports an MD5 mismatch as an **error** even when `aede check` said intact — the frame checksums prove the container, the MD5 proves the audio, and passing one while failing the other is a finding, not a contradiction to arbitrate.

**A folder identifies a release; a *disc* folder does not.** `Release` is keyed on `title|album artist|folder`, and the folder is there to tell two editions apart. `Album/Disc 1` and `Album/Disc 2` are not two editions, so `text::disc_folder` recognises that shape and the builder keys on the parent. One Final Fantasy VII soundtrack was coming back as two albums of the same name, each numbering from one — and the disc-number rendering added just before it was correct and never fired, because no release ever spanned two discs. Two lessons worth keeping: a display that never triggers is a signal to check the *model*, not the display; and when a key is made of several parts, ask of each new layout whether every part still means what it meant.

**A scan may not destroy what it cannot recompute.** Imported analyses travel with the catalog a scan rebuilds. Tags, durations and the graph all come back from reading the files again; an hour of someone else's decoding does not. Anything future that is entered rather than read belongs in that same carry-over.

**Entered data is keyed by path, not by identifier.** Identifiers are positions that every scan renumbers, so anything not rebuilt by the scan would have to be remapped after it — and, worse, could not exist before it. `FileAnalysis` therefore carries the path it describes and no id. That is what lets a report be imported into an empty catalog and attach itself later, which makes the order of "analyse" and "scan" irrelevant. A record whose file the catalog does not hold is waiting, not broken: it is counted (`Catalog::pending_analyses`), reported, and never diagnosed as a defect.

**Matching a file is not the same as describing it.** `analysis::merge_into` is the single place that decides which file a record is about — by path, then by name and size — and both routes end in the same `still_applies` test against that file's size and date. The fallback bypassing it was a real bug: a name and a size can agree while the modification date says the tags were rewritten yesterday. `analysis::reconcile` re-tries the waiting records after a scan, which is what makes a report written against a symbolic link (or `/var` against a canonical `/private/var`) attach at all. Never re-implement this matching in a caller — the scan and `import` share it precisely so the two cannot drift.

**A count that cannot be acted on is half an answer.** `doctor` says how many imported analyses are still waiting for a scan; a user who has already scanned and sees the number unmoved has no way, from a count alone, to tell "not scanned yet" from "will never match — the report was exported against a library that has since moved or was renamed". `Catalog::pending_analyses_list` names them, `aede import --pending` shows them, and `aede import --forget --pending [folder…]` drops exactly that set without touching analyses that did attach. `Catalog::pending_analyses` is defined as `pending_analyses_list().len()` rather than a second copy of the same filter, so the two can never disagree about what "pending" means.

**A listing is grouped by the unit the reader acts on.** Waiting analyses are reported **by folder with a count**, never one row per file — both by `import --pending` and by the import summary (`Attachment::waiting_folders`). A report covering a fourteen-track album is one decision, and fourteen rows spend the screen to say it once. The corollary is that the folder column is left *unbounded* while every other path column in the program uses `Table::path_limit`: that helper cuts the **head** off a path because the file name is what identifies a file — true everywhere except here, where nobody wants a file. `…/1980 Blizzard of Ozz/01 I Don't Know.flac` is precisely the useless half. Before reaching for `path_limit`, ask which end of the path answers the question on screen.

**The one command that writes files writes them outside.** `copy` is the only thing in the program that creates files, and its destination is by definition not a library: it is never scanned, never becomes a catalog, and nothing about it comes back in. Three refusals enforce that rather than documenting it — a destination that does not exist (a missing folder is a drive that is not plugged in, and creating it fills the internal disk instead), a destination under a watched root (the next scan would read the copies back in and `doctor` would report every album as its own duplicate), and a destination without room (checked before the first byte, not discovered on the last album). `copy::plan` decides everything and touches nothing; only the caller writes. That split is what makes `--dry-run` a consequence of the design rather than a feature bolted onto it, and what lets every placement decision be tested without a filesystem.

**A path from the command line is canonicalized before it is compared.** `commands::canonical` is the one place that does it, and every path a command receives and will match against a stored one goes through it. Watched roots are stored canonical and the comparisons are string comparisons on a separator boundary, so a path reached through a symbolic link names the same folder by a string that never compares equal — and on macOS that is the *ordinary* case, `/var` being a link to `/private/var`. The step existed four times, spelled four ways, in `scan`, `roots`, `check` and `copy`; `copy` — the one command that writes — was the one that had left it out, so its "this destination is inside your library" refusal waved through exactly the case it exists to catch. The bug reached the user because both the code and its test were written on Linux, where `/tmp` is not a link. **When a guard compares a user-supplied path against a stored one, the regression test must reach it through a symlink**, which is the portable way to reproduce what macOS does by default.

**An external program is not a dependency, and the difference is load-bearing.** `--compress` runs ffmpeg; nothing is linked, nothing is vendored, and a checkout without ffmpeg builds and passes its tests. That is what makes it acceptable under the dependency rule, and it comes with obligations: the program is looked for **once, before the first byte is written** (not per file, and not after half a copy), its absence is a sentence saying how to install it, and the tests that need it skip themselves *loudly* rather than failing or — worse — silently not running.

**A scan may not destroy what it cannot recompute — and the list keeps growing.** Imported analyses, and now the scan **exclusions**. Both are typed by the user and derived from no file, so `scan::scan`, which rebuilds the catalog from what it reads, has to carry them across explicitly. The exclusions shipped without that line in their first version, and the symptom was precise and baffling: an exclusion that worked on the run that set it and vanished on the next. When adding any field to `Catalog`, the question is not "does it round-trip through `store`" but "does a **rebuild** keep it" — those are different questions and only the second one is about `scan.rs`.

**Decided for M1, before there is any code to argue with: a MusicBrainz value sits beside the tag, never on top of it.** The precedent is `analysis` — attributed to its source, never merged, which is the only reason `doctor` can say two methods disagree. Overwriting the `genre` field with a MusicBrainz genre would lose the provenance, lose the disagreement, and lose the undo; and being derived from no file, it would not survive the next scan's rebuild either (see the rule above). M1 therefore gets its own attributed layer, carried across a rescan, removable, with the display choosing which value to show. Anything in M1 that proposes to write into the fields the tags fill is a design change to raise, not an implementation detail. **And it is stored whole, including where it agrees with the tag**: "checked and matches" and "never checked" are two different states, and a layer that recorded only divergences could not tell them apart — nor answer "does my tag still match" offline the day after the user re-tags a file. Raw tags are already kept per file for the same reason.

**A design that is right can still answer badly.** A bare `loved` asks about the track, and that is correct — five stars on an artist is not five stars on a track. But somebody who marked an *album* a favourite types `loved`, is told nothing matches, and concludes the feature is broken: the semantics were right and the *answer* was misleading. `query::rescoped` re-asks the same question at the album and artist scopes when the result is empty, and the empty answer names the scope that holds something, offering an expression that can be typed back in. The query still means exactly what it says. Generalise this rather than the fix: where a correct-but-surprising rule produces an empty result, the empty result is the place to explain the rule — not the rule's place to bend.

**Two emptinesses, two explanations.** A listing shows no rows either because nothing matched or because the page asked for lies past the last row, and `announce_window` explained both with the paging sentence. Harmless while the listings could only be paged; misleading the day they learned `--query`, where `aede artists --query "year:2050"` answered "0 artist in all, and --offset=0 starts past the end" — sending the reader after a page number they never typed. A message that covers two states has to name the one it is in, and the give-away is a value in it that reads as absurd (`0 in all`, `--offset=0 ... past the end`).

**Before calling something missing, look for it in the rest of the library.** `doctor` learned to report a whole disc absent from a set — `disctotal` says four, three are here — and the first version of it reported a false one on a layout that is common in the wild: `Box CD1` beside `Box CD2`, sibling folders rather than a child of a common parent, which the disc-folder rule does not recognise. That is two releases of the same album, each announcing two discs and holding one, each pointing at the other as lost. The check now consults every release sharing the album key (title and album artist, deliberately *without* the folder that identifies an edition) before it speaks. The general shape: an absence is a claim about the whole library, and it cannot be established from one record in it.

**A fact and an inference are not reported the same way.** An imported report carries both, and `doctor` used to relay them alike: a failed MD5 — two methods compared a checksum and disagreed, and `check` can settle it — beside "early roll-off at 33 kHz, possible transcoding", which is a heuristic its own author hedges. The label flattened the hedge on the way through: `made from a lossy source` headed a line whose detail said *possibly*, and `upsampled` was filed under the same heading though upsampling is not a lossy ancestry. On a 1988 analogue master, where nothing above 30 kHz exists to begin with, a faithful 24/96 transfer looks exactly like the thing being warned about. The spectral verdicts are now imported, stored, kept fresh and said nowhere; `analysis::FileAnalysis::suspect_encoding` is called by nothing, on purpose, and the measurements they are drawn from stay on the file's page. **Relaying another program's guess as your own warning is a category error, not a display detail** — and the test that guards it asserts an absence, which is the only shape that catches a line coming back.

**A store that only shows its failures cannot be trusted about its successes.** `import --pending` listed what had failed to attach; nothing listed what had attached. So a report imported over an artist whose files are clean produced no waiting line, no `doctor` entry and no message at all — every symptom of having done nothing, and the user reasonably concluded the import was broken. `--list` is the missing half, and it reports **three** fates rather than two: attached, waiting, and *stale* (attached to a file whose bytes changed since), the third being invisible everywhere else and the one that silently voids a verdict. The same asymmetry existed one level up: both readings — Aède's own `check` and the imported one — lived on the *track* page only, so verifying an album took one command per track. Generalise it: for every "what went wrong" listing, ask what shows the population it was drawn from.

**An instruction the program can carry out is a chore handed back.** Three commands ended by printing "run `aede scan` to drop them", and a user who forgets is left with a catalog describing a library they no longer have — with nothing on screen saying so. `roots --remove` and `roots --exclude` now run that scan themselves (`commands::scan::take_effect`), with `--no-scan` for the person dropping four folders in a row. Two things make it work rather than merely convenient: the automatic scan takes its roots from the **catalog only** (`Watched::Only`), because the positional of `aede roots --remove ~/Music` is the folder being dropped and feeding it to `resolve_roots` would add it straight back; and **`reset` is deliberately excluded** — it destroys what a scan cannot rebuild, it is the one command that asks for confirmation, and rebuilding a catalog somebody just chose to throw away answers a question nobody asked. The general rule: a command that creates the need for another command should satisfy it, *unless* the first command's whole purpose was to destroy something.

**Two programs that draw the same picture must draw it identically.** `spectrum` reproduces FlacCompagnon's ffmpeg filter, size, colour map, gain and output folder (`spectres/`) character for character, French name included. They are used on the same library, and the point of having two spectrograms is to compare them — a difference in scale or gain makes the pair unreadable while looking, individually, perfectly fine. Where a second tool's output will sit beside a first one's, matching it exactly is the feature, not laziness. The corollary held here too: `find_ffmpeg` and the "how to install it" message now live in `core::ffmpeg`, because two searches that could disagree about which ffmpeg is *the* ffmpeg is one too many.

**A freshness test asks the disk, not the catalog.** `spectrum::out_of_date` compares the picture's date with the **track's date on disk**, never with the `mtime` the catalog recorded at the last scan. Those are two different facts, and the catalog's is the wrong one: a library edited since would keep pictures of bytes nobody has any more, and the first version of the end-to-end test caught exactly that by touching a file without rescanning. Whenever a derived artifact is kept beside a source, the question is "was this made from what is there now", and only the source can answer it.

**Freshness is asked of whatever the artifact is derived *from*.** Two commands write files beside the music and must do nothing on a second run, and they answer the question differently because they are derived from different things. A spectrogram is derived from one file's **bytes**, so `spectrum` compares modification dates. A playlist is derived from the **set** of tracks — adding one changes what the playlist should say without touching any file it already names — so `playlist` renders the text and compares it with what is on disk. Using a date there would miss the added track; using content for a PNG would mean drawing it to find out whether to draw it. Match the test to the derivation, not to the habit. Comparing the text has a second virtue: an unchanged playlist keeps its own modification date, which matters to whatever syncs the folder next.

**A comment and a note are not the same field.** The comment tag lives inside the audio file, put there by whoever tagged it; a note lives in `user.json`, put there by the person using Aède. `search --comments` and `search --notes` therefore stay two options with two sections, and must never be folded into one "free text" search: searching one is searching the library, searching the other is searching yourself, and a hit has to say by which route it was found.

**One predicate answering two questions is one predicate too few.** `is_flag` decided both "does `field:true` mean a yes or a no" and "is this field's bare name a question", and those are not the same question. The consequence was a hole nobody could see: there was **no way at all** to ask which things carried a note, a tag or a rating — a bare `note` fell through to a text search for the word, `note:true` searched for the word "true", and the fallback in `flag_of` written to answer exactly this was unreachable code. `asks_whether_it_holds_anything` is now the second predicate. When a helper's name has an "or" in its meaning, it is two helpers.

**A listing answers the grammar rather than growing filters.** `albums --query` folds a track selection into the releases holding them, because the grammar evaluates over tracks and the coarser question is a fold of the finer one. The expression is wrapped in brackets before being joined with the option terms: juxtaposition binds tighter than `OR`, so `--artist X --query "a OR b"` unbracketed reads as `(X AND a) OR b` — a listing quietly wider than what was asked for. The test for this needs an `OR` branch that **does** match the fixture, or both readings pass and it proves nothing; the first version of it did not, and did not.

**An option is unhonourable by its *value*, not only by its command.** The guard table in `main.rs` answers "does this option mean anything on this command", and that is not the whole question: `--compress wav --quality 128k` passed every guard, reached the encoder, and was dropped on the floor because PCM has no quality knob. The user had asked for small files and would have got the largest possible. So an option whose meaning depends on another option's *value* is checked where that value is known — here in `copy::quality`, which takes the target — and refused, not warned about: the check runs before a single file is read, so stopping costs nothing, whereas a warning scrolls past a plan and a progress line while the wrong run proceeds. Its mirror image counts too: an option that was honoured but had *nothing to do* (`--compress mp3` over a selection already entirely MP3) must say so, or it reads as an option that was ignored.

**Only a lossless source is ever encoded.** `copy::conversion_for` is the whole rule, and it settles three cases with one line: MP3 asked to become MP3 (re-encoding loses quality to produce the same thing), MP3 asked to become Opus (a second lossy pass over a first is audible), MP3 asked to become FLAC (larger, no better, and lossless in name only). Note the trap this created in testing: an end-to-end test using an MP3 source and an MP3 target passes even with the lossless rule deleted, because the "already in that format" branch catches it anyway. **The case that proves the rule is a lossy source with a *different* target**, and the test was vacuous until it had one. Second time this class of vacuous test has appeared here — check which branch your assertion actually exercises.

**Ask the destination, do not infer from its name.** Whether a volume accepts `?` and `:` in a file name is settled by writing one probe file into it, not by reading the filesystem type and consulting a table. The table is wrong exactly where it matters: a FUSE mount, an SMB share of a Windows folder and a card reader all report something it does not list, and the inference then has to guess. This generalises — where a cheap experiment answers a question about the environment, prefer it to a classification of the environment.

**A derived copy is not the library.** Writing tags into a file `copy` produced is not the tag-rewriting this project refuses: the refusal protects *the user's* files, whose mtime, integrity verdict and scan state all depend on not being touched. A copy has none of those. Keep the two apart in any future work — the moment `--compress` writes metadata into a transcoded file, it must be obvious that it is doing so to a derived artifact and never to a source.

**Not every dot ends a name.** `copy::names::split_extension` recognises an extension rather than assuming the last dot introduces one, because `Vol. 1: Live` has a dot that does not. Assuming it made the stem `Vol` and put the disambiguating counter inside the title: `Vol (2). 1_ Live`. A test caught it. The same helper serves the reserved-name check, the shortening and the uniqueness counter, so the three cannot disagree about where a name ends.

**Where a positional list meets an unquoted multi-word name, a separator decides — not a count.** `tag` takes `<kind> <name> <label[,label…]>`, and names are routinely typed unquoted (`aede tag album Kind of Blue jazz` predates the list). "Everything after the name" is therefore not a rule the parser can apply: nothing says where the name stopped. `split_tags` walks from the end and keeps taking words while a **comma** joins them, which adds the list reading without changing what any existing command line means. The residual ambiguity — an unquoted multi-word name with no label — is left as it was rather than papered over, and every confirmation names both the target and the labels so a misreading is visible. The general rule: when a new shape overlaps an old one, make the new shape opt-in by a token the old one never contained, and prove the old readings are untouched with tests that predate it.

**An alias is the command, in every table.** `COMMANDS` is the one place an alias is written down, and `canonical()` resolves it *once* in `main` before any guard runs. The eight guard tables (`CSV_COMMANDS`, `JSON_COMMANDS`, `M3U_COMMANDS`, `SORT_COMMANDS`, `PAGING_COMMANDS`, `OUTPUT_COMMANDS`, `takes_no_argument`, …) list canonical names only, and never learn that aliases exist. Before this, `find` and `favorites` dispatched correctly and were refused their options on the way there, which produced a program contradicting itself in one breath: `aede find … --csv` answered that "find" cannot produce a table and then listed `query` among those that can. The general shape is worth recognising — a fact known to one part of the program and hand-copied into eight others will be right in seven of them.

**A destructive command may not swallow an argument.** `aede import --forget <folder>` used to ignore the folder entirely and delete everything. Positionals now scope `--forget --pending`, and a folder given to a plain `--forget` is an error naming the form that does work. The general rule — an option or argument that cannot be honoured is refused, not dropped — is at its sharpest where the mistake is unrecoverable.

**Scanning never silently narrows the library.** The watched folders live in `Catalog::roots` and accumulate. Dropping one is always an explicit act, and a scan with no folder left is the way a catalog is emptied — refusing it would strand the files with no way out.

**`Various Artists` is not an artist**, it is the absence of an album artist. The same reasoning applies to any placeholder name: do not pollute the catalog with entities that are not entities.

## 4. Writing Rust here

### Language

Everything in this repository is in **English**: identifiers, comments, documentation, error messages, program output, command names, tests, commit messages. The project is published on GitHub and has to be readable by anyone.

Comments explain the **why**; the what is already in the code. A comment must be about the code, never about the conversation that produced it. Do not document tooling choices, alternatives that were rejected in chat, or anything a reader of the repository has no context for.

Good:

```rust
/// Two tracks count as duplicates when the same artist and title come back
/// with a close duration (less than 3 seconds apart).
///
/// The duration is what makes this safe: without it, a live rendition would
/// be flagged as a duplicate of the studio version.
```

No commented-out code left behind. No `TODO` without a sentence saying what is missing and why it is not done yet.

### Documentation

`//!` at the top of every module, saying what it is for and why it is built that way. `///` on every public item — `aede-core` sets `#![warn(missing_docs)]`, so an undocumented public item is a warning and `tools/check.sh` fails on it. Doc examples are compiled by `cargo test`, so they must stay correct.

A doc comment must add what the name does not give. `/// The artist's id.` on `artist_id: Id` is noise; say what it points at and why the field exists. Browse the result with `cargo doc --open`.

### Errors

One error type per module: an explicit enum implementing `Display` and `std::error::Error`, with `From` for the common conversions. Messages address the user in plain language and say **what to do**:

```rust
Err(format!(
    "no catalog in {}.\nRun this first: aede scan <folder>",
    dir.display()
))
```

`unwrap` and `expect` are forbidden in library code, tolerated in tests (with a message saying what was expected) and in `main` when the failure is fatal and already explained.

Never swallow an error silently. When work continues in spite of one — an unreadable folder must not abort a whole scan — report it back to the caller.

### API and style

Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/):

- naming: `snake_case` for functions, `CamelCase` for types, `SCREAMING_CASE` for constants; no `get_` prefix; `as_` (borrow, free), `to_` (expensive), `into_` (consumes);
- take `&str` over `&String`, `&[T]` over `&Vec<T>`;
- return iterators or `Vec` depending on what is easiest to use, not on what is most elegant to write;
- minimal visibility: `pub(crate)` by default, `pub` only for what genuinely belongs to the interface;
- derive `Debug`, `Clone`, `PartialEq` liberally on data types;
- borrow rather than clone, but do not contort the code to avoid cloning a small `String` on a cold path — readability first, targeted optimisation second.

`unsafe` is forbidden, with one documented exception already in the tree: restoring the default `SIGPIPE` handler in `aede-cli/src/main.rs`. Any other use must be justified in a `// Safe:` comment stating the invariant being upheld, and discussed first.

### Concurrency

`std::thread::scope` and the `std::sync` primitives are enough. No async runtime until networking arrives at M1, and then only after discussion. A poisoned `Mutex` must not bring down a scan: use `unwrap_or_else(|e| e.into_inner())`.

## 5. Tests

- unit tests live in the module, under `#[cfg(test)] mod tests`;
- integration tests live in `tests/`, and run against **real files** — the audio fixtures were produced with ffmpeg and cross-checked with ffprobe;
- a new format means a new fixture: reading it correctly is not something a unit test can claim;
- test names describe the behaviour: `truncated_vorbis_comments`, `various_artists_is_not_an_artist`;
- any non-obvious assertion carries a message with the observed value: `assert!(ok, "bitrate obtained: {bitrate} kbps")`;
- **every bug fix starts with a failing test.**

For a binary parser, always think through the three hostile cases: truncated input, a lying length field, a forged signature.

## 6. Command-line interface

Output is for humans first: aligned tables (`ui::Table`), colours that vanish outside a terminal or under `NO_COLOR`, correct plurals (`ui::plural`). Alignment is computed in **display columns**, not bytes — "Björk" takes five columns for six bytes.

An elapsed time goes through `ui::elapsed`, which switches to seconds then to minutes: `260604 ms` makes the reader do the division. A long-running command says what it is about to do before doing it, in volume rather than in a predicted time — how long a read takes depends on the disk, and a wrong estimate is worse than none — and saves its progress as it goes, so that interrupting it costs only the current batch.

A column holding a path is bounded with `Table::path_limit`, which drops the **start** of the text: the file name is what identifies the file, and on macOS a temporary path is sixty columns before the name even begins.

`--json` on the query commands, for machine use. A misspelled option is reported, never silently ignored, and an option that expects a value but was given none stops the command: answering as if it had never been typed would be a wrong answer, not an incomplete one. Any option taking a value belongs in `args::VALUED`, or its space-separated form lands in the positionals. The program does not panic when its output is cut off (`aede stats | head`).

Sizes are **decimal** (`text::format_size`, 1 kB = 1000 bytes), matching Finder and the other tools of this family; dividing by 1024 under a "MB" label understates an album by 5% and reads as a bug. Durations are **rounded** to the nearest second, never truncated — truncating loses a second on half the tracks of a library.

A row measures the set of tracks it counts, never a wider one: in the *Appears on* table the duration, the size and the formats describe the artist's tracks, not the release they sit on.

Count, duration and size on disk go together: a command that shows one of the three shows all three, and `commands::totals` is what computes the last two. In a table a duration is `text::format_duration` (`h:mm:ss`, right-aligned); in a sentence it is `ui::long_duration` (`1 d 22 h 41 min`).

A lookup by name matches exactly first, and only widens when nothing matched — `Catalog::find_releases` and `Catalog::find_tracks` share that rule and report which of the two happened. Returning the first of several partial matches is the fault this replaced: an arbitrary answer, given without saying so.

The shape of an answer does not depend on how many results it has: `aede track` prints the same page whether one track carries the title or four. Any list bounded by a limit says so on screen — a silent truncation reads as "that is all there is".

## 7. Git

Commit messages in English, imperative mood, subject line of 72 characters at most, then a blank line and a body explaining **why**:

```
Treat "Various Artists" as the absence of an album artist

Recording it as an artist made it show up in the rankings and inflated the
artist count of every compilation.
```

One commit, one idea. Automatic reformatting and lint fixes go in their own commit, never mixed with a behaviour change.

## 8. Things to watch for in the coming milestones

**M1 — artist identity.** Tags carry names, not identifiers, and the same person arrives spelled several ways: `Ozzy Osbourne` / `O. Osbourne`, `Glen Benton` / `Benton` as credited on the sleeve. Matching on a fragment of the name is out of the question — it would merge Angus and Neil Young. The answer is a local alias file, applied when the graph is built so a `scan` propagates it, plus `doctor` **suggesting** candidates (one name a suffix of another, sharing releases) without ever applying them. Where MusicBrainz gives an identifier, the identifier wins over the name.

**M1 — MusicBrainz.** A hard limit of one request per second and a mandatory identifying `User-Agent`, on pain of being blocked. Wikipedia is CC BY-SA: attribution is mandatory and carries over to translations. Go through Wikidata to reach the article in the user's language — it already exists in the vast majority of cases, written by humans, and no machine translation is then needed.

Matching a file to a release is the hard problem of this project. Always keep a confidence score and a way to review: never overwrite correct tags on the strength of an approximate match.

**M2 — API.** The HTTP contract freezes early and is versioned. Every future client will depend on it.

**M3 — the decoder.** It is what the FLAC MD5 check waits for: verifying that hash means decoding the audio. The frame walk in `audit/flac.rs` already reads every Rice residual and throws the numbers away — what is missing is the LPC restoration, the inter-channel decorrelation and MD5 itself. Adding the stronger verdict must not change the stored shape, only `integrity_method`.

The two checks are not competing methods and neither replaces the other: the frame CRCs are the cheap pass that reads the container, the MD5 is the deep pass that reads the audio. Both stay. "Aligning on FlacCompagnon" therefore does not mean copying its verdict but computing the same digest over the same bytes — the interleaved little-endian samples the FLAC specification defines, which is the only thing there is to agree on. Two conforming implementations cannot disagree, and if they do, one of them has a bug: that is exactly the check `doctor` already performs on imported reports, and it is why the disagreement is reported rather than resolved. The way to guarantee it is to stop having two implementations at all — extract `audit` into a crate both programs depend on.

**M3/M4 — playback.** The encoder delay and padding (already extracted from LAME tags and the Opus pre-skip) are what make gapless playback possible. Do not lose them along the way.
