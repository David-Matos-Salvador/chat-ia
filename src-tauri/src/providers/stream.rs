//! Peticiones de chat con streaming y lectura de sus eventos.
//!
//! - Anthropic: `POST /v1/messages` con `stream: true` (eventos SSE con nombre).
//! - OpenAI y compatibles: `POST {base}/chat/completions` con `stream: true`
//!   (eventos `data:` y un `[DONE]` final).

use serde_json::{json, Value};

use super::check::Target;
use super::{ANTHROPIC_API_BASE, ANTHROPIC_VERSION, OPENAI_API_BASE};
use crate::db::models::{Message, Role};

/// Tope de tokens de salida para Claude. Con streaming no hay riesgo de timeout HTTP.
const ANTHROPIC_MAX_TOKENS: u32 = 64_000;
/// Beta de Anthropic que reintenta en otro modelo si el elegido rechaza la petición.
const ANTHROPIC_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Un trozo útil de la respuesta, ya interpretado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Usage { input: Option<i64>, output: Option<i64> },
    Stop(String),
    Error(String),
}

/// Modelos de Claude que aceptan `fallbacks: "default"`.
/// Haiku 5.5 no tiene respaldo en servidor y los modelos antiguos no lo conocen.
fn anthropic_supports_fallback(model: &str) -> bool {
    ["claude-opus-5", "claude-sonnet-5-5", "claude-fable-5"]
        .iter()
        .any(|prefix| model.starts_with(prefix))
}

/// Historial para la API: solo mensajes de usuario y asistente con texto.
fn chat_turns(history: &[Message]) -> Vec<(&'static str, &str)> {
    history
        .iter()
        .filter(|m| !m.content.trim().is_empty())
        .filter_map(|m| match m.role {
            Role::User => Some(("user", m.content.as_str())),
            Role::Assistant => Some(("assistant", m.content.as_str())),
            Role::System => None,
        })
        .collect()
}

pub fn anthropic_body(model: &str, system: Option<&str>, history: &[Message]) -> Value {
    let messages: Vec<Value> = chat_turns(history)
        .into_iter()
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect();
    let mut body = json!({
        "model": model,
        "max_tokens": ANTHROPIC_MAX_TOKENS,
        "stream": true,
        "messages": messages,
    });
    if let Some(system) = system {
        body["system"] = json!(system);
    }
    if anthropic_supports_fallback(model) {
        body["fallbacks"] = json!("default");
    }
    body
}

pub fn openai_body(model: &str, system: Option<&str>, history: &[Message], include_usage: bool) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for (role, content) in chat_turns(history) {
        messages.push(json!({ "role": role, "content": content }));
    }
    let mut body = json!({ "model": model, "stream": true, "messages": messages });
    // Algunos servicios compatibles rechazan campos que no conocen, así que solo se
    // pide el uso de tokens a OpenAI.
    if include_usage {
        body["stream_options"] = json!({ "include_usage": true });
    }
    body
}

/// Servicios compatibles que aceptan `stream_options.include_usage` (comprobado).
/// En el resto no se envía, por si rechazan campos desconocidos.
fn reports_usage(base_url: &str) -> bool {
    let host = reqwest::Url::parse(base_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default();
    matches!(host.as_str(), "api.abliteration.ai" | "openrouter.ai")
}

pub fn build_chat_request(
    client: &reqwest::Client,
    target: &Target,
    key: &str,
    model: &str,
    system: Option<&str>,
    history: &[Message],
) -> reqwest::RequestBuilder {
    let req = match target {
        Target::Anthropic => {
            let mut req = client
                .post(format!("{ANTHROPIC_API_BASE}/v1/messages"))
                .header("x-api-key", key)
                .header("anthropic-version", ANTHROPIC_VERSION);
            if anthropic_supports_fallback(model) {
                req = req.header("anthropic-beta", ANTHROPIC_FALLBACK_BETA);
            }
            req.json(&anthropic_body(model, system, history))
        }
        Target::OpenAi => client
            .post(format!("{OPENAI_API_BASE}/v1/chat/completions"))
            .bearer_auth(key)
            .json(&openai_body(model, system, history, true)),
        Target::Compatible { base_url } => client
            .post(format!("{base_url}/chat/completions"))
            .bearer_auth(key)
            .json(&openai_body(model, system, history, reports_usage(base_url))),
    };
    req.header("accept", "text/event-stream")
}

fn error_message(v: &Value) -> String {
    v.pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("el proveedor devolvió un error durante la respuesta")
        .to_string()
}

/// Interpreta un evento SSE de Anthropic.
pub fn parse_anthropic(event: Option<&str>, data: &str) -> Vec<Piece> {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return vec![];
    };
    let kind = event.or_else(|| v.get("type").and_then(Value::as_str)).unwrap_or("");
    match kind {
        "message_start" => {
            let input = v.pointer("/message/usage/input_tokens").and_then(Value::as_i64);
            vec![Piece::Usage { input, output: None }]
        }
        // Solo el texto visible. Los bloques de razonamiento (thinking) y de respaldo
        // (fallback) no se muestran en el chat.
        "content_block_delta" => match v.pointer("/delta/type").and_then(Value::as_str) {
            Some("text_delta") => v
                .pointer("/delta/text")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(|t| vec![Piece::Text(t.to_string())])
                .unwrap_or_default(),
            _ => vec![],
        },
        "message_delta" => {
            let mut pieces = Vec::new();
            let input = v.pointer("/usage/input_tokens").and_then(Value::as_i64);
            let output = v.pointer("/usage/output_tokens").and_then(Value::as_i64);
            if input.is_some() || output.is_some() {
                pieces.push(Piece::Usage { input, output });
            }
            if let Some(reason) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                pieces.push(Piece::Stop(reason.to_string()));
            }
            pieces
        }
        "error" => vec![Piece::Error(error_message(&v))],
        _ => vec![],
    }
}

