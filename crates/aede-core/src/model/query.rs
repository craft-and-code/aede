//! Reading the graph.
//!
//! Every question a command asks of the catalog lands here, and nothing in this
//! file changes anything: the whole module takes `&self`. That is the point of
//! keeping it apart from `build` — a query that had to mutate would be a design
//! mistake, and here it would not even compile.
//!
//! Two rules run through all of it. A lookup that can match several things
//! returns all of them together with how it matched ([`TitleMatch`]), because a
//! command must never pick one answer out of several in silence. And a figure
//! about an artist says which class of role it counts: being audible on a
//! record and having written it are not the same fact.

use std::collections::{BTreeMap, BTreeSet};

use crate::text;

use super::{
    Artist, AudioFile, Catalog, EntityKind, Genre, Id, Label, Release, Track, is_performing_role,
};

impl Catalog {
    /// The artist an id designates, or `None` when the id is out of range.
    pub fn artist(&self, id: Id) -> Option<&Artist> {
        self.artists.get(id as usize)
    }

    /// The release an id designates, or `None` when the id is out of range.
    pub fn release(&self, id: Id) -> Option<&Release> {
        self.releases.get(id as usize)
    }

    /// The track an id designates, or `None` when the id is out of range.
    pub fn track(&self, id: Id) -> Option<&Track> {
        self.tracks.get(id as usize)
    }

    /// The file an id designates, or `None` when the id is out of range.
    pub fn file(&self, id: Id) -> Option<&AudioFile> {
        self.files.get(id as usize)
    }

    /// Every imported analysis that describes this file, whatever the source.
    pub fn analyses_of(
        &self,
        file: &AudioFile,
    ) -> impl Iterator<Item = &crate::analysis::FileAnalysis> {
        self.analyses.iter().filter(|a| a.path == file.path)
    }

    /// Imported analyses describing files the catalog does not hold.
    ///
    /// Not an error and not garbage: the usual reason is that the folder they
    /// speak of has not been scanned yet. Counting them is how the interface
    /// can say "twelve analyses are waiting for a scan" instead of losing them
    /// without a word.
    pub fn pending_analyses(&self) -> usize {
        let known: std::collections::BTreeSet<&str> =
            self.files.iter().map(|f| f.path.as_str()).collect();
        self.analyses
            .iter()
            .filter(|a| !known.contains(a.path.as_str()))
            .count()
    }

    /// The label an id designates, or `None` when the id is out of range.
    pub fn label(&self, id: Id) -> Option<&Label> {
        self.labels.get(id as usize)
    }

    /// The genre an id designates, or `None` when the id is out of range.
    pub fn genre(&self, id: Id) -> Option<&Genre> {
        self.genres.get(id as usize)
    }

    /// Total duration of the library.
    pub fn total_duration_ms(&self) -> u64 {
        self.tracks.iter().filter_map(|t| t.duration_ms).sum()
    }

    /// Total size on disk.
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// The artist's own discography: releases they are the album artist of.
    ///
    /// This is what a listener means by "their albums". A guest appearance on
    /// somebody else's record belongs in [`Catalog::guest_appearances`].
    pub fn releases_as_album_artist(&self, artist_id: Id) -> Vec<Id> {
        self.releases
            .iter()
            .filter(|r| r.album_artist_id == Some(artist_id))
            .map(|r| r.id)
            .collect()
    }

    /// Releases the artist is audible on without being the album artist:
    /// featured vocals, a guest solo, one track on a compilation.
    pub fn guest_appearances(&self, artist_id: Id) -> Vec<Id> {
        let own: BTreeSet<Id> = self
            .releases_as_album_artist(artist_id)
            .into_iter()
            .collect();
        self.releases_for(artist_id, true)
            .into_iter()
            .filter(|id| !own.contains(id))
            .collect()
    }

    /// Releases where the artist holds a credit that is not a performance.
    ///
    /// **All of them**, including the ones they also play on. Ozzy Osbourne
    /// composes most of what he sings: a figure that subtracted those would
    /// report one album where the credits count sixty-nine, and the number
    /// would be right about a set nobody asked for.
    pub fn releases_with_writing_credit(&self, artist_id: Id) -> Vec<Id> {
        self.releases_for(artist_id, false)
    }

