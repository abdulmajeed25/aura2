//! Vault-optimisation reports. Phase 11's "Optimize Vault" is intentionally
//! read-only here: we report what could change, the user reviews and acts.
//!
//! Initial reports:
//! - `find_orphans` — notes with no incoming AND no outgoing links.

use anyhow::Result;
use serde::Serialize;

use crate::db::sqlite::VaultDb;

#[derive(Debug, Clone, Serialize)]
pub struct OrphanNote {
    pub file_id: String,
    pub path: String,
    pub title: String,
}

/// Notes that have neither incoming nor outgoing resolved links.
pub async fn find_orphans(db: &VaultDb) -> Result<Vec<OrphanNote>> {
    let (nodes, edges) = db.fetch_graph_nodes_and_edges().await?;
    let mut linked: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (s, t) in &edges {
        linked.insert(s.clone());
        linked.insert(t.clone());
    }
    let mut out: Vec<OrphanNote> = nodes
        .into_iter()
        .filter(|(id, _, _)| !linked.contains(id))
        .map(|(file_id, path, title)| OrphanNote {
            file_id,
            path,
            title,
        })
        .collect();
    out.sort_by_key(|o| o.title.to_ascii_lowercase());
    Ok(out)
}
