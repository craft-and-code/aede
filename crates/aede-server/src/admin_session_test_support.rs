//! Revoke a real administrator login exactly while a bounded request body is read.
use crate::{accounts_test_support::Fixture, auth};
use axum::{body::Body, extract::Request};
use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

struct LogoutBody {
    logout: Option<Box<dyn FnOnce() + Send>>,
    bytes: Option<axum::body::Bytes>,
}
impl hyper::body::Body for LogoutBody {
    type Data = axum::body::Bytes;
    type Error = Infallible;
    fn poll_frame(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        if let Some(logout) = this.logout.take() {
            logout();
        }
        Poll::Ready(
            this.bytes
                .take()
                .map(|bytes| Ok(hyper::body::Frame::data(bytes))),
        )
    }
}
pub(crate) fn signed_request(fixture: &Fixture, payload: &'static str) -> Request {
    let accounts = fixture.accounts();
    let principal = auth::start_session(&fixture.0, &accounts, accounts.find("operator").unwrap())
        .unwrap()
        .0;
    let state = fixture.0.clone();
    let captured = principal.clone();
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/admin/v1/fetch")
        .header(
            "authorization",
            format!("Bearer {}", auth::session_token(&principal)),
        )
        .body(Body::new(LogoutBody {
            logout: Some(Box::new(move || {
                auth::end_session(&state, &captured).unwrap()
            })),
            bytes: Some(payload.into()),
        }))
        .unwrap();
    request.extensions_mut().insert(principal);
    request
}
