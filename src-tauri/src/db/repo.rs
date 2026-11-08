//! Operaciones sobre la base de datos.
//!
//! Cada función recibe la conexión, así se pueden probar con una base de datos en memoria
//! y combinar varias dentro de una misma transacción si hace falta.

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use super::models::{Conversation, Message, Role, TokenUsage};
use crate::error::{AppError, AppResult};

pub const DEFAULT_TITLE: &str = "Nuevo chat";
const MAX_TITLE_CHARS: usize = 200;
const MAX_SETTING_KEY_CHARS: usize = 100;

/// Marca de tiempo UTC con microsegundos, ordenable como texto.
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn conversation_from_row(row: &Row) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get("id")?,
        title: row.get("title")?,
        model: row.get("model")?,
        system_prompt: row.get("system_prompt")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn message_from_row(row: &Row) -> rusqlite::Result<(Message, String)> {
    let role: String = row.get("role")?;
    Ok((
        Message {
            id: row.get("id")?,
            conversation_id: row.get("conversation_id")?,
            // Se valida fuera del closure porque rusqlite no acepta AppError aquí.
            role: Role::User,
            content: row.get("content")?,
            model: row.get("model")?,
            input_tokens: row.get("input_tokens")?,
            output_tokens: row.get("output_tokens")?,
            created_at: row.get("created_at")?,
        },
        role,
    ))
}

fn finish_message((mut message, role): (Message, String)) -> AppResult<Message> {
    message.role = Role::parse(&role)?;
    Ok(message)
}

fn clean_title(title: &str) -> AppResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::Invalid("el título no puede estar vacío".into()));
    }
    Ok(title.chars().take(MAX_TITLE_CHARS).collect())
}

fn clean_model(model: &str) -> AppResult<String> {
    let model = model.trim();
    if model.is_empty() {
        return Err(AppError::Invalid("el modelo no puede estar vacío".into()));
    }
    Ok(model.to_string())
}

fn clean_prompt(prompt: Option<&str>) -> Option<String> {
    prompt.map(str::trim).filter(|p| !p.is_empty()).map(String::from)
}

/// Escapa `%`, `_` y `\` para usar el texto dentro de un LIKE.
fn like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn ensure_changed(rows: usize, what: &str) -> AppResult<()> {
    if rows == 0 {
        Err(AppError::NotFound(what.to_string()))
    } else {
        Ok(())
    }
}

// ───────────────────────── Conversaciones ─────────────────────────

pub fn create_conversation(
    conn: &Connection,
    title: Option<&str>,
    model: &str,
    system_prompt: Option<&str>,
) -> AppResult<Conversation> {
    let title = match title {
        Some(t) if !t.trim().is_empty() => clean_title(t)?,
        _ => DEFAULT_TITLE.to_string(),
    };
    let conversation = Conversation {
        id: new_id(),
        title,
        model: clean_model(model)?,
        system_prompt: clean_prompt(system_prompt),
        created_at: now(),
        updated_at: now(),
    };
    conn.execute(
        "INSERT INTO conversations (id, title, model, system_prompt, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            conversation.id,
            conversation.title,
            conversation.model,
            conversation.system_prompt,
            conversation.created_at,
            conversation.updated_at,
        ],
    )?;
    Ok(conversation)
}

