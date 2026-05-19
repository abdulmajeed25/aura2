//! Tokio-spawned perpetual cognitive loop.
//!
//! Owns a [`Cortex`] in a background task. Heartbeat: one tick every `dt_ms`.
//! On each beat:
//! 1. Drain any queued observations from the MPSC channel; the most recent
//!    one wins (or zero if none).
//! 2. `cortex.tick(observation)`.
//! 3. Update the curiosity score with the resulting F.
//! 4. Every `snapshot_every` ticks, push a [`CortexSnapshot`] onto the
//!    outbound channel (non-blocking — drop the oldest if Tauri is slow).
//!
//! The loop exits cleanly when the [`LoopHandle`] is shut down or dropped.
//!
//! CPU throttling: tokio's `interval` already paces the loop at the
//! configured `dt_ms`; the 10 ms default keeps each beat well under the
//! 25% CPU budget on a 4-core machine (one beat ≈ 50 µs on the cortex
//! tested in `cortex.rs`).

use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::cognition::cortex::Cortex;
use crate::cognition::curiosity::CuriosityScore;

#[derive(Clone, Debug, Serialize)]
pub struct CortexSnapshot {
    pub tick: u64,
    pub free_energy: f32,
    pub curiosity: f32,
    pub dominant_index: usize,
}

#[derive(Clone, Debug)]
pub struct LoopConfig {
    pub dt_ms: u64,
    pub snapshot_every: u64,
    pub curiosity_window: usize,
    pub obs_buffer: usize,
    pub snap_buffer: usize,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            dt_ms: 10,
            snapshot_every: 10,
            curiosity_window: 32,
            obs_buffer: 64,
            snap_buffer: 64,
        }
    }
}

/// Handle returned by [`spawn`]. Drop or call [`LoopHandle::shutdown`] to
/// stop the task.
pub struct LoopHandle {
    pub obs_tx: mpsc::Sender<Vec<f32>>,
    /// Consumed at most once: take ownership of the snapshot stream and
    /// forward it to Tauri (or wherever).
    snap_rx: Option<mpsc::Receiver<CortexSnapshot>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl LoopHandle {
    /// Take the receiver. Returns `None` on the second call.
    pub fn take_snapshots(&mut self) -> Option<mpsc::Receiver<CortexSnapshot>> {
        self.snap_rx.take()
    }

    /// Stop the loop and wait for the task to finish.
    pub async fn shutdown(mut self) -> Result<(), tokio::task::JoinError> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            join.await?;
        }
        Ok(())
    }
}

impl Drop for LoopHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

