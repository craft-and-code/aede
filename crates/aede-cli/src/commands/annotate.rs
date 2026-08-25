//! What the user writes: `love`, `rate`, `note`, `played`, and the listings
//! that give it back.
//!
//! Every one of these resolves a name the way the page commands already do, so
//! `aede love album "Legion"` finds the album exactly as `aede album "Legion"`
//! does. Naming a thing is one problem, solved once.

use std::error::Error;

use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::user::{self, Annotation, EntityRef, LOCAL_USER, Play, UserData};
use aede_core::{clock, text};

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

/// Rows shown before a listing starts filling the terminal.
const DEFAULT_LIMIT: usize = 50;

/// Loads what the user wrote, reattaching it to the catalog as it comes in.
///
/// The reconciliation happens on every read rather than on a command of its
/// own: a file that moved between two scans should reattach the next time
/// anybody looks, not the next time somebody remembers to ask.
fn read(args: &Args, catalog: &Catalog) -> Result<UserData, Box<dyn Error>> {
    let path = user::user_path(&data_dir(args));
    let mut data = user::load(&path)?.unwrap_or_default();
    user::reconcile(&mut data, catalog);
    Ok(data)
}

fn write(args: &Args, data: &mut UserData) -> Res {
    data.forget_empty();
    user::save(data, &user::user_path(&data_dir(args)))?;
    Ok(())
}

/// The owner of what is being written.
///
/// One value today. Every read filters on it and every write stamps it, so the
/// day there are accounts is the day this function returns something else —
/// not the day the store has to be migrated.
fn owner(_args: &Args) -> String {
    LOCAL_USER.to_string()
}

/// Reads `<kind> <name>` from the command line and finds what it names.
///
/// The kind is spelled out rather than guessed: "Legion" is an album here and a
/// genre somewhere else, and a program that picks for you is a program that
/// quietly rates the wrong thing.
fn target(words: &[String], catalog: &Catalog) -> Result<EntityRef, Box<dyn Error>> {
    // A reference typed back in whole, which is what the error below hands out
    // when a name matches several things. What is shown has to be accepted.
    if let [single] = words
        && let Some(reference) = parse_reference(single)
        && reference.resolve(catalog).is_some()
    {
        return Ok(reference);
    }
    let mut words = words.iter();
    let Some(kind_word) = words.next() else {
        return Err(usage().into());
    };
    let Some(kind) = parse_entity_kind(kind_word) else {
        return Err(format!("\"{kind_word}\" is not a kind of thing.\n{}", usage()).into());
    };
    let name = words.cloned().collect::<Vec<_>>().join(" ");
    if name.trim().is_empty() {
        return Err(format!("give a name: {}", usage()).into());
    }
    find(catalog, kind, &name)
}

/// A `<kind>:<key>` token, with `album` accepted for `release` because that is
/// the word every other command uses.
fn parse_reference(text: &str) -> Option<EntityRef> {
    let (kind, key) = text.split_once(':')?;
    let kind = parse_entity_kind(kind)?;
    Some(EntityRef::new(kind, key))
}

/// The vocabulary as the user types it, which says `album` where the model
/// says `release`.
fn parse_entity_kind(word: &str) -> Option<EntityKind> {
    match text::normalize(word).as_str() {
        "album" | "release" => Some(EntityKind::Release),
        other => EntityKind::parse_kind(other),
    }
}

fn usage() -> String {
    "Name what it is about: aede love album \"Legion\"\n\
     Kinds: track, album, artist, label, genre"
        .to_string()
}

/// The one entity of that kind carrying this name.
///
/// Several matches is an error rather than a choice made silently: rating one
/// of three albums called "Live" without saying which is a wrong answer nobody
/// can see.
fn find(catalog: &Catalog, kind: EntityKind, name: &str) -> Result<EntityRef, Box<dyn Error>> {
    let (ids, what) = match kind {
        EntityKind::Release => {
            let (found, _) = catalog.find_releases(name);
            (found.iter().map(|r| r.id).collect::<Vec<Id>>(), "album")
        }
        EntityKind::Artist => (
            catalog
                .find_artist(name)
                .map(|a| vec![a.id])
                .unwrap_or_default(),
            "artist",
        ),
        EntityKind::Track => {
            let (found, _) = catalog.find_tracks(name);
            (found.iter().map(|t| t.id).collect(), "track")
        }
        EntityKind::Label => {
            let (found, _) = catalog.find_labels(name);
            (found.iter().map(|l| l.id).collect(), "label")
        }
        EntityKind::Genre => {
            let (found, _) = catalog.find_genres(name);
            (found.iter().map(|g| g.id).collect(), "genre")
        }
    };
    match ids.len() {
        0 => Err(format!("no {what} matches \"{name}\"").into()),
        1 => EntityRef::of(catalog, kind, ids[0])
            .ok_or_else(|| format!("\"{name}\" cannot be named stably").into()),
        n => {
            // Listing the names again would repeat the very ambiguity being
            // reported — two albums called "Kind of Blue" print as "Kind of
            // Blue, Kind of Blue", which tells the reader nothing and offers
            // them nothing to type. Each line therefore carries what tells the
            // two apart *and* the reference that names exactly one.
            let mut lines = String::new();
            for id in ids.iter().take(8) {
                let Some(reference) = EntityRef::of(catalog, kind, *id) else {
                    continue;
                };
                lines.push_str(&format!(
                    "\n\t{}\n\t  {}",
                    describe(catalog, kind, *id),
                    reference.to_token()
                ));
            }
            Err(format!(
                "\"{name}\" matches {n} {what}s, and this writes on exactly one.\n\
                 Name the one you mean by its reference:{lines}"
            )
            .into())
        }
    }
}

