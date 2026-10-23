//! HTTPS administration pinned to the owned server certificate, with bounded replies and no redirects.
use crate::{configuration::Identity, process::Server};
use serde_json::{Value, json};
use std::time::Duration;
pub(crate) fn agent(identity: &Identity) -> ureq::Agent {
    let certificate = ureq::tls::Certificate::from_der(&identity.certificate_der).to_owned();
    ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs([certificate].into())
                .build(),
        )
        .build()
        .into()
}
fn response(mut value: ureq::http::Response<ureq::Body>) -> Result<Value, String> {
    if !value.status().is_success() {
        return Err(match value.status().as_u16() {
            401 => "Les accès du serveur ont changé. Vérifiez le compte administrateur.",
            409 | 429 | 503 => "Le serveur est occupé. Attendez la fin de la tâche en cours.",
            _ => "Aède a refusé cette action. Vérifiez sa configuration.",
        }
        .into());
    }
    let bytes = value
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_vec()
        .map_err(|_| "Réponse du serveur trop grande ou interrompue.")?;
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Réponse du serveur illisible.".into())
}
impl Server {
    pub(crate) fn authenticate(&mut self, identity: &Identity) -> Result<(), String> {
        let answer = response(
            self.agent
                .post(format!("{}/api/auth/v1/session", self.address))
                .header("Content-Type", "application/json")
                .send(json!({"username":"aede-desktop","password":identity.password}).to_string())
                .map_err(|_| "Impossible d’authentifier le serveur HTTPS local.")?,
        )?;
        let token = answer["token"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("Session du serveur invalide.")?;
        self.token = Some(token.into());
        Ok(())
    }
    pub(crate) fn request(
        &mut self,
        identity: &Identity,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        for attempt in 0..2 {
            if self.token.is_none() {
                self.authenticate(identity)?;
            }
            let token = self.token.as_deref().ok_or("Session du serveur absente.")?;
            let agent = self.agent.clone();
            let url = format!("{}{path}", self.address);
            let answer = if method == "GET" {
                agent
                    .get(&url)
                    .header("Authorization", format!("Bearer {token}"))
                    .call()
            } else {
                let request = agent
                    .post(&url)
                    .header("Authorization", format!("Bearer {token}"));
                if let Some(body) = body {
                    request
                        .header("Content-Type", "application/json")
                        .send(body.to_string())
                } else {
                    request.send_empty()
                }
            }
            .map_err(|_| "Le serveur HTTPS local ne répond pas.")?;
            // Aède refuses authentication before executing the operation: only 401 is safe to replay.
            if answer.status() == ureq::http::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.token = None;
                continue;
            }
            return response(answer);
        }
        Err("Authentification du serveur impossible.".into())
    }
}
