//! Public static API entry point; no installation or account data is embedded.
use axum::{
    http::header,
    response::{Html, IntoResponse},
};

pub(super) async fn page() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(
            r#"<!doctype html>
<html lang="fr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Aède — serveur musical</title>
<style>body{color:#f1f0eb;background:#111213;font:16px/1.6 system-ui;margin:8vh auto;padding:24px;max-width:700px}h1{font-size:36px}a{color:#d8f36a}code{color:#d8f36a}section{border:1px solid #303234;border-radius:12px;padding:24px}</style>
<section><h1>Aède<span style="color:#d8f36a">.</span></h1>
<p>Ce serveur fournit l’API de votre bibliothèque musicale. Cette adresse n’est pas une interface de lecture.</p>
<p>Pour écouter et parcourir votre musique, connectez <strong>Phémios</strong>. Dans l’application Aède, exportez la connexion locale, puis importez-la dans les paramètres Serveur Aède de Phémios.</p>
<p>Consultez les vues <strong>Statut</strong> et <strong>Journaux</strong> de l’application Aède pour vérifier son fonctionnement. L’API <code>/api/v1/status</code> nécessite une authentification lorsqu’un compte est configuré.</p>
<p><a href="https://craft-and-code.github.io/aede/">Documentation Aède</a></p></section></html>"#,
        ),
    )
}
#[cfg(test)]
#[path = "landing_tests.rs"]
mod tests;
