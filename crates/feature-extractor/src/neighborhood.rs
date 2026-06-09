use std::collections::{HashMap, HashSet};

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::graph::{EdgeData, TransactionGraph};

/// Configuration for k-hop neighborhood extraction.
#[derive(Debug, Clone)]
pub struct NeighborhoodConfig {
    /// Number of hops from the focal user to include. Default: 2.
    pub hops: u32,
    /// Optional maximum number of neighbors to expand per node per hop.
    /// When set, keeps the neighbors with the highest total edge weight.
    /// Prevents blowup on hub nodes (e.g., a merchant with 100K customers).
    pub max_neighbors_per_hop: Option<u32>,
}

impl Default for NeighborhoodConfig {
    fn default() -> Self {
        Self {
            hops: 2,
            max_neighbors_per_hop: None,
        }
    }
}

/// Extract a k-hop ego subgraph around the focal user.
///
/// BFS from the user node out to `config.hops` levels, optionally pruning
/// low-weight neighbors at each hop. Returns a new `TransactionGraph`
/// containing only the nodes within the neighborhood and all edges
/// between them (preserving direction).
pub fn extract_neighborhood(
    graph: &TransactionGraph,
    config: &NeighborhoodConfig,
) -> TransactionGraph {
    let inner = graph.inner();
    let user = graph.user_node();

    // BFS to collect nodes within k hops
    let mut visited = HashSet::<NodeIndex>::new();
    visited.insert(user);
    let mut frontier = vec![user];

    for _hop in 0..config.hops {
        let mut next_frontier = Vec::new();

        for &node in &frontier {
            // Collect all undirected neighbors (both in and out edges)
            let mut neighbors_with_weight: Vec<(NodeIndex, f64)> = Vec::new();

            for edge in inner.edges_directed(node, Direction::Outgoing) {
                let neighbor = edge.target();
                if !visited.contains(&neighbor) {
                    neighbors_with_weight.push((neighbor, edge.weight().total_amount));
                }
            }
            for edge in inner.edges_directed(node, Direction::Incoming) {
                let neighbor = edge.source();
                if !visited.contains(&neighbor) {
                    // Check if we already added this neighbor from outgoing
                    if !neighbors_with_weight.iter().any(|(n, _)| *n == neighbor) {
                        neighbors_with_weight.push((neighbor, edge.weight().total_amount));
                    } else {
                        // Add weight to existing entry
                        if let Some(entry) =
                            neighbors_with_weight.iter_mut().find(|(n, _)| *n == neighbor)
                        {
                            entry.1 += edge.weight().total_amount;
                        }
                    }
                }
            }

            // If max_neighbors_per_hop is set, sort by weight descending and take top N
            if let Some(max_n) = config.max_neighbors_per_hop {
                neighbors_with_weight
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                neighbors_with_weight.truncate(max_n as usize);
            }

            for (neighbor, _weight) in neighbors_with_weight {
                if visited.insert(neighbor) {
                    next_frontier.push(neighbor);
                }
            }
        }

        frontier = next_frontier;
    }

    // Build the induced subgraph: create a new TransactionGraph with only the visited nodes
    // and edges between them
    build_subgraph(graph, &visited)
}

