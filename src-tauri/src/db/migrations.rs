//! Migraciones con versión.
//!
//! La versión del esquema se guarda en `PRAGMA user_version`.
//! Para cambiar el esquema, añade una migración nueva al final de `MIGRATIONS`;
//! nunca modifiques una que ya se haya publicado.

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

const MIGRATIONS: &[&str] = &[
    // v1: esquema inicial
    r#"
    CREATE TABLE conversations (
        id            TEXT PRIMARY KEY,
        title         TEXT NOT NULL,
        model         TEXT NOT NULL,
        system_prompt TEXT,
        created_at    TEXT NOT NULL,
        updated_at    TEXT NOT NULL
    );

    CREATE INDEX idx_conversations_updated ON conversations (updated_at DESC);

    CREATE TABLE messages (
        id              TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
        role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
        content         TEXT NOT NULL,
        model           TEXT,
        input_tokens    INTEGER,
        output_tokens   INTEGER,
        created_at      TEXT NOT NULL
    );

    CREATE INDEX idx_messages_conversation ON messages (conversation_id, created_at);

    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
];

/// Versión del esquema que espera esta versión de la app.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Aplica, en orden y dentro de una transacción cada una, las migraciones pendientes.
pub fn run(conn: &mut Connection) -> AppResult<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if current > SCHEMA_VERSION {
        return Err(AppError::Internal(format!(
            "la base de datos es de una versión más nueva de la app (esquema {current}, esta app usa {SCHEMA_VERSION})"
        )));
    }

    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
    }

    Ok(())
}
