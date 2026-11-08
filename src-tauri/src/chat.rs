//! Enviar un mensaje y recibir la respuesta en streaming.
//!
//! Flujo: guardar el mensaje del usuario → enviar el historial al proveedor →
//! reenviar cada trozo de texto a la interfaz por un `Channel` → guardar la respuesta.
//! Se puede detener en cualquier momento; lo ya recibido se guarda.

use std::collections::HashMap;
use std::sync::Mutex;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::ipc::Channel;
use tokio::sync::watch;

use crate::db::models::{Conversation, Message, Role, TokenUsage};
use crate::db::{repo, Db};
use crate::error::{AppError, AppResult};
use crate::keys::{Keys, Provider};
use crate::providers::check::{provider_detail, Target};
use crate::providers::compatible;
use crate::providers::sse::{SseEvent, SseParser};
use crate::providers::stream::{self, Piece};

/// Eventos que recibe la interfaz mientras llega la respuesta.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StreamEvent {
    /// El mensaje del usuario ya está guardado (en regenerar no hay mensaje nuevo).
    Started { user_message: Option<Message> },
    /// Un trozo de texto de la respuesta.
    Delta { text: String },
}

/// Resultado final de un envío.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatOutcome {
    /// La conversación actualizada (puede tener título nuevo).
    pub conversation: Conversation,
    pub user_message: Option<Message>,
    pub assistant_message: Option<Message>,
    pub stop_reason: Option<String>,
    /// Aviso para mostrar bajo la respuesta (cortada, detenida, error a mitad…).
    pub notice: Option<String>,
    pub stopped: bool,
}

pub enum Mode {
    /// Mensaje nuevo. Con `edit_from`, antes se borra ese mensaje y todo lo posterior.
    Send { content: String, edit_from: Option<String> },
    /// Volver a generar la última respuesta.
    Regenerate,
}

/// Respuestas en curso, una como máximo por conversación, con su señal de parada.
#[derive(Default)]
pub struct Generations(Mutex<HashMap<String, watch::Sender<bool>>>);

impl Generations {
    fn start(&self, conversation_id: &str) -> AppResult<(watch::Receiver<bool>, GenerationGuard<'_>)> {
        let mut map = self.0.lock().map_err(|_| AppError::Internal("estado interno bloqueado".into()))?;
        if map.contains_key(conversation_id) {
            return Err(AppError::Invalid("ya hay una respuesta en curso en esta conversación".into()));
        }
        let (tx, rx) = watch::channel(false);
        map.insert(conversation_id.to_string(), tx);
        Ok((
            rx,
            GenerationGuard {
                gens: self,
                id: conversation_id.to_string(),
            },
        ))
    }

    /// Pide detener la respuesta en curso. Devuelve `false` si no había ninguna.
    pub fn stop(&self, conversation_id: &str) -> bool {
        match self.0.lock() {
            Ok(map) => match map.get(conversation_id) {
                Some(tx) => {
                    tx.send_replace(true);
                    true
                }
                None => false,
            },
            Err(_) => false,
        }
    }
}

/// Al terminar (bien, mal o detenida), la conversación deja de estar "en curso".
struct GenerationGuard<'a> {
    gens: &'a Generations,
    id: String,
}

impl Drop for GenerationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut map) = self.gens.0.lock() {
            map.remove(&self.id);
        }
    }
}

async fn cancelled(rx: &mut watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "Anthropic",
        Provider::OpenAi => "OpenAI",
        Provider::Compatible => "El proveedor",
    }
}

/// Mensaje claro para un error HTTP al pedir la respuesta.
pub fn chat_error_message(provider: Provider, model: &str, status: u16, detail: Option<&str>) -> String {
    let name = provider_name(provider);
    let base = match status {
        401 => format!("{name} rechazó la API key. Revísala en Ajustes."),
        402 => format!("La cuenta de {name} no tiene saldo o tiene un problema de facturación."),
        403 => "La API key no tiene permiso para usar este modelo.".to_string(),
        404 => format!("No se encontró el modelo «{model}». Elige otro en el selector de modelos."),
        429 => format!("Se superó el límite de peticiones de {name}. Espera un momento y vuelve a intentarlo."),
        500..=599 => format!("{name} está saturado o no disponible ahora (HTTP {status}). Inténtalo en unos minutos."),
        _ => format!("{name} rechazó la petición (HTTP {status})."),
    };
    match detail {
        Some(d) => format!("{base} Detalle: «{d}»"),
        None => base,
    }
}

