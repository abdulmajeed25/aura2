use tauri::State;

use crate::core::graph_engine::{compute_graph, GraphSnapshot, LayoutParams};
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

/// Compute the full graph snapshot for the current vault. The Rust layer runs
/// the force-directed layout so the frontend only receives positioned nodes
/// and edges and can render directly to a canvas.
#[tauri::command]
pub async fn get_graph_snapshot(
    state: State<'_, AppState>,
    iterations: Option<u32>,
    width: Option<f32>,
    height: Option<f32>,
) -> CmdResult<GraphSnapshot> {
    let guard = state.vault.lock().await;
    let vault = guard.as_ref().ok_or(AuraError::NoVault)?;

    let mut params = LayoutParams::default();
    if let Some(n) = iterations {
        params.iterations = n.clamp(20, 1000);
    }
    if let Some(w) = width {
        params.width = w.max(200.0);
    }
    if let Some(h) = height {
        params.height = h.max(200.0);
    }

    let snapshot = compute_graph(&vault.db, params)
        .await
        .map_err(AuraError::from)?;
    Ok(snapshot)
}
