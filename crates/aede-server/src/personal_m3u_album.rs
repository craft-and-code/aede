//! Resolve only the CLI's canonical album-playlist destination, under confirmed shared roots.
use super::*;

pub(super) fn target(
    state: &ApiState,
    scope: &Scope,
    reference: &str,
) -> Result<(Vec<String>, Option<PathBuf>), ApiError> {
    bounded_text(reference, "ref", MAX_REFERENCE_BYTES)?;
    let folder = {
        let held = state.catalog.blocking_read();
        let catalog = held.as_ref().ok_or_else(unavailable)?;
        if catalog.scanned_at != scope.scanned_at {
            return Err(failure(library_files::Error::Changed));
        }
        let id = crate::catalog::filter_reference(catalog, reference, EntityKind::Release)?;
        PathBuf::from(&catalog.release(id).ok_or_else(unavailable)?.folder)
    };
    // Release paths are catalog facts, never client-selected filesystem arguments.
    // Resolve them after releasing the catalog guard; final Scope confirmation catches retargeted roots.
    let canonical = folder
        .canonicalize()
        .map_err(|_| failure(library_files::Error::Unavailable))?;
    let admitted = scope
        .canonical_paths
        .iter()
        .take(scope.roots.len())
        .flatten()
        .any(|root| canonical.starts_with(root));
    let excluded = scope
        .canonical_paths
        .iter()
        .skip(scope.roots.len())
        .flatten()
        .any(|path| canonical.starts_with(path))
        || scope
            .excluded
            .iter()
            .any(|path| canonical.starts_with(path));
    if !admitted || excluded {
        return Ok((Vec::new(), None));
    }
    let target = canonical.join(aede_core::playlist::file_name(&folder));
    Ok((
        vec![
            canonical
                .to_str()
                .ok_or_else(|| failure(library_files::Error::InvalidPath))?
                .to_owned(),
        ],
        Some(target),
    ))
}
