//! Evidence-backed relocation and compatibility with old album references.

use super::*;
use crate::json::Json;
use crate::model::AudioFile;

/// Last observed identity evidence for a file named by personal data.
///
/// A basename alone never proves a move. Size, stream characteristics and
/// identifying tags must agree, as must an acoustic fingerprint when one was
/// available. This is a conservative relocation heuristic, not a byte checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackIdentity {
    /// Path at which the evidence was observed.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Codec declared by the stream.
    pub codec: String,
    /// Duration measured from the stream.
    pub duration_ms: Option<u64>,
    /// Sampling frequency in hertz.
    pub sample_rate: Option<u32>,
    /// Stream precision when meaningful.
    pub bit_depth: Option<u16>,
    /// Number of channels.
    pub channels: Option<u16>,
    /// Identifying tags, without notes, lyrics or other large text fields.
    pub metadata: BTreeMap<String, Vec<String>>,
    /// Acoustic fingerprint, when already known without reading the file.
    pub fingerprint: Option<String>,
}

impl TrackIdentity {
    fn of(file: &AudioFile) -> Self {
        let metadata = [
            "title",
            "artist",
            "albumartist",
            "album",
            "tracknumber",
            "discnumber",
            "musicbrainz_recordingid",
            "musicbrainz_albumid",
            "isrc",
        ]
        .into_iter()
        .filter_map(|key| {
            file.tags
                .get(key)
                .map(|values| (key.into(), values.clone()))
        })
        .collect();
        Self {
            path: file.path.clone(),
            size: file.size,
            codec: file.properties.codec.clone(),
            duration_ms: file.properties.duration_ms,
            sample_rate: file.properties.sample_rate,
            bit_depth: file.properties.bit_depth,
            channels: file.properties.channels,
            metadata,
            fingerprint: file.fingerprint.as_ref().map(|fp| fp.data.clone()),
        }
    }

    fn matches(&self, file: &AudioFile) -> bool {
        let current = Self::of(file);
        self.size == current.size
            && self.codec == current.codec
            && self.duration_ms == current.duration_ms
            && self.sample_rate == current.sample_rate
            && self.bit_depth == current.bit_depth
            && self.channels == current.channels
            && self.metadata == current.metadata
            && self
                .fingerprint
                .as_ref()
                .is_none_or(|fp| current.fingerprint.as_ref() == Some(fp))
    }

    pub(super) fn to_json(&self) -> Json {
        let mut row = Json::obj();
        row.set("path", self.path.as_str().into());
        row.set("size", self.size.into());
        row.set("codec", self.codec.as_str().into());
        if let Some(value) = self.duration_ms {
            row.set("duration_ms", value.into());
        }
        if let Some(value) = self.sample_rate {
            row.set("sample_rate", value.into());
        }
        if let Some(value) = self.bit_depth {
            row.set("bit_depth", u32::from(value).into());
        }
        if let Some(value) = self.channels {
            row.set("channels", u32::from(value).into());
        }
        if let Some(value) = &self.fingerprint {
            row.set("fingerprint", value.as_str().into());
        }
        let mut metadata = Json::obj();
        for (key, values) in &self.metadata {
            metadata.set(
                key,
                Json::Arr(values.iter().map(|value| value.as_str().into()).collect()),
            );
        }
        row.set("metadata", metadata);
        row
    }

