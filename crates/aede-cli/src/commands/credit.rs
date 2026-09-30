//! Explicit, reversible corrections to individual sourced credits.

use aede_core::graph::{self, RelationRef};
use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::sources::{
    self, Confidence, CreditLink, Facts, ReleaseFacts, SourceRecord, TrackFacts, WorkLink,
};
use aede_core::user::EntityRef;
use aede_core::{clock, text};

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui;

const MANUAL: &str = "manual";

enum Scope {
    Recording(Id),
    Work {
        recording_id: Id,
        mbid: String,
        title: String,
    },
    Edition(Id),
}

pub fn credit(args: &Args) -> Res {
    let adding = args.has("add");
    let excluding = args.value("exclude");
    let undoing = args.value("undo");
    if usize::from(adding) + usize::from(excluding.is_some()) + usize::from(undoing.is_some()) != 1
    {
        return Err(
            "choose one: --add <scope>, --exclude=<credit ID>, or --undo=<credit ID>".into(),
        );
    }
    let catalog = load(args)?;
    let mut held = sources::load_all(&sources::sources_path(&data_dir(args)))?.unwrap_or_default();
    let path = sources::sources_path(&data_dir(args));
    if let Some(selector) = excluding.or(undoing) {
        if !args.positionals.is_empty()
            || args.has("artist")
            || args.has("artist-id")
            || args.has("role")
            || args.has("instrument")
        {
            return Err("credit decisions take an ID, not a scope or new credit fields".into());
        }
        let edge = graph::select(&graph::edges(&catalog, &held), selector)?;
        if !edge.reference.kind.starts_with("credit:") || edge.reference.provenance == "tags" {
            return Err(
                "only a sourced credit can be excluded; tags remain the local record".into(),
            );
        }
        if excluding.is_some() {
            held.exclude_credit(edge.reference, clock::now_seconds());
            sources::save(&held, &path)?;
            println!(
                "{} credit excluded from navigation and search; source evidence kept",
                ui::green("→")
            );
        } else if held.restore_credit(&edge.reference) {
            sources::save(&held, &path)?;
            println!("{} credit restored", ui::green("→"));
        } else {
            return Err("this credit has no exclusion to undo".into());
        }
        return Ok(());
    }

    let Some(token) = args.positionals.first() else {
        return Err("give a precise scope: recording:<ID>, work:<ID>, or release:<ID>".into());
    };
    if args.positionals.len() != 1 {
        return Err("give exactly one credit scope".into());
    }
    let artist_name = args
        .value("artist")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "--artist names the person to credit".to_string())?;
    let role = args
        .value("role")
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .ok_or_else(|| "--role names what they did".to_string())?
        .to_ascii_lowercase()
        .replace([' ', '-'], "_");
    let artist_mbid = args.value("artist-id").unwrap_or("").trim();
    let instrument = args
        .value("instrument")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let scope = resolve_scope(&catalog, &held, token)?;
    let (target, entity) = match &scope {
        Scope::Recording(id) => (
            EntityRef::of(&catalog, EntityKind::Recording, *id)
                .ok_or("recording has no stable reference")?,
            representative_track(&catalog, *id)?,
        ),
        Scope::Work {
            recording_id, mbid, ..
        } => (
            EntityRef::new(EntityKind::Work, mbid.clone()),
            representative_track(&catalog, *recording_id)?,
        ),
        Scope::Edition(id) => (
            EntityRef::of(&catalog, EntityKind::Release, *id)
                .ok_or("edition has no stable reference")?,
            EntityRef::of(&catalog, EntityKind::Release, *id)
                .ok_or("edition has no stable reference")?,
        ),
    };
    let seed = RelationRef {
        source: EntityRef::new(
            EntityKind::Artist,
            if artist_mbid.is_empty() {
                format!("manual:{}", text::normalize(artist_name))
            } else {
                format!("mbid:{artist_mbid}")
            },
        ),
        kind: format!("credit:{role}"),
        target,
        provenance: MANUAL.into(),
        source_id: instrument.map(text::normalize),
    };
    let relation_id = format!("manual:{}", seed.id());
    let credit = CreditLink {
        relation_id: Some(relation_id.clone()),
        role_id: None,
        role,
        direction: None,
        artist_mbid: artist_mbid.to_string(),
        artist_name: artist_name.to_string(),
        credited_as: None,
        attributes: instrument
            .map(|name| aede_core::model::CreditAttribute {
                id: None,
                name: name.to_string(),
                value: None,
                credited_as: None,
            })
            .into_iter()
            .collect(),
        began: None,
        ended: None,
        over: None,
        order: None,
    };
    let existing = held.get(&entity, MANUAL).cloned();
    let mut facts = existing
        .as_ref()
        .map(|record| record.facts.clone())
        .unwrap_or_else(|| match scope {
            Scope::Edition(_) => Facts::Release(ReleaseFacts::default()),
            _ => Facts::Track(TrackFacts::default()),
        });
    let credits = match (&scope, &mut facts) {
        (Scope::Recording(_), Facts::Track(facts)) => &mut facts.credits,
        (Scope::Work { mbid, title, .. }, Facts::Track(facts)) => {
            let index = match facts.works.iter().position(|work| work.mbid == *mbid) {
                Some(index) => index,
                None => {
                    facts.works.push(WorkLink {
                        mbid: mbid.clone(),
                        title: title.clone(),
                        ..Default::default()
                    });
                    facts.works.len() - 1
                }
            };
            &mut facts.works[index].credits
        }
        (Scope::Edition(id), Facts::Release(facts)) => {
            facts.edition_mbid = catalog
                .release(*id)
                .and_then(|release| release.mbid.clone());
            &mut facts.credits
        }
        _ => {
            return Err(
                "manual source record has a different scope; refusing to overwrite it".into(),
            );
        }
    };
    if credits
        .iter()
        .any(|held| held.relation_id.as_deref() == Some(&relation_id))
    {
        return Err("this exact manual credit is already present".into());
    }
    credits.push(credit);
    held.set(SourceRecord {
        key: entity.key,
        source: MANUAL.into(),
        source_id: existing.and_then(|record| record.source_id),
        fetched_at: clock::now_seconds(),
        confidence: Confidence::Identified,
        facts,
    });
    sources::save(&held, &path)?;
    let id = graph::edges(&catalog, &held)
        .into_iter()
        .find(|edge| {
            edge.reference.provenance == MANUAL
                && edge.reference.source_id.as_deref() == Some(&relation_id)
        })
        .map(|edge| edge.reference.id())
        .unwrap_or(relation_id);
    println!("{} manual credit saved: {id}", ui::green("→"));
    Ok(())
}