fn network_error(provider: Provider, err: &reqwest::Error) -> AppError {
    let name = provider_name(provider);
    if err.is_timeout() {
        AppError::Network(format!("{name} dejó de responder. Revisa tu conexión e inténtalo de nuevo."))
    } else {
        AppError::Network(format!("No se pudo conectar con {name}. Revisa tu conexión a internet."))
    }
}

/// Acumula la respuesta mientras llega.
#[derive(Default)]
struct Accumulator {
    text: String,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    stop_reason: Option<String>,
    error: Option<String>,
}

impl Accumulator {
    fn apply(&mut self, pieces: Vec<Piece>, channel: &Channel<StreamEvent>) {
        for piece in pieces {
            match piece {
                Piece::Text(t) => {
                    self.text.push_str(&t);
                    // Si la interfaz se cerró, se sigue igualmente para guardar la respuesta.
                    let _ = channel.send(StreamEvent::Delta { text: t });
                }
                Piece::Usage { input, output } => {
                    self.input_tokens = input.or(self.input_tokens);
                    self.output_tokens = output.or(self.output_tokens);
                }
                Piece::Stop(reason) => self.stop_reason = Some(reason),
                Piece::Error(message) => self.error = Some(message),
            }
        }
    }

    fn usage(&self) -> Option<TokenUsage> {
        (self.input_tokens.is_some() || self.output_tokens.is_some()).then(|| TokenUsage {
            input_tokens: self.input_tokens.unwrap_or(0),
            output_tokens: self.output_tokens.unwrap_or(0),
        })
    }
}

fn pieces_for(target: &Target, event: &SseEvent) -> Vec<Piece> {
    match target {
        Target::Anthropic => stream::parse_anthropic(event.event.as_deref(), &event.data),
        Target::OpenAi | Target::Compatible { .. } => stream::parse_openai(&event.data),
    }
}

pub async fn run(
    db: &Db,
    keys: &Keys,
    http: &reqwest::Client,
    gens: &Generations,
    conversation_id: &str,
    mode: Mode,
    channel: &Channel<StreamEvent>,
) -> AppResult<ChatOutcome> {
    let (mut cancel_rx, _guard) = gens.start(conversation_id)?;

    // 1. Comprobar todo lo necesario antes de tocar la conversación.
    let (conv, base_url) = db.with(|c| {
        let conv = repo::get_conversation(c, conversation_id)?;
        let base_url = match conv.provider {
            Provider::Compatible => Some(compatible::require_base_url(c)?),
            _ => None,
        };
        Ok((conv, base_url))
    })?;
    let key = keys.secret(conv.provider)?;
    let target = match conv.provider {
        Provider::Anthropic => Target::Anthropic,
        Provider::OpenAi => Target::OpenAi,
        Provider::Compatible => Target::Compatible {
            base_url: base_url.unwrap_or_default(),
        },
    };

    // 2. Guardar el mensaje del usuario (o preparar la regeneración) y leer el historial.
    let (user_message, history) = db.with(|c| {
        let user_message = match &mode {
            Mode::Send { content, edit_from } => {
                let content = content.trim();
                if content.is_empty() {
                    return Err(AppError::Invalid("escribe un mensaje".into()));
                }
                if let Some(id) = edit_from {
                    repo::delete_messages_from(c, conversation_id, id)?;
                }
                let m = repo::add_message(c, conversation_id, Role::User, content, None, None)?;
                repo::auto_title(c, conversation_id, content)?;
                Some(m)
            }
            Mode::Regenerate => {
                if let Some(last) = repo::last_message(c, conversation_id)? {
                    if last.role == Role::Assistant {
                        repo::delete_messages_from(c, conversation_id, &last.id)?;
                    }
                }
                None
            }
        };
        let history = repo::list_messages(c, conversation_id)?;
        if history.last().map(|m| m.role) != Some(Role::User) {
            return Err(AppError::Invalid("no hay ningún mensaje al que responder".into()));
        }
        Ok((user_message, history))
    })?;
    let _ = channel.send(StreamEvent::Started {
        user_message: user_message.clone(),
    });

    // 3. Pedir la respuesta.
    let request = stream::build_chat_request(
        http,
        &target,
        &key,
        &conv.model,
        conv.system_prompt.as_deref(),
        &history,
    );
    let response = tokio::select! {
        r = request.send() => r.map_err(|e| network_error(conv.provider, &e))?,
        _ = cancelled(&mut cancel_rx) => {
            return finish(db, conversation_id, &conv, user_message, Accumulator::default(), true);
        }
    };
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let detail = provider_detail(&body);
        return Err(AppError::Provider {
            status: status.as_u16(),
            message: chat_error_message(conv.provider, &conv.model, status.as_u16(), detail.as_deref()),
        });
    }

    // 4. Leer el stream hasta el final, un error o la orden de parar.
    let mut parser = SseParser::new();
    let mut acc = Accumulator::default();
    let mut bytes = response.bytes_stream();
    let mut stopped = false;
    loop {
        tokio::select! {
            chunk = bytes.next() => match chunk {
                Some(Ok(chunk)) => {
                    for event in parser.feed(&chunk) {
                        acc.apply(pieces_for(&target, &event), channel);
                    }
                }
                Some(Err(err)) => {
                    acc.error = Some(network_error(conv.provider, &err).to_string());
                    break;
                }
                None => {
                    if let Some(event) = parser.finish() {
                        acc.apply(pieces_for(&target, &event), channel);
                    }
                    break;
                }
            },
            _ = cancelled(&mut cancel_rx) => {
                stopped = true;
                break;
            }
        }
        if acc.error.is_some() {
            break;
        }
    }

    // Un error antes de recibir texto es un fallo; con texto parcial, se guarda lo recibido.
    if let Some(error) = &acc.error {
        if acc.text.trim().is_empty() {
            return Err(AppError::Provider {
                status: 0,
                message: format!("{} interrumpió la respuesta: {error}", provider_name(conv.provider)),
            });
        }
    }
    finish(db, conversation_id, &conv, user_message, acc, stopped)
}

