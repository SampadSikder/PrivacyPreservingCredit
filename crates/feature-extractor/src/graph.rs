use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use serde::Deserialize;

/// A single transaction between two accounts.
#[derive(Debug, Clone, Deserialize)]
pub struct Transaction {
    pub from: String,
    pub to: String,
    pub amount: f64,
    pub timestamp: u64,
}

/// Aggregated data for a directed edge between two accounts.
#[derive(Debug, Clone)]
pub struct EdgeData {
    /// Total monetary value across all transactions on this edge.
    pub total_amount: f64,
    /// Number of individual transactions aggregated into this edge.
    pub tx_count: u32,
}

/// JSON input format for loading a transaction graph from file.
#[derive(Debug, Deserialize)]
pub struct GraphInput {
    /// The account ID of the focal user whose credit is being scored.
    pub user: String,
    /// The list of all transactions in the network.
    pub transactions: Vec<Transaction>,
}

/// A directed transaction graph built on top of petgraph.
///
/// Nodes represent account IDs, edges represent aggregated transaction flows.
/// The graph tracks a "focal user" node — the person whose credit is being evaluated.
#[derive(Debug, Clone)]
pub struct TransactionGraph {
    graph: DiGraph<String, EdgeData>,
    node_map: HashMap<String, NodeIndex>,
    user_node: NodeIndex,
    user_id: String,
}

impl TransactionGraph {
    /// Build a `TransactionGraph` from a list of transactions.
    ///
    /// Multiple transactions between the same ordered pair (from, to) are
    /// aggregated into a single directed edge with accumulated amount and count.
    pub fn from_transactions(user: &str, txs: &[Transaction]) -> Self {
        let mut graph = DiGraph::<String, EdgeData>::new();
        let mut node_map = HashMap::<String, NodeIndex>::new();

        // Helper to get-or-insert a node
        let get_node = |graph: &mut DiGraph<String, EdgeData>,
                            map: &mut HashMap<String, NodeIndex>,
                            id: &str|
         -> NodeIndex {
            if let Some(&idx) = map.get(id) {
                idx
            } else {
                let idx = graph.add_node(id.to_string());
                map.insert(id.to_string(), idx);
                idx
            }
        };

        // Ensure the user node exists even if they have no transactions
        let user_node = get_node(&mut graph, &mut node_map, user);

        for tx in txs {
            let from = get_node(&mut graph, &mut node_map, &tx.from);
            let to = get_node(&mut graph, &mut node_map, &tx.to);

            // Check if an edge already exists between these two nodes
            if let Some(edge_idx) = graph.find_edge(from, to) {
                let edge = graph.edge_weight_mut(edge_idx).unwrap();
                edge.total_amount += tx.amount;
                edge.tx_count += 1;
            } else {
                graph.add_edge(
                    from,
                    to,
                    EdgeData {
                        total_amount: tx.amount,
                        tx_count: 1,
                    },
                );
            }
        }

        TransactionGraph {
            graph,
            node_map,
            user_node,
            user_id: user.to_string(),
        }
    }

