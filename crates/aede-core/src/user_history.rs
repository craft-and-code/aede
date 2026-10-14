//! Retention never lets one owner's activity erase another owner's history.

use super::*;
use crate::store::StoreError;

pub(super) fn retain<T>(rows: &mut Vec<T>, owner: impl Fn(&T) -> &str) {
    let mut counts = BTreeMap::<String, usize>::new();
    for row in rows.iter() {
        match counts.get_mut(owner(row)) {
            Some(count) => *count += 1,
            None => {
                counts.insert(owner(row).to_owned(), 1);
            }
        }
    }
    // Rows are ordered by event time; remove only the excess oldest rows from
    // their own namespace, preserving same-time multiplicity and arrival order.
    rows.retain(|row| match counts.get_mut(owner(row)) {
        Some(count) if *count > HISTORY_LIMIT => {
            *count -= 1;
            false
        }
        _ => true,
    });
}

fn validate_rows<T>(rows: &[T], owner: impl Fn(&T) -> &str) -> Result<(), StoreError> {
    if rows.len() > HISTORY_STORE_LIMIT {
        return Err(StoreError::Invalid("personal history store limit exceeded"));
    }
    let mut counts = BTreeMap::<&str, usize>::new();
    for row in rows {
        let count = counts.entry(owner(row)).or_default();
        *count += 1;
        if *count > HISTORY_LIMIT {
            return Err(StoreError::Invalid("personal history owner limit exceeded"));
        }
    }
    Ok(())
}

pub(super) fn validate(data: &UserData) -> Result<(), StoreError> {
    if data
        .counts
        .iter()
        .any(|row| row.completed_count > row.count)
    {
        return Err(StoreError::Invalid(
            "completed listening count exceeds all-time count",
        ));
    }
    validate_rows(&data.plays, |row| &row.owner)?;
    validate_rows(&data.scrobbles, |row| &row.owner)
}
