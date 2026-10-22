//! Transparent measured selections, separate from personal/semantic atmosphere recipes.
use crate::Features;
/// One immutable descriptor-based discovery recipe.
pub struct AudioRecipe {
    /// Stable engine-owned identity, independent of translated names.
    pub id: &'static str,
    /// Presentation selector used by clients that opt into acoustic discovery.
    pub kind: &'static str,
    /// Untranslated reference name.
    pub name: &'static str,
}
/// Fixed first-profile thresholds; these do not infer emotional valence or musical genre.
pub fn audio_recipes() -> [AudioRecipe; 4] {
    [
        AudioRecipe {
            id: "@aede/audio/slow",
            kind: "slow",
            name: "Slow tempo",
        },
        AudioRecipe {
            id: "@aede/audio/fast",
            kind: "fast",
            name: "Fast tempo",
        },
        AudioRecipe {
            id: "@aede/audio/dark",
            kind: "dark",
            name: "Low spectral centre",
        },
        AudioRecipe {
            id: "@aede/audio/bright",
            kind: "bright",
            name: "High spectral centre",
        },
    ]
}
/// Whether an identity belongs to an immutable measured recipe.
pub fn is_audio_recipe_id(id: &str) -> bool {
    audio_recipes().iter().any(|recipe| recipe.id == id)
}
impl AudioRecipe {
    /// A matching measurement with its physical unit; missing/silent evidence never matches.
    pub fn measure(&self, features: &Features) -> Option<(f32, &'static str)> {
        if !features.usable() {
            return None;
        }
        match self.kind {
            "slow" => features
                .tempo_bpm
                .filter(|bpm| *bpm <= 90.)
                .map(|v| (v, "bpm")),
            "fast" => features
                .tempo_bpm
                .filter(|bpm| *bpm >= 130.)
                .map(|v| (v, "bpm")),
            "dark" => {
                (features.centroid_hz.mean <= 1600.).then_some((features.centroid_hz.mean, "hz"))
            }
            "bright" => {
                (features.centroid_hz.mean >= 3000.).then_some((features.centroid_hz.mean, "hz"))
            }
            _ => None,
        }
    }
}
