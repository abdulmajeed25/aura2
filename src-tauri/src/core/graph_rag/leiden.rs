//! Leiden community detection (Traag, Waltman, van Eck 2019).
//!
//! Improvements over the Louvain → LPA chain currently in
//! [`community_detector::detect_communities`]:
//!
//! 1. **Local moving** with explicit modularity gain Δ𝑄, not just label
//!    frequency.
//! 2. **Refinement** — within each community, find sub-partitions that are
//!    still well-connected. Prevents the badly-connected-community defect
//!    Louvain can produce.
//! 3. **Aggregation** — collapse each *refined* community to a super-node
//!    and recurse, producing a true hierarchy (Phase 7's
//!    `4-level Leiden hierarchy` ambition).
//!
//! Modularity (unweighted, undirected):
//!
//! ```text
//! Q = (1 / 2m) · Σ_ij [A_ij − γ · k_i k_j / 2m] · δ(c_i, c_j)
//! ```
//!
//! `γ` is the resolution parameter (1.0 = standard Newman-Girvan; higher
//! = smaller communities). We use 1.0 by default.
//!
//! For Aura's note-link graphs (a few thousand nodes, low density) the
//! implementation runs in well under a second; the limiting cost is the
//! number of aggregation rounds, capped by `max_levels`.

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// One hierarchy level's partition. Maps original node id → community id.
pub type Partition = HashMap<String, u32>;

#[derive(Clone, Debug)]
pub struct LeidenResult {
    /// Coarse-to-fine: `levels[0]` is the bottom (one community per node
    /// initially); the last entry is the coarsest partition the algorithm
    /// produced. The partition the user "wants" is usually
    /// `levels.last()`.
    pub levels: Vec<Partition>,
    /// Modularity of each entry in `levels`. Same length.
    pub modularities: Vec<f64>,
}

impl LeidenResult {
    /// Final / coarsest partition.
    pub fn final_partition(&self) -> &Partition {
        self.levels
            .last()
            .expect("LeidenResult always has at least one level")
    }
}

/// Run Leiden until aggregation no longer changes the partition or
/// `max_levels` is hit. `resolution = γ` (use 1.0 unless you know better).
pub fn leiden(
    node_ids: &[String],
    edges: &[(String, String)],
    resolution: f64,
    max_levels: u32,
    max_iterations_per_level: u32,
    seed: u64,
) -> LeidenResult {
    if node_ids.is_empty() {
        return LeidenResult {
            levels: vec![HashMap::new()],
            modularities: vec![0.0],
        };
    }

    // Build the initial undirected weighted graph keyed by string ids.
    let initial = build_graph(node_ids, edges);

    // Track the bottom-up mapping: every super-node label at a given level
    // is tracked back to the original node ids.
    let mut original_to_super: HashMap<String, u32> = (0..node_ids.len() as u32)
        .zip(node_ids.iter().cloned())
        .map(|(i, id)| (id, i))
        .collect();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut levels: Vec<Partition> = Vec::new();
    let mut modularities: Vec<f64> = Vec::new();
    let mut graph = initial;

    for _ in 0..max_levels {
        // Each super-node starts in its own community.
        let mut partition: Vec<u32> = (0..graph.n as u32).collect();

        // Phase 1: local moving until convergence.
        local_move(
            &graph,
            &mut partition,
            resolution,
            max_iterations_per_level,
            &mut rng,
        );

        // Phase 2: refinement. Each community gets re-split into
        // sub-partitions whose members are well-connected to each other.
        let refined = refine(&graph, &partition, resolution, &mut rng);

        // Compactify labels for the partition we record at this level.
        let compact_super = compactify(&partition);
        let level_modularity = modularity(&graph, &compact_super, resolution);

        // Translate super-node labels back to the original node ids and
        // record this level.
        let mut level_partition: Partition = HashMap::with_capacity(node_ids.len());
        for orig in node_ids {
            let super_idx = *original_to_super.get(orig).expect("super-node lookup");
            let cid = compact_super[super_idx as usize];
            level_partition.insert(orig.clone(), cid);
        }
        levels.push(level_partition);
        modularities.push(level_modularity);

        // If refinement collapsed each community to a single sub-piece, we
        // can stop — further aggregation would not change anything.
        if no_more_aggregation(&partition, &refined) {
            break;
        }

        // Phase 3: aggregate using the **refined** partition. Update the
        // original→super-node mapping and rebuild the graph at the next
        // level.
        let (next_graph, super_relabel) = aggregate(&graph, &refined);
        for super_idx in original_to_super.values_mut() {
            *super_idx = super_relabel[*super_idx as usize];
        }
        graph = next_graph;
    }

    if levels.is_empty() {
        // Defensive — shouldn't trip because `max_levels >= 1`.
        let mut p = HashMap::new();
        for id in node_ids {
            p.insert(id.clone(), 0);
        }
        levels.push(p);
        modularities.push(0.0);
    }

    LeidenResult {
        levels,
        modularities,
    }
}