/// Interpreta un evento SSE de OpenAI o de un servicio compatible.
pub fn parse_openai(data: &str) -> Vec<Piece> {
    if data.trim() == "[DONE]" {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return vec![];
    };
    if v.get("error").is_some_and(|e| !e.is_null()) {
        return vec![Piece::Error(error_message(&v))];
    }
    let mut pieces = Vec::new();
    if let Some(choice) = v.pointer("/choices/0") {
        if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str) {
            if !text.is_empty() {
                pieces.push(Piece::Text(text.to_string()));
            }
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            pieces.push(Piece::Stop(reason.to_string()));
        }
    }
    if let Some(usage) = v.get("usage").filter(|u| u.is_object()) {
        pieces.push(Piece::Usage {
            input: usage.get("prompt_tokens").and_then(Value::as_i64),
            output: usage.get("completion_tokens").and_then(Value::as_i64),
        });
    }
    pieces
}

/// Explica en español por qué terminó una respuesta, si no fue de forma normal.
pub fn stop_notice(reason: &str) -> Option<&'static str> {
    match reason {
        "max_tokens" | "length" => Some("La respuesta se cortó porque alcanzó el límite de longitud."),
        "refusal" | "content_filter" => Some("El modelo no quiso responder a esta petición."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            id: "m".into(),
            conversation_id: "c".into(),
            role,
            content: content.into(),
            model: None,
            input_tokens: None,
            output_tokens: None,
            created_at: "t".into(),
        }
    }

    fn history() -> Vec<Message> {
        vec![
            msg(Role::User, "hola"),
            msg(Role::Assistant, "¡hola!"),
            msg(Role::Assistant, "   "),
            msg(Role::User, "¿qué tal?"),
        ]
    }

    #[test]
    fn anthropic_body_has_system_stream_and_fallback() {
        let body = anthropic_body("claude-opus-5-5", Some("Sé breve"), &history());
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 64_000);
        assert_eq!(body["system"], "Sé breve");
        assert_eq!(body["fallbacks"], "default");
        assert!(body.get("temperature").is_none(), "temperature da 400 en la versión 2026-10-21");
        let roles: Vec<_> = body["messages"].as_array().unwrap().iter().map(|m| m["role"].clone()).collect();
        assert_eq!(roles, vec!["user", "assistant", "user"], "los mensajes vacíos no se envían");
    }

    #[test]
    fn haiku_and_old_models_do_not_send_fallbacks() {
        assert!(anthropic_body("claude-haiku-5-5", None, &history()).get("fallbacks").is_none());
        assert!(anthropic_body("claude-opus-4-6", None, &history()).get("fallbacks").is_none());
        assert!(anthropic_body("claude-sonnet-5-5", None, &history()).get("fallbacks").is_some());
    }

    #[test]
    fn openai_body_puts_system_first_and_usage_only_when_asked() {
        let body = openai_body("abliterated-model", Some("Sé breve"), &history(), false);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0], json!({"role": "system", "content": "Sé breve"}));
        assert_eq!(msgs.len(), 4);
        assert!(body.get("stream_options").is_none());
        assert!(openai_body("x", None, &history(), true).get("stream_options").is_some());
    }

    #[test]
    fn requests_go_to_the_right_endpoint_with_the_right_auth() {
        let client = reqwest::Client::new();
        let h = history();

        let r = build_chat_request(&client, &Target::Anthropic, "sk-ant-x", "claude-opus-5-5", None, &h)
            .build()
            .unwrap();
        assert_eq!(r.url().as_str(), "https://api.anthropic.com/v1/messages");
        assert_eq!(r.headers()["x-api-key"], "sk-ant-x");
        assert_eq!(r.headers()["anthropic-beta"], ANTHROPIC_FALLBACK_BETA);

        let r = build_chat_request(&client, &Target::Anthropic, "k", "claude-haiku-5-5", None, &h)
            .build()
            .unwrap();
        assert!(r.headers().get("anthropic-beta").is_none());

        let target = Target::Compatible { base_url: "https://api.abliteration.ai/v1".into() };
        let r = build_chat_request(&client, &target, "ak_x", "abliterated-model", None, &h).build().unwrap();
        assert_eq!(r.url().as_str(), "https://api.abliteration.ai/v1/chat/completions");
        assert_eq!(r.headers()["authorization"], "Bearer ak_x");
        assert_eq!(r.headers()["accept"], "text/event-stream");
    }

    #[test]
    fn parses_anthropic_stream() {
        let start = r#"{"type":"message_start","message":{"usage":{"input_tokens":25,"output_tokens":1}}}"#;
        assert_eq!(
            parse_anthropic(Some("message_start"), start),
            vec![Piece::Usage { input: Some(25), output: None }]
        );
        let delta = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hola"}}"#;
        assert_eq!(parse_anthropic(Some("content_block_delta"), delta), vec![Piece::Text("Hola".into())]);
        let thinking = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"..."}}"#;
        assert!(parse_anthropic(Some("content_block_delta"), thinking).is_empty());
        let end = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":12}}"#;
        assert_eq!(
            parse_anthropic(Some("message_delta"), end),
            vec![Piece::Usage { input: None, output: Some(12) }, Piece::Stop("end_turn".into())]
        );
        let err = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert_eq!(parse_anthropic(Some("error"), err), vec![Piece::Error("Overloaded".into())]);
        assert!(parse_anthropic(Some("ping"), r#"{"type":"ping"}"#).is_empty());
        assert!(parse_anthropic(None, "no es json").is_empty());
    }

    #[test]
    fn parses_openai_stream() {
        let chunk = r#"{"choices":[{"index":0,"delta":{"content":"Hola"},"finish_reason":null}]}"#;
        assert_eq!(parse_openai(chunk), vec![Piece::Text("Hola".into())]);
        let role_only = r#"{"choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}"#;
        assert!(parse_openai(role_only).is_empty());
        let end = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        assert_eq!(parse_openai(end), vec![Piece::Stop("stop".into())]);
        let usage = r#"{"choices":[],"usage":{"prompt_tokens":9,"completion_tokens":3}}"#;
        assert_eq!(parse_openai(usage), vec![Piece::Usage { input: Some(9), output: Some(3) }]);
        let null_usage = r#"{"choices":[{"delta":{"content":"x"}}],"usage":null}"#;
        assert_eq!(parse_openai(null_usage), vec![Piece::Text("x".into())]);
        let err = r#"{"error":{"message":"model not found"}}"#;
        assert_eq!(parse_openai(err), vec![Piece::Error("model not found".into())]);
        assert!(parse_openai("[DONE]").is_empty());
    }

    /// Formato real de Abliteration AI (grabado el 2026-11-08): envía el razonamiento en
    /// `delta.reasoning`, que no se muestra, y el uso de tokens al final.
    #[test]
    fn parses_recorded_abliteration_stream() {
        let raw = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"model\":\"abliterated-model\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"logprobs\":null,\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning\":\"Thinking\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\\n\\nhola\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[],\"usage\":{\"prompt_tokens\":19,\"completion_tokens\":171}}\n\n",
            "data: [DONE]\n\n",
        );
        let mut parser = crate::providers::sse::SseParser::new();
        let pieces: Vec<Piece> = parser
            .feed(raw.as_bytes())
            .iter()
            .flat_map(|e| parse_openai(&e.data))
            .collect();
        assert_eq!(
            pieces,
            vec![
                Piece::Text("\n\nhola".into()),
                Piece::Stop("stop".into()),
                Piece::Usage { input: Some(19), output: Some(171) },
            ]
        );
    }

    #[test]
    fn usage_is_requested_only_from_services_known_to_support_it() {
        let client = reqwest::Client::new();
        let h = history();
        let body_of = |base: &str| {
            let target = Target::Compatible { base_url: base.into() };
            let req = build_chat_request(&client, &target, "k", "m", None, &h).build().unwrap();
            serde_json::from_slice::<Value>(req.body().unwrap().as_bytes().unwrap()).unwrap()
        };
        assert!(body_of("https://api.abliteration.ai/v1").get("stream_options").is_some());
        assert!(body_of("https://openrouter.ai/api/v1").get("stream_options").is_some());
        assert!(body_of("http://localhost:11434/v1").get("stream_options").is_none());
    }

    #[test]
    fn explains_abnormal_stops() {
        assert!(stop_notice("max_tokens").is_some());
        assert!(stop_notice("length").is_some());
        assert!(stop_notice("refusal").is_some());
        assert!(stop_notice("end_turn").is_none());
        assert!(stop_notice("stop").is_none());
    }
}
