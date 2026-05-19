//! Cortex Tauri commands.
//!
//! Lets the frontend start / stop the perpetual cognitive loop, push
//! observations into it, and forwards snapshot events on the
//! `cortex://snapshot` Tauri event channel.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::cognition::cortex::Cortex;
use crate::cognition::perpetual_loop::{spawn, LoopConfig, LoopHandle};
use crate::cognition::reflection_writer::{ReflectionWriter, ReflectionWriterConfig};
use crate::cognition::shared_cortex::CortexConfig;
use crate::utils::error::{AuraError, CmdResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct CortexStatus {
    pub running: bool,
    pub cognitive_dim: Option<usize>,
    pub reservoir_dim: Option<usize>,
    pub dt_ms: Option<u64>,
}

/// Start the perpetual cognitive loop. Idempotent on the success path: a
/// second `start_cortex` shuts down the existing loop and replaces it.
#[tauri::command]
pub async fn start_cortex(
    state: State<'_, AppState>,
    app: AppHandle,
    cognitive_dim: Option<usize>,
    reservoir_dim: Option<usize>,
    dt_ms: Option<u64>,
    seed: Option<u64>,
) -> CmdResult<CortexStatus> {
    // Drop any existing loop first.
    let prev = {
        let mut guard = state.cortex.lock().await;
        guard.take()
    };
    if let Some(h) = prev {
        h.shutdown()
            .await
            .map_err(|e| AuraError::Other(format!("previous cortex shutdown: {e}")))?;
    }

    let mut cfg = CortexConfig::default();
    if let Some(d) = cognitive_dim {
        cfg.cognitive_dim = d;
    }
    if let Some(r) = reservoir_dim {
        cfg.reservoir_dim = r;
    }
    let mut loop_cfg = LoopConfig::default();
    if let Some(ms) = dt_ms {
        loop_cfg.dt_ms = ms;
    }

    let cortex = Cortex::with_seeded_weights(cfg.clone(), seed.unwrap_or(0));
    let mut handle = spawn(cortex, loop_cfg.clone());

    // Snapshot forwarder: always emit `cortex://snapshot` for the UI; if a
    // vault is open, also feed each snapshot to a `ReflectionWriter`. When
    // the writer fires, emit `reflection://written` with the path.
    if let Some(mut rx) = handle.take_snapshots() {
        let app_clone = app.clone();
        let vault_root = state
            .vault
            .lock()
            .await
            .as_ref()
            .map(|v| v.root.clone());
        tokio::spawn(async move {
            let mut writer = vault_root
                .map(|root| ReflectionWriter::new(&root, ReflectionWriterConfig::default()));
            while let Some(snap) = rx.recv().await {
                let _ = app_clone.emit("cortex://snapshot", snap.clone());
                if let Some(w) = writer.as_mut() {
                    match w.consider(&snap) {
                        Ok(Some(path)) => {
                            let _ = app_clone
                                .emit("reflection://written", path.display().to_string());
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!(
                            target: "aura::reflection",
                            "write failed: {e}"
                        ),
                    }
                }
            }
        });
    }

    *state.cortex.lock().await = Some(handle);

    Ok(CortexStatus {
        running: true,
        cognitive_dim: Some(cfg.cognitive_dim),
        reservoir_dim: Some(cfg.reservoir_dim),
        dt_ms: Some(loop_cfg.dt_ms),
    })
}

/// Stop the perpetual loop. No-op if not running.
#[tauri::command]
pub async fn stop_cortex(state: State<'_, AppState>) -> CmdResult<CortexStatus> {
    let prev = {
        let mut guard = state.cortex.lock().await;
        guard.take()
    };
    if let Some(h) = prev {
        h.shutdown()
            .await
            .map_err(|e| AuraError::Other(format!("cortex shutdown: {e}")))?;
    }
    Ok(CortexStatus {
        running: false,
        cognitive_dim: None,
        reservoir_dim: None,
        dt_ms: None,
    })
}

/// Push an observation into the running loop.
#[tauri::command]
pub async fn send_observation(
    state: State<'_, AppState>,
    observation: Vec<f32>,
) -> CmdResult<()> {
    let guard = state.cortex.lock().await;
    let handle: &LoopHandle = guard
        .as_ref()
        .ok_or_else(|| AuraError::Other("cortex not running".into()))?;
    handle
        .obs_tx
        .send(observation)
        .await
        .map_err(|e| AuraError::Other(format!("obs channel: {e}")))?;
    Ok(())
}

#[tauri::command]
pub async fn cortex_status(state: State<'_, AppState>) -> CmdResult<CortexStatus> {
    let guard = state.cortex.lock().await;
    Ok(CortexStatus {
        running: guard.is_some(),
        cognitive_dim: None,
        reservoir_dim: None,
        dt_ms: None,
    })
}