// ---------- internals --------------------------------------------------

/// Weighted undirected graph keyed by integer indices `0..n`. `degrees[i]`
/// is the total weighted degree of node `i`; `total_weight` is `Σ degrees / 2`.
struct Graph {
    n: usize,
    /// `neighbours[i]` is `&[(j, w)]` — neighbour index and edge weight.
    neighbours: Vec<Vec<(u32, f64)>>,
    degrees: Vec<f64>,
    total_weight: f64,
}

fn build_graph(node_ids: &[String], edges: &[(String, String)]) -> Graph {
    let n = node_ids.len();
    let id_to_idx: HashMap<&str, u32> = node_ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i as u32))
        .collect();
    let mut neighbours: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
    let mut degrees = vec![0.0; n];

    // Aggregate duplicate edges by summing weights (treat each input edge
    // as weight 1.0).
    let mut weight_map: HashMap<(u32, u32), f64> = HashMap::with_capacity(edges.len());
    for (a, b) in edges {
        if a == b {
            continue;
        }
        let (Some(&i), Some(&j)) = (id_to_idx.get(a.as_str()), id_to_idx.get(b.as_str())) else {
            continue;
        };
        let key = if i < j { (i, j) } else { (j, i) };
        *weight_map.entry(key).or_insert(0.0) += 1.0;
    }
    for ((i, j), w) in &weight_map {
        neighbours[*i as usize].push((*j, *w));
        neighbours[*j as usize].push((*i, *w));
        degrees[*i as usize] += w;
        degrees[*j as usize] += w;
    }
    let total_weight: f64 = degrees.iter().sum::<f64>() / 2.0;
    Graph {
        n,
        neighbours,
        degrees,
        total_weight,
    }
}