/// Todas las conversaciones, la más reciente primero.
pub fn list_conversations(conn: &Connection) -> AppResult<Vec<Conversation>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM conversations ORDER BY updated_at DESC, rowid DESC",
    )?;
    let rows = stmt.query_map([], conversation_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_conversation(conn: &Connection, id: &str) -> AppResult<Conversation> {
    conn.query_row(
        "SELECT * FROM conversations WHERE id = ?1",
        [id],
        conversation_from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound("la conversación".into()))
}

pub fn rename_conversation(conn: &Connection, id: &str, title: &str) -> AppResult<Conversation> {
    let rows = conn.execute(
        "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![clean_title(title)?, now(), id],
    )?;
    ensure_changed(rows, "la conversación")?;
    get_conversation(conn, id)
}

pub fn set_conversation_model(conn: &Connection, id: &str, model: &str) -> AppResult<Conversation> {
    let rows = conn.execute(
        "UPDATE conversations SET model = ?1, updated_at = ?2 WHERE id = ?3",
        params![clean_model(model)?, now(), id],
    )?;
    ensure_changed(rows, "la conversación")?;
    get_conversation(conn, id)
}

pub fn set_system_prompt(
    conn: &Connection,
    id: &str,
    system_prompt: Option<&str>,
) -> AppResult<Conversation> {
    let rows = conn.execute(
        "UPDATE conversations SET system_prompt = ?1, updated_at = ?2 WHERE id = ?3",
        params![clean_prompt(system_prompt), now(), id],
    )?;
    ensure_changed(rows, "la conversación")?;
    get_conversation(conn, id)
}

/// Borra la conversación y, en cascada, todos sus mensajes.
pub fn delete_conversation(conn: &Connection, id: &str) -> AppResult<()> {
    let rows = conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
    ensure_changed(rows, "la conversación")
}

/// Conversaciones cuyo título o mensajes contienen `query` (sin distinguir mayúsculas en ASCII).
pub fn search_conversations(conn: &Connection, query: &str) -> AppResult<Vec<Conversation>> {
    let query = query.trim();
    if query.is_empty() {
        return list_conversations(conn);
    }
    let pattern = like_pattern(query);
    let mut stmt = conn.prepare(
        "SELECT c.* FROM conversations c
         WHERE c.title LIKE ?1 ESCAPE '\\'
            OR EXISTS (
                SELECT 1 FROM messages m
                WHERE m.conversation_id = c.id AND m.content LIKE ?1 ESCAPE '\\'
            )
         ORDER BY c.updated_at DESC, c.rowid DESC",
    )?;
    let rows = stmt.query_map([pattern], conversation_from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ───────────────────────── Mensajes ─────────────────────────

/// Añade un mensaje y mueve la conversación al principio de la lista.
// Lo usa el envío de mensajes de la fase 5; hasta entonces solo lo usan las pruebas.
#[allow(dead_code)]
pub fn add_message(
    conn: &mut Connection,
    conversation_id: &str,
    role: Role,
    content: &str,
    model: Option<&str>,
    usage: Option<TokenUsage>,
) -> AppResult<Message> {
    let message = Message {
        id: new_id(),
        conversation_id: conversation_id.to_string(),
        role,
        content: content.to_string(),
        model: model.map(String::from),
        input_tokens: usage.map(|u| u.input_tokens),
        output_tokens: usage.map(|u| u.output_tokens),
        created_at: now(),
    };

    let tx = conn.transaction()?;
    let rows = tx.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        params![message.created_at, conversation_id],
    )?;
    ensure_changed(rows, "la conversación")?;
    tx.execute(
        "INSERT INTO messages
            (id, conversation_id, role, content, model, input_tokens, output_tokens, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            message.id,
            message.conversation_id,
            message.role.as_str(),
            message.content,
            message.model,
            message.input_tokens,
            message.output_tokens,
            message.created_at,
        ],
    )?;
    tx.commit()?;
    Ok(message)
}

/// Mensajes de una conversación en orden de llegada.
pub fn list_messages(conn: &Connection, conversation_id: &str) -> AppResult<Vec<Message>> {
    get_conversation(conn, conversation_id)?;
    let mut stmt = conn.prepare(
        "SELECT * FROM messages WHERE conversation_id = ?1 ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map([conversation_id], message_from_row)?;
    rows.map(|r| finish_message(r?)).collect()
}

// ───────────────────────── Ajustes ─────────────────────────

fn check_setting_key(key: &str) -> AppResult<()> {
    if key.trim().is_empty() || key.chars().count() > MAX_SETTING_KEY_CHARS {
        return Err(AppError::Invalid("clave de ajuste no válida".into()));
    }
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    check_setting_key(key)?;
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0))
        .optional()?)
}