    pub(super) fn from_json(row: &Json) -> Option<Self> {
        let Json::Obj(fields) = row.get("metadata")? else {
            return None;
        };
        let metadata = fields
            .iter()
            .map(|(key, values)| {
                Some((
                    key.clone(),
                    values
                        .as_arr()?
                        .iter()
                        .map(Json::as_string)
                        .collect::<Option<Vec<_>>>()?,
                ))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        Some(Self {
            path: row.field_str("path")?,
            size: row.field_u64("size")?,
            codec: row.field_str("codec")?,
            duration_ms: row.field_u64("duration_ms"),
            sample_rate: row.field_u32("sample_rate"),
            bit_depth: row.field_u32("bit_depth").and_then(|n| n.try_into().ok()),
            channels: row.field_u32("channels").and_then(|n| n.try_into().ok()),
            metadata,
            fingerprint: row.field_str("fingerprint"),
        })
    }
}

/// Counts personal records attached, waiting, or relocated during reconciliation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Attachment {
    /// Records whose target is in the catalog.
    pub attached: usize,
    /// Records whose target is absent and retained unchanged.
    pub waiting: usize,
    /// Records whose reference was migrated or whose file moved.
    pub moved: usize,
}

pub(super) fn resolve_release(reference: &EntityRef, catalog: &Catalog) -> Option<Id> {
    if let Some(release) = catalog.releases.iter().find(|release| {
        EntityRef::of(catalog, EntityKind::Release, release.id).as_ref() == Some(reference)
    }) {
        return Some(release.id);
    }
    let (prefix, folder) = reference.key.rsplit_once(RELEASE_KEY_SEPARATOR)?;
    let legacy_parent = text::disc_folder(text::file_name(folder)).map(|_| text::folder(folder));
    let mut matches = catalog.releases.iter().filter(|release| {
        EntityRef::of(catalog, EntityKind::Release, release.id).is_some_and(|current| {
            let Some((current_prefix, current_folder)) =
                current.key.rsplit_once(RELEASE_KEY_SEPARATOR)
            else {
                return false;
            };
            current_prefix == prefix
                && (current_folder == folder || legacy_parent == Some(current_folder))
        })
    });
    let found = matches.next()?;
    matches.next().is_none().then_some(found.id)
}

/// Resolution for a reconciliation pass, including legacy recording aliases.
/// Building once keeps a referenced library from being walked for every record.
struct ReferenceIndex {
    current: BTreeMap<EntityRef, (Id, usize)>,
}

impl ReferenceIndex {
    fn new(catalog: &Catalog, kinds: BTreeSet<EntityKind>) -> Self {
        let mut current: BTreeMap<EntityRef, (Id, usize)> = BTreeMap::new();
        for kind in kinds {
            let ids: Vec<_> = match kind {
                EntityKind::Track => catalog.tracks.iter().map(|row| row.id).collect(),
                EntityKind::Release => catalog.releases.iter().map(|row| row.id).collect(),
                EntityKind::Artist => catalog.artists.iter().map(|row| row.id).collect(),
                EntityKind::Recording => catalog.recordings.iter().map(|row| row.id).collect(),
                EntityKind::Work => catalog.works.iter().map(|row| row.id).collect(),
                EntityKind::ReleaseGroup => {
                    catalog.release_groups.iter().map(|row| row.id).collect()
                }
                EntityKind::Label => catalog.labels.iter().map(|row| row.id).collect(),
                EntityKind::Genre => catalog.genres.iter().map(|row| row.id).collect(),
            };
            for id in ids {
                if let Some(reference) = EntityRef::of(catalog, kind, id) {
                    current
                        .entry(reference)
                        .and_modify(|value| value.1 += 1)
                        .or_insert((id, 1));
                }
            }
            if kind == EntityKind::Recording {
                for recording in &catalog.recordings {
                    for track in &recording.track_ids {
                        if let Some(file) = catalog
                            .track(*track)
                            .and_then(|track| catalog.file(track.file_id))
                        {
                            current
                                .entry(EntityRef::new(kind, format!("local:{}", file.path)))
                                .or_insert((recording.id, 1));
                        }
                    }
                }
            }
        }
        Self { current }
    }

