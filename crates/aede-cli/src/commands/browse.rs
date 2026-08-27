//! Flat listings: artists, albums, genres, labels, years.

use aede_core::model::Id;
use aede_core::stats;
use aede_core::text;

use super::{
    Res, announce_window, copy_marker, export, load, role_key, role_label, roles_offered, totals,
};
use crate::args::Args;
use crate::ui::{self, Align, Table};

/// What a listing can be put in order by.
///
/// **One vocabulary across every listing**, rather than one per command:
/// `--sort tracks` means the same thing on artists, genres, labels and years,
/// and somebody who learnt it on one does not relearn it on the next. It is
/// also the same spelling the query grammar uses, down to the trailing `-`
/// that reverses — `aede query --sort size-` and `aede albums --sort size-`
/// are one thing to remember.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    /// The name the row is filed under.
    Name,
    /// The main artist, by filing name.
    Artist,
    /// How many tracks.
    Tracks,
    /// How many albums.
    Albums,
    /// Playing time.
    Duration,
    /// Size on disk.
    Size,
    /// Year.
    Year,
}

/// Every spelling, and what it means.
const ORDERS: &[(&str, Order)] = &[
    ("name", Order::Name),
    ("title", Order::Name),
    ("artist", Order::Artist),
    ("tracks", Order::Tracks),
    ("albums", Order::Albums),
    ("duration", Order::Duration),
    ("length", Order::Duration),
    ("size", Order::Size),
    ("year", Order::Year),
];

/// The order asked for, or `None` when `--sort` was not given.
///
/// `None` rather than a default, deliberately: every listing already has an
/// order that was chosen for it — genres by how much of the library they hold,
/// albums by year — and replacing those with one generic default would be a
/// regression dressed as a feature. `--sort` overrides; its absence changes
/// nothing.
///
/// Read strictly, and refused against the keys **this** listing has: sorting
/// genres by year would be sorting on a column that is not there, and a value
/// silently ignored is the fault this whole class of guard exists to prevent.
fn order(
    args: &Args,
    allowed: &[Order],
) -> Result<Option<(Order, bool)>, Box<dyn std::error::Error>> {
    let Some(raw) = args.value("sort") else {
        return Ok(None);
    };
    let raw = raw.trim();
    let (name, descending) = match raw.strip_suffix('-').or_else(|| raw.strip_prefix('-')) {
        Some(rest) => (rest.trim(), true),
        None => (raw, false),
    };
    let wanted = name.to_lowercase();
    let offered = |allowed: &[Order]| {
        let mut names: Vec<&str> = ORDERS
            .iter()
            .filter(|(_, order)| allowed.contains(order))
            .map(|(name, _)| *name)
            .collect();
        names.dedup();
        names.join(", ")
    };
    let Some((_, key)) = ORDERS.iter().find(|(n, _)| *n == wanted) else {
        return Err(format!(
            "\"{name}\" is not something to sort on.\nTry: {}",
            offered(allowed)
        )
        .into());
    };
    if !allowed.contains(key) {
        return Err(format!(
            "this listing has no {wanted} to sort on.\nTry: {}",
            offered(allowed)
        )
        .into());
    }
    Ok(Some((*key, descending)))
}

/// Applies an order to rows already reduced to their measures.
///
/// The tuple is `(name, artist, tracks, albums, duration, size, year)` — what
/// every listing has some of. Ties fall back on the name, so two genres of the
/// same size come back in the same places twice running; without that,
/// `--offset` would show one row twice and hide another.
fn put_in_order<T>(
    rows: &mut [T],
    ordering: (Order, bool),
    measures: impl Fn(&T) -> (String, String, usize, usize, u64, u64, u32),
) {
    let (key, descending) = ordering;
    rows.sort_by(|a, b| {
        let (a, b) = (measures(a), measures(b));
        let ordered = match key {
            Order::Name => a.0.cmp(&b.0),
            Order::Artist => a.1.cmp(&b.1),
            Order::Tracks => a.2.cmp(&b.2),
            Order::Albums => a.3.cmp(&b.3),
            Order::Duration => a.4.cmp(&b.4),
            Order::Size => a.5.cmp(&b.5),
            Order::Year => a.6.cmp(&b.6),
        };
        match descending {
            true => ordered.reverse().then_with(|| a.0.cmp(&b.0)),
            false => ordered.then_with(|| a.0.cmp(&b.0)),
        }
    });
}

