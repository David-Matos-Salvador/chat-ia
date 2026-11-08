//! API keys de los proveedores de IA.
//!
//! Se guardan en el Administrador de credenciales de Windows (cifradas por el sistema
//! con la cuenta del usuario). Reglas:
//! - La key completa nunca vuelve al frontend: solo `configured` y una versión enmascarada.
//! - Nunca se escribe en la base de datos, en archivos ni en los logs.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Nombre del servicio en el Administrador de credenciales.
pub const KEYRING_SERVICE: &str = "com.chatia.app";

const MIN_KEY_CHARS: usize = 20;
const MAX_KEY_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Anthropic, Provider::OpenAi];

    /// Identificador estable (se usa como nombre de usuario en el almacén de credenciales).
    pub fn id(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::OpenAi => "OpenAI",
        }
    }
}

/// Lo que el frontend puede saber de una key.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub provider: Provider,
    pub configured: bool,
    /// Por ejemplo `sk-ant…a1b2`. Nunca la key completa.
    pub masked: Option<String>,
}

/// Almacén de secretos. Se abstrae para poder probar la lógica sin tocar Windows.
pub trait KeyStore: Send + Sync {
    fn get(&self, provider: Provider) -> AppResult<Option<String>>;
    fn set(&self, provider: Provider, key: &str) -> AppResult<()>;
    /// Devuelve `true` si había una key y se borró.
    fn delete(&self, provider: Provider) -> AppResult<bool>;
}

/// Almacén real: Administrador de credenciales de Windows (vía `keyring`).
pub struct SystemKeyStore {
    service: String,
}

impl SystemKeyStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, provider: Provider) -> AppResult<keyring::Entry> {
        Ok(keyring::Entry::new(&self.service, provider.id())?)
    }
}

impl KeyStore for SystemKeyStore {
    fn get(&self, provider: Provider) -> AppResult<Option<String>> {
        match self.entry(provider)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn set(&self, provider: Provider, key: &str) -> AppResult<()> {
        Ok(self.entry(provider)?.set_password(key)?)
    }

    fn delete(&self, provider: Provider) -> AppResult<bool> {
        match self.entry(provider)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(err) => Err(err.into()),
        }
    }
}

/// Gestor de keys. Tauri lo guarda como estado de la app.
pub struct Keys {
    store: Box<dyn KeyStore>,
}

impl Keys {
    pub fn new(store: impl KeyStore + 'static) -> Self {
        Self {
            store: Box::new(store),
        }
    }

    pub fn status(&self, provider: Provider) -> AppResult<KeyStatus> {
        let key = self.store.get(provider)?;
        Ok(KeyStatus {
            provider,
            configured: key.is_some(),
            masked: key.as_deref().map(mask),
        })
    }

    pub fn status_all(&self) -> AppResult<Vec<KeyStatus>> {
        Provider::ALL.into_iter().map(|p| self.status(p)).collect()
    }

    /// Valida, guarda y devuelve el estado (enmascarado).
    pub fn save(&self, provider: Provider, raw: &str) -> AppResult<KeyStatus> {
        let key = validate_key(raw)?;
        self.store.set(provider, &key)?;
        self.status(provider)
    }

    /// Borra la key. Si no había ninguna, no es un error.
    pub fn delete(&self, provider: Provider) -> AppResult<KeyStatus> {
        self.store.delete(provider)?;
        self.status(provider)
    }

    /// Key completa para hacer peticiones. Solo para uso interno del backend.
    pub fn secret(&self, provider: Provider) -> AppResult<String> {
        self.store
            .get(provider)?
            .ok_or_else(|| AppError::NotFound(format!("una API key de {}; añádela en Ajustes", provider.label())))
    }
}

/// Limpia y valida el texto pegado por el usuario.
pub fn validate_key(raw: &str) -> AppResult<String> {
    let key = raw.trim();
    if key.is_empty() {
        return Err(AppError::Invalid("pega una API key".into()));
    }
    if key.chars().any(char::is_whitespace) {
        return Err(AppError::Invalid("la API key no puede tener espacios ni saltos de línea".into()));
    }
    let len = key.chars().count();
    if len < MIN_KEY_CHARS {
        return Err(AppError::Invalid("la API key es demasiado corta; revisa que la copiaste completa".into()));
    }
    if len > MAX_KEY_CHARS {
        return Err(AppError::Invalid("la API key es demasiado larga".into()));
    }
    Ok(key.to_string())
}

