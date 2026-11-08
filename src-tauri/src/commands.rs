//! Comandos que el frontend llama con `invoke`.
//!
//! Los nombres de los argumentos llegan en camelCase desde JavaScript
//! (`systemPrompt`) y Tauri los convierte a snake_case (`system_prompt`).

use serde::Serialize;
use tauri::State;

use crate::db::models::{Conversation, DbStatus, Message};
use crate::db::{repo, Db, SCHEMA_VERSION};
use crate::error::AppResult;
use crate::keys::{KeyStatus, Keys, Provider};
use crate::providers::check::{check_key, KeyCheck, Target};
use crate::providers::compatible::{self, CompatibleConfig};

#[derive(Serialize)]
pub struct AppInfo {
    name: String,
    version: String,
}

/// Nombre y versión de la app. La pantalla base lo usa para confirmar que el IPC funciona.
#[tauri::command]
pub fn app_info(app: tauri::AppHandle) -> AppInfo {
    let pkg = app.package_info();
    AppInfo {
        name: pkg.name.clone(),
        version: pkg.version.to_string(),
    }
}

#[tauri::command]
pub fn db_status(db: State<'_, Db>) -> AppResult<DbStatus> {
    let (conversations, messages) = db.with(|c| repo::count_rows(c))?;
    Ok(DbStatus {
        schema_version: SCHEMA_VERSION,
        conversations,
        messages,
        path: db.path().to_string(),
    })
}

// ───────────────────────── Conversaciones ─────────────────────────

#[tauri::command]
pub fn conversation_create(
    db: State<'_, Db>,
    model: String,
    title: Option<String>,
    system_prompt: Option<String>,
) -> AppResult<Conversation> {
    db.with(|c| repo::create_conversation(c, title.as_deref(), &model, system_prompt.as_deref()))
}

#[tauri::command]
pub fn conversation_list(db: State<'_, Db>) -> AppResult<Vec<Conversation>> {
    db.with(|c| repo::list_conversations(c))
}

#[tauri::command]
pub fn conversation_get(db: State<'_, Db>, id: String) -> AppResult<Conversation> {
    db.with(|c| repo::get_conversation(c, &id))
}

#[tauri::command]
pub fn conversation_search(db: State<'_, Db>, query: String) -> AppResult<Vec<Conversation>> {
    db.with(|c| repo::search_conversations(c, &query))
}

#[tauri::command]
pub fn conversation_rename(db: State<'_, Db>, id: String, title: String) -> AppResult<Conversation> {
    db.with(|c| repo::rename_conversation(c, &id, &title))
}

#[tauri::command]
pub fn conversation_set_model(db: State<'_, Db>, id: String, model: String) -> AppResult<Conversation> {
    db.with(|c| repo::set_conversation_model(c, &id, &model))
}

#[tauri::command]
pub fn conversation_set_system_prompt(
    db: State<'_, Db>,
    id: String,
    system_prompt: Option<String>,
) -> AppResult<Conversation> {
    db.with(|c| repo::set_system_prompt(c, &id, system_prompt.as_deref()))
}

#[tauri::command]
pub fn conversation_delete(db: State<'_, Db>, id: String) -> AppResult<()> {
    db.with(|c| repo::delete_conversation(c, &id))
}

// ───────────────────────── Mensajes ─────────────────────────
// Los mensajes se crean desde el backend al enviar y al recibir la respuesta (fase 5),
// por eso aquí solo se exponen para leerlos.

#[tauri::command]
pub fn message_list(db: State<'_, Db>, conversation_id: String) -> AppResult<Vec<Message>> {
    db.with(|c| repo::list_messages(c, &conversation_id))
}

// ───────────────────────── Ajustes ─────────────────────────

#[tauri::command]
pub fn setting_get(db: State<'_, Db>, key: String) -> AppResult<Option<String>> {
    db.with(|c| repo::get_setting(c, &key))
}

#[tauri::command]
pub fn setting_set(db: State<'_, Db>, key: String, value: String) -> AppResult<()> {
    db.with(|c| repo::set_setting(c, &key, &value))
}

// ───────────────────────── API keys ─────────────────────────
// La key completa solo entra (al guardarla); nunca sale hacia el frontend.

#[tauri::command]
pub fn key_status_all(keys: State<'_, Keys>) -> AppResult<Vec<KeyStatus>> {
    keys.status_all()
}

#[tauri::command]
pub fn key_save(keys: State<'_, Keys>, provider: Provider, key: String) -> AppResult<KeyStatus> {
    keys.save(provider, &key)
}

#[tauri::command]
pub fn key_delete(keys: State<'_, Keys>, provider: Provider) -> AppResult<KeyStatus> {
    keys.delete(provider)
}

/// Comprueba la key guardada con una petición gratuita a la API del proveedor.
#[tauri::command]
pub async fn key_test(
    keys: State<'_, Keys>,
    db: State<'_, Db>,
    http: State<'_, reqwest::Client>,
    provider: Provider,
) -> AppResult<KeyCheck> {
    let target = match provider {
        Provider::Anthropic => Target::Anthropic,
        Provider::OpenAi => Target::OpenAi,
        Provider::Compatible => Target::Compatible {
            base_url: db.with(|c| compatible::require_base_url(c))?,
        },
    };
    let secret = keys.secret(provider)?;
    Ok(check_key(&http, &target, &secret).await)
}

// ───────────────────────── Proveedor compatible con OpenAI ─────────────────────────

#[tauri::command]
pub fn compatible_config_get(db: State<'_, Db>) -> AppResult<Option<CompatibleConfig>> {
    db.with(|c| compatible::load(c))
}

#[tauri::command]
pub fn compatible_config_set(db: State<'_, Db>, name: String, base_url: String) -> AppResult<CompatibleConfig> {
    db.with(|c| compatible::save(c, &name, &base_url))
}
