/// A known empty match is zero; missing durations, invalid legacy rules and overflow remain unknown.
pub(super) fn duration(context: &aede_core::query::Context<'_>, expression: &str) -> Option<u64> {
    let parsed = aede_core::query::parse(expression).ok()?;
    aede_core::query::run(&parsed, context)
        .into_iter()
        .try_fold(0u64, |total, id| {
            total.checked_add(context.catalog.track(id)?.duration_ms?)
        })
}
#[cfg(test)]
#[path = "personal_collection_duration_tests.rs"]
mod tests;