    /// Tracks where the artist holds a credit that is not a performance.
    pub fn writing_tracks_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let mut set = BTreeSet::new();
        for credit in self.credits.iter() {
            if credit.artist_id == artist_id
                && credit.entity_kind == EntityKind::Track
                && !is_performing_role(&credit.role)
            {
                set.insert(credit.entity_id);
            }
        }
        set.into_iter().collect()
    }

    /// Releases the artist wrote or produced **and is not audible on**.
    ///
    /// Not a measure of anything — a display set. It exists so the artist page
    /// can show the records that would otherwise never appear on it, without
    /// repeating the discography above. Never label it "writing": that is
    /// [`Catalog::releases_with_writing_credit`].
    pub fn releases_written_without_performing(&self, artist_id: Id) -> Vec<Id> {
        let heard: BTreeSet<Id> = self
            .releases_as_album_artist(artist_id)
            .into_iter()
            .chain(self.guest_appearances(artist_id))
            .collect();
        self.releases_for(artist_id, false)
            .into_iter()
            .filter(|id| !heard.contains(id))
            .collect()
    }

    /// Releases reached through the artist's credits, keeping either the
    /// performing roles or the writing ones.
    fn releases_for(&self, artist_id: Id, performing: bool) -> Vec<Id> {
        let mut out = BTreeSet::new();
        for credit in self.credits.iter().filter(|c| c.artist_id == artist_id) {
            if is_performing_role(&credit.role) != performing {
                continue;
            }
            let release_id = match credit.entity_kind {
                EntityKind::Release => Some(credit.entity_id),
                EntityKind::Track => self.track(credit.entity_id).and_then(|t| t.release_id),
                _ => None,
            };
            if let Some(id) = release_id {
                out.insert(id);
            }
        }
        out.into_iter().collect()
    }

    /// Tracks the artist is audible on, without duplicates.
    pub fn performed_tracks_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let mut set = BTreeSet::new();
        for credit in self.credits.iter() {
            if credit.artist_id == artist_id
                && credit.entity_kind == EntityKind::Track
                && is_performing_role(&credit.role)
            {
                set.insert(credit.entity_id);
            }
        }
        set.into_iter().collect()
    }

    /// Tracks the artist wrote or produced **and is not audible on**.
    ///
    /// The display counterpart of [`Catalog::performed_tracks_of_artist`]:
    /// together they cover every track credit without counting one twice, a
    /// track where someone both plays and composes going to the first. For
    /// "how much did this person write", use
    /// [`Catalog::writing_tracks_of_artist`].
    pub fn written_tracks_without_performing(&self, artist_id: Id) -> Vec<Id> {
        let performed: BTreeSet<Id> = self
            .performed_tracks_of_artist(artist_id)
            .into_iter()
            .collect();
        let mut set = BTreeSet::new();
        for credit in self.credits.iter() {
            if credit.artist_id == artist_id
                && credit.entity_kind == EntityKind::Track
                && !is_performing_role(&credit.role)
                && !performed.contains(&credit.entity_id)
            {
                set.insert(credit.entity_id);
            }
        }
        set.into_iter().collect()
    }

    /// Every release the artist appears on, whatever their role.
    pub fn releases_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let mut set = BTreeSet::new();
        for credit in self.credits.iter().filter(|c| c.artist_id == artist_id) {
            match credit.entity_kind {
                EntityKind::Release => {
                    set.insert(credit.entity_id);
                }
                EntityKind::Track => {
                    if let Some(release_id) =
                        self.track(credit.entity_id).and_then(|t| t.release_id)
                    {
                        set.insert(release_id);
                    }
                }
                _ => {}
            }
        }
        set.into_iter().collect()
    }

    /// Tracks credited to an artist, without duplicates.
    ///
    /// A single artist may hold several roles on one track (performer **and**
    /// composer): without deduplication, every count would be inflated.
    pub fn tracks_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let mut set = BTreeSet::new();
        for credit in self.credits.iter() {
            if credit.artist_id == artist_id && credit.entity_kind == EntityKind::Track {
                set.insert(credit.entity_id);
            }
        }
        set.into_iter().collect()
    }

    /// Artists credited on an entity, along with their role.
    pub fn credits_on(&self, kind: EntityKind, id: Id) -> Vec<(&Artist, &str)> {
        self.credits
            .iter()
            .filter(|c| c.entity_kind == kind && c.entity_id == id)
            .filter_map(|c| self.artist(c.artist_id).map(|a| (a, c.role.as_str())))
            .collect()
    }

    /// An artist's neighbours in the graph, from strongest link to weakest.
    pub fn neighbours_of_artist(&self, artist_id: Id) -> Vec<(&Artist, u32, &str)> {
        let mut out: Vec<(&Artist, u32, &str)> = self
            .relations
            .iter()
            .filter(|r| r.source_kind == EntityKind::Artist && r.source_id == artist_id)
            .filter_map(|r| {
                self.artist(r.target_id)
                    .map(|a| (a, r.weight, r.kind.as_str()))
            })
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
        out
    }

    /// Tracks on which two artists are both credited in a performing role.
    ///
    /// This is what the weight of a `collaborated` relation counts, recomputed
    /// on demand rather than stored: the `credit` table already holds the
    /// answer, and a second copy is a second thing that can fall out of step.
    pub fn tracks_in_common(&self, a: Id, b: Id) -> Vec<Id> {
        let performing_on = |artist: Id| -> BTreeSet<Id> {
            self.credits
                .iter()
                .filter(|c| {
                    c.entity_kind == EntityKind::Track
                        && c.artist_id == artist
                        && is_performing_role(&c.role)
                })
                .map(|c| c.entity_id)
                .collect()
        };
        let (left, right) = (performing_on(a), performing_on(b));
        left.intersection(&right).copied().collect()
    }

    /// Releases tied to this one by a relation of the given kind.
    ///
    /// Used with [`crate::model::DUPLICATE`] and [`crate::model::OTHER_EDITION`]
    /// to answer "is this album here twice, and on purpose?".
    pub fn related_releases(&self, release_id: Id, kind: &str) -> Vec<Id> {
        self.relations
            .iter()
            .filter(|r| {
                r.source_kind == EntityKind::Release && r.source_id == release_id && r.kind == kind
            })
            .map(|r| r.target_id)
            .collect()
    }

    /// Genres attached to an entity.
    pub fn genres_of(&self, kind: EntityKind, id: Id) -> Vec<&Genre> {
        self.genre_links
            .iter()
            .filter(|g| g.entity_kind == kind && g.entity_id == id)
            .filter_map(|g| self.genre(g.genre_id))
            .collect()
    }

    /// Case- and accent-insensitive search, across every entity.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let needle = text::normalize(query);
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<SearchHit> = Vec::new();

        let mut push = |kind: EntityKind, id: Id, name: &str, key: &str, detail: String| {
            let score = match () {
                _ if key == needle => 0u8,
                _ if key.starts_with(&needle) => 1,
                _ if key.contains(&needle) => 2,
                _ => return,
            };
            hits.push(SearchHit {
                kind,
                id,
                name: name.to_string(),
                detail,
                score,
            });
        };

        for a in &self.artists {
            push(EntityKind::Artist, a.id, &a.name, &a.key, String::new());
        }
        for r in &self.releases {
            let detail = r
                .album_artist_id
                .and_then(|id| self.artist(id))
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Various Artists".to_string());
            push(EntityKind::Release, r.id, &r.title, &r.key, detail);
        }
        for t in &self.tracks {
            let key = text::normalize(&t.title);
            let detail = t
                .release_id
                .and_then(|id| self.release(id))
                .map(|r| r.title.clone())
                .unwrap_or_default();
            push(EntityKind::Track, t.id, &t.title, &key, detail);
        }
        for l in &self.labels {
            push(EntityKind::Label, l.id, &l.name, &l.key, String::new());
        }

        hits.sort_by(|a, b| {
            a.score
                .cmp(&b.score)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.name.len().cmp(&b.name.len()))
                .then_with(|| a.name.cmp(&b.name))
        });
        hits.truncate(limit);
        hits
    }

    /// Finds an artist by name, up to normalization.
    pub fn find_artist(&self, name: &str) -> Option<&Artist> {
        let key = text::normalize(name);
        self.artists.iter().find(|a| a.key == key)
    }

    /// Every release whose title matches, exactly or failing that partially.
    ///
    /// Same rule as [`Catalog::find_tracks`], and for the same reason: a
    /// command must not pick one answer out of several without saying so.
    /// Unlike tracks, though, two matching albums are two *different* albums —
    /// a shared prefix is not an ambiguity — which is why the exact match is
    /// what usually ends the search.
    pub fn find_releases(&self, title: &str) -> (Vec<&Release>, TitleMatch) {
        let key = text::normalize(title);
        if key.is_empty() {
            return (Vec::new(), TitleMatch::Exact);
        }
        let exact: Vec<&Release> = self.releases.iter().filter(|r| r.key == key).collect();
        if !exact.is_empty() {
            return (exact, TitleMatch::Exact);
        }
        let partial = self
            .releases
            .iter()
            .filter(|r| r.key.contains(&key))
            .collect();
        (partial, TitleMatch::Partial)
    }

    /// Every track carrying this title, up to normalization.
    ///
    /// A title is not an identifier: the same one legitimately comes back on
    /// the album, on a single and on a live record, and those are different
    /// recordings. All of them are returned, in catalog order.
    ///
    /// Exact matches win. Only when there is none does the search widen to the
    /// titles containing the text, so that a half-remembered title still leads
    /// somewhere; [`TitleMatch`] says which of the two happened.
    pub fn find_tracks(&self, title: &str) -> (Vec<&Track>, TitleMatch) {
        let key = text::normalize(title);
        if key.is_empty() {
            return (Vec::new(), TitleMatch::Exact);
        }
        let exact: Vec<&Track> = self
            .tracks
            .iter()
            .filter(|t| text::normalize(&t.title) == key)
            .collect();
        if !exact.is_empty() {
            return (exact, TitleMatch::Exact);
        }
        let partial = self
            .tracks
            .iter()
            .filter(|t| text::normalize(&t.title).contains(&key))
            .collect();
        (partial, TitleMatch::Partial)
    }

    /// Every genre whose name matches, exactly or failing that partially.
    ///
    /// Same rule as [`Catalog::find_releases`]: "metal" must reach the genre
    /// spelled exactly that way without hiding "Black Metal" and "Doom Metal"
    /// when there is no exact one.
    pub fn find_genres(&self, name: &str) -> (Vec<&Genre>, TitleMatch) {
        let key = text::normalize(name);
        if key.is_empty() {
            return (Vec::new(), TitleMatch::Exact);
        }
        let exact: Vec<&Genre> = self.genres.iter().filter(|g| g.key == key).collect();
        if !exact.is_empty() {
            return (exact, TitleMatch::Exact);
        }
        (
            self.genres
                .iter()
                .filter(|g| g.key.contains(&key))
                .collect(),
            TitleMatch::Partial,
        )
    }

    /// Every label whose name matches, exactly or failing that partially.
    pub fn find_labels(&self, name: &str) -> (Vec<&Label>, TitleMatch) {
        let key = text::normalize(name);
        if key.is_empty() {
            return (Vec::new(), TitleMatch::Exact);
        }
        let exact: Vec<&Label> = self.labels.iter().filter(|l| l.key == key).collect();
        if !exact.is_empty() {
            return (exact, TitleMatch::Exact);
        }
        (
            self.labels
                .iter()
                .filter(|l| l.key.contains(&key))
                .collect(),
            TitleMatch::Partial,
        )
    }

    /// Tracks carrying a genre, directly or through their release.
    ///
    /// A genre attached to an album is a genre of every track on it: tags put
    /// it in either place, and a listener does not think of the difference.
    pub fn tracks_of_genre(&self, genre_id: Id) -> Vec<Id> {
        let mut tracks: BTreeSet<Id> = BTreeSet::new();
        for link in &self.genre_links {
            if link.genre_id != genre_id {
                continue;
            }
            match link.entity_kind {
                EntityKind::Track => {
                    tracks.insert(link.entity_id);
                }
                EntityKind::Release => {
                    if let Some(release) = self.release(link.entity_id) {
                        tracks.extend(release.track_ids.iter().copied());
                    }
                }
                EntityKind::Artist | EntityKind::Label => {}
            }
        }
        tracks.into_iter().collect()
    }

    /// Releases holding at least one track of a genre, or carrying it whole.
    pub fn releases_of_genre(&self, genre_id: Id) -> Vec<Id> {
        let tracks: BTreeSet<Id> = self.tracks_of_genre(genre_id).into_iter().collect();
        let mut releases: BTreeSet<Id> = BTreeSet::new();
        for track in self.tracks.iter().filter(|t| tracks.contains(&t.id)) {
            if let Some(id) = track.release_id {
                releases.insert(id);
            }
        }
        releases.into_iter().collect()
    }

    /// Tracks published under a label, through the releases carrying it.
    pub fn tracks_of_label(&self, label_id: Id) -> Vec<Id> {
        self.releases
            .iter()
            .filter(|r| r.label_ids.contains(&label_id))
            .flat_map(|r| r.track_ids.iter().copied())
            .collect()
    }

    /// Releases published under a label.
    pub fn releases_of_label(&self, label_id: Id) -> Vec<Id> {
        self.releases
            .iter()
            .filter(|r| r.label_ids.contains(&label_id))
            .map(|r| r.id)
            .collect()
    }

    /// Everyone credited in a role, with how many credits each holds.
    ///
    /// This is the inverse of the artist page: that one answers "what did this
    /// person do", this one answers "who does this in my library". The whole
    /// point of storing roles rather than a bare artist column is that the
    /// question can be asked in both directions.
    ///
    /// Sorted by number of credits, then by sort name, so the answer is stable
    /// and the most present come first.
    pub fn artists_in_role(&self, role: &str) -> Vec<(Id, usize)> {
        let mut counts: BTreeMap<Id, usize> = BTreeMap::new();
        for credit in self.credits.iter().filter(|c| c.role == role) {
            *counts.entry(credit.artist_id).or_insert(0) += 1;
        }
        let mut rows: Vec<(Id, usize)> = counts.into_iter().collect();
        rows.sort_by(|a, b| {
            b.1.cmp(&a.1).then_with(|| {
                self.artist(a.0)
                    .map(|x| x.sort_name.as_str())
                    .cmp(&self.artist(b.0).map(|x| x.sort_name.as_str()))
            })
        });
        rows
    }

    /// Tracks on which an artist holds one particular role.
    ///
    /// The other direction of [`Catalog::artists_in_role`], one person at a
    /// time: not "who produces here" but "what did this person produce". The
    /// artist page separates performing from writing; this goes one step
    /// finer, down to the single role.
    ///
    /// `album` is credited on the **release**, not on its tracks, so asking
    /// for it yields every track of the releases they sign — otherwise the
    /// answer would be empty for the one role every album artist holds.
    pub fn tracks_of_artist_in_role(&self, artist_id: Id, role: &str) -> Vec<Id> {
        let mut tracks: BTreeSet<Id> = BTreeSet::new();
        for credit in self
            .credits
            .iter()
            .filter(|c| c.artist_id == artist_id && c.role == role)
        {
            match credit.entity_kind {
                EntityKind::Track => {
                    tracks.insert(credit.entity_id);
                }
                EntityKind::Release => {
                    if let Some(release) = self.release(credit.entity_id) {
                        tracks.extend(release.track_ids.iter().copied());
                    }
                }
                EntityKind::Artist | EntityKind::Label => {}
            }
        }
        tracks.into_iter().collect()
    }

    /// The roles one artist holds, with how many credits each.
    ///
    /// What makes an empty answer readable: told that nobody is credited a
    /// given way, the user has to be able to see what this person *is*
    /// credited as.
    pub fn roles_of_artist(&self, artist_id: Id) -> Vec<(&str, usize)> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for credit in self.credits.iter().filter(|c| c.artist_id == artist_id) {
            *counts.entry(credit.role.as_str()).or_insert(0) += 1;
        }
        let mut rows: Vec<(&str, usize)> = counts.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        rows
    }

    /// Every role actually used in this catalog, in the order they are shown.
    ///
    /// Read from the credits rather than from a fixed list: a role that came
    /// from a tag nobody anticipated is still a role, and it must be offerable.
    pub fn roles_in_use(&self) -> Vec<&str> {
        let mut roles: BTreeSet<&str> = BTreeSet::new();
        for credit in &self.credits {
            roles.insert(credit.role.as_str());
        }
        roles.into_iter().collect()
    }

    /// Tracks whose file carries a comment containing this text.
    ///
    /// The comment belongs to the **file**, not to the track: it is free text a
    /// user wrote, and it survives in the raw tags exactly as it was typed.
    /// Matching is done on the normalized form, so accents and case are no
    /// obstacle — a comment is prose, and prose is typed carelessly.
    pub fn tracks_with_comment(&self, text: &str) -> Vec<Id> {
        let needle = text::normalize(text);
        if needle.is_empty() {
            return Vec::new();
        }
        self.tracks
            .iter()
            .filter(|t| {
                self.file(t.file_id)
                    .and_then(|f| f.first_tag("comment"))
                    .map(|c| text::normalize(c).contains(&needle))
                    .unwrap_or(false)
            })
            .map(|t| t.id)
            .collect()
    }

    /// The comment carried by the file behind a track, when there is one.
    pub fn comment_of_track(&self, track_id: Id) -> Option<&str> {
        let track = self.track(track_id)?;
        self.file(track.file_id)?.first_tag("comment")
    }
}