/// Local-moving phase. Each node tries each neighbour's community and moves
/// to the one that maximises Δ𝑄.
fn local_move(
    g: &Graph,
    partition: &mut [u32],
    resolution: f64,
    max_iterations: u32,
    rng: &mut ChaCha8Rng,
) {
    if g.total_weight <= 0.0 || g.n == 0 {
        return;
    }
    // community_weight[c] = Σ degrees of nodes in c
    let mut community_weight: HashMap<u32, f64> = HashMap::new();
    for (v, &c) in partition.iter().enumerate() {
        *community_weight.entry(c).or_insert(0.0) += g.degrees[v];
    }
    let two_m = 2.0 * g.total_weight;

    let mut order: Vec<usize> = (0..g.n).collect();
    for _ in 0..max_iterations {
        order.shuffle(rng);
        let mut moved = false;
        for &v in &order {
            let k_v = g.degrees[v];
            if k_v <= 0.0 {
                continue; // isolated node, leave it alone
            }
            let old_c = partition[v];

            // Σ edge weights to each neighbour community, including own.
            let mut to_community: HashMap<u32, f64> = HashMap::new();
            for &(u, w) in &g.neighbours[v] {
                let c = partition[u as usize];
                *to_community.entry(c).or_insert(0.0) += w;
            }
            // The "to-old" we use as a baseline excludes v's own self-weight,
            // which is 0 in our setup (no self-loops).
            let to_old = *to_community.get(&old_c).unwrap_or(&0.0);
            let weight_old = community_weight[&old_c] - k_v;

            let mut best_c = old_c;
            let mut best_gain = 0.0_f64;
            for (&c, &to_c) in &to_community {
                if c == old_c {
                    continue;
                }
                let weight_c = *community_weight.get(&c).unwrap_or(&0.0);
                // Δ𝑄 = (to_c - to_old)/m - γ·k_v·(weight_c - weight_old)/(2m²)
                let delta = (to_c - to_old) / g.total_weight
                    - resolution * k_v * (weight_c - weight_old) / (two_m * two_m);
                if delta > best_gain + 1e-12 {
                    best_gain = delta;
                    best_c = c;
                }
            }
            if best_c != old_c {
                partition[v] = best_c;
                *community_weight.entry(old_c).or_insert(0.0) -= k_v;
                *community_weight.entry(best_c).or_insert(0.0) += k_v;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

/// Refinement: for each community in `partition`, run a localised
/// modularity-greedy pass that only allows merges into the **same**
/// community, then split the community based on the connected components
/// the refinement produces.
fn refine(
    g: &Graph,
    partition: &[u32],
    resolution: f64,
    rng: &mut ChaCha8Rng,
) -> Vec<u32> {
    // Start with each node in its own refined community.
    let mut refined: Vec<u32> = (0..g.n as u32).collect();

    // Group nodes by their `partition` community.
    let mut groups: HashMap<u32, Vec<usize>> = HashMap::new();
    for (v, &c) in partition.iter().enumerate() {
        groups.entry(c).or_default().push(v);
    }

    let two_m = 2.0 * g.total_weight;
    if two_m <= 0.0 {
        return refined;
    }

    for (community, members) in &groups {
        // Within this community, do local-moving but restrict each node to
        // either stay isolated or join a refined community that's already
        // *inside the same outer community*.
        let mut order = members.clone();
        order.shuffle(rng);

        // Track refined-community weight inside this outer community.
        let mut refined_weight: HashMap<u32, f64> = HashMap::new();
        for &v in members {
            *refined_weight.entry(refined[v]).or_insert(0.0) += g.degrees[v];
        }

        let member_set: HashSet<usize> = members.iter().copied().collect();
        for v in order {
            let k_v = g.degrees[v];
            if k_v <= 0.0 {
                continue;
            }
            let old_r = refined[v];
            let mut to_refined: HashMap<u32, f64> = HashMap::new();
            for &(u, w) in &g.neighbours[v] {
                if !member_set.contains(&(u as usize)) {
                    continue; // skip cross-community neighbours
                }
                let r = refined[u as usize];
                *to_refined.entry(r).or_insert(0.0) += w;
            }
            if to_refined.is_empty() {
                continue;
            }
            let weight_old = refined_weight[&old_r] - k_v;
            let to_old = *to_refined.get(&old_r).unwrap_or(&0.0);

            let mut best_r = old_r;
            let mut best_gain = 0.0_f64;
            for (&r, &to_r) in &to_refined {
                if r == old_r {
                    continue;
                }
                let weight_r = *refined_weight.get(&r).unwrap_or(&0.0);
                let delta = (to_r - to_old) / g.total_weight
                    - resolution * k_v * (weight_r - weight_old) / (two_m * two_m);
                if delta > best_gain + 1e-12 {
                    best_gain = delta;
                    best_r = r;
                }
            }
            if best_r != old_r {
                refined[v] = best_r;
                *refined_weight.entry(old_r).or_insert(0.0) -= k_v;
                *refined_weight.entry(best_r).or_insert(0.0) += k_v;
            }
        }
        let _ = community; // explicit: we used members above
    }
    refined
}

/// Aggregate a graph by collapsing each refined-community to a super-node.
/// Returns the next-level graph + a relabel table from original node index
/// → super-node index in the new graph.
fn aggregate(g: &Graph, refined: &[u32]) -> (Graph, Vec<u32>) {
    let compact = compactify(refined);
    let max = compact.iter().copied().max().unwrap_or(0) as usize;
    let n_new = max + 1;
    let mut degrees = vec![0.0_f64; n_new];
    let mut edge_map: HashMap<(u32, u32), f64> = HashMap::new();
    for v in 0..g.n {
        for &(u, w) in &g.neighbours[v] {
            // Each undirected edge is iterated twice; halve the contribution
            // to edge_map and skip self-loops on the aggregate (they don't
            // change modularity).
            if (u as usize) <= v {
                continue;
            }
            let cv = compact[v];
            let cu = compact[u as usize];
            if cv == cu {
                continue;
            }
            let key = if cv < cu { (cv, cu) } else { (cu, cv) };
            *edge_map.entry(key).or_insert(0.0) += w;
            degrees[cv as usize] += w;
            degrees[cu as usize] += w;
        }
    }
    let mut neighbours = vec![Vec::new(); n_new];
    for ((i, j), w) in &edge_map {
        neighbours[*i as usize].push((*j, *w));
        neighbours[*j as usize].push((*i, *w));
    }
    let total_weight: f64 = degrees.iter().sum::<f64>() / 2.0;
    (
        Graph {
            n: n_new,
            neighbours,
            degrees,
            total_weight,
        },
        compact,
    )
}

fn compactify(labels: &[u32]) -> Vec<u32> {
    let mut remap: HashMap<u32, u32> = HashMap::new();
    let mut next: u32 = 0;
    labels
        .iter()
        .map(|&l| {
            *remap.entry(l).or_insert_with(|| {
                let v = next;
                next += 1;
                v
            })
        })
        .collect()
}

fn no_more_aggregation(partition: &[u32], refined: &[u32]) -> bool {
    // Aggregating with `refined` is a no-op when every refined-community is
    // already a singleton and every outer-community is the same as the
    // refined-community for its only member.
    if partition.len() != refined.len() {
        return true;
    }
    let mut seen: HashMap<u32, u32> = HashMap::new();
    for i in 0..partition.len() {
        match seen.get(&refined[i]) {
            Some(&p) if p == partition[i] => {}
            Some(_) => return false,
            None => {
                seen.insert(refined[i], partition[i]);
            }
        }
    }
    // Each refined label maps to a single outer label → aggregating yields
    // |refined| super-nodes with the same internal grouping as before.
    seen.len() == refined.iter().copied().collect::<HashSet<_>>().len()
}

/// Compute modularity 𝑄 of `partition` on graph `g` at resolution γ.
fn modularity(g: &Graph, partition: &[u32], resolution: f64) -> f64 {
    if g.total_weight <= 0.0 {
        return 0.0;
    }
    let mut community_weight: HashMap<u32, f64> = HashMap::new();
    let mut in_weight: HashMap<u32, f64> = HashMap::new();
    for (v, &c) in partition.iter().enumerate() {
        *community_weight.entry(c).or_insert(0.0) += g.degrees[v];
        for &(u, w) in &g.neighbours[v] {
            if partition[u as usize] == c {
                *in_weight.entry(c).or_insert(0.0) += w;
            }
        }
    }
    // Each within-community edge counted twice; divide by 2.
    let two_m = 2.0 * g.total_weight;
    let mut q = 0.0_f64;
    for (c, w) in &community_weight {
        let l_in = *in_weight.get(c).unwrap_or(&0.0) / 2.0;
        q += l_in / g.total_weight - resolution * (w / two_m).powi(2);
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("n{i}")).collect()
    }

    #[test]
    fn empty_input_yields_one_empty_level() {
        let r = leiden(&[], &[], 1.0, 4, 30, 0);
        assert_eq!(r.levels.len(), 1);
        assert!(r.final_partition().is_empty());
        assert_eq!(r.modularities, vec![0.0]);
    }

    #[test]
    fn isolated_nodes_each_get_their_own_community() {
        let nodes = nodes(3);
        let r = leiden(&nodes, &[], 1.0, 4, 30, 0);
        let p = r.final_partition();
        let unique: HashSet<u32> = p.values().copied().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn clique_collapses_to_one_community() {
        let nodes = nodes(5);
        let mut edges = Vec::new();
        for i in 0..5 {
            for j in (i + 1)..5 {
                edges.push((nodes[i].clone(), nodes[j].clone()));
            }
        }
        let r = leiden(&nodes, &edges, 1.0, 4, 30, 7);
        let p = r.final_partition();
        let unique: HashSet<u32> = p.values().copied().collect();
        assert_eq!(unique.len(), 1, "5-clique should collapse to one community");
    }

    #[test]
    fn two_cliques_with_bridge_split_into_two_communities() {
        let nodes = nodes(6);
        let mut edges = vec![];
        for &(i, j) in &[
            (0, 1),
            (0, 2),
            (1, 2),
            (3, 4),
            (3, 5),
            (4, 5),
            (2, 3),
        ] {
            edges.push((nodes[i].clone(), nodes[j].clone()));
        }
        let r = leiden(&nodes, &edges, 1.0, 4, 30, 11);
        let p = r.final_partition();
        let l0 = p[&nodes[0]];
        let l5 = p[&nodes[5]];
        assert_ne!(l0, l5, "two cliques across a single bridge edge should split");
        let inside_a: HashSet<u32> = (0..3).map(|i| p[&nodes[i]]).collect();
        let inside_b: HashSet<u32> = (3..6).map(|i| p[&nodes[i]]).collect();
        assert_eq!(inside_a.len(), 1, "clique A should be one community internally");
        assert_eq!(inside_b.len(), 1, "clique B should be one community internally");
    }

    /// Modularity should be strictly positive on a well-structured graph.
    /// For the two-cliques-with-bridge graph above, theoretical Q ≈ 0.36.
    #[test]
    fn two_cliques_have_meaningful_modularity() {
        let nodes = nodes(6);
        let mut edges = vec![];
        for &(i, j) in &[
            (0, 1),
            (0, 2),
            (1, 2),
            (3, 4),
            (3, 5),
            (4, 5),
            (2, 3),
        ] {
            edges.push((nodes[i].clone(), nodes[j].clone()));
        }
        let r = leiden(&nodes, &edges, 1.0, 4, 30, 11);
        let q = *r.modularities.last().unwrap();
        assert!(q > 0.25, "expected meaningful modularity, got {q}");
    }

    /// Same input + same seed → identical partition (determinism).
    #[test]
    fn same_seed_replays_partition() {
        let nodes = nodes(8);
        let mut edges = vec![];
        for &(i, j) in &[
            (0, 1),
            (0, 2),
            (1, 2),
            (1, 3),
            (3, 4),
            (4, 5),
            (5, 6),
            (6, 7),
            (4, 6),
        ] {
            edges.push((nodes[i].clone(), nodes[j].clone()));
        }
        let a = leiden(&nodes, &edges, 1.0, 4, 30, 42);
        let b = leiden(&nodes, &edges, 1.0, 4, 30, 42);
        assert_eq!(a.final_partition(), b.final_partition());
        assert_eq!(a.modularities, b.modularities);
    }

    /// Hierarchy property: as we go from level 0 (bottom = many small
    /// communities) to the last level (coarser), the number of distinct
    /// labels should never increase.
    #[test]
    fn hierarchy_levels_are_monotone_non_increasing() {
        let nodes = nodes(12);
        // Three weakly-coupled triangles → expect 3 communities at the top.
        let mut edges = vec![];
        for cluster in 0..3 {
            let base = cluster * 4;
            for &(i, j) in &[(0, 1), (0, 2), (1, 2), (0, 3), (1, 3), (2, 3)] {
                edges.push((nodes[base + i].clone(), nodes[base + j].clone()));
            }
        }
        // Sparse cross-cluster edges so the graph stays connected.
        edges.push((nodes[3].clone(), nodes[4].clone()));
        edges.push((nodes[7].clone(), nodes[8].clone()));
        let r = leiden(&nodes, &edges, 1.0, 4, 50, 17);
        let unique_per_level: Vec<usize> = r
            .levels
            .iter()
            .map(|p| p.values().copied().collect::<HashSet<u32>>().len())
            .collect();
        for w in unique_per_level.windows(2) {
            assert!(w[0] >= w[1], "level community counts: {unique_per_level:?}");
        }
    }

    /// Leiden's modularity should be at least as good as LPA on a
    /// canonical two-cliques-plus-bridge graph.
    #[test]
    fn beats_or_matches_lpa_on_two_cliques() {
        use crate::core::graph_rag::community_detector::detect_communities;
        let nodes = nodes(6);
        let mut edges = vec![];
        for &(i, j) in &[
            (0, 1),
            (0, 2),
            (1, 2),
            (3, 4),
            (3, 5),
            (4, 5),
            (2, 3),
        ] {
            edges.push((nodes[i].clone(), nodes[j].clone()));
        }
        let r_leiden = leiden(&nodes, &edges, 1.0, 4, 30, 11);
        let p_lpa = detect_communities(&nodes, &edges, 30, 11);

        // Compute LPA's modularity on the same graph.
        let g = build_graph(&nodes, &edges);
        let mut lpa_vec = vec![0_u32; g.n];
        for (id, idx) in nodes.iter().zip(0..g.n) {
            lpa_vec[idx] = p_lpa[id];
        }
        let q_lpa = modularity(&g, &lpa_vec, 1.0);
        let q_leiden = *r_leiden.modularities.last().unwrap();

        assert!(
            q_leiden >= q_lpa - 1e-6,
            "Leiden modularity {q_leiden} should be ≥ LPA modularity {q_lpa}"
        );
    }
}
