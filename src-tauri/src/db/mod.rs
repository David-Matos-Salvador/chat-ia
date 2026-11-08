//! Base de datos local (SQLite).
//!
//! Guarda conversaciones, mensajes y ajustes. Las API keys NO se guardan aquí:
//! viven en el Administrador de credenciales de Windows (fase 3).

mod migrations;
pub mod models;
pub mod repo;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub use migrations::SCHEMA_VERSION;

/// Conexión compartida. Tauri la guarda como estado de la app.
pub struct Db {
    conn: Mutex<Connection>,
    path: String,
}

impl Db {
    /// Abre (o crea) la base de datos en `path` y aplica las migraciones pendientes.
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::init(Connection::open(path)?, path.display().to_string())
    }

    /// Base de datos en memoria, para las pruebas.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?, ":memory:".into())
    }

    /// Ruta del archivo de la base de datos.
    pub fn path(&self) -> &str {
        &self.path
    }

    fn init(mut conn: Connection, path: String) -> AppResult<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        // WAL permite leer mientras se escribe (por ejemplo, durante el streaming).
        // PRAGMA journal_mode devuelve una fila, por eso se usa query_row.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        migrations::run(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }

    /// Ejecuta `f` con acceso exclusivo a la conexión.
    pub fn with<T>(&self, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Internal("la base de datos quedó bloqueada por un error anterior".into()))?;
        f(&mut conn)
    }
}
