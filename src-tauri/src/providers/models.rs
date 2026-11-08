//! Lista de modelos disponibles para cada proveedor (`GET /v1/models`).
//!
//! Se pide a la API en lugar de escribirla a mano: así aparecen los modelos nuevos
//! sin actualizar la app, y en los servicios compatibles solo los que existen ahí.

use serde::Serialize;
use serde_json::Value;

use super::check::{provider_detail, Target};
use super::{ANTHROPIC_API_BASE, ANTHROPIC_VERSION, OPENAI_API_BASE};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// En OpenAI la lista incluye modelos que no sirven para chatear (voz, imágenes, embeddings…).
fn is_openai_chat_model(id: &str) -> bool {
    const SKIP: &[&str] = &[
        "embedding", "tts", "whisper", "dall-e", "moderation", "audio", "realtime", "transcribe", "image",
        "search", "davinci", "babbage", "sora",
    ];
    !SKIP.iter().any(|s| id.contains(s))
}

pub fn parse_models(target: &Target, body: &Value) -> Vec<ModelInfo> {
    let mut models: Vec<ModelInfo> = body
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            let name = m
                .get("display_name")
                .or_else(|| m.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_string();
            Some(ModelInfo { id, name })
        })
        .filter(|m| !matches!(target, Target::OpenAi) || is_openai_chat_model(&m.id))
        .collect();
    // Anthropic ya las devuelve de la más nueva a la más antigua; el resto, por nombre.
    if !matches!(target, Target::Anthropic) {
        models.sort_by(|a, b| a.id.cmp(&b.id));
    }
    models
}

pub async fn list_models(client: &reqwest::Client, target: &Target, key: &str) -> AppResult<Vec<ModelInfo>> {
    let req = match target {
        Target::Anthropic => client
            .get(format!("{ANTHROPIC_API_BASE}/v1/models?limit=100"))
            .header("x-api-key", key)
            .header("anthropic-version", ANTHROPIC_VERSION),
        Target::OpenAi => client.get(format!("{OPENAI_API_BASE}/v1/models")).bearer_auth(key),
        Target::Compatible { base_url } => client.get(format!("{base_url}/models")).bearer_auth(key),
    };
    let response = req
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| AppError::Network(format!("no se pudo obtener la lista de modelos: {e}")))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let detail = provider_detail(&text).unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        return Err(AppError::Provider {
            status: status.as_u16(),
            message: format!("no se pudo obtener la lista de modelos: {detail}"),
        });
    }
    let body: Value = serde_json::from_str(&text)
        .map_err(|_| AppError::Internal("la lista de modelos no tiene un formato válido".into()))?;
    Ok(parse_models(target, &body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_anthropic_models_with_display_names_in_api_order() {
        let body = json!({"data": [
            {"id": "claude-opus-5-5", "display_name": "Claude Opus 5.5"},
            {"id": "claude-haiku-5-5", "display_name": "Claude Haiku 5.5"}
        ]});
        let models = parse_models(&Target::Anthropic, &body);
        assert_eq!(models[0], ModelInfo { id: "claude-opus-5-5".into(), name: "Claude Opus 5.5".into() });
        assert_eq!(models[1].id, "claude-haiku-5-5");
    }

    #[test]
    fn filters_non_chat_openai_models_and_sorts() {
        let body = json!({"data": [
            {"id": "text-embedding-3-large"}, {"id": "gpt-b"}, {"id": "whisper-1"}, {"id": "gpt-a"}, {"id": "tts-1"}
        ]});
        let ids: Vec<_> = parse_models(&Target::OpenAi, &body).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["gpt-a", "gpt-b"]);
    }

    #[test]
    fn keeps_all_compatible_models_and_uses_name_field() {
        let target = Target::Compatible { base_url: "https://api.abliteration.ai/v1".into() };
        let body = json!({"object": "list", "data": [
            {"id": "abliterated-model", "object": "model", "name": "Abliterated Model"}
        ]});
        assert_eq!(
            parse_models(&target, &body),
            vec![ModelInfo { id: "abliterated-model".into(), name: "Abliterated Model".into() }]
        );
        assert!(parse_models(&target, &json!({})).is_empty());
    }
}
