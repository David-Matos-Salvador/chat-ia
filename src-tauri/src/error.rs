use serde::ser::{Serialize, SerializeStruct, Serializer};

/// Error común del backend.
///
/// Llega al frontend como `{ kind, message }`, así la interfaz puede
/// decidir qué mostrar según `kind` sin analizar el texto.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("error de base de datos: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("no se encontró {0}")]
    NotFound(String),

    #[error("{0}")]
    Invalid(String),

    #[error("error de archivo: {0}")]
    Io(#[from] std::io::Error),

    #[error("error del Administrador de credenciales: {0}")]
    Keyring(String),

    /// Error de conexión con el proveedor (sin internet, DNS, tiempo de espera…).
    #[error("{0}")]
    Network(String),

    /// El proveedor respondió con un error HTTP.
    #[error("{message}")]
    Provider { status: u16, message: String },

    #[error("{0}")]
    Internal(String),
}

impl From<keyring::Error> for AppError {
    fn from(err: keyring::Error) -> Self {
        AppError::Keyring(err.to_string())
    }
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Db(_) => "db",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::Io(_) => "io",
            AppError::Keyring(_) => "keyring",
            AppError::Network(_) => "network",
            AppError::Provider { .. } => "provider",
            AppError::Internal(_) => "internal",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
