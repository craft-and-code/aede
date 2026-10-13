//! Explainable atmosphere selection from normalized owner tags and catalog genres.
//! No network, model, decoder or mastering-to-emotion inference is involved.
#![warn(missing_docs)]
mod selection;
pub use selection::{TagScope, TagSelector};
/// One configurable OR rule over complete normalized tags/genres.
#[derive(Clone, Debug)]
pub struct Rule {
    /// Stable rule identity, independent of its display name.
    pub id: String,
    /// User-facing name; a presentation client localizes built-in IDs.
    pub name: String,
    /// Normalized exact tag spellings.
    pub tags: Vec<String>,
    /// Normalized exact genre spellings.
    pub genres: Vec<String>,
}
/// Evidence for a selected track, retained for explanation.
#[derive(Debug, PartialEq, Eq)]
pub struct Match {
    /// Matching exact tag, if available.
    pub tag: Option<String>,
    /// Matching exact genre, if available.
    pub genre: Option<String>,
}
impl Rule {
    /// Refuse unbounded or empty rules; callers normalize with their catalog's text policy.
    pub fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 128
            && !self.name.trim().is_empty()
            && self.name.len() <= 256
            && self.tags.len() + self.genres.len() > 0
            && self.tags.len() <= 32
            && self.genres.len() <= 32
            && self
                .tags
                .iter()
                .chain(&self.genres)
                .all(|value| !value.is_empty() && value.len() <= 256)
    }
    /// Exact matches avoid guessing that a substring or unrelated guest describes the track.
    pub fn matches(&self, tags: &[String], genres: &[String]) -> Option<Match> {
        let tag = self.tags.iter().find(|value| tags.contains(value)).cloned();
        let genre = self
            .genres
            .iter()
            .find(|value| genres.contains(value))
            .cloned();
        (tag.is_some() || genre.is_some()).then_some(Match { tag, genre })
    }
}
/// Transparent starter recipes; they are suggestions rather than measured emotions.
pub fn presets() -> Vec<Rule> {
    [
        (
            "calm",
            "Relaxation",
            vec!["calm", "calme", "relaxation", "relax"],
            vec!["ambient", "downtempo", "new age"],
        ),
        (
            "energy",
            "Energy",
            vec!["energy", "energie", "sport", "workout", "intense"],
            vec!["metal", "hard rock", "punk"],
        ),
        (
            "focus",
            "Focus",
            vec!["focus", "concentration", "study", "travail", "instrumental"],
            vec!["minimal"],
        ),
        (
            "party",
            "Party",
            vec!["party", "fete", "soiree", "dance"],
            vec!["dance", "disco"],
        ),
        (
            "melancholy",
            "Melancholy",
            vec!["melancholy", "melancolie", "sad", "triste"],
            vec![],
        ),
    ]
    .into_iter()
    .map(|(id, name, tags, genres)| Rule {
        id: id.into(),
        name: name.into(),
        tags: tags.into_iter().map(String::from).collect(),
        genres: genres.into_iter().map(String::from).collect(),
    })
    .collect()
}
#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
