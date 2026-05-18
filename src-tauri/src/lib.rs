pub mod commands;
pub mod core;
pub mod db;
pub mod utils;

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::core::vault::VaultState;

/// Application-wide state shared across Tauri command handlers.
pub struct AppState {
    pub vault: Arc<Mutex<Option<VaultState>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let app_state = AppState {
        vault: Arc::new(Mutex::new(None)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::vault::open_vault,
            commands::vault::close_vault,
            commands::vault::current_vault,
            commands::vault::reindex_vault,
            commands::file::list_files,
            commands::file::read_file,
            commands::file::write_file,
            commands::file::create_file,
            commands::file::delete_file,
            commands::file::rename_file,
            commands::file::file_tree,
            commands::link::get_backlinks,
            commands::link::get_outgoing_links,
            commands::link::get_outline,
            commands::link::list_link_candidates,
            commands::block::resolve_embed,
            commands::graph::get_graph_snapshot,
            commands::search::search_vault,
            commands::related::find_related,
        ])
        .setup(|_app| {
            tracing::info!("Aura starting up");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
