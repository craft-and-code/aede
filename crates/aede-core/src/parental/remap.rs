//! Dense ID remapping for an in-memory, non-persisted parental subgraph.
use crate::model::{Catalog, EntityKind, Id};
use std::collections::BTreeSet;

pub(super) struct Table(Vec<Option<Id>>);
impl Table {
    fn new(len: usize) -> Self {
        Self(vec![None; len])
    }
    pub(super) fn keep(&mut self, id: Id) -> Result<(), String> {
        *self
            .0
            .get_mut(id as usize)
            .ok_or("invalid catalogue reference in parental graph")? = Some(0);
        Ok(())
    }
    pub(super) fn has(&self, id: Id) -> bool {
        self.0.get(id as usize).is_some_and(Option::is_some)
    }
    fn finish(&mut self) -> Result<(), String> {
        for (next, value) in self.0.iter_mut().flatten().enumerate() {
            *value =
                Id::try_from(next).map_err(|_| "parental graph exceeds identifier capacity")?;
        }
        Ok(())
    }
    fn get(&self, id: Id) -> Option<Id> {
        self.0.get(id as usize).copied().flatten()
    }
    fn required(&self, id: Id) -> Result<Id, String> {
        self.get(id)
            .ok_or_else(|| "parental graph lost a required reference".into())
    }
    fn list(&self, ids: &[Id]) -> Vec<Id> {
        ids.iter().filter_map(|id| self.get(*id)).collect()
    }
}
pub(super) struct Maps {
    pub(super) files: Table,
    pub(super) artists: Table,
    pub(super) releases: Table,
    pub(super) tracks: Table,
    pub(super) recordings: Table,
    pub(super) works: Table,
    pub(super) release_groups: Table,
    pub(super) labels: Table,
    pub(super) genres: Table,
}
impl Maps {
    pub(super) fn new(catalog: &Catalog) -> Result<Self, String> {
        fn dense(ids: impl Iterator<Item = Id>) -> bool {
            ids.enumerate().all(|(index, id)| id as usize == index)
        }
        if !dense(catalog.files.iter().map(|row| row.id))
            || !dense(catalog.artists.iter().map(|row| row.id))
            || !dense(catalog.releases.iter().map(|row| row.id))
            || !dense(catalog.tracks.iter().map(|row| row.id))
            || !dense(catalog.recordings.iter().map(|row| row.id))
            || !dense(catalog.works.iter().map(|row| row.id))
            || !dense(catalog.release_groups.iter().map(|row| row.id))
            || !dense(catalog.labels.iter().map(|row| row.id))
            || !dense(catalog.genres.iter().map(|row| row.id))
        {
            return Err("parental graph requires valid dense catalogue identities".into());
        }
        Ok(Self {
            files: Table::new(catalog.files.len()),
            artists: Table::new(catalog.artists.len()),
            releases: Table::new(catalog.releases.len()),
            tracks: Table::new(catalog.tracks.len()),
            recordings: Table::new(catalog.recordings.len()),
            works: Table::new(catalog.works.len()),
            release_groups: Table::new(catalog.release_groups.len()),
            labels: Table::new(catalog.labels.len()),
            genres: Table::new(catalog.genres.len()),
        })
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        for table in [
            &mut self.files,
            &mut self.artists,
            &mut self.releases,
            &mut self.tracks,
            &mut self.recordings,
            &mut self.works,
            &mut self.release_groups,
            &mut self.labels,
            &mut self.genres,
        ] {
            table.finish()?;
        }
        Ok(())
    }
    fn table(&self, kind: EntityKind) -> &Table {
        match kind {
            EntityKind::Artist => &self.artists,
            EntityKind::Release => &self.releases,
            EntityKind::Track => &self.tracks,
            EntityKind::Recording => &self.recordings,
            EntityKind::Work => &self.works,
            EntityKind::ReleaseGroup => &self.release_groups,
            EntityKind::Label => &self.labels,
            EntityKind::Genre => &self.genres,
        }
    }
    pub(super) fn kept(&self, kind: EntityKind, id: Id) -> bool {
        self.table(kind).has(id)
    }
    fn get(&self, kind: EntityKind, id: Id) -> Option<Id> {
        self.table(kind).get(id)
    }
}
pub(super) fn compact(source: &Catalog, maps: &Maps) -> Result<Catalog, String> {
    let mut result = Catalog {
        scanned_at: source.scanned_at,
        roots: source.roots.clone(),
        excluded: source.excluded.clone(),
        ..Catalog::default()
    };
    for old in source.files.iter().filter(|row| maps.files.has(row.id)) {
        let mut row = old.clone();
        row.id = maps.files.required(row.id)?;
        result.files.push(row);
    }
    for old in source.artists.iter().filter(|row| maps.artists.has(row.id)) {
        let mut row = old.clone();
        row.id = maps.artists.required(row.id)?;
        result.artists.push(row);
    }
    for old in source
        .releases
        .iter()
        .filter(|row| maps.releases.has(row.id))
    {
        let mut row = old.clone();
        row.id = maps.releases.required(row.id)?;
        row.album_artist_id = row
            .album_artist_id
            .map(|id| maps.artists.required(id))
            .transpose()?;
        row.label_ids = maps.labels.list(&row.label_ids);
        row.track_ids = maps.tracks.list(&row.track_ids);
        row.release_group_id = row
            .release_group_id
            .and_then(|id| maps.release_groups.get(id));
        result.releases.push(row);
    }
    for old in source.tracks.iter().filter(|row| maps.tracks.has(row.id)) {
        let mut row = old.clone();
        row.id = maps.tracks.required(row.id)?;
        row.file_id = maps.files.required(row.file_id)?;
        row.release_id = row
            .release_id
            .map(|id| maps.releases.required(id))
            .transpose()?;
        row.recording_id = maps.recordings.required(row.recording_id)?;
        result.tracks.push(row);
    }
    for old in source
        .recordings
        .iter()
        .filter(|row| maps.recordings.has(row.id))
    {
        let mut row = old.clone();
        row.id = maps.recordings.required(row.id)?;
        row.track_ids = maps.tracks.list(&row.track_ids);
        row.work_ids = maps.works.list(&row.work_ids);
        result.recordings.push(row);
    }
    for old in source.works.iter().filter(|row| maps.works.has(row.id)) {
        let mut row = old.clone();
        row.id = maps.works.required(row.id)?;
        row.recording_ids = maps.recordings.list(&row.recording_ids);
        result.works.push(row);
    }
    for old in source
        .release_groups
        .iter()
        .filter(|row| maps.release_groups.has(row.id))
    {
        let mut row = old.clone();
        row.id = maps.release_groups.required(row.id)?;
        row.release_ids = maps.releases.list(&row.release_ids);
        result.release_groups.push(row);
    }
    for old in source.labels.iter().filter(|row| maps.labels.has(row.id)) {
        let mut row = old.clone();
        row.id = maps.labels.required(row.id)?;
        result.labels.push(row);
    }
    for old in source.genres.iter().filter(|row| maps.genres.has(row.id)) {
        let mut row = old.clone();
        row.id = maps.genres.required(row.id)?;
        result.genres.push(row);
    }
    for old in &source.credits {
        if let (Some(artist_id), Some(entity_id)) = (
            maps.artists.get(old.artist_id),
            maps.get(old.entity_kind, old.entity_id),
        ) {
            let mut row = old.clone();
            row.artist_id = artist_id;
            row.entity_id = entity_id;
            result.credits.push(row);
        }
    }
    for old in &source.genre_links {
        if let (Some(genre_id), Some(entity_id)) = (
            maps.genres.get(old.genre_id),
            maps.get(old.entity_kind, old.entity_id),
        ) {
            let mut row = old.clone();
            row.genre_id = genre_id;
            row.entity_id = entity_id;
            result.genre_links.push(row);
        }
    }
    let paths = result
        .files
        .iter()
        .map(|row| row.path.as_str())
        .collect::<BTreeSet<_>>();
    result.file_mtime_subseconds = source
        .file_mtime_subseconds
        .iter()
        .filter(|(path, _)| paths.contains(path.as_str()))
        .map(|(path, value)| (path.clone(), *value))
        .collect();
    result.file_added_at = source
        .file_added_at
        .iter()
        .filter(|(path, _)| paths.contains(path.as_str()))
        .map(|(path, value)| (path.clone(), *value))
        .collect();
    result.analyses = source
        .analyses
        .iter()
        .filter(|row| paths.contains(row.path.as_str()))
        .cloned()
        .collect();
    crate::model::rebuild_relations(&mut result);
    for old in source.relations.iter().filter(|row| row.source != "tags") {
        if let (Some(source_id), Some(target_id)) = (
            maps.get(old.source_kind, old.source_id),
            maps.get(old.target_kind, old.target_id),
        ) {
            let mut row = old.clone();
            row.source_id = source_id;
            row.target_id = target_id;
            result.relations.push(row);
        }
    }
    Ok(result)
}
