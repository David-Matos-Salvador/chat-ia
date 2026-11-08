mod chat;
mod commands;
mod db;
mod error;
mod keys;
mod providers;

use tauri::Manager;

use db::Db;
use keys::{Keys, SystemKeyStore, KEYRING_SERVICE};

/// Nombre del archivo de la base de datos dentro de la carpeta de datos de la app
/// (en Windows: %APPDATA%\com.chatia.app).
const DB_FILE: &str = "chat-ia.db";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(Db::open(&data_dir.join(DB_FILE))?);
            app.manage(Keys::new(SystemKeyStore::new(KEYRING_SERVICE)));
            app.manage(providers::http_client()?);
            app.manage(chat::Generations::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::db_status,
            commands::conversation_create,
            commands::conversation_list,
            commands::conversation_get,
            commands::conversation_search,
            commands::conversation_rename,
            commands::conversation_set_model,
            commands::conversation_set_system_prompt,
            commands::conversation_delete,
            commands::conversation_export,
            commands::conversation_fork,
            commands::models_list,
            commands::chat_send,
            commands::chat_regenerate,
            commands::chat_stop,
            commands::message_list,
            commands::setting_get,
            commands::setting_set,
            commands::key_status_all,
            commands::key_save,
            commands::key_delete,
            commands::key_test,
            commands::compatible_config_get,
            commands::compatible_config_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
