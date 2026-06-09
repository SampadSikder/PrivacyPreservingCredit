use std::collections::{HashMap, VecDeque};

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::graph::TransactionGraph;


/// `in_degree_centrality(v) = in_degree(v) / (n - 1)`

pub fn in_degree_centrality(graph: &TransactionGraph, node: NodeIndex) -> f64 {
    let n = graph.node_count();
    if n <= 1 {
        return 0.0;
    }
    graph.in_degree(node) as f64 / (n - 1) as f64
}


/// `out_degree_centrality(v) = out_degree(v) / (n - 1)`

pub fn out_degree_centrality(graph: &TransactionGraph, node: NodeIndex) -> f64 {
    let n = graph.node_count();
    if n <= 1 {
        return 0.0;
    }
    graph.out_degree(node) as f64 / (n - 1) as f64
}

/// Compute betweenness centrality for all nodes using Brandes' algorithm (BFS-based).
/// This implementation treats all edges as unweighted (each edge has unit distance).
/// For directed graphs, the result is normalized by `1 / ((n-1)(n-2))`.
/// Returns a map from NodeIndex to its betweenness centrality value in [0, 1].
pub fn betweenness_centrality(graph: &TransactionGraph) -> HashMap<NodeIndex, f64> {
    let inner = graph.inner();
    let nodes: Vec<NodeIndex> = inner.node_indices().collect();
    let n = nodes.len();

    let mut betweenness: HashMap<NodeIndex, f64> = nodes.iter().map(|&v| (v, 0.0)).collect();

    // If there are only two nodes or less, betweenness is 0
    if n <= 2 {
        return betweenness;
    }

    for &s in &nodes {
        let mut stack = Vec::<NodeIndex>::new();
        let mut predecessors: HashMap<NodeIndex, Vec<NodeIndex>> =
            nodes.iter().map(|&v| (v, Vec::new())).collect();

        // sigma[v] = number of shortest paths from s to v
        let mut sigma: HashMap<NodeIndex, f64> = nodes.iter().map(|&v| (v, 0.0)).collect();
        *sigma.get_mut(&s).unwrap() = 1.0;

        // dist[v] = distance from s to v (-1 = not visited)
        let mut dist: HashMap<NodeIndex, i64> = nodes.iter().map(|&v| (v, -1)).collect();
        *dist.get_mut(&s).unwrap() = 0;

        let mut queue = VecDeque::new();
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            stack.push(v);

            for edge in inner.edges_directed(v, Direction::Outgoing) {
                let w = edge.target();
                let dv = dist[&v];

                if dist[&w] < 0 {
                    *dist.get_mut(&w).unwrap() = dv + 1;
                    queue.push_back(w);
                }

                if dist[&w] == dv + 1 {
                    *sigma.get_mut(&w).unwrap() += sigma[&v];
                    predecessors.get_mut(&w).unwrap().push(v);
                }
            }
        }

        // --- Accumulation phase ---
        let mut delta: HashMap<NodeIndex, f64> = nodes.iter().map(|&v| (v, 0.0)).collect();

        // Process nodes in reverse BFS order (farthest first)
        while let Some(w) = stack.pop() {
            for &v in &predecessors[&w] {
                let contribution = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                *delta.get_mut(&v).unwrap() += contribution;
            }
            if w != s {
                *betweenness.get_mut(&w).unwrap() += delta[&w];
            }
        }
    }

    // Normalize to bound from [0,1]
    let normalization = ((n - 1) * (n - 2)) as f64;
    if normalization > 0.0 {
        for val in betweenness.values_mut() {
            *val /= normalization;
        }
    }

    betweenness
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Transaction;

    #[test]
    fn test_degree_centrality_star() {
        // Star: alice -> {bob, carol, dave}
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "alice".into(), to: "carol".into(), amount: 100.0, timestamp: 2 },
            Transaction { from: "alice".into(), to: "dave".into(), amount: 100.0, timestamp: 3 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);
        let alice = graph.user_node();

        // alice has 3 outgoing, 0 incoming, out of 4 nodes total
        assert!((out_degree_centrality(&graph, alice) - 1.0).abs() < 1e-9);
        assert!((in_degree_centrality(&graph, alice) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_degree_centrality_single_node() {
        let graph = TransactionGraph::from_transactions("alice", &[]);
        let alice = graph.user_node();
        assert_eq!(in_degree_centrality(&graph, alice), 0.0);
        assert_eq!(out_degree_centrality(&graph, alice), 0.0);
    }

    #[test]
    fn test_betweenness_star() {
        // Star: center -> {a, b, c, d}. Center should have high betweenness.
        let txs = vec![
            Transaction { from: "center".into(), to: "a".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "center".into(), to: "b".into(), amount: 100.0, timestamp: 2 },
            Transaction { from: "center".into(), to: "c".into(), amount: 100.0, timestamp: 3 },
            Transaction { from: "center".into(), to: "d".into(), amount: 100.0, timestamp: 4 },
        ];
        let graph = TransactionGraph::from_transactions("center", &txs);
        let bc = betweenness_centrality(&graph);

        let center = graph.user_node();
        // Leaf nodes should have 0 betweenness
        for (node, &val) in &bc {
            if *node != center {
                assert!(val.abs() < 1e-9, "Leaf node should have 0 betweenness");
            }
        }
    }

    #[test]
    fn test_betweenness_chain() {
        // Chain: a -> b -> c -> d
        // b and c should have non-zero betweenness
        let txs = vec![
            Transaction { from: "a".into(), to: "b".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "b".into(), to: "c".into(), amount: 100.0, timestamp: 2 },
            Transaction { from: "c".into(), to: "d".into(), amount: 100.0, timestamp: 3 },
        ];
        let graph = TransactionGraph::from_transactions("a", &txs);
        let bc = betweenness_centrality(&graph);

        let b = graph.node_index("b").unwrap();
        let c = graph.node_index("c").unwrap();

        // b is on path a->b->c and a->b->c->d
        // c is on path b->c->d and a->b->c->d
        assert!(bc[&b] > 0.0, "Middle node b should have positive betweenness");
        assert!(bc[&c] > 0.0, "Middle node c should have positive betweenness");
    }

    #[test]
    fn test_betweenness_two_nodes() {
        let txs = vec![
            Transaction { from: "a".into(), to: "b".into(), amount: 100.0, timestamp: 1 },
        ];
        let graph = TransactionGraph::from_transactions("a", &txs);
        let bc = betweenness_centrality(&graph);

        // With n=2, all betweenness should be 0
        for &val in bc.values() {
            assert_eq!(val, 0.0);
        }
    }

    #[test]
    fn test_betweenness_clique() {
        // Complete directed graph on 4 nodes: every pair has a direct edge
        // Betweenness should be 0 for all (all shortest paths are direct)
        let nodes = ["a", "b", "c", "d"];
        let mut txs = Vec::new();
        let mut ts = 1u64;
        for &from in &nodes {
            for &to in &nodes {
                if from != to {
                    txs.push(Transaction {
                        from: from.into(),
                        to: to.into(),
                        amount: 100.0,
                        timestamp: ts,
                    });
                    ts += 1;
                }
            }
        }
        let graph = TransactionGraph::from_transactions("a", &txs);
        let bc = betweenness_centrality(&graph);

        for &val in bc.values() {
            assert!(val.abs() < 1e-9, "Clique nodes should have ~0 betweenness");
        }
    }
}