/// What the user wrote about one thing, ready to be printed under its page.
///
/// A rating given and never shown again is a rating nobody trusts. Every page
/// that names an entity ends with this, and prints nothing at all when nothing
/// was written — a heading over an empty block says "you have nothing here",
/// which is a claim, and a tedious one to read on every page.
pub fn panel(args: &Args, catalog: &Catalog, reference: &EntityRef) {
    let Ok(data) = read(args, catalog) else {
        // Unreadable user data must not take a page down with it: the page is
        // about the music, and the music is still there.
        return;
    };
    let Some(entry) = data.find(&owner(args), reference) else {
        return;
    };

    // The marks first, in one line: they are labels on the thing.
    let mut marks: Vec<String> = Vec::new();
    if let Some(rating) = entry.rating {
        marks.push(stars_of(rating));
    }
    if entry.loved {
        marks.push("♥".to_string());
    }
    if !entry.tags.is_empty() {
        marks.push(entry.tags.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    let played = if reference.kind == EntityKind::Track {
        data.play_count(&owner(args), reference)
    } else {
        0
    };
    if !marks.is_empty() || played > 0 {
        println!("{}", ui::section("Yours"));
        if !marks.is_empty() {
            println!("  {}", marks.join("   "));
        }
        if played > 0 {
            println!(
                "  {}",
                ui::dim(&format!("played {}", ui::plural(played as usize, "time")))
            );
        }
    }

    // The note second, under a heading of its own. A rating is a label on a
    // thing; a note is a text the user wrote, sometimes at length, and burying
    // it in a row of stars and tags says it matters less than they do. One
    // note per entity, so the section is that note and nothing else.
    print_note(entry);
}

/// The written note, with a heading of its own and the date it was last
/// touched.
fn print_note(entry: &Annotation) {
    let Some(note) = &entry.note else {
        return;
    };
    println!("{}", ui::section("Notes"));
    // Printed exactly as it was written, blank lines and all. The text belongs
    // to whoever typed it: no wrapping, no trimming, no reflowing — a note is
    // not a field to be tidied.
    for line in note.lines() {
        println!("  {line}");
    }
    if entry.updated_at > 0 {
        println!(
            "\n  {}",
            ui::dim(&format!(
                "written {}",
                ui::ago(aede_core::clock::now_seconds().saturating_sub(entry.updated_at))
            ))
        );
    }
}

/// The panel for whatever entity a page is about, by kind and identifier.
pub fn panel_for(args: &Args, catalog: &Catalog, kind: EntityKind, id: Id) {
    if let Some(reference) = EntityRef::of(catalog, kind, id) {
        panel(args, catalog, &reference);
    }
}

/// `aede love <kind> <name>` — and `--remove` to take it back.
pub fn love(args: &Args) -> Res {
    let catalog = load(args)?;
    let reference = target(&args.positionals, &catalog)?;
    let mut data = read(args, &catalog)?;
    let now = clock::now_seconds();
    let wanted = !args.has("remove");
    let entry = data.entry(&owner(args), &reference, now);
    let changed = entry.loved != wanted;
    entry.loved = wanted;
    entry.updated_at = now;
    write(args, &mut data)?;

    let name = reference.display_name(&catalog);
    println!(
        "{} {name} {}",
        ui::green(if wanted { "♥" } else { "→" }),
        match (wanted, changed) {
            (true, true) => "is a favourite",
            (true, false) => "was already a favourite",
            (false, true) => "is no longer a favourite",
            (false, false) => "was not a favourite",
        }
    );
    Ok(())
}

/// `aede rate <kind> <name> --stars N`, or `--remove`.
pub fn rate(args: &Args) -> Res {
    let catalog = load(args)?;
    let reference = target(&args.positionals, &catalog)?;
    let mut data = read(args, &catalog)?;
    let now = clock::now_seconds();

    let stars = match args.whole_number("stars")? {
        Some(n) if (1..=5).contains(&n) => Some(n as u8),
        Some(n) => {
            return Err(format!("--stars takes 1 to 5: got {n}").into());
        }
        // Removing is explicit. Defaulting to "no rating" when the number is
        // missing would erase one on a typo.
        None if args.has("remove") => None,
        None => return Err("say how many: --stars 4, or --remove to take it back".into()),
    };

    let entry = data.entry(&owner(args), &reference, now);
    entry.rating = stars;
    entry.updated_at = now;
    write(args, &mut data)?;

    let name = reference.display_name(&catalog);
    match stars {
        Some(n) => println!("{} {name}: {}", ui::green("→"), stars_of(n)),
        None => println!("{} {name} is no longer rated", ui::green("→")),
    }
    Ok(())
}

/// `aede note <kind> <name> --text "…"`, `--remove`, or `--from <token>`.
pub fn note(args: &Args) -> Res {
    let catalog = load(args)?;
    let reference = target(&args.positionals, &catalog)?;
    let mut data = read(args, &catalog)?;
    let now = clock::now_seconds();
    let name = reference.display_name(&catalog);

    // Copying everything said about one thing onto another, so that a note
    // written once is not retyped for every disc of a box set.
    if let Some(token) = args.value("from") {
        let source = match EntityRef::parse_token(token) {
            Some(reference) => reference,
            None => {
                let (kind, rest) = token
                    .split_once(':')
                    .ok_or("--from takes <kind>:<name>, as in --from album:\"Legion\"")?;
                let kind = parse_entity_kind(kind)
                    .ok_or_else(|| format!("\"{kind}\" is not a kind of thing"))?;
                find(&catalog, kind, rest)?
            }
        };
        if !data.copy(&owner(args), &source, &reference, now) {
            return Err(format!(
                "nothing has been written about {}, so there is nothing to copy",
                source.display_name(&catalog)
            )
            .into());
        }
        write(args, &mut data)?;
        println!(
            "{} copied onto {name} what was said about {}",
            ui::green("→"),
            source.display_name(&catalog)
        );
        return Ok(());
    }

    // Where the text comes from. A note is a written thing, and a written
    // thing does not fit on a command line: `--file` reads one from disk, and
    // `--file -` from whatever is piped in, which is how a note gets written in
    // a real editor rather than between two quotation marks.
    let written: Option<String> = match (args.value("text"), args.value("file")) {
        (Some(_), Some(_)) => {
            return Err("--text and --file both say what to write; give one".into());
        }
        (Some(text), None) => Some(text.to_string()),
        (None, Some("-")) => {
            use std::io::Read;
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            Some(buffer.trim_end_matches('\n').to_string())
        }
        (None, Some(path)) => Some(
            std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read {path}: {e}"))?
                .trim_end_matches('\n')
                .to_string(),
        ),
        (None, None) => None,
    };

    let entry = data.entry(&owner(args), &reference, now);
    match (written, args.has("remove")) {
        (Some(text), _) => {
            // Appending keeps a note growing without retyping it, with a blank
            // line between what was there and what is added: two thoughts a
            // month apart are not one paragraph.
            entry.note = match (args.has("append"), entry.note.take()) {
                (true, Some(existing)) if !existing.trim().is_empty() => {
                    Some(format!("{existing}\n\n{text}"))
                }
                _ => Some(text),
            };
            entry.updated_at = now;
            write(args, &mut data)?;
            println!(
                "{} {} on {name}",
                ui::green("→"),
                if args.has("append") {
                    "added to the note"
                } else {
                    "noted"
                }
            );
        }
        (None, true) => {
            let had = entry.note.take().is_some();
            entry.updated_at = now;
            write(args, &mut data)?;
            println!(
                "{} {name} {}",
                ui::green("→"),
                if had {
                    "no longer carries a note"
                } else {
                    "carried no note"
                }
            );
        }
        // Naming a thing and saying nothing about it is a question, so it is
        // answered rather than refused.
        (None, false) => {
            if entry.note.is_some() {
                println!("{}", ui::section(&name));
                print_note(entry);
            } else {
                println!("{} nothing has been noted on {name}", ui::dim("—"));
            }
        }
    }
    Ok(())
}

/// Splits the positionals of `tag` into what is being named and what is being
/// labelled with.
///
/// The whole difficulty is that a name may be several words and is not
/// required to be quoted — `aede tag album Kind of Blue jazz` has always
/// worked, taking the last word as the label. Letting the label become a
/// *list* therefore cannot simply mean "every word after the name": nothing
/// would say where the name stopped.
///
/// **The comma is what says so.** A comma binds the words around it into one
/// list, so the walk goes from the end and keeps taking words while a comma
/// joins them to what follows:
///
/// | typed                                    | name          | tags               |
/// |------------------------------------------|---------------|--------------------|
/// | `album Kind of Blue jazz`                | Kind of Blue  | jazz               |
/// | `album Scream music,ep,record`            | Scream        | music, ep, record  |
/// | `album Scream music, ep, record`          | Scream        | music, ep, record  |
/// | `album Scream --remove`                   | Scream        | *(none: all)*      |
///
/// The first row is the old shape, unchanged — which is the point: the comma
/// only ever *adds* a reading, so nothing anyone already types changes meaning.
///
/// The one thing it cannot resolve is an unquoted multi-word name with no tag
/// at all: `album Kind of Blue --remove` reads "Blue" as the tag, exactly as it
/// did before. Quoting the name settles it, and the confirmation names both the
/// thing and the tags, so a misreading is visible rather than silent.
fn split_tags(rest: &[String]) -> (Vec<String>, Vec<String>) {
    if rest.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut first = rest.len() - 1;
    while first > 0 && (rest[first - 1].ends_with(',') || rest[first].starts_with(',')) {
        first -= 1;
    }
    // Every word was swallowed by the comma walk, so nothing is left to name
    // the target. The caller reads that as "the tail was the name after all",
    // which is what makes `aede tag album Scream --remove` mean the album.
    (rest[..first].to_vec(), labels_of(&rest[first..]))
}

/// The labels a run of words holds, however the commas were spaced.
///
/// Empty pieces are dropped rather than stored: a trailing comma is a slip, and
/// a tag whose name is nothing would be invisible on every screen that shows it
/// while still matching `tag:` in a query.
fn labels_of(words: &[String]) -> Vec<String> {
    words
        .join(" ")
        .split(',')
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect()
}

/// `aede tag <kind> <name> <label[,label…]>`, `--remove` to take them off.
///
/// `--remove` with no label at all takes off **every** tag: having listed them
/// one by one to put them on, being made to list them one by one to take them
/// off again is the kind of asymmetry that makes a command tiring.
pub fn tag(args: &Args) -> Res {
    let catalog = load(args)?;
    let words = args.positionals.clone();
    if words.is_empty() {
        return Err("give a tag: aede tag album \"Legion\" vinyl,rare".into());
    }
    let removing = args.has("remove");
    // A reference pasted back in names the target on its own, so everything
    // after it is labels and none of the guessing below applies. Handled first
    // because it is the form the disambiguation error hands out: what is shown
    // has to be accepted, here as everywhere.
    let (named, labels) = if parse_reference(&words[0]).is_some() {
        (vec![words[0].clone()], labels_of(&words[1..]))
    } else {
        // Otherwise the head names the target — `<kind> <name>` — and the tail
        // is the labels. Where one stops and the other starts is
        // [`split_tags`]'s whole subject.
        let (head, labels) = split_tags(&words[1..]);
        match head.is_empty() {
            // Nothing stands before the labels, and a target is mandatory: the
            // tail can only have been the name. `aede tag artist "Miles Davis"
            // --remove` reaches here, and means every tag on Miles Davis.
            true => (words.clone(), Vec::new()),
            false => {
                let mut named = vec![words[0].clone()];
                named.extend(head);
                (named, labels)
            }
        }
    };
    if labels.is_empty() && !removing {
        return Err("give a tag: aede tag album \"Legion\" vinyl,rare".into());
    }
    if named.len() < 2 && parse_reference(&named[0]).is_none() {
        return Err(usage().into());
    }
    let reference = target(&named, &catalog)?;

    let mut data = read(args, &catalog)?;
    let now = clock::now_seconds();
    let entry = data.entry(&owner(args), &reference, now);
    entry.updated_at = now;
    let name = reference.display_name(&catalog);

    if removing {
        // Nothing named means all of them. Collected first: a set cannot be
        // iterated while it is being emptied, and the message has to name what
        // went — "every tag removed" is unverifiable, and the whole point of
        // this branch is that the user did not type the list.
        let taken: Vec<String> = match labels.is_empty() {
            true => std::mem::take(&mut entry.tags).into_iter().collect(),
            false => labels
                .iter()
                .filter(|label| entry.tags.remove(*label))
                .cloned()
                .collect(),
        };
        let missed: Vec<&String> = labels
            .iter()
            .filter(|label| !taken.contains(label))
            .collect();
        write(args, &mut data)?;
        if taken.is_empty() {
            println!(
                "{} {name} carried {}",
                ui::dim("—"),
                match labels.is_empty() {
                    true => "no tag at all".to_string(),
                    false => format!("none of {}", quoted(&labels)),
                }
            );
        } else {
            println!(
                "{} {name} no longer carries {}",
                ui::green("→"),
                quoted(&taken)
            );
        }
        // Said apart, and only when it happened: a label that was not there is
        // very often a typo for one that is, and folding it into the line above
        // would report a removal that did not take place.
        if !missed.is_empty() && !taken.is_empty() {
            println!(
                "  {}",
                ui::dim(&format!(
                    "it did not carry {}",
                    quoted(&missed.into_iter().cloned().collect::<Vec<_>>())
                ))
            );
        }
    } else {
        let added: Vec<String> = labels
            .iter()
            .filter(|label| entry.tags.insert((*label).clone()))
            .cloned()
            .collect();
        write(args, &mut data)?;
        if added.is_empty() {
            println!(
                "{} {name} already carried {}",
                ui::dim("—"),
                quoted(&labels)
            );
        } else {
            println!("{} {name} is tagged {}", ui::green("→"), quoted(&added));
            let already: Vec<String> = labels
                .iter()
                .filter(|label| !added.contains(label))
                .cloned()
                .collect();
            if !already.is_empty() {
                println!(
                    "  {}",
                    ui::dim(&format!("it already carried {}", quoted(&already)))
                );
            }
        }
    }
    Ok(())
}

/// Labels as a readable list: `"vinyl"`, `"vinyl" and "rare"`, `"a", "b" and "c"`.
///
/// Quoted because a tag may hold a space, and a list of bare words would then
/// be unreadable in exactly the case where reading it matters.
fn quoted(labels: &[String]) -> String {
    let quoted: Vec<String> = labels.iter().map(|label| format!("\"{label}\"")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Says where what the user wrote actually is, when the question found it
/// nowhere.
///
/// A bare `loved` asks about the **track**, by design: five stars on an artist
/// is not five stars on a track, and a field that folded the scopes together
/// could never say which was meant. The cost of that design is one badly
/// misleading answer — somebody who marked an *album* a favourite types
/// `loved`, is told nothing matches, and reasonably concludes the feature is
/// broken.
///
/// So the empty answer asks the same question again of the album and of the
/// artist, and names the scope that holds something. It changes nothing about
/// what the query means; it only stops an empty result from looking like an
/// empty library.
fn elsewhere(parsed: &aede_core::query::Query, context: &aede_core::query::Context, typed: &str) {
    use aede_core::query::{Scope, asks_about_the_track_itself, rescoped, run};
    if !asks_about_the_track_itself(parsed) {
        return;
    }
    for (scope, prefix) in [(Scope::Album, "album."), (Scope::Artist, "artist.")] {
        let found = run(&rescoped(parsed, scope), context);
        if found.is_empty() {
            continue;
        }
        println!(
            "  {}",
            ui::yellow(&format!(
                "{} if you ask it of the {} — that is where you wrote it",
                ui::plural(found.len(), "track"),
                prefix.trim_end_matches('.')
            ))
        );
        println!(
            "  {}",
            ui::dim(&format!("aede query \"{}\"", scoped_text(typed, prefix)))
        );
        return;
    }
}

/// The expression as it would be typed at another scope.
///
/// Textual, deliberately: what is offered has to be typeable back in, and the
/// user typed words rather than a syntax tree. Only the four fields that carry
/// a scope are touched, and only where they stand alone — a `note:` already
/// written `album.note:` is left as it is.
fn scoped_text(typed: &str, prefix: &str) -> String {
    typed
        .split(' ')
        .map(|word| {
            let (negation, rest) = match word.strip_prefix('-') {
                Some(rest) => ("-", rest),
                None => ("", word),
            };
            let name = rest.split_once(':').map(|(n, _)| n).unwrap_or(rest);
            match matches!(name, "rating" | "loved" | "tag" | "note") {
                true => format!("{negation}{prefix}{rest}"),
                false => word.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `aede played <title>` — records a listen from outside.
///
/// Aède plays nothing yet, and will not until M3. Until then the history is
/// filled by whatever does: `aede artist Ozzy --m3u | mpv`, and this command
/// afterwards. The shape is the one M3 will write into, so the history built
/// this way is not thrown away when playback arrives.
pub fn played(args: &Args) -> Res {
    let catalog = load(args)?;
    let name = args.positionals.join(" ");
    if name.trim().is_empty() {
        return Err("which track? aede played \"So What\"".into());
    }
    // A reference pasted back in names one track exactly; a title may name
    // several, and then the error says which.
    let reference = match parse_reference(&name) {
        Some(reference) if reference.resolve(&catalog).is_some() => reference,
        _ => find(&catalog, EntityKind::Track, &name)?,
    };
    let played_ms = reference
        .resolve(&catalog)
        .and_then(|id| catalog.track(id))
        .and_then(|t| t.duration_ms)
        .unwrap_or(0);

    let mut data = read(args, &catalog)?;
    data.record_play(Play {
        owner: owner(args),
        track: reference.clone(),
        at: clock::now_seconds(),
        ms_played: played_ms,
        completed: true,
    });
    write(args, &mut data)?;
    println!(
        "{} {} — played {}",
        ui::green("→"),
        reference.display_name(&catalog),
        ui::plural(data.play_count(&owner(args), &reference) as usize, "time")
    );
    Ok(())
}

/// `aede query <expression>` — the tracks an expression matches.
///
/// What it produces is a **selection**, so `--csv`, `--json` and `--m3u` apply
/// to it exactly as they do to an album or an artist, and M3's queue will
/// consume it unchanged. That is the whole point of defining the grammar over
/// tracks: a saved query is a smart collection, and a smart collection is
/// already playable.
pub fn query(args: &Args) -> Res {
    let expression = args.positionals.join(" ");
    run_query(args, &expression, &expression)
}

/// Runs one expression and shows what it matched.
///
/// `shown` is what the heading calls it: an expression when it was typed, a
/// name when it came from a saved collection — because a collection that
/// answered under the text of its query would make the name it was saved under
/// look like it had been ignored.
fn run_query(args: &Args, expression: &str, shown: &str) -> Res {
    let catalog = load(args)?;
    let parsed = aede_core::query::parse(expression)?;
    let data = read(args, &catalog)?;
    let context = aede_core::query::Context {
        catalog: &catalog,
        data: &data,
        owner: &owner(args),
    };
    // A value naming nothing in the library is a misunderstanding, not an
    // empty result, and the two read differently.
    if let Some((what, value)) = aede_core::query::unknown_values(&parsed, &context).first() {
        return Err(
            format!("no {what} matches \"{value}\".\nRun \"aede {what}s\" for the list.").into(),
        );
    }
    let mut tracks = aede_core::query::run(&parsed, &context);
    if let Some(order) = args.value("sort") {
        aede_core::query::sort(&mut tracks, aede_core::query::Sort::parse(order)?, &context);
    }

    if let Some(result) = super::selection_output(&catalog, &tracks, args) {
        return result;
    }
    if tracks.is_empty() {
        println!("{}", ui::dim(&format!("nothing matches {expression:?}")));
        elsewhere(&parsed, &context, expression);
        return Ok(());
    }

    let window = args.window(DEFAULT_LIMIT)?;
    println!("{}", ui::section(&format!("{shown} ({})", tracks.len())));
    let mut t = Table::new(&["Track", "Artist", "Album", "Year", "Length"])
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(0, 34)
        .limit(1, 24)
        .limit(2, 28);
    let total = tracks.len();
    for id in tracks
        .iter()
        .copied()
        .skip(window.offset)
        .take(window.limit)
    {
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let release = track.release_id.and_then(|r| catalog.release(r));
        let artist = catalog
            .credits_on(EntityKind::Track, id)
            .into_iter()
            .find(|(_, role)| *role == "main")
            .map(|(a, _)| a.name.clone())
            .unwrap_or_default();
        t.push(vec![
            track.title.clone(),
            artist,
            release.map(|r| r.title.clone()).unwrap_or_default(),
            release
                .and_then(|r| r.year)
                .map(|y| y.to_string())
                .unwrap_or_else(|| "—".into()),
            track
                .duration_ms
                .map(text::format_duration)
                .unwrap_or_default(),
        ]);
    }
    print!("{}", t.render());
    super::announce_window(window, total, "track");

    let (duration, size) = super::totals(&catalog, &tracks);
    println!(
        "  {}",
        ui::dim(&format!(
            "{} · {} · {}",
            ui::plural(tracks.len(), "track"),
            text::format_duration(duration),
            text::format_size(size)
        ))
    );
    Ok(())
}

/// `aede collection <name>` — save a query, run it, or drop it.
///
/// It keeps the **expression**, not the result, which is the whole difference
/// between a smart collection and a playlist: it answers with what the library
/// holds now. And since running it produces a selection, `--m3u` turns one into
/// a playlist without a line of code written for the purpose.
pub fn collection(args: &Args) -> Res {
    let name = args.positionals.join(" ");
    if name.trim().is_empty() {
        return Err("which collection? aede collection metal --query \"genre:metal\"".into());
    }
    let catalog = load(args)?;
    let mut data = read(args, &catalog)?;
    let owner = owner(args);
    let now = clock::now_seconds();

    if let Some(expression) = args.value("query") {
        // Refused before it is saved rather than the next time it is run: a
        // collection that only fails when somebody opens it is a trap left for
        // later.
        aede_core::query::parse(expression)?;
        let replaced = data.save_collection(&owner, &name, expression, now);
        write(args, &mut data)?;
        println!(
            "{} {name} {}: {expression}",
            ui::green("→"),
            if replaced { "now reads" } else { "saved" }
        );
        return Ok(());
    }

    if args.has("remove") {
        if !data.forget_collection(&owner, &name) {
            return Err(format!("no collection is called \"{name}\"").into());
        }
        write(args, &mut data)?;
        println!("{} {name} is no longer saved", ui::green("→"));
        return Ok(());
    }

    let Some(saved) = data.collection(&owner, &name) else {
        let known: Vec<&str> = data
            .collections
            .iter()
            .filter(|c| c.owner == owner)
            .map(|c| c.name.as_str())
            .collect();
        return Err(if known.is_empty() {
            format!(
                "no collection is called \"{name}\", and none is saved yet.\n\
                 To save one: aede collection {name} --query \"genre:metal loved\""
            )
        } else {
            format!(
                "no collection is called \"{name}\".\nSaved: {}",
                known.join(", ")
            )
        }
        .into());
    };
    let (title, expression) = (saved.name.clone(), saved.expression.clone());
    run_query(args, &expression, &title)
}

/// `aede collections` — the saved queries, and what each one asks.
pub fn collections(args: &Args) -> Res {
    let catalog = load(args)?;
    let data = read(args, &catalog)?;
    let owner = owner(args);
    let mine: Vec<_> = data
        .collections
        .iter()
        .filter(|c| c.owner == owner)
        .collect();
    if mine.is_empty() {
        println!(
            "{}",
            ui::dim("nothing is saved yet: aede collection metal --query \"genre:metal\"")
        );
        return Ok(());
    }

    // How many tracks each one holds *now*, which is the only number worth
    // showing for a question that answers itself afresh every time.
    let context = aede_core::query::Context {
        catalog: &catalog,
        data: &data,
        owner: &owner,
    };
    println!("{}", ui::section(&format!("Collections ({})", mine.len())));
    let mut t = Table::new(&["Name", "Tracks", "Query"])
        .align(1, Align::Right)
        .limit(2, 52);
    for saved in mine {
        let count = match aede_core::query::parse(&saved.expression) {
            Ok(parsed) => aede_core::query::run(&parsed, &context).len().to_string(),
            // A grammar that loses a field must not make the whole listing
            // fail: the collection is shown, and said to be unreadable.
            Err(_) => "?".to_string(),
        };
        t.push(vec![saved.name.clone(), count, saved.expression.clone()]);
    }
    print!("{}", t.render());
    Ok(())
}

/// `aede favourites` — everything loved, whatever its kind.
pub fn favourites(args: &Args) -> Res {
    let catalog = load(args)?;
    let data = read(args, &catalog)?;
    let owner = owner(args);
    let rows: Vec<&Annotation> = data
        .annotations
        .iter()
        .filter(|a| a.owner == owner && a.loved)
        .collect();
    if rows.is_empty() {
        println!(
            "{}",
            ui::dim("nothing is a favourite yet: aede love album \"<title>\"")
        );
        return Ok(());
    }
    print_annotations(&catalog, &rows, args, "Favourites")
}

/// `aede notes` — everything written, whatever kind it was written on.
pub fn notes(args: &Args) -> Res {
    let catalog = load(args)?;
    let mut data = read(args, &catalog)?;

    // The only irreplaceable data in the program deserves a way out and a way
    // back in. Out is the file itself, so a backup is readable and repairable
    // by hand; in is a **merge**, because someone restoring half a backup wants
    // their two halves, and an import that replaced would be the one operation
    // here able to lose everything at once.
    if args.has("export") {
        return super::export::emit(args, &user::to_json(&data).to_string_pretty());
    }
    if let Some(path) = args.value("import") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
        let incoming = user::from_json(&aede_core::json::parse(&text)?)?;
        let report = user::merge(&mut data, incoming);
        write(args, &mut data)?;
        println!(
            "{} {} added, {} updated, {} kept as they were",
            ui::green("→"),
            report.added,
            report.updated,
            report.kept
        );
        // Only what actually came in: a line reading "0 listens" answers a
        // question nobody asked, on a screen where every other number matters.
        let mut also: Vec<String> = Vec::new();
        if report.plays > 0 {
            also.push(ui::plural(report.plays, "listen"));
        }
        if report.collections > 0 {
            also.push(ui::plural(report.collections, "collection"));
        }
        if !also.is_empty() {
            println!("  {}", ui::dim(&also.join(", ")));
        }
        return Ok(());
    }

    let owner = owner(args);
    let rows: Vec<&Annotation> = data
        .annotations
        .iter()
        .filter(|a| a.owner == owner)
        .filter(|a| match args.value("tag") {
            Some(wanted) => a
                .tags
                .iter()
                .any(|t| text::normalize(t) == text::normalize(wanted)),
            None => true,
        })
        .collect();
    if rows.is_empty() {
        println!("{}", ui::dim("nothing has been written yet"));
        return Ok(());
    }
    print_annotations(&catalog, &rows, args, "Notes")
}

/// `aede history` — what was played, most recent first.
pub fn history(args: &Args) -> Res {
    let catalog = load(args)?;
    let data = read(args, &catalog)?;
    let window = args.window(DEFAULT_LIMIT)?;
    let owner = owner(args);

    let mut plays: Vec<&Play> = data.plays.iter().filter(|p| p.owner == owner).collect();
    plays.reverse();
    if plays.is_empty() {
        println!(
            "{}",
            ui::dim("nothing has been played yet: aede played \"<title>\"")
        );
        return Ok(());
    }

    println!("{}", ui::section("History"));
    let mut t = Table::new(&["When", "Track", "Artist", "Played"]).align(3, Align::Right);
    let total = plays.len();
    for play in plays.into_iter().skip(window.offset).take(window.limit) {
        let resolved = play.track.resolve(&catalog);
        let artist = resolved
            .and_then(|id| catalog.track(id))
            .and_then(|track| {
                catalog
                    .credits_on(EntityKind::Track, track.id)
                    .into_iter()
                    .find(|(_, role)| *role == "main")
                    .map(|(a, _)| a.name.clone())
            })
            .unwrap_or_default();
        t.push(vec![
            ui::ago(clock::now_seconds().saturating_sub(play.at)),
            play.track.display_name(&catalog),
            artist,
            if play.completed {
                text::format_duration(play.ms_played)
            } else {
                format!("{} (skipped)", text::format_duration(play.ms_played))
            },
        ]);
    }
    print!("{}", t.render());
    super::announce_window(window, total, "play");

    // The counters, which the bounded log cannot answer for.
    let mut counts: Vec<_> = data.counts.iter().filter(|c| c.owner == owner).collect();
    counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.track.cmp(&b.track)));
    if let Some(top) = counts.first() {
        println!(
            "  {}",
            ui::dim(&format!(
                "most played: {} ({})",
                top.track.display_name(&catalog),
                ui::plural(top.count as usize, "time")
            ))
        );
    }
    Ok(())
}

/// The shared table behind `favourites` and `notes`.
fn print_annotations(catalog: &Catalog, rows: &[&Annotation], args: &Args, heading: &str) -> Res {
    let window = args.window(DEFAULT_LIMIT)?;
    let mut sorted: Vec<&&Annotation> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.target.cmp(&b.target))
    });

    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = sorted
            .iter()
            .skip(window.offset)
            .take(window.limit)
            .map(|a| {
                vec![
                    a.target.kind.as_str().to_string(),
                    a.target.display_name(catalog),
                    a.target.to_token(),
                    if a.loved { "true" } else { "false" }.into(),
                    a.rating.map(|r| r.to_string()).unwrap_or_default(),
                    a.note.clone().unwrap_or_default(),
                    a.tags.iter().cloned().collect::<Vec<_>>().join(" / "),
                    a.updated_at.to_string(),
                ]
            })
            .collect();
        return super::export::rows_table(
            &[
                "kind",
                "name",
                "reference",
                "loved",
                "rating",
                "note",
                "tags",
                "updated_at",
            ],
            &table,
            args,
        );
    }

    println!("{}", ui::section(&format!("{heading} ({})", sorted.len())));
    let mut t = Table::new(&["Kind", "Name", "", "Rating", "Tags", "Note"])
        .limit(1, 34)
        .limit(4, 20)
        .limit(5, 40);
    let total = sorted.len();
    for a in sorted.into_iter().skip(window.offset).take(window.limit) {
        t.push(vec![
            a.target.kind.as_str().to_string(),
            a.target.display_name(catalog),
            if a.loved { "♥".into() } else { String::new() },
            a.rating.map(stars_of).unwrap_or_default(),
            a.tags.iter().cloned().collect::<Vec<_>>().join(", "),
            a.note.clone().unwrap_or_default().replace('\n', " "),
        ]);
    }
    print!("{}", t.render());
    super::announce_window(window, total, "entry");

    // A record whose target is not in the catalog is not a mistake: the drive
    // may be unplugged. Saying so is what stops it reading as data loss.
    let waiting = rows
        .iter()
        .filter(|a| a.target.resolve(catalog).is_none())
        .count();
    if waiting > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} of these point at something not in the catalog right now; \
                 they are kept, not lost",
                waiting
            ))
        );
    }
    Ok(())
}