fn representative_track(catalog: &Catalog, recording_id: Id) -> Result<EntityRef, String> {
    catalog
        .recording(recording_id)
        .ok_or_else(|| "recording is not in the local catalog".to_string())?
        .track_ids
        .iter()
        .filter_map(|&id| EntityRef::of(catalog, EntityKind::Track, id))
        .min_by(|left, right| left.key.cmp(&right.key))
        .ok_or_else(|| "recording has no local file".to_string())
}

fn resolve_scope(
    catalog: &Catalog,
    sources: &sources::Sources,
    token: &str,
) -> Result<Scope, String> {
    let (kind, key) = token
        .split_once(':')
        .ok_or_else(|| "scope must begin recording:, work:, or release:".to_string())?;
    match kind {
        "recording" => EntityRef::new(EntityKind::Recording, key)
            .resolve(catalog)
            .map(Scope::Recording)
            .ok_or_else(|| format!("no local recording has identity {key}")),
        "release" => {
            let matching: Vec<_> = catalog
                .releases
                .iter()
                .filter(|release| release.mbid.as_deref() == Some(key))
                .collect();
            if matching.len() > 1 {
                return Err(format!(
                    "{key} identifies several local editions; use release:<local release key>"
                ));
            }
            matching
                .first()
                .map(|release| release.id)
                .or_else(|| EntityRef::new(EntityKind::Release, key).resolve(catalog))
                .map(Scope::Edition)
                .ok_or_else(|| format!("no local edition has identity {key}"))
        }
        "work" => {
            let local = catalog
                .works
                .iter()
                .find(|work| work.mbid == key)
                .and_then(|work| {
                    work.recording_ids
                        .first()
                        .copied()
                        .map(|id| (id, work.title.clone()))
                });
            let sourced = sources
                .find_sourced_works(catalog, key)
                .into_iter()
                .find(|work| work.mbid == key)
                .and_then(|work| {
                    work.links
                        .first()
                        .map(|link| (link.recording_id, work.title))
                });
            local
                .or(sourced)
                .map(|(recording_id, title)| Scope::Work {
                    recording_id,
                    mbid: key.into(),
                    title,
                })
                .ok_or_else(|| format!("no local recording is linked to work {key}"))
        }
        _ => Err("scope must begin recording:, work:, or release:".into()),
    }
}
