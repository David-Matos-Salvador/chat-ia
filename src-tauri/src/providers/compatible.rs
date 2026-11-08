//! Proveedor compatible con OpenAI.
//!
//! Sirve para cualquier servicio que implemente la API de OpenAI
//! (`GET {base}/models`, `POST {base}/chat/completions`, auth `Bearer`).
//! El nombre y la URL base se guardan en la tabla `settings`; la key, en el
//! Administrador de credenciales como cualquier otra.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::repo;
use crate::error::{AppError, AppResult};

const SETTING_NAME: &str = "compatible.name";
const SETTING_BASE_URL: &str = "compatible.base_url";
const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleConfig {
    /// Nombre que se muestra en la interfaz, por ejemplo "Abliteration AI".
    pub name: String,
    /// Por ejemplo `https://api.abliteration.ai/v1` (sin barra final).
    pub base_url: String,
}

/// Lee la configuración guardada, si existe.
pub fn load(conn: &Connection) -> AppResult<Option<CompatibleConfig>> {
    let base_url = repo::get_setting(conn, SETTING_BASE_URL)?;
    let name = repo::get_setting(conn, SETTING_NAME)?;
    Ok(base_url.map(|base_url| CompatibleConfig {
        name: name.unwrap_or_else(|| "Proveedor compatible".into()),
        base_url,
    }))
}

/// Valida y guarda la configuración. Devuelve la versión normalizada.
pub fn save(conn: &Connection, name: &str, base_url: &str) -> AppResult<CompatibleConfig> {
    let config = CompatibleConfig {
        name: validate_name(name)?,
        base_url: validate_base_url(base_url)?,
    };
    repo::set_setting(conn, SETTING_NAME, &config.name)?;
    repo::set_setting(conn, SETTING_BASE_URL, &config.base_url)?;
    Ok(config)
}

/// URL base guardada, o un error que explica qué hacer.
pub fn require_base_url(conn: &Connection) -> AppResult<String> {
    load(conn)?
        .map(|c| c.base_url)
        .ok_or_else(|| AppError::Invalid("configura primero la URL base del proveedor compatible".into()))
}

fn validate_name(raw: &str) -> AppResult<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AppError::Invalid("escribe un nombre para el proveedor".into()));
    }
    Ok(name.chars().take(MAX_NAME_CHARS).collect())
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// Acepta `https://…`, o `http://` solo para servidores locales (por ejemplo Ollama).
/// Quita la barra final para poder añadir `/models` o `/chat/completions`.
pub fn validate_base_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::Invalid("escribe la URL base del proveedor".into()));
    }
    let url = reqwest::Url::parse(trimmed)
        .map_err(|_| AppError::Invalid("la URL base no es válida; debe empezar por https://".into()))?;

    let host = url.host_str().unwrap_or_default();
    match url.scheme() {
        "https" => {}
        "http" if is_local_host(host) => {}
        "http" => {
            return Err(AppError::Invalid(
                "usa https://; http:// solo se permite para servidores locales (localhost)".into(),
            ))
        }
        _ => return Err(AppError::Invalid("la URL base debe empezar por https://".into())),
    }
    if host.is_empty() {
        return Err(AppError::Invalid("a la URL base le falta el dominio".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::Invalid(
            "no pongas usuario ni contraseña en la URL; la key va en su propio campo".into(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(AppError::Invalid("la URL base no debe tener parámetros (? o #)".into()));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn accepts_and_normalizes_valid_urls() {
        assert_eq!(
            validate_base_url("  https://api.abliteration.ai/v1/  ").unwrap(),
            "https://api.abliteration.ai/v1"
        );
        assert_eq!(validate_base_url("https://openrouter.ai/api/v1").unwrap(), "https://openrouter.ai/api/v1");
        assert_eq!(validate_base_url("http://localhost:11434/v1").unwrap(), "http://localhost:11434/v1");
        assert_eq!(validate_base_url("http://127.0.0.1:8080/v1").unwrap(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn rejects_unsafe_or_malformed_urls() {
        for bad in [
            "",
            "api.abliteration.ai/v1",
            "http://api.abliteration.ai/v1",
            "ftp://example.com",
            "https://user:pass@example.com/v1",
            "https://example.com/v1?key=abc",
            "https://example.com/v1#x",
        ] {
            assert_eq!(validate_base_url(bad).unwrap_err().kind(), "invalid", "debería rechazar {bad:?}");
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            assert_eq!(load(c)?, None);
            assert_eq!(require_base_url(c).unwrap_err().kind(), "invalid");

            let saved = save(c, "  Abliteration AI ", "https://api.abliteration.ai/v1/")?;
            assert_eq!(saved.name, "Abliteration AI");
            assert_eq!(saved.base_url, "https://api.abliteration.ai/v1");
            assert_eq!(load(c)?, Some(saved));
            assert_eq!(require_base_url(c)?, "https://api.abliteration.ai/v1");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn invalid_config_is_not_saved() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            assert!(save(c, "X", "http://example.com").is_err());
            assert!(save(c, "  ", "https://example.com").is_err());
            assert_eq!(load(c)?, None);
            Ok(())
        })
        .unwrap();
    }
}
