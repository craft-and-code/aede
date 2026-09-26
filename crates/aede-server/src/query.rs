//! Strict list query validation.

use super::*;

pub(super) fn decimal(value: &str, field: &str, pagination: bool) -> Result<usize, ApiError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        let message = format!("{field} must be a non-negative decimal integer");
        return Err(if pagination {
            error(StatusCode::BAD_REQUEST, "invalid_pagination", message)
        } else {
            invalid_query(message)
        });
    }
    value.parse::<usize>().map_err(|_| {
        let message = format!("{field} is too large");
        if pagination {
            error(StatusCode::BAD_REQUEST, "invalid_pagination", message)
        } else {
            invalid_query(message)
        }
    })
}

pub(super) fn list_query(
    query: Result<Query<ListQuery>, QueryRejection>,
    kind: ListKind,
) -> Result<ListOptions, ApiError> {
    let Query(query) = query.map_err(|rejection| invalid_query(rejection.body_text()))?;
    let offset = query
        .offset
        .as_deref()
        .map(|value| decimal(value, "offset", true))
        .transpose()?
        .unwrap_or(0);
    let limit = query
        .limit
        .as_deref()
        .map(|value| decimal(value, "limit", true))
        .transpose()?
        .unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_pagination",
            format!("limit must be between 1 and {MAX_LIMIT}"),
        ));
    }
    let q = query
        .q
        .map(|value| text::normalize(&value))
        .map(|normalized| {
            if normalized.is_empty() {
                Err(invalid_query("q must contain searchable text"))
            } else {
                Ok(normalized)
            }
        })
        .transpose()?;
    let sort = match query.sort.as_deref().unwrap_or("catalog") {
        "catalog" => SortMode::Catalog,
        "name" if kind == ListKind::Artist => SortMode::Name,
        "title" if kind != ListKind::Artist => SortMode::Title,
        "year" if kind == ListKind::Release => SortMode::Year,
        _ => return Err(invalid_query("sort is not supported for this list")),
    };
    let descending = match query.order.as_deref().unwrap_or("asc") {
        "asc" => false,
        "desc" => true,
        _ => return Err(invalid_query("order must be asc or desc")),
    };
    if query.mbid.is_some() && kind != ListKind::Artist
        || query.year.is_some() && kind != ListKind::Release
        || query.artist.is_some() && kind != ListKind::Release
        || query.release.is_some() && kind != ListKind::Track
        || query.work.is_some() && kind != ListKind::Recording
    {
        return Err(invalid_query("filter is not supported for this list"));
    }
    if query.mbid.as_deref() == Some("") {
        return Err(invalid_query("mbid cannot be empty"));
    }
    let year = query
        .year
        .as_deref()
        .map(|value| decimal(value, "year", false))
        .transpose()?
        .map(|value| u32::try_from(value).map_err(|_| invalid_query("year is too large")))
        .transpose()?;
    Ok(ListOptions {
        offset,
        limit,
        q,
        sort,
        descending,
        mbid: query.mbid,
        year,
        artist: query.artist,
        release: query.release,
        work: query.work,
    })
}
