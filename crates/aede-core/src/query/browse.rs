//! Versioned dynamic browsing collections; audio and catalog identities remain unchanged.
use super::{Context, QueryError, error};
use crate::model::{Catalog, EntityKind, Id};
use crate::sources::{Facts, Sources};
use crate::user::EntityRef;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Validated filters retained in the ordinary saved-query representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    view: String,
    reverse: bool,
    mode: String,
    years: [u64; 2],
    alphabet: [u64; 2],
    unknown: bool,
    query: String,
    countries: Vec<Option<String>>,
    genres: Vec<String>,
    ratings: Option<[u64; 2]>,
    tags: Vec<String>,
    label: Option<EntityRef>,
    composers: Vec<EntityRef>,
}
/// Parse the bounded JSON payload following `browse-v1:`. Unknown keys are errors.
pub fn parse(input: &str) -> Result<Filter, QueryError> {
    if input.len() > 8192 {
        return Err(error("browse filter exceeds 8192 bytes"));
    }
    let v: Value = serde_json::from_str(input).map_err(|_| error("invalid browse filter JSON"))?;
    let o = v
        .as_object()
        .ok_or_else(|| error("browse filter must be an object"))?;
    if o.keys().any(|k| {
        ![
            "view",
            "reverse",
            "mode",
            "years",
            "alphabet",
            "includeUnknownYear",
            "query",
            "countries",
            "genre",
            "genres",
            "ratings",
            "tags",
            "label",
            "composers",
        ]
        .contains(&k.as_str())
    }) {
        return Err(error("unknown browse filter field"));
    }
    let string = |key: &str| {
        v[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| error(format!("invalid browse {key}")))
    };
    let interval = |key: &str, max: u64| -> Result<[u64; 2], QueryError> {
        let values = v[key]
            .as_array()
            .filter(|a| a.len() == 2)
            .ok_or_else(|| error("invalid browse interval"))?;
        let low = values[0]
            .as_u64()
            .ok_or_else(|| error("invalid browse bound"))?;
        let high = values[1]
            .as_u64()
            .filter(|h| *h <= max && *h >= low)
            .ok_or_else(|| error("invalid browse bound"))?;
        Ok([low, high])
    };
    let view = string("view")?;
    let mode = string("mode")?;
    if ![
        "artists",
        "tracks",
        "albums",
        "compilations",
        "countries",
        "genres",
        "composers",
        "rated",
        "favourites",
        "tags",
        "label",
    ]
    .contains(&view.as_str())
        || !["none", "years", "alphabet", "both"].contains(&mode.as_str())
    {
        return Err(error("invalid browse view or mode"));
    }
    let reverse = match v.get("reverse") {
        None => false,
        Some(value) if view == "tracks" => value
            .as_bool()
            .ok_or_else(|| error("invalid track order"))?,
        Some(_) => return Err(error("reverse requires the tracks view")),
    };
    let label = if view == "label" {
        Some(
            EntityRef::parse_token(&string("label")?)
                .filter(|r| r.kind == EntityKind::Label && r.key.len() <= 4096)
                .ok_or_else(|| error("invalid browse label reference"))?,
        )
    } else {
        if o.contains_key("label") {
            return Err(error("label requires the label view"));
        }
        None
    };
    let composers = if view == "composers" {
        v["composers"]
            .as_array()
            .filter(|rows| rows.len() <= 100)
            .ok_or_else(|| error("invalid browse composers"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .and_then(EntityRef::parse_token)
                    .filter(|reference| {
                        reference.kind == EntityKind::Artist && reference.key.len() <= 4096
                    })
                    .ok_or_else(|| error("invalid browse composer reference"))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        if o.contains_key("composers") {
            return Err(error("composers require the composers view"));
        }
        Vec::new()
    };
    let ratings = if view == "rated" {
        let bounds = interval("ratings", 5)?;
        if bounds[0] == 0 {
            return Err(error("minimum rating must be at least one"));
        }
        Some(bounds)
    } else {
        if o.contains_key("ratings") {
            return Err(error("ratings require the rated view"));
        }
        None
    };
    let countries = match v.get("countries") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .filter(|a| a.len() <= 256)
            .ok_or_else(|| error("invalid browse countries"))?
            .iter()
            .map(|v| {
                if v.is_null() {
                    Ok(None)
                } else {
                    v.as_str()
                        .filter(|s| s.len() <= 256)
                        .map(|s| Some(s.to_owned()))
                        .ok_or_else(|| error("invalid browse country"))
                }
            })
            .collect::<Result<_, _>>()?,
    };
    let mut genres = match v.get("genres") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .filter(|a| a.len() <= 256)
            .ok_or_else(|| error("invalid browse genres"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|s| !s.trim().is_empty() && s.len() <= 256)
                    .map(str::to_owned)
                    .ok_or_else(|| error("invalid browse genre"))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let genre = match v.get("genre") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .filter(|s| s.len() <= 256)
                .ok_or_else(|| error("invalid browse genre"))?
                .to_owned(),
        ),
    };
    if let Some(genre) = genre {
        genres.push(genre);
    }
    genres.sort();
    genres.dedup();
    let tags = if view == "tags" {
        if mode != "none" || !countries.is_empty() || !genres.is_empty() {
            return Err(error(
                "tag collections require hidden rails and no country/genre criteria",
            ));
        }
        v["tags"]
            .as_array()
            .filter(|a| a.len() <= 100)
            .ok_or_else(|| error("invalid browse tags"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|s| !s.trim().is_empty() && s.len() <= 256 && !s.contains('\0'))
                    .map(folded)
                    .ok_or_else(|| error("invalid browse tag"))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        if o.contains_key("tags") {
            return Err(error("tags require the tags view"));
        }
        Vec::new()
    };
    let query = string("query")?;
    if query.len() > 2048 {
        return Err(error("browse search exceeds 2048 bytes"));
    }
    if view == "tracks" && (!countries.is_empty() || !genres.is_empty()) {
        return Err(error(
            "track filters do not support country or genre facets",
        ));
    }
    Ok(Filter {
        view,
        reverse,
        mode,
        years: interval("years", 9999)?,
        alphabet: interval("alphabet", 26)?,
        unknown: v["includeUnknownYear"]
            .as_bool()
            .ok_or_else(|| error("invalid unknown-year choice"))?,
        query,
        countries,
        genres,
        ratings,
        tags,
        label,
        composers,
    })
}
#[derive(Default)]
pub(super) struct Claims {
    origins: BTreeMap<Id, Option<String>>,
    prose: BTreeMap<Id, String>,
}
impl Claims {
    pub(super) fn new(catalog: &Catalog, sources: &Sources) -> Self {
        let mut claims = Self::default();
        for artist in &catalog.artists {
            let Some(reference) = EntityRef::of(catalog, EntityKind::Artist, artist.id) else {
                continue;
            };
            for record in sources
                .about(&reference)
                .filter(|r| sources.is_trusted(catalog, r))
            {
                if let Facts::Artist(facts) = &record.facts {
                    if record.source == crate::sources::MUSICBRAINZ {
                        claims
                            .origins
                            .insert(artist.id, facts.country_code.clone().or(facts.area.clone()));
                    }
                    if let Some(prose) = &facts.summary {
                        claims
                            .prose
                            .entry(artist.id)
                            .or_insert_with(|| prose.text.clone());
                    }
                }
            }
        }
        claims
    }
}
fn folded(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .flat_map(crate::text::fold_char)
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect()
}
fn initial(s: &str) -> u64 {
    // Preserve the explicit # bucket for a non-letter first character.
    let first = s.trim().chars().next().unwrap_or('#');
    let folded = folded(&first.to_string());
    match folded.bytes().next() {
        Some(c @ b'a'..=b'z') => u64::from(c - b'a' + 1),
        _ => 0,
    }
}
fn alphabet(name: &str, f: &Filter) -> bool {
    !["alphabet", "both"].contains(&f.mode.as_str())
        || (f.alphabet[0]..=f.alphabet[1]).contains(&initial(name))
}
fn annotation(context: &Context, kind: EntityKind, id: Id) -> String {
    EntityRef::of(context.catalog, kind, id)
        .and_then(|r| context.data.find(context.owner, &r))
        .map(|a| {
            format!(
                "{} {} {} {}",
                a.tags.iter().cloned().collect::<Vec<_>>().join(" "),
                a.note.as_deref().unwrap_or(""),
                a.rating.map(|r| r.to_string()).unwrap_or_default(),
                if a.loved { "favori" } else { "" }
            )
        })
        .unwrap_or_default()
}
/// Evaluate once per collection; each matching edition contributes its complete track order.
pub(super) fn tracks(f: &Filter, context: &Context) -> Vec<Id> {
    let c = context.catalog;
    if f.view == "tracks" {
        return track_selection(f, context);
    }
    if f.view == "tags" {
        return tagged_tracks(f, context);
    }
    let label = f.label.as_ref().and_then(|r| r.resolve(c));
    if f.label.is_some() && label.is_none() {
        return Vec::new();
    }
    let composer_index = (f.view == "composers").then(|| c.composers_by_release());
    let selected_composers: BTreeSet<_> = f
        .composers
        .iter()
        .filter_map(|reference| reference.resolve(c))
        .collect();
    let artist_view = ["artists", "countries", "genres"].contains(&f.view.as_str());
    let principal: BTreeSet<_> = c
        .releases
        .iter()
        .filter_map(|r| r.album_artist_id)
        .collect();
    let mut artist_text = BTreeMap::new();
    let mut eligible = BTreeSet::new();
    for artist in &c.artists {
        let releases: BTreeSet<_> = c
            .releases_of_artist(artist.id)
            .into_iter()
            .chain(
                c.releases
                    .iter()
                    .filter(|r| r.album_artist_id == Some(artist.id))
                    .map(|r| r.id),
            )
            .collect();
        let genres: BTreeSet<_> = releases
            .iter()
            .flat_map(|id| c.genres_of(EntityKind::Release, *id))
            .map(|g| g.name.as_str())
            .collect();
        let country = context
            .browse_claims
            .origins
            .get(&artist.id)
            .cloned()
            .flatten();
        let valid = (!artist_view || (principal.contains(&artist.id) && alphabet(&artist.name, f)))
            && (f.countries.is_empty() || f.countries.contains(&country))
            && (f.genres.is_empty() || f.genres.iter().any(|g| genres.contains(g.as_str())));
        if valid {
            eligible.insert(artist.id);
        }
        artist_text.insert(
            artist.id,
            folded(&format!(
                "{} {} {} {} {} {} {} {}",
                artist.name,
                artist.sort_name,
                artist.mbid.as_deref().unwrap_or(""),
                artist.aliases.join(" "),
                genres.into_iter().collect::<Vec<_>>().join(" "),
                country.unwrap_or_default(),
                context
                    .browse_claims
                    .prose
                    .get(&artist.id)
                    .cloned()
                    .unwrap_or_default(),
                annotation(context, EntityKind::Artist, artist.id)
            )),
        );
    }
    let term = folded(&f.query);
    let mut tracks = Vec::new();
    for release in &c.releases {
        if let Some(index) = &composer_index {
            let Some(credited) = index.get(&release.id) else {
                continue;
            };
            if !f.composers.is_empty() && credited.is_disjoint(&selected_composers) {
                continue;
            }
        }
        if label.is_some_and(|id| !release.label_ids.contains(&id)) {
            continue;
        }
        if f.view == "rated" || f.view == "favourites" {
            let own = EntityRef::of(c, EntityKind::Release, release.id)
                .and_then(|r| context.data.find(context.owner, &r));
            let principal = release
                .album_artist_id
                .and_then(|id| EntityRef::of(c, EntityKind::Artist, id))
                .and_then(|r| context.data.find(context.owner, &r));
            let rating = own
                .and_then(|a| a.rating)
                .unwrap_or(0)
                .max(principal.and_then(|a| a.rating).unwrap_or(0));
            if f.ratings
                .is_some_and(|r| !(r[0]..=r[1]).contains(&u64::from(rating)))
                || (f.view == "favourites"
                    && !own.is_some_and(|a| a.loved)
                    && !principal.is_some_and(|a| a.loved))
            {
                continue;
            }
        }
        if f.view == "compilations" && !release.is_compilation {
            continue;
        }
        if ["years", "both"].contains(&f.mode.as_str())
            && !release.year.map_or(f.unknown, |y| {
                (f.years[0]..=f.years[1]).contains(&u64::from(y))
            })
        {
            continue;
        }
        if !artist_view && !alphabet(&release.title, f) {
            continue;
        }
        let mut related: BTreeSet<_> = release.album_artist_id.into_iter().collect();
        related.extend(
            c.credits_on(EntityKind::Release, release.id)
                .into_iter()
                .map(|(a, _)| a.id),
        );
        let mut text = format!(
            "{} {} {} {}",
            release.title,
            release.year.map(|y| y.to_string()).unwrap_or_default(),
            c.genres_of(EntityKind::Release, release.id)
                .into_iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            annotation(context, EntityKind::Release, release.id)
        );
        for id in &release.track_ids {
            related.extend(
                c.credits_on(EntityKind::Track, *id)
                    .into_iter()
                    .map(|(a, _)| a.id),
            );
            if let Some(file) = c.track(*id).and_then(|t| c.file(t.file_id)) {
                for (key, values) in &file.tags {
                    text.push_str(&format!(" {key} {}", values.join(" ")));
                }
            }
        }
        let text = folded(&text);
        let matches = related.iter().any(|a| {
            eligible.contains(a)
                && (term.is_empty()
                    || text.contains(&term)
                    || artist_text.get(a).is_some_and(|s| s.contains(&term)))
        }) || (!artist_view
            && related.is_empty()
            && f.countries.is_empty()
            && f.genres.is_empty()
            && (term.is_empty() || text.contains(&term)));
        if matches {
            tracks.extend(&release.track_ids);
        }
    }
    tracks
}

fn tagged_tracks(f: &Filter, context: &Context) -> Vec<Id> {
    let term = folded(&f.query);
    let matches = |kind, id| {
        EntityRef::of(context.catalog, kind, id)
            .and_then(|r| context.data.find(context.owner, &r))
            .is_some_and(|a| {
                a.tags.iter().any(|tag| {
                    let key = folded(tag);
                    (f.tags.is_empty() || f.tags.contains(&key)) && key.contains(&term)
                })
            })
    };
    context
        .catalog
        .tracks
        .iter()
        .filter(|track| {
            matches(EntityKind::Track, track.id)
                || track
                    .release_id
                    .and_then(|id| context.catalog.release(id))
                    .is_some_and(|release| {
                        matches(EntityKind::Release, release.id)
                            || release
                                .album_artist_id
                                .is_some_and(|id| matches(EntityKind::Artist, id))
                    })
        })
        .map(|track| track.id)
        .collect()
}
#[cfg(test)]
#[path = "browse_tests.rs"]
mod tests;

// Track selections retain individual membership, including albumless placements.
fn track_selection(f: &Filter, context: &Context) -> Vec<Id> {
    let c = context.catalog;
    let term = folded(&f.query);
    let has = |value: &str| folded(value).contains(&term);
    let mut credited = BTreeSet::new();
    if !term.is_empty() {
        for credit in &c.credits {
            if credit.entity_kind == EntityKind::Track
                && (has(&credit.role)
                    || c.artist(credit.artist_id)
                        .is_some_and(|artist| has(&artist.name)))
            {
                credited.insert(credit.entity_id);
            }
        }
    }
    let mut rows: Vec<_> = c
        .tracks
        .iter()
        .filter(|track| {
            let release = track.release_id.and_then(|id| c.release(id));
            if !alphabet(&track.title, f)
                || (["years", "both"].contains(&f.mode.as_str())
                    && !release.and_then(|r| r.year).map_or(f.unknown, |y| {
                        (f.years[0]..=f.years[1]).contains(&u64::from(y))
                    }))
            {
                return false;
            }
            term.is_empty()
                || has(&track.title)
                || credited.contains(&track.id)
                || release.is_some_and(|release| {
                    has(&release.title)
                        || release
                            .album_artist_id
                            .and_then(|id| c.artist(id))
                            .is_some_and(|artist| has(&artist.name))
                })
                || c.file(track.file_id).is_some_and(|file| {
                    file.tags
                        .iter()
                        .any(|(key, values)| has(key) || values.iter().any(|value| has(value)))
                })
        })
        .map(|track| track.id)
        .collect();
    if f.reverse {
        rows.reverse();
    }
    rows
}