/// How [`Catalog::find_tracks`] reached its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleMatch {
    /// The titles are the given one, up to normalization.
    Exact,
    /// No title matched, so the ones containing the text were returned.
    Partial,
}

/// One answer returned by [`Catalog::search`], ready to be shown and followed.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Table the hit lives in, which tells the caller where to navigate.
    pub kind: EntityKind,
    /// Identifier within that table.
    pub id: Id,
    /// Display form, unnormalized, as it should appear on screen.
    pub name: String,
    /// Context shown next to the name (album artist, track's album…).
    pub detail: String,
    score: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::build;
    use crate::model::tests::{example_catalog, first_release, track};

    #[test]
    fn the_shared_tracks_match_the_collaboration_weight() {
        // The weight of a `collaborated` relation is a count; the tracks it
        // counts must be reachable, or the graph cannot be walked.
        let c = example_catalog();
        let garou = c.find_artist("Garou").expect("Garou present");
        let celine = c.find_artist("Céline Dion").expect("Céline Dion present");
        let shared = c.tracks_in_common(garou.id, celine.id);
        let (_, weight, _) = c.neighbours_of_artist(garou.id)[0];
        assert_eq!(shared.len() as u32, weight, "count and list must agree");
        assert_eq!(
            c.track(shared[0]).map(|t| t.title.as_str()),
            Some("Sous le vent")
        );
        // A composer credit is not a collaboration, and neither is being alone.
        assert!(c.tracks_in_common(garou.id, garou.id).len() == 1);
    }

    #[test]
    fn an_exact_album_title_wins_over_the_ones_beginning_with_it() {
        // "Danzig" used to match "Danzig 4" and return whichever came first in
        // the catalog: an arbitrary answer, given without saying so.
        let album = |folder: &str, title: &'static str| {
            track(
                &format!("{folder}/01.flac"),
                &[
                    ("title", "A song"),
                    ("artist", "Danzig"),
                    ("albumartist", "Danzig"),
                    ("album", title),
                ],
                120_000,
            )
        };
        let c = build(
            vec![
                album("/m/4", "Danzig 4"),
                album("/m/1", "Danzig"),
                album("/m/2", "Danzig II: Lucifuge"),
            ],
            vec!["/m".into()],
            0,
        );

        let (exact, kind) = c.find_releases("Danzig");
        assert_eq!(kind, TitleMatch::Exact);
        assert_eq!(exact.len(), 1, "one album is titled exactly that");
        assert_eq!(exact[0].title, "Danzig");
        assert_eq!(
            first_release(&c, "Danzig").map(|r| r.title.as_str()),
            Some("Danzig")
        );

        // With no exact title, every match is returned rather than one of them.
        let (partial, kind) = c.find_releases("danzig i");
        assert_eq!(kind, TitleMatch::Partial);
        assert_eq!(partial.len(), 1);
        // Normalization trims the query, so a trailing space is not a way to
        // ask for "the longer titles"; a fragment is.
        let (several, kind) = c.find_releases("anzig");
        assert_eq!(kind, TitleMatch::Partial);
        assert_eq!(several.len(), 3, "every title containing it");
    }

    #[test]
    fn every_track_sharing_a_title_is_returned() {
        // The studio version and the live one are two different recordings
        // that happen to share a name; neither may hide the other.
        let c = build(
            vec![
                track(
                    "/m/a/01 Ride the Lightning.flac",
                    &[
                        ("title", "Ride the Lightning"),
                        ("artist", "Metallica"),
                        ("album", "Ride the Lightning"),
                    ],
                    120_000,
                ),
                track(
                    "/m/b/03 Ride the Lightning.flac",
                    &[
                        ("title", "Ride the Lightning"),
                        ("artist", "Metallica"),
                        ("album", "Live Shit"),
                    ],
                    400_000,
                ),
            ],
            vec!["/m".to_string()],
            0,
        );
        let (found, kind) = c.find_tracks("ride the lightning");
        assert_eq!(kind, TitleMatch::Exact, "the title matches as written");
        assert_eq!(found.len(), 2, "both recordings are returned");
        let durations: Vec<Option<u64>> = found.iter().map(|t| t.duration_ms).collect();
        assert!(durations.contains(&Some(400_000)), "the live one is there");
    }

    #[test]
    fn a_partial_title_widens_the_search_only_as_a_last_resort() {
        let c = example_catalog();
        let (exact, kind) = c.find_tracks("Fight Fire with Fire");
        assert_eq!(kind, TitleMatch::Exact);
        assert_eq!(exact.len(), 1);

        let (partial, kind) = c.find_tracks("fight fire");
        assert_eq!(kind, TitleMatch::Partial, "no title is exactly that");
        assert_eq!(partial.len(), 1);

        let (nothing, _) = c.find_tracks("nothing of the sort");
        assert!(nothing.is_empty());
    }

    #[test]
    fn tracks_are_ordered_within_the_release() {
        let c = example_catalog();
        let album = first_release(&c, "Ride the Lightning").expect("album found");
        let titles: Vec<&str> = album
            .track_ids
            .iter()
            .map(|&id| c.track(id).unwrap().title.as_str())
            .collect();
        assert_eq!(titles, ["Fight Fire with Fire", "Ride the Lightning"]);
        assert_eq!(album.year, Some(1984));
    }

    #[test]
    fn a_guest_appearance_is_not_part_of_the_discography() {
        // Exactly the shape that made "The Sinister Urge" show up under Ozzy
        // Osbourne: he sings one track on a Rob Zombie album.
        let c = build(
            vec![
                track(
                    "/m/Rob Zombie/The Sinister Urge/04 Never Gonna Stop.flac",
                    &[
                        ("title", "Never Gonna Stop"),
                        ("artist", "Rob Zombie"),
                        ("albumartist", "Rob Zombie"),
                        ("album", "The Sinister Urge"),
                        ("date", "2001"),
                    ],
                    60_000,
                ),
                track(
                    "/m/Rob Zombie/The Sinister Urge/05 Iron Head.flac",
                    &[
                        ("title", "Iron Head"),
                        ("artist", "Rob Zombie"),
                        ("performer", "Ozzy Osbourne"),
                        ("albumartist", "Rob Zombie"),
                        ("album", "The Sinister Urge"),
                        ("date", "2001"),
                    ],
                    60_000,
                ),
                track(
                    "/m/Ozzy Osbourne/Blizzard of Ozz/01 I Dont Know.flac",
                    &[
                        ("title", "I Don't Know"),
                        ("artist", "Ozzy Osbourne"),
                        ("albumartist", "Ozzy Osbourne"),
                        ("album", "Blizzard of Ozz"),
                        ("date", "1980"),
                    ],
                    60_000,
                ),
            ],
            vec!["/m".into()],
            0,
        );

        let ozzy = c
            .find_artist("Ozzy Osbourne")
            .expect("Ozzy is in the catalog");
        let own: Vec<&str> = c
            .releases_as_album_artist(ozzy.id)
            .iter()
            .filter_map(|&id| c.release(id))
            .map(|r| r.title.as_str())
            .collect();
        assert_eq!(
            own,
            ["Blizzard of Ozz"],
            "the discography holds his own albums only"
        );

        let guest: Vec<&str> = c
            .guest_appearances(ozzy.id)
            .iter()
            .filter_map(|&id| c.release(id))
            .map(|r| r.title.as_str())
            .collect();
        assert_eq!(
            guest,
            ["The Sinister Urge"],
            "the guest album is listed apart"
        );

        // The old, undifferentiated view still returns both.
        assert_eq!(c.releases_of_artist(ozzy.id).len(), 2);
    }

    #[test]
    fn a_writing_credit_is_neither_discography_nor_appearance() {
        let c = build(
            vec![track(
                "/m/Ozzy Osbourne/Blizzard of Ozz/01 Crazy Train.flac",
                &[
                    ("title", "Crazy Train"),
                    ("artist", "Ozzy Osbourne"),
                    ("albumartist", "Ozzy Osbourne"),
                    ("album", "Blizzard of Ozz"),
                    ("composer", "Randy Rhoads"),
                    ("date", "1980"),
                ],
                60_000,
            )],
            vec!["/m".into()],
            0,
        );

        let rhoads = c
            .find_artist("Randy Rhoads")
            .expect("the composer is an entity");
        assert!(c.releases_as_album_artist(rhoads.id).is_empty());
        assert!(c.guest_appearances(rhoads.id).is_empty());
        assert_eq!(c.releases_written_without_performing(rhoads.id).len(), 1);
        assert!(c.performed_tracks_of_artist(rhoads.id).is_empty());
    }

    #[test]
    fn what_someone_wrote_is_counted_even_when_they_play_it_too() {
        // Ozzy Osbourne, sixty-nine composer credits and sixty-eight as
        // lyricist, was announced on his own page as writing one track: the
        // figure reported the size of a display table, which leaves out
        // everything he also sings on. A number that answers a narrower
        // question than its label is worse than no number.
        let c = build(
            vec![track(
                "/m/Ozzy Osbourne/Blizzard of Ozz/01 Crazy Train.flac",
                &[
                    ("title", "Crazy Train"),
                    ("artist", "Ozzy Osbourne"),
                    ("albumartist", "Ozzy Osbourne"),
                    ("album", "Blizzard of Ozz"),
                    ("composer", "Ozzy Osbourne"),
                    ("date", "1980"),
                ],
                60_000,
            )],
            vec!["/m".into()],
            0,
        );
        let ozzy = c.find_artist("Ozzy Osbourne").expect("the artist");

        // He sings it and he wrote it: both are true, and both are counted.
        assert_eq!(c.performed_tracks_of_artist(ozzy.id).len(), 1);
        assert_eq!(
            c.writing_tracks_of_artist(ozzy.id).len(),
            1,
            "writing counts what he wrote, not what he wrote and does not play"
        );
        assert_eq!(c.releases_with_writing_credit(ozzy.id).len(), 1);

        // The display set is the one that subtracts, and it is empty here:
        // the album is already in the discography above it.
        assert!(c.written_tracks_without_performing(ozzy.id).is_empty());
        assert!(c.releases_written_without_performing(ozzy.id).is_empty());

        // The property that makes the page consistent: every non-performing
        // credit the artist holds on a track is in the writing set. This is
        // what the summary line and the Roles panel are both read from, so
        // they cannot contradict one another again.
        let written: std::collections::BTreeSet<Id> =
            c.writing_tracks_of_artist(ozzy.id).into_iter().collect();
        for credit in c.credits.iter().filter(|x| x.artist_id == ozzy.id) {
            if credit.entity_kind == EntityKind::Track && !is_performing_role(&credit.role) {
                assert!(
                    written.contains(&credit.entity_id),
                    "a {} credit is missing from the writing set",
                    credit.role
                );
            }
        }
    }

    #[test]
    fn navigation_from_artist_to_albums() {
        let c = example_catalog();
        let metallica = c.find_artist("Metallica").unwrap();
        let albums = c.releases_of_artist(metallica.id);
        assert_eq!(albums.len(), 1);
        assert_eq!(c.release(albums[0]).unwrap().title, "Ride the Lightning");
    }

    #[test]
    fn search_is_accent_insensitive() {
        let c = example_catalog();
        let hits = c.search("celine", 10);
        assert!(
            hits.iter().any(|h| h.name == "Céline Dion"),
            "got: {hits:?}"
        );
    }

    #[test]
    fn genres_attached_to_track_and_album() {
        let c = example_catalog();
        let album = first_release(&c, "Ride the Lightning").unwrap();
        let genres = c.genres_of(EntityKind::Release, album.id);
        assert_eq!(genres.len(), 1);
        assert_eq!(genres[0].name, "Thrash Metal");
    }
}