/// What tells one match from another, which the bare name by definition does
/// not.
fn describe(catalog: &Catalog, kind: EntityKind, id: Id) -> String {
    match kind {
        EntityKind::Release => catalog
            .release(id)
            .map(|r| {
                let artist = r
                    .album_artist_id
                    .and_then(|a| catalog.artist(a))
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Various Artists".into());
                match r.year {
                    Some(year) => format!("{} — {artist} ({year})", r.title),
                    None => format!("{} — {artist}", r.title),
                }
            })
            .unwrap_or_default(),
        EntityKind::Track => catalog
            .track(id)
            .map(|t| {
                let album = t
                    .release_id
                    .and_then(|r| catalog.release(r))
                    .map(|r| r.title.clone())
                    .unwrap_or_default();
                let file = catalog
                    .file(t.file_id)
                    .map(|f| text::file_name(&f.path).to_string())
                    .unwrap_or_default();
                format!("{} — {album} [{file}]", t.title)
            })
            .unwrap_or_default(),
        _ => EntityRef::of(catalog, kind, id)
            .map(|r| r.display_name(catalog))
            .unwrap_or_default(),
    }
}

/// `★★★☆☆` for a rating, which reads faster than a number in a column.
fn stars_of(n: u8) -> String {
    let n = n.min(5) as usize;
    format!("{}{}", "★".repeat(n), "☆".repeat(5 - n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(words: &[&str]) -> (String, Vec<String>) {
        let rest: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
        let (name, labels) = split_tags(&rest);
        (name.join(" "), labels)
    }

    #[test]
    fn a_comma_is_what_says_where_the_name_stops() {
        // The three shapes a user may reasonably type, and they must all mean
        // the same thing. Tested here rather than through the binary because
        // the fixture library cannot tell them apart: every one of them ends up
        // tagging *something*, and only the split says what was named and what
        // was labelled.
        let expected = vec!["music".to_string(), "ep".to_string(), "record".to_string()];
        assert_eq!(
            split(&["Scream", "music,ep,record"]),
            ("Scream".into(), expected.clone()),
            "commas inside one word"
        );
        assert_eq!(
            split(&["Scream", "music,", "ep,", "record"]),
            ("Scream".into(), expected.clone()),
            "a space after each comma"
        );
        assert_eq!(
            split(&["Scream", "music", ",", "ep", ",", "record"]),
            ("Scream".into(), expected),
            "a comma standing on its own"
        );
    }

    #[test]
    fn the_shape_that_already_worked_still_means_what_it_did() {
        // `aede tag album Kind of Blue jazz` predates the list entirely. A new
        // reading that changed an old one would be a regression dressed as a
        // feature.
        assert_eq!(
            split(&["Kind", "of", "Blue", "jazz"]),
            ("Kind of Blue".into(), vec!["jazz".to_string()])
        );
        assert_eq!(
            split(&["Legion", "vinyl"]),
            ("Legion".into(), vec!["vinyl".to_string()])
        );
    }

    #[test]
    fn a_name_alone_leaves_nothing_before_the_labels() {
        // Nothing stands before "Scream", and a target is mandatory — so the
        // caller reads the tail as the name rather than as a label with nothing
        // to put it on. That is what makes `--remove` with only a name mean
        // "every tag", and it is decided in the caller, not here: this function
        // reports the split it found and does not guess at intent.
        assert_eq!(split(&["Scream"]), ("".into(), vec!["Scream".to_string()]));
        assert_eq!(split(&[]), ("".into(), Vec::new()));
    }

    #[test]
    fn a_trailing_comma_does_not_invent_an_empty_tag() {
        // A list typed with a trailing separator is a slip, not a request for a
        // tag whose name is nothing — and an empty tag would be invisible on
        // every screen that shows it while still matching `tag:` in a query.
        assert_eq!(
            split(&["Scream", "music,", "ep,"]),
            ("Scream".into(), vec!["music".to_string(), "ep".to_string()])
        );
        assert_eq!(
            split(&["Scream", "music,,ep"]),
            ("Scream".into(), vec!["music".to_string(), "ep".to_string()])
        );
    }

    #[test]
    fn a_label_may_hold_a_space() {
        // "to rip again" is the example in the model's own doc comment, so it
        // had better survive a list.
        assert_eq!(
            split(&["Scream", "to rip again,", "vinyl"]),
            (
                "Scream".into(),
                vec!["to rip again".to_string(), "vinyl".to_string()]
            )
        );
    }

    #[test]
    fn a_list_is_read_out_the_way_it_is_said() {
        assert_eq!(quoted(&["a".to_string()]), "\"a\"");
        assert_eq!(
            quoted(&["a".to_string(), "b".to_string()]),
            "\"a\" and \"b\""
        );
        assert_eq!(
            quoted(&["a".to_string(), "b".to_string(), "c".to_string()]),
            "\"a\", \"b\" and \"c\""
        );
    }
}
