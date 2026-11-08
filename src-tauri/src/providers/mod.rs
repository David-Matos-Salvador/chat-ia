//! Proveedores de IA: Anthropic, OpenAI y cualquier API compatible con OpenAI.
//!
//! La fase 3 solo comprueba keys; la fase 4 añade el streaming de respuestas.

pub mod check;
pub mod compatible;

use std::time::Duration;

/// API de Anthropic (Claude). Se usa HTTP directo porque no hay SDK oficial para Rust.
pub const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
/// Versión de la API de Anthropic que se envía en el header `anthropic-version`.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// API de OpenAI.
pub const OPENAI_API_BASE: &str = "https://api.openai.com";

/// Cliente HTTP compartido por toda la app.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("chat-ia/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .build()
}
