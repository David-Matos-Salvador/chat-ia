//! Comprobar si una API key funciona, sin gastar tokens.
//!
//! Se pide la lista de modelos (`GET /v1/models`): es gratis y exige una key válida.

use std::time::Duration;

use serde::Serialize;

use super::{ANTHROPIC_API_BASE, ANTHROPIC_VERSION, OPENAI_API_BASE};
use crate::keys::Provider;

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

/// Resultado de la comprobación, listo para mostrar en la interfaz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCheck {
    pub provider: Provider,
    /// `true` si la key se aceptó (aunque el servicio limite peticiones en ese momento).
    pub ok: bool,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Valid,
    Invalid,
    Forbidden,
    Billing,
    RateLimited,
    Unavailable,
    Network,
    Unexpected,
}

/// Traduce el código HTTP de la respuesta a un resultado comprensible.
pub fn classify(provider: Provider, http_status: u16) -> KeyCheck {
    let name = provider.label();
    let (ok, status, message) = match http_status {
        200..=299 => (true, CheckStatus::Valid, "La key es válida.".to_string()),
        401 => (
            false,
            CheckStatus::Invalid,
            format!("{name} rechazó la key: no es válida o fue revocada."),
        ),
        402 => (
            false,
            CheckStatus::Billing,
            format!("La key es correcta, pero la cuenta de {name} tiene un problema de facturación o saldo."),
        ),
        403 => (
            false,
            CheckStatus::Forbidden,
            "La key no tiene permiso para usar la API.".to_string(),
        ),
        429 => (
            true,
            CheckStatus::RateLimited,
            "La key es válida, pero se superó el límite de peticiones. Prueba de nuevo en unos minutos.".to_string(),
        ),
        500..=599 => (
            false,
            CheckStatus::Unavailable,
            format!("El servicio de {name} no está disponible ahora (HTTP {http_status}). No se pudo comprobar la key."),
        ),
        other => (
            false,
            CheckStatus::Unexpected,
            format!("Respuesta inesperada de {name} (HTTP {other})."),
        ),
    };
    KeyCheck {
        provider,
        ok,
        status,
        message,
    }
}

fn network_failure(provider: Provider, err: &reqwest::Error) -> KeyCheck {
    let message = if err.is_timeout() {
        format!("{} tardó demasiado en responder. Revisa tu conexión e inténtalo de nuevo.", provider.label())
    } else {
        format!("No se pudo conectar con {}. Revisa tu conexión a internet.", provider.label())
    };
    KeyCheck {
        provider,
        ok: false,
        status: CheckStatus::Network,
        message,
    }
}

/// A quién se le pregunta. El proveedor compatible lleva su URL base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Anthropic,
    OpenAi,
    Compatible { base_url: String },
}

impl Target {
    pub fn provider(&self) -> Provider {
        match self {
            Target::Anthropic => Provider::Anthropic,
            Target::OpenAi => Provider::OpenAi,
            Target::Compatible { .. } => Provider::Compatible,
        }
    }
}

fn build_request(client: &reqwest::Client, target: &Target, key: &str) -> reqwest::RequestBuilder {
    let builder = match target {
        Target::Anthropic => client
            .get(format!("{ANTHROPIC_API_BASE}/v1/models?limit=1"))
            .header("x-api-key", key)
            .header("anthropic-version", ANTHROPIC_VERSION),
        Target::OpenAi => client
            .get(format!("{OPENAI_API_BASE}/v1/models"))
            .bearer_auth(key),
        Target::Compatible { base_url } => client
            .get(format!("{base_url}/models"))
            .bearer_auth(key),
    };
    builder.timeout(CHECK_TIMEOUT)
}

const MAX_DETAIL_CHARS: usize = 200;

/// Extrae el texto de error que devuelven las APIs.
/// Anthropic, OpenAI y las compatibles usan `{"error": {"message": "..."}}`.
/// El cuerpo de error nunca incluye la key, así que se puede mostrar.
pub fn provider_detail(body: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    let message = json
        .pointer("/error/message")
        .or_else(|| json.get("message"))
        .and_then(|m| m.as_str())?
        .trim();
    if message.is_empty() {
        return None;
    }
    Some(message.chars().take(MAX_DETAIL_CHARS).collect())
}