/// Build a new `TransactionGraph` from a subset of nodes of the original graph.
///
/// Includes all edges from the original graph where both endpoints are in `nodes`.
fn build_subgraph(graph: &TransactionGraph, nodes: &HashSet<NodeIndex>) -> TransactionGraph {
    let inner = graph.inner();
    let mut new_graph = petgraph::graph::DiGraph::<String, EdgeData>::new();
    let mut old_to_new = HashMap::<NodeIndex, NodeIndex>::new();
    let mut new_node_map = HashMap::<String, NodeIndex>::new();

    // Add nodes
    for &old_idx in nodes {
        let label = inner[old_idx].clone();
        let new_idx = new_graph.add_node(label.clone());
        old_to_new.insert(old_idx, new_idx);
        new_node_map.insert(label, new_idx);
    }

    // Add edges where both endpoints are in the subgraph
    for &old_idx in nodes {
        for edge in inner.edges_directed(old_idx, Direction::Outgoing) {
            let target = edge.target();
            if let (Some(&new_src), Some(&new_tgt)) =
                (old_to_new.get(&old_idx), old_to_new.get(&target))
            {
                let data = edge.weight();
                new_graph.add_edge(
                    new_src,
                    new_tgt,
                    EdgeData {
                        total_amount: data.total_amount,
                        tx_count: data.tx_count,
                    },
                );
            }
        }
    }

    // Map the user node to the new graph
    let new_user = old_to_new[&graph.user_node()];

    TransactionGraph::from_parts(new_graph, new_node_map, new_user, graph.user_id().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Transaction;

    /// Build a chain: alice -> bob -> carol -> dave -> eve
    fn chain_graph() -> TransactionGraph {
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "bob".into(), to: "carol".into(), amount: 80.0, timestamp: 2 },
            Transaction { from: "carol".into(), to: "dave".into(), amount: 60.0, timestamp: 3 },
            Transaction { from: "dave".into(), to: "eve".into(), amount: 40.0, timestamp: 4 },
        ];
        TransactionGraph::from_transactions("alice", &txs)
    }

    #[test]
    fn test_1_hop_neighborhood() {
        let graph = chain_graph();
        let config = NeighborhoodConfig { hops: 1, max_neighbors_per_hop: None };
        let sub = extract_neighborhood(&graph, &config);

        // alice + bob
        assert_eq!(sub.node_count(), 2);
        assert!(sub.node_index("alice").is_some());
        assert!(sub.node_index("bob").is_some());
        assert!(sub.node_index("carol").is_none());
    }

    #[test]
    fn test_2_hop_neighborhood() {
        let graph = chain_graph();
        let config = NeighborhoodConfig { hops: 2, max_neighbors_per_hop: None };
        let sub = extract_neighborhood(&graph, &config);

        // alice + bob + carol
        assert_eq!(sub.node_count(), 3);
        assert!(sub.node_index("alice").is_some());
        assert!(sub.node_index("bob").is_some());
        assert!(sub.node_index("carol").is_some());
        assert!(sub.node_index("dave").is_none());
    }

    #[test]
    fn test_full_hop_neighborhood() {
        let graph = chain_graph();
        let config = NeighborhoodConfig { hops: 10, max_neighbors_per_hop: None };
        let sub = extract_neighborhood(&graph, &config);

        // All 5 nodes
        assert_eq!(sub.node_count(), 5);
    }

    #[test]
    fn test_max_neighbors_pruning() {
        // Star graph: alice -> {bob, carol, dave, eve, frank}
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 500.0, timestamp: 1 },
            Transaction { from: "alice".into(), to: "carol".into(), amount: 400.0, timestamp: 2 },
            Transaction { from: "alice".into(), to: "dave".into(), amount: 300.0, timestamp: 3 },
            Transaction { from: "alice".into(), to: "eve".into(), amount: 200.0, timestamp: 4 },
            Transaction { from: "alice".into(), to: "frank".into(), amount: 100.0, timestamp: 5 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);

        let config = NeighborhoodConfig { hops: 1, max_neighbors_per_hop: Some(2) };
        let sub = extract_neighborhood(&graph, &config);

        // alice + top 2 by weight (bob=500, carol=400)
        assert_eq!(sub.node_count(), 3);
        assert!(sub.node_index("alice").is_some());
        assert!(sub.node_index("bob").is_some());
        assert!(sub.node_index("carol").is_some());
    }

    #[test]
    fn test_user_node_preserved() {
        let graph = chain_graph();
        let config = NeighborhoodConfig::default();
        let sub = extract_neighborhood(&graph, &config);

        assert_eq!(sub.user_id(), "alice");
        assert_eq!(sub.node_id(sub.user_node()), "alice");
    }

    #[test]
    fn test_edges_preserved_in_subgraph() {
        let graph = chain_graph();
        let config = NeighborhoodConfig { hops: 2, max_neighbors_per_hop: None };
        let sub = extract_neighborhood(&graph, &config);

        // Should have edges: alice->bob, bob->carol
        assert_eq!(sub.edge_count(), 2);
    }
}