/// Guarda la respuesta (si hay texto) y arma el resultado.
fn finish(
    db: &Db,
    conversation_id: &str,
    conv: &Conversation,
    user_message: Option<Message>,
    acc: Accumulator,
    stopped: bool,
) -> AppResult<ChatOutcome> {
    let notice = if let Some(error) = &acc.error {
        Some(format!("La respuesta se interrumpió: {error}"))
    } else if stopped {
        Some("Respuesta detenida.".to_string())
    } else {
        acc.stop_reason.as_deref().and_then(stream::stop_notice).map(String::from)
    };

    db.with(|c| {
        // Algunos modelos empiezan con saltos de línea; no aportan nada al guardarlo.
        let text = acc.text.trim();
        let assistant_message = if text.is_empty() {
            None
        } else {
            Some(repo::add_message(
                c,
                conversation_id,
                Role::Assistant,
                text,
                Some(&conv.model),
                acc.usage(),
            )?)
        };
        Ok(ChatOutcome {
            conversation: repo::get_conversation(c, conversation_id)?,
            user_message,
            assistant_message,
            stop_reason: acc.stop_reason.clone(),
            notice,
            stopped,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_are_specific_and_include_detail() {
        let m = chat_error_message(Provider::Compatible, "x", 401, Some("API key is invalid"));
        assert!(m.contains("rechazó la API key"));
        assert!(m.contains("«API key is invalid»"));
        assert!(chat_error_message(Provider::Anthropic, "claude-x", 404, None).contains("«claude-x»"));
        assert!(chat_error_message(Provider::OpenAi, "m", 429, None).contains("límite"));
        assert!(chat_error_message(Provider::Anthropic, "m", 529, None).contains("saturado"));
        assert!(chat_error_message(Provider::Anthropic, "m", 402, None).contains("saldo"));
    }

    #[test]
    fn generations_allow_one_response_per_conversation() {
        let gens = Generations::default();
        let (_rx, guard) = gens.start("a").unwrap();
        assert!(matches!(gens.start("a"), Err(e) if e.kind() == "invalid"));
        assert!(gens.start("b").is_ok(), "otra conversación sí puede");
        drop(guard);
        assert!(gens.start("a").is_ok(), "al terminar se libera");
    }

    #[tokio::test]
    async fn stop_wakes_up_the_waiting_response() {
        let gens = Generations::default();
        let (mut rx, _guard) = gens.start("a").unwrap();
        assert!(!gens.stop("otra"));
        assert!(gens.stop("a"));
        tokio::time::timeout(std::time::Duration::from_secs(1), cancelled(&mut rx))
            .await
            .expect("la señal de parada debería llegar");
    }

    #[test]
    fn accumulator_merges_usage_and_text() {
        let mut acc = Accumulator::default();
        assert_eq!(acc.usage(), None);
        acc.text.push_str("hola");
        acc.input_tokens = Some(10);
        acc.output_tokens = None;
        assert_eq!(acc.usage(), Some(TokenUsage { input_tokens: 10, output_tokens: 0 }));
    }
}