    fn resolve(&self, target: &EntityRef) -> Option<Id> {
        if let Some((id, _)) = self.current.get(target) {
            return Some(*id);
        }
        if target.kind != EntityKind::Release {
            return None;
        }
        let (prefix, folder) = target.key.rsplit_once(RELEASE_KEY_SEPARATOR)?;
        text::disc_folder(text::file_name(folder))?;
        let parent = EntityRef::new(
            EntityKind::Release,
            format!("{prefix}{RELEASE_KEY_SEPARATOR}{}", text::folder(folder)),
        );
        self.current
            .get(&parent)
            .and_then(|(id, count)| (*count == 1).then_some(*id))
    }
}

/// Reconciles personal references without guessing from a filename alone.
///
/// Existing references capture optional file evidence. Missing tracks move only
/// to one matching basename, size, metadata and stream identity, with no personal
/// data already at the destination for that owner. Legacy references lacking
/// evidence remain waiting. Album references using a former disc folder migrate
/// to the release's canonical folder only when their resolution is unambiguous.
/// No annotation, play or counter is deleted.
pub fn reconcile(data: &mut UserData, catalog: &Catalog) -> Attachment {
    let mut report = Attachment::default();
    let mut files_by_name: BTreeMap<&str, Vec<&AudioFile>> = BTreeMap::new();
    for file in &catalog.files {
        files_by_name
            .entry(file.file_name())
            .or_default()
            .push(file);
    }
    let identities: BTreeMap<_, _> = data
        .track_identities
        .iter()
        .map(|identity| (identity.path.as_str(), identity))
        .collect();
    let existing: BTreeSet<_> = data
        .annotations
        .iter()
        .map(|a| (a.owner.clone(), a.target.clone()))
        .chain(
            data.plays
                .iter()
                .map(|p| (p.owner.clone(), p.track.clone())),
        )
        .chain(
            data.counts
                .iter()
                .map(|c| (c.owner.clone(), c.track.clone())),
        )
        .chain(
            data.scrobbles
                .iter()
                .map(|p| (p.owner.clone(), p.track.clone())),
        )
        .chain(data.playlists.iter().flat_map(|p| {
            p.tracks
                .iter()
                .map(|track| (p.owner.clone(), track.clone()))
        }))
        .chain(data.relation_annotations.iter().flat_map(|a| {
            [
                (a.owner.clone(), a.relation.source.clone()),
                (a.owner.clone(), a.relation.target.clone()),
            ]
        }))
        .collect();
    let references = ReferenceIndex::new(
        catalog,
        existing.iter().map(|(_, target)| target.kind).collect(),
    );
    let undone: BTreeSet<_> = data
        .relinks
        .iter()
        .filter(|event| event.undone_at.is_some())
        .map(|event| (&event.owner, &event.from))
        .collect();
    let mut moves = BTreeMap::new();
    for (owner, target) in &existing {
        let next = if let Some(id) = references.resolve(target) {
            EntityRef::of(catalog, target.kind, id)
        } else if target.kind == EntityKind::Track {
            if undone.contains(&(owner, target)) {
                continue;
            }
            identities.get(target.key.as_str()).and_then(|identity| {
                let mut matches = files_by_name
                    .get(text::file_name(&target.key))?
                    .iter()
                    .filter(|file| identity.matches(file));
                let candidate = matches.next()?;
                matches
                    .next()
                    .is_none()
                    .then(|| EntityRef::new(EntityKind::Track, &candidate.path))
            })
        } else {
            None
        };
        if let Some(next) = next
            && next != *target
            && !existing.contains(&(owner.clone(), next.clone()))
        {
            moves.insert((owner.clone(), target.clone()), next);
        }
    }
    // Two missing references converging on one file are just as ambiguous as
    // two candidate files. Do not manufacture duplicate owner/target records.
    let mut destinations = BTreeMap::new();
    for ((owner, _), to) in &moves {
        *destinations
            .entry((owner.clone(), to.clone()))
            .or_insert(0usize) += 1;
    }
    moves.retain(|(owner, _), to| destinations.get(&(owner.clone(), to.clone())) == Some(&1));
    for (owner, target) in data
        .annotations
        .iter_mut()
        .map(|a| (&a.owner, &mut a.target))
        .chain(data.plays.iter_mut().map(|p| (&p.owner, &mut p.track)))
        .chain(data.counts.iter_mut().map(|c| (&c.owner, &mut c.track)))
        .chain(data.scrobbles.iter_mut().map(|p| (&p.owner, &mut p.track)))
        .chain(
            data.playlists
                .iter_mut()
                .flat_map(|p| p.tracks.iter_mut().map(|track| (&p.owner, track))),
        )
    {
        if let Some(next) = moves.get(&(owner.clone(), target.clone())) {
            *target = next.clone();
            report.moved += 1;
        }
        if references.resolve(target).is_some() {
            report.attached += 1;
        } else {
            report.waiting += 1;
        }
    }
    for annotation in &mut data.relation_annotations {
        for endpoint in [
            &mut annotation.relation.source,
            &mut annotation.relation.target,
        ] {
            if let Some(next) = moves.get(&(annotation.owner.clone(), endpoint.clone())) {
                *endpoint = next.clone();
            }
        }
    }
    let paths: BTreeSet<_> = data
        .annotations
        .iter()
        .map(|a| &a.target)
        .chain(data.plays.iter().map(|p| &p.track))
        .chain(data.counts.iter().map(|c| &c.track))
        .chain(data.scrobbles.iter().map(|p| &p.track))
        .chain(data.playlists.iter().flat_map(|p| &p.tracks))
        .chain(
            data.relation_annotations
                .iter()
                .flat_map(|a| [&a.relation.source, &a.relation.target]),
        )
        .filter(|target| target.kind == EntityKind::Track)
        .map(|target| target.key.as_str())
        .collect();
    let mut identities: BTreeMap<_, _> = std::mem::take(&mut data.track_identities)
        .into_iter()
        .map(|identity| (identity.path.clone(), identity))
        .collect();
    for file in catalog
        .files
        .iter()
        .filter(|file| paths.contains(file.path.as_str()))
    {
        identities.insert(file.path.clone(), TrackIdentity::of(file));
    }
    let relink_paths: BTreeSet<_> = data
        .relinks
        .iter()
        .filter(|event| event.from.kind == EntityKind::Track)
        .flat_map(|event| [&event.from.key, &event.to.key])
        .collect();
    data.track_identities = identities
        .into_values()
        .filter(|identity| {
            paths.contains(identity.path.as_str()) || relink_paths.contains(&identity.path)
        })
        .collect();
    report
}

#[cfg(test)]
#[path = "user_identity_tests.rs"]
mod tests;
