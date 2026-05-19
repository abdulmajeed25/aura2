pub mod cognition;
pub mod commands;
pub mod core;
pub mod db;
pub mod protocols;
pub mod utils;

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cognition::perpetual_loop::LoopHandle;
use crate::core::ssm::StreamingState;
use crate::core::vault::VaultState;
use crate::protocols::server::McpServerHandle;

/// Application-wide state shared across Tauri command handlers.
pub struct AppState {
    pub vault: Arc<Mutex<Option<VaultState>>>,
    pub ssm: Arc<Mutex<Option<StreamingState>>>,
    pub mcp: Arc<Mutex<Option<McpServerHandle>>>,
    pub cortex: Arc<Mutex<Option<LoopHandle>>>,
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
        ssm: Arc::new(Mutex::new(None)),
        mcp: Arc::new(Mutex::new(None)),
        cortex: Arc::new(Mutex::new(None)),
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
            commands::graph_rag::rebuild_graph_rag,
            commands::graph_rag::graph_rag_query,
            commands::streaming::ssm_status,
            commands::streaming::ssm_reset,
            commands::streaming::ssm_step_text,
            commands::streaming::streaming_chat,
            commands::media::media_tools_status,
            commands::media::ingest_media,
            commands::media::scan_media,
            commands::media::list_media,
            commands::media::delete_media,
            commands::integrations::start_mcp_server,
            commands::integrations::stop_mcp_server,
            commands::integrations::mcp_status,
            commands::agent::suggest_links,
            commands::agent::find_orphan_notes,
            commands::agent::apply_link_suggestion,
            commands::canvas::list_canvases,
            commands::canvas::read_canvas,
            commands::canvas::write_canvas,
            commands::canvas::create_canvas,
            commands::cortex::start_cortex,
            commands::cortex::stop_cortex,
            commands::cortex::send_observation,
            commands::cortex::cortex_status,
            commands::embeddings::embeddings_model_status,
            commands::embeddings::download_embeddings_model,
        ])
        .setup(|_app| {
            tracing::info!("Aura starting up");
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // Hard Rule #4: do not panic in production paths. The Tauri
            // event loop can fail to initialise (missing display, GTK init
            // failure, denied permissions). Surface the error to stderr and
            // exit with a non-zero code so the OS / shell can react.
            eprintln!("aura: tauri event loop terminated: {}", e);
            std::process::exit(1);
        });
}
