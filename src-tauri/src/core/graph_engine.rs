use std::collections::HashMap;

use anyhow::Result;
use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use serde::Serialize;

use crate::db::sqlite::VaultDb;

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub path: String,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub degree: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub iterations: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutParams {
    pub iterations: u32,
    pub width: f32,
    pub height: f32,
    pub seed: u64,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            iterations: 250,
            width: 2000.0,
            height: 2000.0,
            seed: 0xA11A2026,
        }
    }
}

/// Pull files + resolved links out of the DB and produce a positioned graph.
pub async fn compute_graph(db: &VaultDb, params: LayoutParams) -> Result<GraphSnapshot> {
    let (nodes_raw, edges_raw) = db.fetch_graph_nodes_and_edges().await?;

    let mut index: HashMap<String, usize> = HashMap::with_capacity(nodes_raw.len());
    for (i, n) in nodes_raw.iter().enumerate() {
        index.insert(n.0.clone(), i);
    }

    // Deduplicate edges (multiple links between the same pair collapse).
    let mut edge_pairs: Vec<(usize, usize)> = Vec::with_capacity(edges_raw.len());
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut degree = vec![0u32; nodes_raw.len()];
    for (s, t) in &edges_raw {
        let (Some(&a), Some(&b)) = (index.get(s), index.get(t)) else {
            continue;
        };
        if a == b {
            continue;
        }
        let key = if a < b { (a, b) } else { (b, a) };
        if seen.insert(key) {
            edge_pairs.push((a, b));
            degree[a] += 1;
            degree[b] += 1;
        }
    }

    let (positions, iterations) = layout(nodes_raw.len(), &edge_pairs, params);

    let nodes = nodes_raw
        .into_iter()
        .enumerate()
        .map(|(i, (id, path, title))| GraphNode {
            id,
            path,
            title,
            x: positions[i].0,
            y: positions[i].1,
            degree: degree[i],
        })
        .collect::<Vec<_>>();

    let edges = edge_pairs
        .into_iter()
        .map(|(a, b)| GraphEdge {
            source: nodes[a].id.clone(),
            target: nodes[b].id.clone(),
        })
        .collect();

    Ok(GraphSnapshot {
        nodes,
        edges,
        iterations,
    })
}

/// Classic Fruchterman-Reingold layout with parallel force accumulation.
/// O(n²) per iteration; fine for ≤ a few thousand nodes. For larger vaults
/// the inner loop should be swapped for a Barnes-Hut quadtree.
fn layout(
    n: usize,
    edges: &[(usize, usize)],
    params: LayoutParams,
) -> (Vec<(f32, f32)>, u32) {
    if n == 0 {
        return (Vec::new(), 0);
    }

    let mut rng = ChaCha8Rng::seed_from_u64(params.seed);
    let mut pos: Vec<(f32, f32)> = (0..n)
        .map(|_| {
            (
                rng.gen_range(-params.width / 2.0..params.width / 2.0),
                rng.gen_range(-params.height / 2.0..params.height / 2.0),
            )
        })
        .collect();

    if n == 1 {
        return (pos, 0);
    }

    let area = params.width * params.height;
    let k = (area / n as f32).sqrt();
    let mut temperature = (params.width.max(params.height)) / 10.0;
    let cooling = (0.95f32).max(0.001);

    for it in 0..params.iterations {
        // Repulsive forces (all pairs, parallel by node).
        let disp: Vec<(f32, f32)> = (0..n)
            .into_par_iter()
            .map(|v| {
                let (vx, vy) = pos[v];
                let mut dx = 0.0f32;
                let mut dy = 0.0f32;
                for (u, &(ux, uy)) in pos.iter().enumerate() {
                    if u == v {
                        continue;
                    }
                    let mut ex = vx - ux;
                    let mut ey = vy - uy;
                    let mut dist = (ex * ex + ey * ey).sqrt();
                    if dist < 0.01 {
                        // Jitter to avoid singularities.
                        ex = 0.01;
                        ey = 0.01;
                        dist = 0.0141;
                    }
                    let force = (k * k) / dist;
                    dx += (ex / dist) * force;
                    dy += (ey / dist) * force;
                }
                (dx, dy)
            })
            .collect();

        let mut disp = disp;

        // Attractive forces (per edge, sequential — typical edge counts are small).
        for &(a, b) in edges {
            let (ax, ay) = pos[a];
            let (bx, by) = pos[b];
            let ex = ax - bx;
            let ey = ay - by;
            let dist = (ex * ex + ey * ey).sqrt().max(0.01);
            let force = (dist * dist) / k;
            let fx = (ex / dist) * force;
            let fy = (ey / dist) * force;
            disp[a].0 -= fx;
            disp[a].1 -= fy;
            disp[b].0 += fx;
            disp[b].1 += fy;
        }

        // Apply displacement bounded by the current temperature, clamp to frame.
        let half_w = params.width / 2.0;
        let half_h = params.height / 2.0;
        for v in 0..n {
            let (dx, dy) = disp[v];
            let mag = (dx * dx + dy * dy).sqrt().max(1e-6);
            let limited = mag.min(temperature);
            pos[v].0 = (pos[v].0 + (dx / mag) * limited).clamp(-half_w, half_w);
            pos[v].1 = (pos[v].1 + (dy / mag) * limited).clamp(-half_h, half_h);
        }

        temperature *= cooling;

        // Early-out if temperature is effectively zero.
        if temperature < 0.05 {
            return (pos, it + 1);
        }
    }

    (pos, params.iterations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_layout_returns_no_positions() {
        let (pos, iters) = layout(0, &[], LayoutParams::default());
        assert!(pos.is_empty());
        assert_eq!(iters, 0);
    }

    #[test]
    fn two_connected_nodes_settle_apart() {
        let params = LayoutParams {
            iterations: 200,
            width: 1000.0,
            height: 1000.0,
            seed: 42,
        };
        let (pos, _) = layout(2, &[(0, 1)], params);
        assert_eq!(pos.len(), 2);
        let dx = pos[0].0 - pos[1].0;
        let dy = pos[0].1 - pos[1].1;
        let dist = (dx * dx + dy * dy).sqrt();
        assert!(dist > 1.0, "nodes should be separated, got dist {}", dist);
    }

    #[test]
    fn star_graph_centers_hub() {
        // Hub at 0, four leaves at 1..4. The hub should end up roughly central.
        let params = LayoutParams {
            iterations: 250,
            width: 1000.0,
            height: 1000.0,
            seed: 7,
        };
        let edges = vec![(0, 1), (0, 2), (0, 3), (0, 4)];
        let (pos, _) = layout(5, &edges, params);
        let cx = pos.iter().map(|(x, _)| *x).sum::<f32>() / 5.0;
        let cy = pos.iter().map(|(_, y)| *y).sum::<f32>() / 5.0;
        let hub_offset = ((pos[0].0 - cx).powi(2) + (pos[0].1 - cy).powi(2)).sqrt();
        let leaf_avg = (1..5)
            .map(|i| ((pos[i].0 - cx).powi(2) + (pos[i].1 - cy).powi(2)).sqrt())
            .sum::<f32>()
            / 4.0;
        assert!(
            hub_offset < leaf_avg,
            "hub should be closer to centroid than leaves: hub={} avg leaf={}",
            hub_offset,
            leaf_avg
        );
    }
}