/// Hace la petición de prueba. Nunca devuelve error: cualquier fallo se describe en `KeyCheck`.
pub async fn check_key(client: &reqwest::Client, target: &Target, key: &str) -> KeyCheck {
    let provider = target.provider();
    let response = match build_request(client, target, key).send().await {
        Ok(response) => response,
        Err(err) => return network_failure(provider, &err),
    };

    let code = response.status().as_u16();
    let mut check = classify(provider, code);
    if check.status == CheckStatus::Valid {
        return check;
    }

    // En un proveedor compatible, un 404 casi siempre es la URL base mal escrita.
    if provider == Provider::Compatible && code == 404 {
        check.message = "No se encontró la API en esa URL. Revisa la URL base: suele terminar en /v1 \
                         (por ejemplo https://api.abliteration.ai/v1)."
            .into();
        return check;
    }

    // Añadir el motivo exacto que da el proveedor ayuda a resolver el problema.
    if let Ok(body) = response.text().await {
        if let Some(detail) = provider_detail(&body) {
            check.message = format!("{} Respuesta del servidor: «{detail}»", check.message);
        }
    }
    check
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_every_documented_status() {
        let cases = [
            (200, true, CheckStatus::Valid),
            (401, false, CheckStatus::Invalid),
            (402, false, CheckStatus::Billing),
            (403, false, CheckStatus::Forbidden),
            (429, true, CheckStatus::RateLimited),
            (500, false, CheckStatus::Unavailable),
            (529, false, CheckStatus::Unavailable),
            (404, false, CheckStatus::Unexpected),
        ];
        for (code, ok, status) in cases {
            let result = classify(Provider::Anthropic, code);
            assert_eq!((result.ok, result.status), (ok, status), "HTTP {code}");
            assert!(!result.message.is_empty());
        }
    }

    #[test]
    fn messages_name_the_provider() {
        assert!(classify(Provider::OpenAi, 401).message.contains("OpenAI"));
        assert!(classify(Provider::Anthropic, 529).message.contains("Anthropic"));
    }

    #[test]
    fn anthropic_request_uses_key_header_and_version() {
        let client = reqwest::Client::new();
        let req = build_request(&client, &Target::Anthropic, "sk-ant-test").build().unwrap();
        assert_eq!(req.method(), reqwest::Method::GET);
        assert_eq!(req.url().as_str(), "https://api.anthropic.com/v1/models?limit=1");
        assert_eq!(req.headers()["x-api-key"], "sk-ant-test");
        assert_eq!(req.headers()["anthropic-version"], ANTHROPIC_VERSION);
        assert!(req.headers().get("authorization").is_none());
    }

    #[test]
    fn openai_request_uses_bearer_auth() {
        let client = reqwest::Client::new();
        let req = build_request(&client, &Target::OpenAi, "sk-proj-test").build().unwrap();
        assert_eq!(req.url().as_str(), "https://api.openai.com/v1/models");
        assert_eq!(req.headers()["authorization"], "Bearer sk-proj-test");
        assert!(req.headers().get("x-api-key").is_none());
    }

    #[test]
    fn compatible_request_uses_configured_base_url() {
        let client = reqwest::Client::new();
        let target = Target::Compatible {
            base_url: "https://api.abliteration.ai/v1".into(),
        };
        let req = build_request(&client, &target, "abl-test-key").build().unwrap();
        assert_eq!(req.url().as_str(), "https://api.abliteration.ai/v1/models");
        assert_eq!(req.headers()["authorization"], "Bearer abl-test-key");
        assert_eq!(target.provider(), Provider::Compatible);
    }

    #[test]
    fn extracts_provider_error_detail() {
        let abliteration = r#"{"error":{"code":"invalid_api_key","message":"API key is invalid, expired, or revoked.","type":"auth_error"}}"#;
        assert_eq!(
            provider_detail(abliteration).as_deref(),
            Some("API key is invalid, expired, or revoked.")
        );
        let anthropic = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(provider_detail(anthropic).as_deref(), Some("invalid x-api-key"));
        assert_eq!(provider_detail(r#"{"message":"Unauthorized"}"#).as_deref(), Some("Unauthorized"));

        assert_eq!(provider_detail("<html>502 Bad Gateway</html>"), None);
        assert_eq!(provider_detail(r#"{"error":{"message":"   "}}"#), None);
        let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(500));
        assert_eq!(provider_detail(&long).unwrap().chars().count(), MAX_DETAIL_CHARS);
    }

    #[test]
    fn serializes_status_in_snake_case() {
        let json = serde_json::to_value(classify(Provider::Anthropic, 429)).unwrap();
        assert_eq!(json["status"], "rate_limited");
        assert_eq!(json["provider"], "anthropic");
    }
}