pub fn list_artists(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;
    // Read strictly, like the window: `--sort banana` used to fall through to
    // sorting by name, which is an answer to a question nobody asked and looks
    // exactly like a correct one. It used to accept two words of its own;
    // it now reads the vocabulary every listing shares, and `tracks` and
    // `name` still mean what they always did.
    const ARTIST_ORDERS: &[Order] = &[
        Order::Name,
        Order::Tracks,
        Order::Albums,
        Order::Duration,
        Order::Size,
    ];
    let ordering = order(args, ARTIST_ORDERS)?;

    // Who does *this* in my library — the inverse of the artist page, and the
    // whole reason credits carry a role rather than being a bare artist
    // column. A role nobody is credited with is an error, with the list of the
    // ones that exist: guessing the spelling of a role is not the user's job.
    let role: Option<String> = match args.value("role") {
        Some(typed) => match role_key(&catalog, typed) {
            Some(key) => Some(key),
            None => {
                return Err(format!(
                    "no role is called \"{typed}\".\nRoles in use: {}",
                    roles_offered(&catalog)
                )
                .into());
            }
        },
        None => None,
    };
    let in_role: Option<std::collections::BTreeSet<Id>> = match role.as_ref() {
        Some(role) => {
            let found: std::collections::BTreeSet<Id> = catalog
                .artists_in_role(role)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            // A real role that this library happens not to hold: an empty
            // result, not a misunderstanding, and the two read differently.
            if found.is_empty() {
                return Err(format!(
                    "nobody here is credited as {}.\nRoles in use: {}",
                    role_label(role),
                    roles_offered(&catalog)
                )
                .into());
            }
            Some(found)
        }
        None => None,
    };

    // Whatever the options cannot say, the grammar can — the same addition
    // `albums` received, and the same rule: an artist is kept when any track
    // the expression matches is credited to them. The coarser question is a
    // fold of the finer one, which is why the grammar evaluates over tracks.
    let matched: Option<std::collections::BTreeSet<Id>> = match args.value("query") {
        None => None,
        Some(expression) => {
            let parsed = aede_core::query::parse(expression)?;
            let data = super::user_data(args, &catalog)?;
            let context = aede_core::query::Context {
                catalog: &catalog,
                data: &data,
                owner: aede_core::user::LOCAL_USER,
            };
            if let Some((what, value)) = aede_core::query::unknown_values(&parsed, &context).first()
            {
                return Err(format!(
                    "no {what} matches \"{value}\".\nRun \"aede {what}s\" for the list."
                )
                .into());
            }
            let mut artists: std::collections::BTreeSet<Id> = Default::default();
            for track in aede_core::query::run(&parsed, &context) {
                for (artist, _) in catalog.credits_on(aede_core::model::EntityKind::Track, track) {
                    artists.insert(artist.id);
                }
            }
            Some(artists)
        }
    };

    let mut rows: Vec<(Id, usize, usize, u64, u64)> = catalog
        .artists
        .iter()
        .filter(|a| match &in_role {
            Some(ids) => ids.contains(&a.id),
            None => true,
        })
        .filter(|a| match &matched {
            Some(ids) => ids.contains(&a.id),
            None => true,
        })
        .map(|a| {
            let tracks = catalog.tracks_of_artist(a.id);
            let albums = catalog.releases_of_artist(a.id).len();
            let (duration, size) = totals(&catalog, &tracks);
            (a.id, tracks.len(), albums, duration, size)
        })
        .collect();

    // The order that was chosen for this listing before `--sort` existed: most
    // heard first, which is what "who is in my library" means.
    rows.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            catalog.artists[a.0 as usize]
                .sort_name
                .cmp(&catalog.artists[b.0 as usize].sort_name)
        })
    });
    if let Some(ordering) = ordering {
        put_in_order(&mut rows, ordering, |row| {
            let name = catalog
                .artists
                .get(row.0 as usize)
                .map(|a| a.sort_name.clone())
                .unwrap_or_default();
            (name.clone(), name, row.1, row.2, row.3, row.4, 0)
        });
    }

    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = rows
            .iter()
            .skip(window.offset)
            .take(window.limit)
            .map(|&(id, tracks, albums, duration, size)| {
                let a = &catalog.artists[id as usize];
                vec![
                    a.name.clone(),
                    a.sort_name.clone(),
                    tracks.to_string(),
                    albums.to_string(),
                    duration.to_string(),
                    size.to_string(),
                    a.mbid.clone().unwrap_or_default(),
                ]
            })
            .collect();
        return export::rows_table(
            &[
                "artist",
                "sort_name",
                "tracks",
                "albums",
                "duration_ms",
                "size_bytes",
                "musicbrainz_artistid",
            ],
            &table,
            args,
        );
    }

    // "3 in total" over one row is a count contradicting the rows under it.
    // The unfiltered listing can say how many the library holds, because that
    // is what it is showing; the moment anything narrows it, the number has to
    // be the number of rows.
    let narrowed = role.is_some() || matched.is_some();
    let heading = match (&role, narrowed) {
        (Some(role), _) => format!("Artists credited as {} ({})", role_label(role), rows.len()),
        (None, true) => format!("Artists ({} matching)", rows.len()),
        (None, false) => format!("Artists ({} in total)", catalog.artists.len()),
    };
    println!("{}", ui::section(&heading));
    let mut t = Table::new(&["Artist", "Tracks", "Albums", "Duration", "Size"])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(0, 50);
    let total = rows.len();
    for (id, tracks, albums, duration, size) in
        rows.into_iter().skip(window.offset).take(window.limit)
    {
        let a = &catalog.artists[id as usize];
        t.push(vec![
            a.name.clone(),
            tracks.to_string(),
            albums.to_string(),
            text::format_duration(duration),
            text::format_size(size),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, total, "artist");
    Ok(())
}

/// Turns the filter options into one expression.
///
/// **The options are shorthand, not a second implementation.** They used to be
/// their own filter loop, which meant `aede albums --genre metal` and
/// `aede query "genre:metal"` were two evaluators answering the same question
/// — one too many, and the day one of them changed nobody would have seen it.
///
/// The mapping is deliberate rather than mechanical in one place: `--artist` on
/// an album listing means the **album artist**, so it becomes `albumartist:`
/// and not `artist:`. Mapping it to `artist:` would have quietly started
/// listing every album Ozzy appears on as a guest under "albums by Ozzy".
fn albums_query(args: &Args) -> Result<String, Box<dyn std::error::Error>> {
    // The two flags are opposites and cannot both be honoured.
    if args.has("compilations") && args.has("no-compilations") {
        return Err("--compilations and --no-compilations ask for opposite things".into());
    }
    let mut terms: Vec<String> = Vec::new();
    if let Some(artist) = args.value("artist") {
        terms.push(format!("albumartist:{}", quoted(artist)));
    }
    if let Some(raw) = args.value("year") {
        // Read here rather than by the grammar so that the message names the
        // option the user actually typed.
        raw.parse::<u32>()
            .map_err(|_| format!("--year expects a year: --year=1969, not \"{raw}\""))?;
        terms.push(format!("year:{raw}"));
    }
    for (option, field) in [
        ("genre", "genre"),
        ("label", "label"),
        ("comment", "comment"),
    ] {
        if let Some(value) = args.value(option) {
            terms.push(format!("{field}:{}", quoted(value)));
        }
    }
    if args.has("compilations") {
        terms.push("compilation:true".into());
    }
    if args.has("no-compilations") {
        terms.push("compilation:false".into());
    }
    // Whatever the options cannot say, the grammar can. Wrapped in brackets so
    // that an expression holding an `OR` narrows *with* the options rather than
    // swallowing them: `--artist X --query "a OR b"` must mean X and (a or b),
    // and juxtaposition binding tighter than OR would otherwise make it
    // (X and a) or b — a listing quietly wider than what was asked for.
    if let Some(expression) = args.value("query") {
        aede_core::query::parse(expression)?;
        terms.push(format!("({expression})"));
    }
    Ok(terms.join(" "))
}

/// Wraps a value so that a name with spaces survives being put in a query.
fn quoted(value: &str) -> String {
    if value.contains(char::is_whitespace) {
        format!("\"{}\"", value.replace('"', ""))
    } else {
        value.to_string()
    }
}

pub fn list_albums(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;

    let expression = albums_query(args)?;
    let parsed = aede_core::query::parse(&expression)?;
    let data = super::user_data(args, &catalog)?;
    let context = aede_core::query::Context {
        catalog: &catalog,
        data: &data,
        owner: aede_core::user::LOCAL_USER,
    };

    // A value naming nothing in the library is a misunderstanding, not an
    // empty result, and the two read differently. This is the distinction the
    // hand-written filter drew and the grammar now draws for everyone.
    if let Some((what, value)) = aede_core::query::unknown_values(&parsed, &context).first() {
        return Err(
            format!("no {what} matches \"{value}\".\nRun \"aede {what}s\" for the list.").into(),
        );
    }

    // An album is kept when any of its tracks answers: the coarser question is
    // a fold of the finer one, which is why the grammar evaluates over tracks.
    let matched: std::collections::BTreeSet<Id> = aede_core::query::run(&parsed, &context)
        .into_iter()
        .filter_map(|id| catalog.track(id).and_then(|t| t.release_id))
        .collect();
    let mut rows: Vec<&aede_core::model::Release> = catalog
        .releases
        .iter()
        .filter(|r| matched.contains(&r.id))
        .collect();

    let compilations_only = args.has("compilations");
    let albums_only = args.has("no-compilations");

    // The order chosen for this listing before `--sort` existed: chronological,
    // which is how a discography reads.
    rows.sort_by(|a, b| {
        a.year
            .unwrap_or(u32::MAX)
            .cmp(&b.year.unwrap_or(u32::MAX))
            .then_with(|| a.title.cmp(&b.title))
    });
    const ALBUM_ORDERS: &[Order] = &[
        Order::Name,
        Order::Artist,
        Order::Year,
        Order::Tracks,
        Order::Duration,
        Order::Size,
    ];
    if let Some(ordering) = order(args, ALBUM_ORDERS)? {
        put_in_order(&mut rows, ordering, |release| {
            let (duration, size) = totals(&catalog, &release.track_ids);
            let artist = release
                .album_artist_id
                .and_then(|id| catalog.artist(id))
                .map(|a| a.sort_name.clone())
                .unwrap_or_default();
            (
                release.title.clone(),
                artist,
                release.track_ids.len(),
                0,
                duration,
                size,
                release.year.unwrap_or(0),
            )
        });
    }

    if args.has("csv") || args.has("json") {
        // The same table as `export --csv`, restricted to what the filters kept:
        // one file for a whole discography is the usual reason to ask.
        let ids: Vec<Id> = rows
            .iter()
            .skip(window.offset)
            .take(window.limit)
            .map(|r| r.id)
            .collect();
        return export::albums_table(&catalog, &ids, args);
    }

    let heading = if compilations_only {
        "Compilations"
    } else if albums_only {
        "Albums, compilations left out"
    } else {
        "Albums"
    };
    println!(
        "{}",
        ui::section(&format!("{heading} ({} matching)", rows.len()))
    );
    // What was narrowed, spelled out. A filter that leaves the count unchanged
    // is indistinguishable from a filter that was ignored, and this project has
    // shipped two options that really were ignored.
    let mut applied: Vec<String> = Vec::new();
    for (name, value) in [
        ("artist", args.value("artist")),
        ("year", args.value("year")),
        ("genre", args.value("genre")),
        ("label", args.value("label")),
        ("comment", args.value("comment")),
    ] {
        if let Some(value) = value {
            applied.push(format!("{name} \"{value}\""));
        }
    }
    if !applied.is_empty() {
        println!(
            "  {}",
            ui::dim(&format!("filtered on {}", applied.join(", ")))
        );
    }
    let mut t = Table::new(&[
        "Year", "Album", "Artist", "Tracks", "Duration", "Size", "Format",
    ])
    .align(3, Align::Right)
    .align(4, Align::Right)
    .align(5, Align::Right)
    .limit(1, 40)
    .limit(2, 30)
    .limit(6, 30);
    let total = rows.len();
    for release in rows.into_iter().skip(window.offset).take(window.limit) {
        let artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Various Artists".into());
        let formats: std::collections::BTreeSet<String> = release
            .track_ids
            .iter()
            .filter_map(|&id| catalog.track(id))
            .filter_map(|t| catalog.file(t.file_id))
            .map(|f| f.properties.quality_label())
            .collect();
        let (duration, size) = totals(&catalog, &release.track_ids);
        t.push(vec![
            release
                .year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "—".into()),
            format!("{}{}", release.title, copy_marker(&catalog, release.id)),
            artist,
            release.track_ids.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            formats.into_iter().collect::<Vec<_>>().join(", "),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, total, "album");
    Ok(())
}

pub fn list_genres(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;
    // Ranked in full, cut at display: the notice below can only be honest if
    // the count of what was left out is known.
    let mut top = stats::top_genres(&catalog, usize::MAX);
    const GENRE_ORDERS: &[Order] = &[Order::Name, Order::Tracks, Order::Duration, Order::Size];
    if let Some(ordering) = order(args, GENRE_ORDERS)? {
        put_in_order(&mut top, ordering, |&(id, count)| {
            let (duration, size) = totals(&catalog, &tracks_of_genre(&catalog, id));
            let name = catalog
                .genre(id)
                .map(|g| g.name.clone())
                .unwrap_or_default();
            (name.clone(), name, count, 0, duration, size, 0)
        });
    }
    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = top
            .iter()
            .map(|&(id, count)| {
                let tracks = tracks_of_genre(&catalog, id);
                let (duration, size) = totals(&catalog, &tracks);
                vec![
                    catalog
                        .genre(id)
                        .map(|g| g.name.clone())
                        .unwrap_or_default(),
                    count.to_string(),
                    duration.to_string(),
                    size.to_string(),
                ]
            })
            .collect();
        return export::rows_table(
            &["genre", "tracks", "duration_ms", "size_bytes"],
            &table,
            args,
        );
    }
    println!(
        "{}",
        ui::section(&format!("Genres ({} in total)", catalog.genres.len()))
    );
    let max = top.first().map(|(_, n)| *n).unwrap_or(0);
    let mut t = Table::new(&["Genre", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .limit(0, 40);
    let total = top.len();
    for (id, count) in top.into_iter().skip(window.offset).take(window.limit) {
        let name = catalog
            .genre(id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let (duration, size) = totals(&catalog, &tracks_of_genre(&catalog, id));
        t.push(vec![
            name,
            count.to_string(),
            text::format_duration(duration),
            text::format_size(size),
            ui::bar(count, max, 20),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, total, "genre");
    Ok(())
}

pub fn list_labels(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;
    let mut top = stats::top_labels(&catalog, usize::MAX);
    const LABEL_ORDERS: &[Order] = &[
        Order::Name,
        Order::Albums,
        Order::Tracks,
        Order::Duration,
        Order::Size,
    ];
    if let Some(ordering) = order(args, LABEL_ORDERS)? {
        put_in_order(&mut top, ordering, |&(id, count)| {
            let tracks = tracks_of_label(&catalog, id);
            let (duration, size) = totals(&catalog, &tracks);
            let albums = catalog
                .releases
                .iter()
                .filter(|r| r.label_ids.contains(&id))
                .count();
            let name = catalog
                .label(id)
                .map(|l| l.name.clone())
                .unwrap_or_default();
            (name.clone(), name, count, albums, duration, size, 0)
        });
    }
    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = top
            .iter()
            .map(|&(id, count)| {
                let tracks = tracks_of_label(&catalog, id);
                let (duration, size) = totals(&catalog, &tracks);
                vec![
                    catalog
                        .label(id)
                        .map(|l| l.name.clone())
                        .unwrap_or_default(),
                    count.to_string(),
                    tracks.len().to_string(),
                    duration.to_string(),
                    size.to_string(),
                ]
            })
            .collect();
        return export::rows_table(
            &["label", "albums", "tracks", "duration_ms", "size_bytes"],
            &table,
            args,
        );
    }
    println!(
        "{}",
        ui::section(&format!("Labels ({} in total)", catalog.labels.len()))
    );
    let max = top.first().map(|(_, n)| *n).unwrap_or(0);
    let mut t = Table::new(&["Label", "Albums", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(0, 40);
    let total = top.len();
    for (id, count) in top.into_iter().skip(window.offset).take(window.limit) {
        let name = catalog
            .label(id)
            .map(|l| l.name.clone())
            .unwrap_or_default();
        let tracks = tracks_of_label(&catalog, id);
        let (duration, size) = totals(&catalog, &tracks);
        t.push(vec![
            name,
            count.to_string(),
            tracks.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            ui::bar(count, max, 20),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, total, "label");
    Ok(())
}

pub fn list_years(args: &Args) -> Res {
    let catalog = load(args)?;
    let mut by_year: std::collections::BTreeMap<u32, (usize, Vec<Id>)> = Default::default();
    for release in &catalog.releases {
        let Some(year) = release.year else { continue };
        let entry = by_year.entry(year).or_default();
        entry.0 += 1;
        entry.1.extend(release.track_ids.iter().copied());
    }
    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = by_year
            .iter()
            .map(|(year, (albums, tracks))| {
                let (duration, size) = totals(&catalog, tracks);
                vec![
                    year.to_string(),
                    albums.to_string(),
                    tracks.len().to_string(),
                    duration.to_string(),
                    size.to_string(),
                ]
            })
            .collect();
        return export::rows_table(
            &["year", "albums", "tracks", "duration_ms", "size_bytes"],
            &table,
            args,
        );
    }

    println!("{}", ui::section("Years"));
    // A BTreeMap is already in year order, which is the order chosen for this
    // listing. `--sort` turns it into a list so another order can be given.
    const YEAR_ORDERS: &[Order] = &[
        Order::Year,
        Order::Albums,
        Order::Tracks,
        Order::Duration,
        Order::Size,
    ];
    let ordering = order(args, YEAR_ORDERS)?;
    let max = by_year.values().map(|(a, _)| *a).max().unwrap_or(0);
    let mut by_year: Vec<(u32, (usize, Vec<Id>))> = by_year.into_iter().collect();
    if let Some(ordering) = ordering {
        put_in_order(&mut by_year, ordering, |(year, (albums, tracks))| {
            let (duration, size) = totals(&catalog, tracks);
            let name = year.to_string();
            (
                name.clone(),
                name,
                tracks.len(),
                *albums,
                duration,
                size,
                *year,
            )
        });
    }
    let mut t = Table::new(&["Year", "Albums", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right);
    for (year, (albums, tracks)) in by_year {
        let (duration, size) = totals(&catalog, &tracks);
        t.push(vec![
            year.to_string(),
            albums.to_string(),
            tracks.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            ui::bar(albums, max, 20),
        ]);
    }
    print!("{}", t.render());
    Ok(())
}

fn tracks_of_genre(catalog: &aede_core::model::Catalog, genre_id: Id) -> Vec<Id> {
    use aede_core::model::EntityKind;
    let mut tracks: std::collections::BTreeSet<Id> = Default::default();
    for link in &catalog.genre_links {
        if link.genre_id != genre_id {
            continue;
        }
        match link.entity_kind {
            EntityKind::Track => {
                tracks.insert(link.entity_id);
            }
            EntityKind::Release => {
                if let Some(release) = catalog.release(link.entity_id) {
                    tracks.extend(release.track_ids.iter().copied());
                }
            }
            _ => {}
        }
    }
    tracks.into_iter().collect()
}

/// Tracks issued on a label, through the releases carrying it.
fn tracks_of_label(catalog: &aede_core::model::Catalog, label_id: Id) -> Vec<Id> {
    catalog
        .releases
        .iter()
        .filter(|r| r.label_ids.contains(&label_id))
        .flat_map(|r| r.track_ids.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Args;

    fn expression(words: &[&str]) -> String {
        let args = Args::parse(words.iter().map(|w| w.to_string()));
        albums_query(&args).expect("a readable expression")
    }

    #[test]
    fn an_album_listing_asks_for_the_album_artist_and_not_any_credit() {
        // The one mapping that is a decision rather than a transcription, and
        // the one no end-to-end test on this library could catch: the fixtures
        // hold nobody who guests on somebody else's album, so both spellings
        // answer the same there. The decision is therefore tested where it is
        // taken. Mapping `--artist` onto `artist:` would quietly list every
        // album an artist appears on as one of theirs.
        assert_eq!(
            expression(&["albums", "--artist", "Ozzy"]),
            "albumartist:Ozzy"
        );
        assert!(
            !expression(&["albums", "--artist", "Ozzy"]).starts_with("artist:"),
            "any credit is a different question from the album's own artist"
        );
    }

    #[test]
    fn every_filter_option_becomes_one_term() {
        assert_eq!(expression(&["albums", "--genre", "metal"]), "genre:metal");
        assert_eq!(
            expression(&["albums", "--label", "Earache"]),
            "label:Earache"
        );
        assert_eq!(expression(&["albums", "--year", "1969"]), "year:1969");
        assert_eq!(
            expression(&["albums", "--comment", "vinyl"]),
            "comment:vinyl"
        );
        assert_eq!(
            expression(&["albums", "--compilations"]),
            "compilation:true"
        );
        assert_eq!(
            expression(&["albums", "--no-compilations"]),
            "compilation:false"
        );
        assert_eq!(expression(&["albums"]), "", "no filter is no expression");

        // A name with spaces has to survive being put into a query, or
        // `--artist Miles Davis` would become two terms and quietly ask for
        // albums whose artist is "Miles" *and* something called "Davis".
        assert_eq!(
            expression(&["albums", "--artist", "Miles Davis"]),
            "albumartist:\"Miles Davis\""
        );

        // Several options join with a space, which the grammar reads as AND.
        assert_eq!(
            expression(&["albums", "--genre", "metal", "--year", "1994"]),
            "year:1994 genre:metal"
        );
    }

    #[test]
    fn a_year_that_is_not_one_is_refused_by_the_option_that_named_it() {
        let args = Args::parse(["albums", "--year", "abc"].iter().map(|w| w.to_string()));
        let error = albums_query(&args).expect_err("not a year");
        assert!(
            error.to_string().contains("--year expects a year"),
            "the message names the option typed, not the grammar: {error}"
        );
    }
}