pub fn spawn(mut cortex: Cortex, config: LoopConfig) -> LoopHandle {
    let cognitive_dim = cortex.state.config.cognitive_dim;
    let (obs_tx, mut obs_rx) = mpsc::channel::<Vec<f32>>(config.obs_buffer.max(1));
    let (snap_tx, snap_rx) = mpsc::channel::<CortexSnapshot>(config.snap_buffer.max(1));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    let dt = std::time::Duration::from_millis(config.dt_ms.max(1));
    let snapshot_every = config.snapshot_every.max(1);
    let curiosity_window = config.curiosity_window.max(2);

    let join = tokio::spawn(async move {
        let mut interval = tokio::time::interval(dt);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut curiosity = CuriosityScore::new(curiosity_window);

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                _ = interval.tick() => {
                    // Drain pending observations; the latest one wins. If
                    // none arrived, use zeros (idle drift).
                    let mut latest = vec![0.0_f32; cognitive_dim];
                    let mut got_one = false;
                    while let Ok(obs) = obs_rx.try_recv() {
                        if obs.len() == cognitive_dim {
                            latest = obs;
                            got_one = true;
                        }
                    }
                    let _ = got_one;
                    let event = match cortex.tick(&latest) {
                        Ok(ev) => ev,
                        Err(e) => {
                            tracing::warn!(target: "aura::cortex", "tick failed: {e}");
                            continue;
                        }
                    };
                    curiosity.push(event.free_energy);
                    if event.tick % snapshot_every == 0 {
                        let snap = CortexSnapshot {
                            tick: event.tick,
                            free_energy: event.free_energy,
                            curiosity: curiosity.score(),
                            dominant_index: event.dominant_index,
                        };
                        // try_send: drop if downstream is full rather than
                        // backpressuring the cortex loop.
                        let _ = snap_tx.try_send(snap);
                    }
                }
            }
        }
    });

    LoopHandle {
        obs_tx,
        snap_rx: Some(snap_rx),
        shutdown_tx: Some(shutdown_tx),
        join: Some(join),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::shared_cortex::CortexConfig;

    fn tiny_config() -> CortexConfig {
        CortexConfig {
            cognitive_dim: 4,
            reservoir_dim: 8,
            beta: 1.0,
            dt: 0.01,
            tau: 1.0,
            alpha: 0.2,
        }
    }

    /// Loop emits snapshots at the configured cadence.
    #[tokio::test]
    async fn loop_emits_snapshots() {
        let cortex = Cortex::with_seeded_weights(tiny_config(), 1);
        let cfg = LoopConfig {
            dt_ms: 2,
            snapshot_every: 1,
            curiosity_window: 4,
            obs_buffer: 16,
            snap_buffer: 16,
        };
        let mut handle = spawn(cortex, cfg);
        let mut rx = handle.take_snapshots().unwrap();

        // Wait up to 200 ms for at least 3 snapshots.
        let mut collected = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
        while collected.len() < 3 && std::time::Instant::now() < deadline {
            if let Ok(Some(snap)) = tokio::time::timeout(
                std::time::Duration::from_millis(50),
                rx.recv(),
            )
            .await
            {
                collected.push(snap);
            }
        }

        handle.shutdown().await.unwrap();
        assert!(
            collected.len() >= 3,
            "expected ≥3 snapshots in 200ms, got {} (tick numbers: {:?})",
            collected.len(),
            collected.iter().map(|s| s.tick).collect::<Vec<_>>()
        );
        for s in &collected {
            assert!(s.free_energy.is_finite(), "F not finite: {}", s.free_energy);
            assert!(s.dominant_index < 4, "dominant_index out of range");
        }
    }

    /// Observations flow into the cortex via the obs channel.
    #[tokio::test]
    async fn observations_reach_the_cortex() {
        let cortex = Cortex::with_seeded_weights(tiny_config(), 2);
        let cfg = LoopConfig {
            dt_ms: 2,
            snapshot_every: 1,
            curiosity_window: 4,
            obs_buffer: 16,
            snap_buffer: 16,
        };
        let mut handle = spawn(cortex, cfg);
        let mut rx = handle.take_snapshots().unwrap();

        // Send a strong observation, then wait for snapshots.
        for _ in 0..5 {
            handle.obs_tx.send(vec![1.0, -1.0, 0.5, -0.5]).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        // Collect snapshots for 100ms.
        let mut got = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
        while std::time::Instant::now() < deadline {
            if let Ok(Some(snap)) = tokio::time::timeout(
                std::time::Duration::from_millis(30),
                rx.recv(),
            )
            .await
            {
                // Any non-trivial F means the observation reached the cortex.
                if snap.free_energy > 0.0 {
                    got = true;
                    break;
                }
            }
        }
        handle.shutdown().await.unwrap();
        assert!(got, "no F > 0 observed — observations didn't reach cortex");
    }

    /// Loop terminates cleanly on shutdown.
    #[tokio::test]
    async fn shutdown_completes() {
        let cortex = Cortex::with_seeded_weights(tiny_config(), 3);
        let handle = spawn(cortex, LoopConfig::default());
        // Immediate shutdown — task should join within 1 s.
        tokio::time::timeout(std::time::Duration::from_secs(1), handle.shutdown())
            .await
            .expect("shutdown timed out")
            .expect("join failed");
    }
}