    /// Deserialize a transaction graph from a JSON string.
    ///
    /// Expected format:
    /// ```json
    /// {
    ///   "user": "alice",
    ///   "transactions": [
    ///     { "from": "alice", "to": "bob", "amount": 100.0, "timestamp": 1700000000 }
    ///   ]
    /// }
    /// ```
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let input: GraphInput = serde_json::from_str(json)?;
        Ok(Self::from_transactions(&input.user, &input.transactions))
    }

    /// Returns the NodeIndex of the focal user.
    pub fn user_node(&self) -> NodeIndex {
        self.user_node
    }

    /// Returns the account ID of the focal user.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Returns a reference to the underlying petgraph DiGraph.
    pub fn inner(&self) -> &DiGraph<String, EdgeData> {
        &self.graph
    }

    /// Returns the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Returns the in-degree of a node (number of incoming edges).
    pub fn in_degree(&self, node: NodeIndex) -> usize {
        self.graph.edges_directed(node, Direction::Incoming).count()
    }

    /// Returns the out-degree of a node (number of outgoing edges).
    pub fn out_degree(&self, node: NodeIndex) -> usize {
        self.graph.edges_directed(node, Direction::Outgoing).count()
    }

    /// Returns the NodeIndex for a given account ID, if it exists.
    pub fn node_index(&self, id: &str) -> Option<NodeIndex> {
        self.node_map.get(id).copied()
    }

    /// Returns the account ID for a given NodeIndex.
    pub fn node_id(&self, idx: NodeIndex) -> &str {
        &self.graph[idx]
    }

    /// Returns the node map (account ID → NodeIndex).
    pub fn node_map(&self) -> &HashMap<String, NodeIndex> {
        &self.node_map
    }

    /// Construct a `TransactionGraph` from pre-built components.
    ///
    /// Used internally by neighborhood extraction to build subgraphs.
    pub(crate) fn from_parts(
        graph: DiGraph<String, EdgeData>,
        node_map: HashMap<String, NodeIndex>,
        user_node: NodeIndex,
        user_id: String,
    ) -> Self {
        TransactionGraph {
            graph,
            node_map,
            user_node,
            user_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_transactions_basic() {
        let txs = vec![
            Transaction {
                from: "alice".into(),
                to: "bob".into(),
                amount: 100.0,
                timestamp: 1,
            },
            Transaction {
                from: "bob".into(),
                to: "alice".into(),
                amount: 50.0,
                timestamp: 2,
            },
        ];

        let graph = TransactionGraph::from_transactions("alice", &txs);
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 2); // two directed edges
        assert_eq!(graph.node_id(graph.user_node()), "alice");
    }

    #[test]
    fn test_edge_aggregation() {
        let txs = vec![
            Transaction {
                from: "alice".into(),
                to: "bob".into(),
                amount: 100.0,
                timestamp: 1,
            },
            Transaction {
                from: "alice".into(),
                to: "bob".into(),
                amount: 200.0,
                timestamp: 2,
            },
        ];

        let graph = TransactionGraph::from_transactions("alice", &txs);
        assert_eq!(graph.edge_count(), 1); // aggregated into one edge

        let alice = graph.node_index("alice").unwrap();
        let bob = graph.node_index("bob").unwrap();
        let edge = graph.inner().find_edge(alice, bob).unwrap();
        let data = &graph.inner()[edge];
        assert_eq!(data.total_amount, 300.0);
        assert_eq!(data.tx_count, 2);
    }

    #[test]
    fn test_from_json() {
        let json = r#"{
            "user": "alice",
            "transactions": [
                { "from": "alice", "to": "bob", "amount": 100.0, "timestamp": 1 },
                { "from": "bob", "to": "carol", "amount": 50.0, "timestamp": 2 }
            ]
        }"#;

        let graph = TransactionGraph::from_json(json).unwrap();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.user_id(), "alice");
    }

    #[test]
    fn test_degree() {
        let txs = vec![
            Transaction {
                from: "alice".into(),
                to: "bob".into(),
                amount: 100.0,
                timestamp: 1,
            },
            Transaction {
                from: "alice".into(),
                to: "carol".into(),
                amount: 50.0,
                timestamp: 2,
            },
            Transaction {
                from: "dave".into(),
                to: "alice".into(),
                amount: 30.0,
                timestamp: 3,
            },
        ];

        let graph = TransactionGraph::from_transactions("alice", &txs);
        let alice = graph.user_node();
        assert_eq!(graph.out_degree(alice), 2);
        assert_eq!(graph.in_degree(alice), 1);
    }

    #[test]
    fn test_empty_transactions() {
        let graph = TransactionGraph::from_transactions("alice", &[]);
        assert_eq!(graph.node_count(), 1); // user node still exists
        assert_eq!(graph.edge_count(), 0);
    }
}
