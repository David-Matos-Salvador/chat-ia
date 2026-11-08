//! Operaciones sobre la base de datos.
//!
//! Cada función recibe la conexión, así se pueden probar con una base de datos en memoria
//! y combinar varias dentro de una misma transacción si hace falta.

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use super::models::{Conversation, Message, Role, TokenUsage};
use crate::error::{AppError, AppResult};
use crate::keys::Provider;

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
        provider: row.get("provider")?,
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
    provider: Provider,
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
        provider,
        model: clean_model(model)?,
        system_prompt: clean_prompt(system_prompt),
        created_at: now(),
        updated_at: now(),
    };
    conn.execute(
        "INSERT INTO conversations (id, title, provider, model, system_prompt, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            conversation.id,
            conversation.title,
            conversation.provider,
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

/// Cambia proveedor y modelo. Se puede cambiar a mitad de conversación:
/// el historial se envía al nuevo modelo como texto.
pub fn set_conversation_model(
    conn: &Connection,
    id: &str,
    provider: Provider,
    model: &str,
) -> AppResult<Conversation> {
    let rows = conn.execute(
        "UPDATE conversations SET provider = ?1, model = ?2, updated_at = ?3 WHERE id = ?4",
        params![provider, clean_model(model)?, now(), id],
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

/// Último mensaje de la conversación, si hay alguno.
pub fn last_message(conn: &Connection, conversation_id: &str) -> AppResult<Option<Message>> {
    let row = conn
        .query_row(
            "SELECT * FROM messages WHERE conversation_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [conversation_id],
            message_from_row,
        )
        .optional()?;
    row.map(finish_message).transpose()
}

/// Borra `message_id` y todos los mensajes posteriores de la conversación.
/// Se usa al editar un mensaje (se rehace la conversación desde ahí) y al regenerar.
pub fn delete_messages_from(conn: &Connection, conversation_id: &str, message_id: &str) -> AppResult<usize> {
    let (created_at, rowid): (String, i64) = conn
        .query_row(
            "SELECT created_at, rowid FROM messages WHERE id = ?1 AND conversation_id = ?2",
            [message_id, conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("el mensaje".into()))?;
    Ok(conn.execute(
        "DELETE FROM messages
         WHERE conversation_id = ?1
           AND (created_at > ?2 OR (created_at = ?2 AND rowid >= ?3))",
        params![conversation_id, created_at, rowid],
    )?)
}

/// Crea una conversación nueva con una copia de los mensajes hasta `up_to_message_id`
/// (incluido), con el mismo proveedor, modelo e instrucciones. La original no cambia.
pub fn fork_conversation(conn: &mut Connection, conversation_id: &str, up_to_message_id: &str) -> AppResult<Conversation> {
    let source = get_conversation(conn, conversation_id)?;
    let messages = list_messages(conn, conversation_id)?;
    let end = messages
        .iter()
        .position(|m| m.id == up_to_message_id)
        .ok_or_else(|| AppError::NotFound("el mensaje".into()))?;

    let title: String = format!("{} (bifurcación)", source.title).chars().take(MAX_TITLE_CHARS).collect();
    let tx = conn.transaction()?;
    let fork = create_conversation(&tx, source.provider, Some(&title), &source.model, source.system_prompt.as_deref())?;
    for m in &messages[..=end] {
        tx.execute(
            "INSERT INTO messages
                (id, conversation_id, role, content, model, input_tokens, output_tokens, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                new_id(),
                fork.id,
                m.role.as_str(),
                m.content,
                m.model,
                m.input_tokens,
                m.output_tokens,
                m.created_at,
            ],
        )?;
    }
    tx.commit()?;
    Ok(fork)
}

const AUTO_TITLE_CHARS: usize = 48;

/// Título a partir del primer mensaje: primera línea, recortada en una palabra completa.
pub fn title_from_text(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.chars().count() <= AUTO_TITLE_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(AUTO_TITLE_CHARS).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > AUTO_TITLE_CHARS / 2 => &cut[..i],
        _ => cut.as_str(),
    };
    format!("{}…", cut.trim_end())
}

/// Si la conversación aún tiene el título por defecto, le pone uno a partir de `text`.
pub fn auto_title(conn: &Connection, conversation_id: &str, text: &str) -> AppResult<()> {
    let title = title_from_text(text);
    if title.is_empty() {
        return Ok(());
    }
    conn.execute(
        "UPDATE conversations SET title = ?1 WHERE id = ?2 AND title = ?3",
        params![title, conversation_id, DEFAULT_TITLE],
    )?;
    Ok(())
}

/// Conversación completa en Markdown, para exportarla.
pub fn export_markdown(conn: &Connection, conversation_id: &str) -> AppResult<String> {
    let conv = get_conversation(conn, conversation_id)?;
    let mut out = format!("# {}\n\n", conv.title);
    out.push_str(&format!("_Modelo: {} · Creada: {}_\n\n", conv.model, conv.created_at));
    if let Some(prompt) = &conv.system_prompt {
        out.push_str(&format!("> **Instrucciones:** {}\n\n", prompt.replace('\n', "\n> ")));
    }
    for m in list_messages(conn, conversation_id)? {
        let who = match m.role {
            Role::User => "Tú",
            Role::Assistant => "Asistente",
            Role::System => "Sistema",
        };
        out.push_str(&format!("---\n\n### {who}\n\n{}\n\n", m.content.trim_end()));
    }
    Ok(out)
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
            let a = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
            assert_eq!(a.title, DEFAULT_TITLE);
            assert_eq!(a.system_prompt, None);

            let b = create_conversation(c, Provider::Anthropic, Some("  Plan  "), &format!(" {MODEL} "), Some("   "))?;
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
        let err = db.with(|c| create_conversation(c, Provider::Anthropic, None, "  ", None)).unwrap_err();
        assert_eq!(err.kind(), "invalid");
    }

    #[test]
    fn new_message_moves_conversation_to_top() {
        let db = db();
        db.with(|c| {
            let first = create_conversation(c, Provider::Anthropic, Some("Primero"), MODEL, None)?;
            std::thread::sleep(std::time::Duration::from_millis(2));
            let second = create_conversation(c, Provider::Anthropic, Some("Segundo"), MODEL, None)?;

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
            let conv = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
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
            let conv = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
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
            let conv = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
            let conv2 = set_conversation_model(c, &conv.id, Provider::OpenAi, "gpt-modelo")?;
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
            let conv = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
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
            let by_title = create_conversation(c, Provider::Anthropic, Some("Receta de pasta"), MODEL, None)?;
            let by_content = create_conversation(c, Provider::Anthropic, Some("Varios"), MODEL, None)?;
            add_message(c, &by_content.id, Role::User, "Necesito una RECETA rápida", None, None)?;
            let percent = create_conversation(c, Provider::Anthropic, Some("Descuento 50%"), MODEL, None)?;
            create_conversation(c, Provider::Anthropic, Some("Otra cosa"), MODEL, None)?;

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

    #[test]
    fn provider_is_stored_and_can_change() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, Provider::Compatible, None, "abliterated-model", None)?;
            assert_eq!(get_conversation(c, &conv.id)?.provider, Provider::Compatible);
            let changed = set_conversation_model(c, &conv.id, Provider::Anthropic, MODEL)?;
            assert_eq!((changed.provider, changed.model.as_str()), (Provider::Anthropic, MODEL));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn delete_messages_from_removes_that_message_and_later_ones() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
            let m1 = add_message(c, &conv.id, Role::User, "uno", None, None)?;
            let m2 = add_message(c, &conv.id, Role::Assistant, "dos", Some(MODEL), None)?;
            add_message(c, &conv.id, Role::User, "tres", None, None)?;
            add_message(c, &conv.id, Role::Assistant, "cuatro", Some(MODEL), None)?;

            assert_eq!(delete_messages_from(c, &conv.id, &m2.id)?, 3);
            let left: Vec<_> = list_messages(c, &conv.id)?.into_iter().map(|m| m.id).collect();
            assert_eq!(left, vec![m1.id.clone()]);
            assert_eq!(last_message(c, &conv.id)?.map(|m| m.id), Some(m1.id));

            assert_eq!(delete_messages_from(c, &conv.id, "no-existe").unwrap_err().kind(), "not_found");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn fork_copies_messages_up_to_the_chosen_one_and_keeps_the_original() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, Provider::Compatible, Some("Plan"), "abliterated-model", Some("Sé breve"))?;
            add_message(c, &conv.id, Role::User, "uno", None, None)?;
            let usage = TokenUsage { input_tokens: 5, output_tokens: 7 };
            let a1 = add_message(c, &conv.id, Role::Assistant, "dos", Some("abliterated-model"), Some(usage))?;
            add_message(c, &conv.id, Role::User, "tres", None, None)?;
            add_message(c, &conv.id, Role::Assistant, "cuatro", Some("abliterated-model"), None)?;

            let fork = fork_conversation(c, &conv.id, &a1.id)?;
            assert_ne!(fork.id, conv.id);
            assert_eq!(fork.title, "Plan (bifurcación)");
            assert_eq!(fork.provider, Provider::Compatible);
            assert_eq!(fork.model, "abliterated-model");
            assert_eq!(fork.system_prompt.as_deref(), Some("Sé breve"));

            let copied = list_messages(c, &fork.id)?;
            let texts: Vec<_> = copied.iter().map(|m| m.content.as_str()).collect();
            assert_eq!(texts, vec!["uno", "dos"]);
            assert_eq!(copied[1].output_tokens, Some(7), "conserva el uso de tokens");
            assert!(copied.iter().all(|m| m.id != a1.id), "los mensajes copiados tienen id nuevo");

            assert_eq!(list_messages(c, &conv.id)?.len(), 4, "la original no cambia");
            assert_eq!(list_conversations(c)?[0].id, fork.id, "la bifurcación aparece primero");
            assert_eq!(fork_conversation(c, &conv.id, "no-existe").unwrap_err().kind(), "not_found");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn last_message_is_none_for_empty_conversation() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
            assert_eq!(last_message(c, &conv.id)?, None);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn title_from_text_uses_first_line_and_cuts_on_a_word() {
        assert_eq!(title_from_text("\n  Hola, ¿qué tal?\nsegunda línea"), "Hola, ¿qué tal?");
        let long = "Explícame cómo funciona el streaming de respuestas en una app de escritorio con Rust";
        let title = title_from_text(long);
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= AUTO_TITLE_CHARS + 1);
        assert!(!title.contains("escritorio con Rust"));
        assert_eq!(title_from_text("   "), "");
    }

    #[test]
    fn auto_title_only_replaces_the_default_title() {
        let db = db();
        db.with(|c| {
            let fresh = create_conversation(c, Provider::Anthropic, None, MODEL, None)?;
            auto_title(c, &fresh.id, "Receta de pan casero")?;
            assert_eq!(get_conversation(c, &fresh.id)?.title, "Receta de pan casero");

            let named = create_conversation(c, Provider::Anthropic, Some("Mi título"), MODEL, None)?;
            auto_title(c, &named.id, "Otra cosa")?;
            assert_eq!(get_conversation(c, &named.id)?.title, "Mi título");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn export_markdown_includes_title_prompt_and_messages() {
        let db = db();
        db.with(|c| {
            let conv = create_conversation(c, Provider::Anthropic, Some("Ideas"), MODEL, Some("Sé breve"))?;
            add_message(c, &conv.id, Role::User, "¿Una idea?", None, None)?;
            add_message(c, &conv.id, Role::Assistant, "Un huerto urbano.", Some(MODEL), None)?;
            let md = export_markdown(c, &conv.id)?;
            assert!(md.starts_with("# Ideas\n"));
            assert!(md.contains("**Instrucciones:** Sé breve"));
            assert!(md.contains("### Tú\n\n¿Una idea?"));
            assert!(md.contains("### Asistente\n\nUn huerto urbano."));
            Ok(())
        })
        .unwrap();
    }
}
