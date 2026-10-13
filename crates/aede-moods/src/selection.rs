//! Prefer specific personal mood tags over inherited classifications.
use std::collections::BTreeSet;

use crate::Rule;

/// Most specific target carrying a tag recognized by an available owner rule.
/// Ordering also defines stable result priority; these are evidence levels, not emotional scores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TagScope {
    /// Explicit track annotation.
    Track,
    /// Annotation inherited from its album.
    Album,
    /// Annotation inherited from its principal album artist.
    Artist,
    /// Catalog genre fallback, when no recognized personal tag exists.
    Genre,
}

/// Index the owner's rule vocabulary once per catalogue projection.
pub struct TagSelector {
    recognized: BTreeSet<String>,
}
impl TagSelector {
    /// Include custom and overridden recipes so arbitrary unrelated tags cannot block fallback.
    pub fn new(rules: &[Rule]) -> Self {
        Self {
            recognized: rules
                .iter()
                .flat_map(|rule| rule.tags.iter().cloned())
                .collect(),
        }
    }
    /// Return the strongest recognized personal layer. A conflicting less specific layer is ignored.
    /// Complete normalized spellings are required; substrings and guest annotations never qualify.
    pub fn scope(&self, layers: [&[String]; 3]) -> TagScope {
        for (tags, scope) in
            layers
                .into_iter()
                .zip([TagScope::Track, TagScope::Album, TagScope::Artist])
        {
            if tags.iter().any(|tag| self.recognized.contains(tag)) {
                return scope;
            }
        }
        TagScope::Genre
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
