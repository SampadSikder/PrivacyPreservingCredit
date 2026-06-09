use std::collections::HashSet;

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::graph::TransactionGraph;

/// Compute the local clustering coefficient for a node.
/// Uses the undirected projection: treats all edges as bidirectional for the
/// purpose of neighbor enumeration and triangle counting.

pub fn clustering_coefficient(graph: &TransactionGraph, node: NodeIndex) -> f64 {
    let inner = graph.inner();

    let mut neighbors = HashSet::<NodeIndex>::new();
    for edge in inner.edges_directed(node, Direction::Outgoing) {
        neighbors.insert(edge.target());
    }
    for edge in inner.edges_directed(node, Direction::Incoming) {
        neighbors.insert(edge.source());
    }

    neighbors.remove(&node);

    let deg = neighbors.len();
    if deg < 2 {
        return 0.0;
    }

    let neighbor_vec: Vec<NodeIndex> = neighbors.iter().copied().collect();
    let mut triangle_edges = 0u64;

    for i in 0..neighbor_vec.len() {
        for j in (i + 1)..neighbor_vec.len() {
            let u = neighbor_vec[i];
            let v = neighbor_vec[j];
            // Check if there's any directed edge between u and v (either direction)
            if inner.find_edge(u, v).is_some() || inner.find_edge(v, u).is_some() {
                triangle_edges += 1;
            }
        }
    }

    let max_edges = (deg * (deg - 1)) / 2;

    triangle_edges as f64 / max_edges as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Transaction;

    #[test]
    fn test_clustering_star() {
        // Star: alice -> {bob, carol, dave}. No edges between leaves.
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "alice".into(), to: "carol".into(), amount: 100.0, timestamp: 2 },
            Transaction { from: "alice".into(), to: "dave".into(), amount: 100.0, timestamp: 3 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);
        let alice = graph.user_node();
        assert_eq!(clustering_coefficient(&graph, alice), 0.0);
    }

    #[test]
    fn test_clustering_clique() {
        // Complete graph on {alice, bob, carol}: every pair connected
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "alice".into(), to: "carol".into(), amount: 100.0, timestamp: 2 },
            Transaction { from: "bob".into(), to: "alice".into(), amount: 100.0, timestamp: 3 },
            Transaction { from: "bob".into(), to: "carol".into(), amount: 100.0, timestamp: 4 },
            Transaction { from: "carol".into(), to: "alice".into(), amount: 100.0, timestamp: 5 },
            Transaction { from: "carol".into(), to: "bob".into(), amount: 100.0, timestamp: 6 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);
        let alice = graph.user_node();

        // All neighbors are connected to each other
        assert!((clustering_coefficient(&graph, alice) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_clustering_partial() {
        // alice -> {bob, carol, dave}, bob -> carol (one triangle edge out of 3 possible)
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "alice".into(), to: "carol".into(), amount: 100.0, timestamp: 2 },
            Transaction { from: "alice".into(), to: "dave".into(), amount: 100.0, timestamp: 3 },
            Transaction { from: "bob".into(), to: "carol".into(), amount: 50.0, timestamp: 4 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);
        let alice = graph.user_node();

        // 3 neighbors, 1 edge between them, max = 3
        let cc = clustering_coefficient(&graph, alice);
        assert!((cc - 1.0 / 3.0).abs() < 1e-9, "Expected 1/3, got {}", cc);
    }

    #[test]
    fn test_clustering_single_neighbor() {
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);
        let alice = graph.user_node();
        assert_eq!(clustering_coefficient(&graph, alice), 0.0);
    }

    #[test]
    fn test_clustering_no_neighbors() {
        let graph = TransactionGraph::from_transactions("alice", &[]);
        let alice = graph.user_node();
        assert_eq!(clustering_coefficient(&graph, alice), 0.0);
    }
}
