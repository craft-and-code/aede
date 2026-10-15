//! Adapter to the existing independently versioned backup JSON vocabulary.
use super::ParentalStore;
use crate::{json::Json, store::StoreError};

/// Preserve the protected parental document inside an Aède backup.
/// This document contains account restrictions and usage, never credentials.
pub fn to_json(store: &ParentalStore) -> Json {
    // The derived model contains no floating-point values or fallible custom
    // serializers. Bounds are validated before persistence/backup publication.
    convert(serde_json::json!(store))
}
/// Validate a parental part independently from the other backup stores.
pub fn from_json(document: &Json) -> Result<ParentalStore, StoreError> {
    let bytes = document.to_string_compact();
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(StoreError::Invalid("parental backup part exceeds 8 MiB"));
    }
    let value: ParentalStore = serde_json::from_str(&bytes)
        .map_err(|_| StoreError::Invalid("invalid parental backup part"))?;
    value
        .validate()
        .map_err(|_| StoreError::Invalid("inconsistent parental backup part"))?;
    Ok(value)
}
fn convert(value: serde_json::Value) -> Json {
    match value {
        serde_json::Value::Null => Json::Null,
        serde_json::Value::Bool(value) => Json::Bool(value),
        serde_json::Value::Number(value) => value.as_f64().map_or(Json::Null, Json::Num),
        serde_json::Value::String(value) => Json::Str(value),
        serde_json::Value::Array(values) => Json::Arr(values.into_iter().map(convert).collect()),
        serde_json::Value::Object(values) => Json::Obj(
            values
                .into_iter()
                .map(|(key, value)| (key, convert(value)))
                .collect(),
        ),
    }
}
