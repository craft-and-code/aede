//! Persistent collection identities and compatibility with name-keyed stores.
use super::*;
use crate::{json::Json, store::StoreError};

/// Recognize opaque random IDs and deterministic IDs assigned to legacy rows.
pub fn valid_collection_id(id: &str) -> bool {
    let digits = id
        .strip_prefix("collection-")
        .filter(|s| s.len() == 64)
        .or_else(|| {
            id.strip_prefix("collection-legacy-")
                .filter(|s| s.len() == 32)
        });
    digits.is_some_and(|s| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

impl UserData {
    /// Resolve an immutable collection ID only within its owner's namespace.
    pub fn collection_by_id(&self, owner: &str, id: &str) -> Option<&Collection> {
        self.collections
            .iter()
            .find(|row| row.owner == owner && row.id == id)
    }
}

pub(super) fn validate(data: &UserData) -> Result<(), StoreError> {
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for row in &data.collections {
        if !valid_collection_id(&row.id)
            || !ids.insert(&row.id)
            || !names.insert((&row.owner, text::normalize(&row.name)))
        {
            return Err(StoreError::Invalid(
                "invalid or duplicate collection identity",
            ));
        }
    }
    Ok(())
}

pub(super) fn read(root: &Json) -> Result<Vec<Collection>, StoreError> {
    use md5::{Digest, Md5};
    let mut data = UserData::default();
    for row in root
        .get("collections")
        .and_then(Json::as_arr)
        .unwrap_or(&[])
    {
        let (Some(name), Some(expression)) = (row.field_str("name"), row.field_str("query")) else {
            return Err(StoreError::Invalid("invalid collection row"));
        };
        let owner = read_owner(row)?;
        let created_at = row.field_u64("created_at").unwrap_or(0);
        let id = if row.get("id").is_some() {
            row.field_str("id")
                .ok_or(StoreError::Invalid("invalid collection identity"))?
        } else {
            if root
                .field_u32("format_version")
                .is_some_and(|version| version >= 3)
            {
                return Err(StoreError::Invalid("missing collection identity"));
            }
            // Length-delimited JSON avoids concatenation ambiguities. This is an
            // identity migration, never an authentication or integrity hash.
            let key = serde_json::to_vec(&(&owner, &name, created_at))
                .map_err(|_| StoreError::Invalid("invalid legacy collection"))?;
            format!("collection-legacy-{:x}", Md5::digest(key))
        };
        data.collections.push(Collection {
            id,
            owner,
            name,
            expression,
            created_at,
            updated_at: row.field_u64("updated_at").unwrap_or(0),
        });
    }
    validate(&data)?;
    Ok(data.collections)
}

pub(super) fn merge(into: &mut UserData, incoming: Vec<Collection>, report: &mut Merge) {
    for mut row in incoming {
        // Existing local identity wins a name collision. A renamed backup row
        // matches by ID first, but cannot overwrite another collection's name.
        let same_id = into.collections.iter().position(|c| c.id == row.id);
        let same_name = into.collections.iter().position(|c| {
            c.owner == row.owner && text::normalize(&c.name) == text::normalize(&row.name)
        });
        if same_id.is_some_and(|i| into.collections[i].owner != row.owner)
            || (same_id.is_some() && same_name.is_some() && same_id != same_name)
        {
            report.kept += 1;
            continue;
        }
        match same_id.or(same_name) {
            Some(i) if into.collections[i].updated_at >= row.updated_at => report.kept += 1,
            Some(i) => {
                row.id = into.collections[i].id.clone();
                into.collections[i] = row;
                report.collections += 1;
            }
            None => {
                into.collections.push(row);
                report.collections += 1;
            }
        }
    }
}

#[cfg(test)]
#[path = "user_collections_tests.rs"]
mod tests;