/// `sk-ant-api03-abc…xyz1234` → `sk-ant…1234`.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() < 12 {
        return "••••".into();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Almacén en memoria para las pruebas.
    #[derive(Default)]
    pub struct MemoryKeyStore(Mutex<HashMap<Provider, String>>);

    impl KeyStore for MemoryKeyStore {
        fn get(&self, provider: Provider) -> AppResult<Option<String>> {
            Ok(self.0.lock().unwrap().get(&provider).cloned())
        }
        fn set(&self, provider: Provider, key: &str) -> AppResult<()> {
            self.0.lock().unwrap().insert(provider, key.to_string());
            Ok(())
        }
        fn delete(&self, provider: Provider) -> AppResult<bool> {
            Ok(self.0.lock().unwrap().remove(&provider).is_some())
        }
    }

    const ANTHROPIC_KEY: &str = "sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWXYZ-a1b2";
    const OPENAI_KEY: &str = "sk-proj-0123456789abcdefghijklmnop-z9y8";

    fn keys() -> Keys {
        Keys::new(MemoryKeyStore::default())
    }

    #[test]
    fn starts_with_no_keys() {
        let all = keys().status_all().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|s| !s.configured && s.masked.is_none()));
    }

    #[test]
    fn save_trims_and_returns_masked_status() {
        let keys = keys();
        let status = keys.save(Provider::Anthropic, &format!("  {ANTHROPIC_KEY}\n")).unwrap();
        assert!(status.configured);
        assert_eq!(status.masked.as_deref(), Some("sk-ant…a1b2"));
        assert_eq!(keys.secret(Provider::Anthropic).unwrap(), ANTHROPIC_KEY);
        // El otro proveedor no se ve afectado.
        assert!(!keys.status(Provider::OpenAi).unwrap().configured);
    }

    #[test]
    fn status_never_contains_the_full_key() {
        let keys = keys();
        keys.save(Provider::OpenAi, OPENAI_KEY).unwrap();
        let json = serde_json::to_string(&keys.status_all().unwrap()).unwrap();
        assert!(!json.contains(OPENAI_KEY));
        assert!(!json.contains("0123456789abcdef"));
    }

    #[test]
    fn delete_is_idempotent() {
        let keys = keys();
        keys.save(Provider::OpenAi, OPENAI_KEY).unwrap();
        assert!(!keys.delete(Provider::OpenAi).unwrap().configured);
        assert!(!keys.delete(Provider::OpenAi).unwrap().configured);
    }

    #[test]
    fn missing_secret_is_not_found_with_helpful_message() {
        let err = keys().secret(Provider::Anthropic).unwrap_err();
        assert_eq!(err.kind(), "not_found");
        assert!(err.to_string().contains("Anthropic"));
    }

    #[test]
    fn validation_rejects_bad_input() {
        let too_long = "x".repeat(MAX_KEY_CHARS + 1);
        for bad in ["", "   ", "sk-ant corta con espacios", "sk-short", too_long.as_str()] {
            assert_eq!(validate_key(bad).unwrap_err().kind(), "invalid", "debería rechazar {bad:?}");
        }
        assert!(keys().save(Provider::Anthropic, "sk-short").is_err());
        assert!(!keys().status(Provider::Anthropic).unwrap().configured);
    }

    #[test]
    fn mask_hides_the_middle() {
        assert_eq!(mask(ANTHROPIC_KEY), "sk-ant…a1b2");
        assert_eq!(mask("short"), "••••");
    }

    #[test]
    fn provider_ids_match_the_frontend() {
        assert_eq!(serde_json::to_string(&Provider::OpenAi).unwrap(), "\"openai\"");
        assert_eq!(serde_json::to_string(&Provider::Anthropic).unwrap(), "\"anthropic\"");
        let p: Provider = serde_json::from_str("\"openai\"").unwrap();
        assert_eq!(p, Provider::OpenAi);
    }

    /// Prueba contra el Administrador de credenciales real.
    /// Se ejecuta a mano: `cargo test -- --ignored`. Usa un servicio de prueba y lo borra al final.
    #[test]
    #[ignore]
    fn system_keystore_roundtrip() {
        let store = SystemKeyStore::new("com.chatia.app.test");
        store.delete(Provider::Anthropic).unwrap();
        assert_eq!(store.get(Provider::Anthropic).unwrap(), None);
        store.set(Provider::Anthropic, ANTHROPIC_KEY).unwrap();
        assert_eq!(store.get(Provider::Anthropic).unwrap().as_deref(), Some(ANTHROPIC_KEY));
        assert!(store.delete(Provider::Anthropic).unwrap());
        assert_eq!(store.get(Provider::Anthropic).unwrap(), None);
    }
}
