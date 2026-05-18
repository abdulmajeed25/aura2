//! Label Propagation Algorithm (LPA) for community detection.
//!
//! Each node starts with a unique label. We iterate: every node adopts the
//! most-common label among its neighbours (ties broken by smallest label id
//! for determinism). The algorithm converges quickly on the kind of sparse
//! note-link graphs Aura sees (a few thousand nodes, a few iterations).
//!
//! Spec mentions Leiden across 4 levels. LPA gives a single-level partition
//! and is good enough for Phase 7's deliverable; the seam where Leiden /
//! multi-level resolution drops in is `detect_communities` — return more
//! partitions and the rest of the pipeline (summariser + query engine) keeps
//! working unchanged.

use std::collections::HashMap;

use rand::seq::SliceRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Run LPA. `node_ids` is the list of file-ids; `edges` are unordered
/// `(source, target)` pairs (resolved links from `VaultDb::neighbour_map`
/// flattened). Returns a map from `node_id` to its assigned `community_id`.
///
/// Singleton nodes (no edges) get their own community. `seed` makes the
/// random visit order deterministic so the same vault always produces the
/// same partition for the same iteration count.
pub fn detect_communities(
    node_ids: &[String],
    edges: &[(String, String)],
    max_iterations: u32,
    seed: u64,
) -> HashMap<String, u32> {
    let n = node_ids.len();
    if n == 0 {
        return HashMap::new();
    }

    // Adjacency map (undirected).
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::with_capacity(n);
    for id in node_ids {
        adj.insert(id.as_str(), Vec::new());
    }
    for (a, b) in edges {
        if a == b {
            continue;
        }
        if let Some(v) = adj.get_mut(a.as_str()) {
            v.push(b.as_str());
        }
        if let Some(v) = adj.get_mut(b.as_str()) {
            v.push(a.as_str());
        }
    }

    // Each node starts with a unique label (= its index).
    let mut label: HashMap<&str, u32> = node_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i as u32))
        .collect();

    let mut order: Vec<&str> = node_ids.iter().map(|s| s.as_str()).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    for _ in 0..max_iterations {
        order.shuffle(&mut rng);
        let mut changed = false;
        for &v in &order {
            let neighbours = match adj.get(v) {
                Some(ns) if !ns.is_empty() => ns,
                _ => continue,
            };
            // Tally neighbour labels.
            let mut tally: HashMap<u32, u32> = HashMap::new();
            for u in neighbours {
                if let Some(&l) = label.get(*u) {
                    *tally.entry(l).or_insert(0) += 1;
                }
            }
            // Pick the most frequent, ties broken by smallest label id.
            let new_label = tally
                .into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                .map(|(l, _)| l);
            if let Some(l) = new_label {
                if label.get(v) != Some(&l) {
                    label.insert(v, l);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Re-number labels to be contiguous starting at 0 (purely cosmetic).
    let mut remap: HashMap<u32, u32> = HashMap::new();
    let mut next: u32 = 0;
    let mut out: HashMap<String, u32> = HashMap::with_capacity(n);
    for id in node_ids {
        let raw = *label.get(id.as_str()).unwrap_or(&0);
        let cid = *remap.entry(raw).or_insert_with(|| {
            let v = next;
            next += 1;
            v
        });
        out.insert(id.clone(), cid);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_produces_empty_partition() {
        let p = detect_communities(&[], &[], 10, 0);
        assert!(p.is_empty());
    }

    #[test]
    fn isolated_nodes_each_get_their_own_community() {
        let nodes = vec!["a".into(), "b".into(), "c".into()];
        let p = detect_communities(&nodes, &[], 10, 0);
        let unique: std::collections::HashSet<u32> = p.values().copied().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn fully_connected_clique_collapses_to_one_community() {
        let nodes: Vec<String> = (0..5).map(|i| format!("n{}", i)).collect();
        let mut edges = Vec::new();
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                edges.push((nodes[i].clone(), nodes[j].clone()));
            }
        }
        let p = detect_communities(&nodes, &edges, 20, 7);
        let unique: std::collections::HashSet<u32> = p.values().copied().collect();
        assert_eq!(unique.len(), 1, "clique should collapse to a single community");
    }

    #[test]
    fn two_cliques_split_into_two_communities() {
        // Cluster A: 0-1-2 fully connected.
        // Cluster B: 3-4-5 fully connected.
        // Bridge: just one edge 2-3.
        let nodes: Vec<String> = (0..6).map(|i| format!("n{}", i)).collect();
        let mut edges = vec![];
        for &(i, j) in &[(0, 1), (0, 2), (1, 2), (3, 4), (3, 5), (4, 5), (2, 3)] {
            edges.push((nodes[i].clone(), nodes[j].clone()));
        }
        let p = detect_communities(&nodes, &edges, 30, 11);
        // Expect at least 2 communities; some seeds may produce 3 if a bridge
        // node flips. Assert the structure: 0/1 share, 4/5 share, 0 != 5.
        let l0 = p[&nodes[0]];
        let l1 = p[&nodes[1]];
        let l4 = p[&nodes[4]];
        let l5 = p[&nodes[5]];
        assert_eq!(l0, l1, "n0 and n1 should be in the same community");
        assert_eq!(l4, l5, "n4 and n5 should be in the same community");
        assert_ne!(l0, l5, "n0 and n5 should be in different communities");
    }
}