/// Crea o reemplaza un ajuste. No guardes API keys aquí: usa el gestor de keys.
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    check_setting_key(key)?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ───────────────────────── Estado ─────────────────────────

pub fn count_rows(conn: &Connection) -> AppResult<(i64, i64)> {
    let conversations = conn.query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))?;
    let messages = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    Ok((conversations, messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, SCHEMA_VERSION};

    const MODEL: &str = "claude-sonnet-5-5";

    fn db() -> Db {
        Db::open_in_memory().expect("abrir base de datos en memoria")
    }

    #[test]
    fn migrations_set_schema_version_and_are_idempotent() {
        let db = db();
        db.with(|c| {
            let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            assert_eq!(v, SCHEMA_VERSION);
            // Volver a ejecutarlas no debe fallar ni cambiar nada.
            super::super::migrations::run(c)?;
            let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            assert_eq!(v, SCHEMA_VERSION);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn rejects_database_from_newer_app() {
        let db = db();
        let err = db
            .with(|c| {
                c.pragma_update(None, "user_version", SCHEMA_VERSION + 1)?;
                super::super::migrations::run(c)
            })
            .unwrap_err();
        assert_eq!(err.kind(), "internal");
    }

    #[test]
    fn create_uses_default_title_and_trims_input() {
        let db = db();
        db.with(|c| {
            let a = create_conversation(c, None, MODEL, None)?;
            assert_eq!(a.title, DEFAULT_TITLE);
            assert_eq!(a.system_prompt, None);

            let b = create_conversation(c, Some("  Plan  "), &format!(" {MODEL} "), Some("   "))?;
            assert_eq!(b.title, "Plan");
            assert_eq!(b.model, MODEL);
            assert_eq!(b.system_prompt, None, "un prompt vacío se guarda como NULL");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn create_rejects_empty_model() {
        let db = db();
        let err = db.with(|c| create_conversation(c, None, "  ", None)).unwrap_err();
        assert_eq!(err.kind(), "invalid");
    }

    #[test]
    fn new_message_moves_conversation_to_top() {
        let db = db();
        db.with(|c| {
            let first = create_conversation(c, Some("Primero"), MODEL, None)?;
            std::thread::sleep(std::time::Duration::from_millis(2));
            let second = create_conversation(c, Some("Segundo"), MODEL, None)?;

            let ids: Vec<_> = list_conversations(c)?.into_iter().map(|x| x.id).collect();
            assert_eq!(ids, vec![second.id.clone(), first.id.clone()]);

            std::thread::sleep(std::time::Duration::from_millis(2));
            add_message(c, &first.id, Role::User, "hola", None, None)?;

            let ids: Vec<_> = list_conversations(c)?.into_iter().map(|x| x.id).collect();
            assert_eq!(ids, vec![first.id, second.id]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn messages_keep_order_role_and_usage() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, None, MODEL, None)?;
            add_message(c, &conv.id, Role::User, "pregunta", None, None)?;
            let usage = TokenUsage { input_tokens: 12, output_tokens: 34 };
            add_message(c, &conv.id, Role::Assistant, "respuesta", Some(MODEL), Some(usage))?;

            let msgs = list_messages(c, &conv.id)?;
            assert_eq!(msgs.len(), 2);
            assert_eq!(msgs[0].role, Role::User);
            assert_eq!(msgs[0].content, "pregunta");
            assert_eq!(msgs[1].role, Role::Assistant);
            assert_eq!(msgs[1].model.as_deref(), Some(MODEL));
            assert_eq!(msgs[1].input_tokens, Some(12));
            assert_eq!(msgs[1].output_tokens, Some(34));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn add_message_to_missing_conversation_is_not_found() {
        let db = db();
        let err = db
            .with(|c| add_message(c, "no-existe", Role::User, "hola", None, None))
            .unwrap_err();
        assert_eq!(err.kind(), "not_found");
    }

    #[test]
    fn rename_validates_and_updates() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, None, MODEL, None)?;
            let renamed = rename_conversation(c, &conv.id, "  Ideas  ")?;
            assert_eq!(renamed.title, "Ideas");

            let long = "x".repeat(MAX_TITLE_CHARS + 50);
            let renamed = rename_conversation(c, &conv.id, &long)?;
            assert_eq!(renamed.title.chars().count(), MAX_TITLE_CHARS);

            assert_eq!(rename_conversation(c, &conv.id, "   ").unwrap_err().kind(), "invalid");
            assert_eq!(rename_conversation(c, "no-existe", "Hola").unwrap_err().kind(), "not_found");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn model_and_system_prompt_can_change() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, None, MODEL, None)?;
            let conv2 = set_conversation_model(c, &conv.id, "gpt-modelo")?;
            assert_eq!(conv2.model, "gpt-modelo");
            let conv3 = set_system_prompt(c, &conv.id, Some("Responde en español"))?;
            assert_eq!(conv3.system_prompt.as_deref(), Some("Responde en español"));
            let conv4 = set_system_prompt(c, &conv.id, None)?;
            assert_eq!(conv4.system_prompt, None);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn delete_cascades_to_messages() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, None, MODEL, None)?;
            add_message(c, &conv.id, Role::User, "hola", None, None)?;
            add_message(c, &conv.id, Role::Assistant, "hola, ¿qué tal?", Some(MODEL), None)?;
            assert_eq!(count_rows(c)?, (1, 2));

            delete_conversation(c, &conv.id)?;
            assert_eq!(count_rows(c)?, (0, 0));
            assert_eq!(delete_conversation(c, &conv.id).unwrap_err().kind(), "not_found");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_matches_title_or_content_and_escapes_wildcards() {
        let db = db();
        db.with(|c| {
            let by_title = create_conversation(c, Some("Receta de pasta"), MODEL, None)?;
            let by_content = create_conversation(c, Some("Varios"), MODEL, None)?;
            add_message(c, &by_content.id, Role::User, "Necesito una RECETA rápida", None, None)?;
            let percent = create_conversation(c, Some("Descuento 50%"), MODEL, None)?;
            create_conversation(c, Some("Otra cosa"), MODEL, None)?;

            let mut found: Vec<_> = search_conversations(c, "receta")?.into_iter().map(|x| x.id).collect();
            found.sort();
            let mut expected = vec![by_title.id, by_content.id];
            expected.sort();
            assert_eq!(found, expected);

            // "%" se busca literalmente, no como comodín.
            let found: Vec<_> = search_conversations(c, "50%")?.into_iter().map(|x| x.id).collect();
            assert_eq!(found, vec![percent.id]);
            assert!(search_conversations(c, "%")?.len() == 1);

            // Una búsqueda vacía devuelve todo.
            assert_eq!(search_conversations(c, "  ")?.len(), 4);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn settings_upsert_and_read() {
        let db = db();
        db.with(|c| {
            assert_eq!(get_setting(c, "theme")?, None);
            set_setting(c, "theme", "dark")?;
            assert_eq!(get_setting(c, "theme")?.as_deref(), Some("dark"));
            set_setting(c, "theme", "light")?;
            assert_eq!(get_setting(c, "theme")?.as_deref(), Some("light"));
            assert_eq!(set_setting(c, " ", "x").unwrap_err().kind(), "invalid");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn error_serializes_with_kind_and_message() {
        let json = serde_json::to_value(AppError::NotFound("la conversación".into())).unwrap();
        assert_eq!(json["kind"], "not_found");
        assert_eq!(json["message"], "no se encontró la conversación");
    }
}
