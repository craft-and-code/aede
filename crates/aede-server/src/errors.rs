//! JSON errors shared by HTTP routes.

use super::*;

#[derive(Serialize)]
pub(super) struct ErrorBody {
    pub(super) error: ErrorDetail,
}

#[derive(Serialize)]
pub(super) struct ErrorDetail {
    pub(super) code: &'static str,
    pub(super) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) candidates: Option<Vec<EntityCandidate>>,
}

#[derive(Debug, Serialize)]
pub(super) struct EntityCandidate {
    pub(super) reference: String,
    pub(super) name: String,
}

#[derive(Debug)]
pub(super) struct ApiError {
    pub(super) status: StatusCode,
    pub(super) code: &'static str,
    pub(super) message: String,
    pub(super) candidates: Option<Vec<EntityCandidate>>,
}

impl ApiError {
    pub(super) fn with_candidates(mut self, candidates: Vec<EntityCandidate>) -> Self {
        self.candidates = Some(candidates);
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: ErrorDetail {
                    code: self.code,
                    message: self.message,
                    candidates: self.candidates,
                },
            }),
        )
            .into_response()
    }
}

pub(super) fn error(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
) -> ApiError {
    ApiError {
        status,
        code,
        message: message.into(),
        candidates: None,
    }
}

pub(super) fn invalid_query(message: impl Into<String>) -> ApiError {
    error(StatusCode::BAD_REQUEST, "invalid_query", message)
}

pub(super) fn unavailable() -> ApiError {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "catalog_unavailable",
        "the catalog is not available; run aede scan",
    )
}

pub(super) async fn not_found() -> ApiError {
    error(StatusCode::NOT_FOUND, "not_found", "unknown API route")
}

pub(super) async fn method_not_allowed() -> ApiError {
    error(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "this API route does not support that method",
    )
}

pub(super) fn error_message(
    status: StatusCode,
    code: &'static str,
    message: impl ToString,
) -> ApiError {
    error(status, code, message.to_string())
}
