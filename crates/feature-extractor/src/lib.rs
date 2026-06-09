//! # Feature Extractor
//!
//! Graph-based credit feature extraction from transaction networks.
//!
//! This crate takes a user's private transaction graph and produces a
//! `FeatureVector` of credit-relevant metrics. Features are computed on
//! a k-hop ego subgraph around the focal user to bound computation on
//! large networks.
//!
//! ## Pipeline
//!
//! ```text
//! Full Transaction Graph
//!          │
//!          ▼
//!   k-hop Ego Subgraph  (neighborhood.rs)
//!          │
//!          ▼
//!   Feature Computation  (centrality.rs, clustering.rs)
//!          │
//!          ▼
//!     FeatureVector
//!          │
//!          ▼
//!  QuantizedFeatureVector  (for ZKP circuits)
//! ```

pub mod centrality;
pub mod clustering;
pub mod graph;
pub mod neighborhood;

pub use graph::{EdgeData, Transaction, TransactionGraph};
pub use neighborhood::NeighborhoodConfig;

use petgraph::visit::EdgeRef;
use petgraph::Direction;

/// Scale factor for quantizing float features to integers.
/// Provides 4 decimal places of precision.
pub const SCALE_FACTOR: f64 = 10_000.0;

/// The feature vector produced from a user's transaction graph.
///
/// Float values suitable for analysis and display. Use `quantize()` to
/// convert to integer representation for ZKP circuit consumption.
#[derive(Debug, Clone)]
pub struct FeatureVector {
    /// Normalized in-degree centrality of the user node in [0, 1].
    pub in_degree_centrality: f64,
    /// Normalized out-degree centrality of the user node in [0, 1].
    pub out_degree_centrality: f64,
    /// Betweenness centrality of the user node in [0, 1] (BFS-based, unweighted).
    pub betweenness_centrality: f64,
    /// Local clustering coefficient of the user node in [0, 1].
    pub clustering_coefficient: f64,
    /// Sum of all edge amounts involving the user (both in and out).
    pub total_tx_volume: f64,
    /// Average transaction amount across all user edges.
    pub avg_tx_amount: f64,
    /// Number of distinct accounts the user has transacted with.
    pub unique_counterparties: u32,
    /// Total number of individual transactions involving the user.
    pub tx_count: u32,
    /// Number of nodes in the k-hop ego subgraph.
    pub neighborhood_size: u32,
}

/// Integer-quantized feature vector for use inside ZKP arithmetic circuits.
///
/// Field elements in ZKP circuits are integers (field elements), so we scale
/// floating-point features to fixed-point integers by multiplying by
/// `SCALE_FACTOR` (10,000) and rounding.
#[derive(Debug, Clone)]
pub struct QuantizedFeatureVector {
    pub in_degree_centrality: u64,
    pub out_degree_centrality: u64,
    pub betweenness_centrality: u64,
    pub clustering_coefficient: u64,
    pub total_tx_volume: u64,
    pub avg_tx_amount: u64,
    pub unique_counterparties: u32,
    pub tx_count: u32,
    pub neighborhood_size: u32,
}

impl FeatureVector {
    /// Convert to integer representation for ZKP circuit consumption.
    ///
    /// Float fields are multiplied by `SCALE_FACTOR` (10,000) and rounded.
    /// Integer fields are passed through unchanged.
    pub fn quantize(&self) -> QuantizedFeatureVector {
        QuantizedFeatureVector {
            in_degree_centrality: (self.in_degree_centrality * SCALE_FACTOR).round() as u64,
            out_degree_centrality: (self.out_degree_centrality * SCALE_FACTOR).round() as u64,
            betweenness_centrality: (self.betweenness_centrality * SCALE_FACTOR).round() as u64,
            clustering_coefficient: (self.clustering_coefficient * SCALE_FACTOR).round() as u64,
            total_tx_volume: (self.total_tx_volume * SCALE_FACTOR).round() as u64,
            avg_tx_amount: (self.avg_tx_amount * SCALE_FACTOR).round() as u64,
            unique_counterparties: self.unique_counterparties,
            tx_count: self.tx_count,
            neighborhood_size: self.neighborhood_size,
        }
    }

    /// Return features as a fixed-size array for commitment schemes.
    ///
    /// Order: [in_deg, out_deg, betweenness, clustering, volume, avg_amount,
    ///         counterparties, tx_count, neighborhood_size]
    pub fn to_array(&self) -> [f64; 9] {
        [
            self.in_degree_centrality,
            self.out_degree_centrality,
            self.betweenness_centrality,
            self.clustering_coefficient,
            self.total_tx_volume,
            self.avg_tx_amount,
            self.unique_counterparties as f64,
            self.tx_count as f64,
            self.neighborhood_size as f64,
        ]
    }
}

impl QuantizedFeatureVector {
    /// Return quantized features as a fixed-size array of u64 for circuit input.
    pub fn to_array(&self) -> [u64; 9] {
        [
            self.in_degree_centrality,
            self.out_degree_centrality,
            self.betweenness_centrality,
            self.clustering_coefficient,
            self.total_tx_volume,
            self.avg_tx_amount,
            self.unique_counterparties as u64,
            self.tx_count as u64,
            self.neighborhood_size as u64,
        ]
    }
}

/// Extract all features for the focal user from their transaction graph.
///
/// Pipeline:
/// 1. Extract k-hop ego subgraph around the user (bounded by `config`)
/// 2. Compute centrality metrics on the subgraph
/// 3. Compute clustering coefficient on the subgraph
/// 4. Aggregate transaction volume statistics
/// 5. Return `FeatureVector`
pub fn extract_features(graph: &TransactionGraph, config: &NeighborhoodConfig) -> FeatureVector {
    // Step 1: Extract k-hop neighborhood
    let subgraph = neighborhood::extract_neighborhood(graph, config);
    let user = subgraph.user_node();
    let neighborhood_size = subgraph.node_count() as u32;

    // Step 2: Degree centrality
    let in_deg = centrality::in_degree_centrality(&subgraph, user);
    let out_deg = centrality::out_degree_centrality(&subgraph, user);

    // Step 3: Betweenness centrality (BFS-based, unweighted)
    let bc_map = centrality::betweenness_centrality(&subgraph);
    let betweenness = bc_map.get(&user).copied().unwrap_or(0.0);

    // Step 4: Clustering coefficient
    let cc = clustering::clustering_coefficient(&subgraph, user);

    // Step 5: Transaction volume statistics
    let inner = subgraph.inner();
    let mut total_volume = 0.0;
    let mut total_tx_count = 0u32;
    let mut counterparties = std::collections::HashSet::new();

    // Outgoing edges
    for edge in inner.edges_directed(user, Direction::Outgoing) {
        let data = edge.weight();
        total_volume += data.total_amount;
        total_tx_count += data.tx_count;
        counterparties.insert(edge.target());
    }

    // Incoming edges
    for edge in inner.edges_directed(user, Direction::Incoming) {
        let data = edge.weight();
        total_volume += data.total_amount;
        total_tx_count += data.tx_count;
        counterparties.insert(edge.source());
    }

    let unique_counterparties = counterparties.len() as u32;
    let avg_tx_amount = if total_tx_count > 0 {
        total_volume / total_tx_count as f64
    } else {
        0.0
    };

    FeatureVector {
        in_degree_centrality: in_deg,
        out_degree_centrality: out_deg,
        betweenness_centrality: betweenness,
        clustering_coefficient: cc,
        total_tx_volume: total_volume,
        avg_tx_amount,
        unique_counterparties,
        tx_count: total_tx_count,
        neighborhood_size,
    }
}

impl std::fmt::Display for FeatureVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Feature Vector:")?;
        writeln!(f, "  In-degree centrality:  {:.4}", self.in_degree_centrality)?;
        writeln!(f, "  Out-degree centrality: {:.4}", self.out_degree_centrality)?;
        writeln!(f, "  Betweenness centrality:{:.4}", self.betweenness_centrality)?;
        writeln!(f, "  Clustering coefficient:{:.4}", self.clustering_coefficient)?;
        writeln!(f, "  Total TX volume:       {:.2}", self.total_tx_volume)?;
        writeln!(f, "  Avg TX amount:         {:.2}", self.avg_tx_amount)?;
        writeln!(f, "  Unique counterparties: {}", self.unique_counterparties)?;
        writeln!(f, "  TX count:              {}", self.tx_count)?;
        writeln!(f, "  Neighborhood size:     {}", self.neighborhood_size)?;
        Ok(())
    }
}

impl std::fmt::Display for QuantizedFeatureVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Quantized Feature Vector (scale={})", SCALE_FACTOR)?;
        writeln!(f, "  In-degree centrality:  {}", self.in_degree_centrality)?;
        writeln!(f, "  Out-degree centrality: {}", self.out_degree_centrality)?;
        writeln!(f, "  Betweenness centrality:{}", self.betweenness_centrality)?;
        writeln!(f, "  Clustering coefficient:{}", self.clustering_coefficient)?;
        writeln!(f, "  Total TX volume:       {}", self.total_tx_volume)?;
        writeln!(f, "  Avg TX amount:         {}", self.avg_tx_amount)?;
        writeln!(f, "  Unique counterparties: {}", self.unique_counterparties)?;
        writeln!(f, "  TX count:              {}", self.tx_count)?;
        writeln!(f, "  Neighborhood size:     {}", self.neighborhood_size)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_features_basic() {
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "alice".into(), to: "carol".into(), amount: 200.0, timestamp: 2 },
            Transaction { from: "bob".into(), to: "alice".into(), amount: 50.0, timestamp: 3 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);
        let features = extract_features(&graph, &NeighborhoodConfig::default());

        assert_eq!(features.unique_counterparties, 2); // bob and carol
        assert_eq!(features.tx_count, 3);
        assert!((features.total_tx_volume - 350.0).abs() < 1e-9);
        assert!(features.out_degree_centrality > 0.0);
        assert!(features.in_degree_centrality > 0.0);
    }

    #[test]
    fn test_extract_features_empty_graph() {
        let graph = TransactionGraph::from_transactions("alice", &[]);
        let features = extract_features(&graph, &NeighborhoodConfig::default());

        assert_eq!(features.unique_counterparties, 0);
        assert_eq!(features.tx_count, 0);
        assert_eq!(features.total_tx_volume, 0.0);
        assert_eq!(features.in_degree_centrality, 0.0);
        assert_eq!(features.out_degree_centrality, 0.0);
        assert_eq!(features.clustering_coefficient, 0.0);
        assert_eq!(features.neighborhood_size, 1); // just alice
    }

    #[test]
    fn test_quantize() {
        let fv = FeatureVector {
            in_degree_centrality: 0.5,
            out_degree_centrality: 0.75,
            betweenness_centrality: 0.123456,
            clustering_coefficient: 1.0,
            total_tx_volume: 1234.56,
            avg_tx_amount: 100.0,
            unique_counterparties: 5,
            tx_count: 10,
            neighborhood_size: 8,
        };

        let qfv = fv.quantize();
        assert_eq!(qfv.in_degree_centrality, 5000);
        assert_eq!(qfv.out_degree_centrality, 7500);
        assert_eq!(qfv.betweenness_centrality, 1235); // rounded
        assert_eq!(qfv.clustering_coefficient, 10000);
        assert_eq!(qfv.total_tx_volume, 12345600);
        assert_eq!(qfv.avg_tx_amount, 1000000);
        assert_eq!(qfv.unique_counterparties, 5);
        assert_eq!(qfv.tx_count, 10);
        assert_eq!(qfv.neighborhood_size, 8);
    }

    #[test]
    fn test_to_array() {
        let fv = FeatureVector {
            in_degree_centrality: 0.5,
            out_degree_centrality: 0.75,
            betweenness_centrality: 0.1,
            clustering_coefficient: 1.0,
            total_tx_volume: 1000.0,
            avg_tx_amount: 100.0,
            unique_counterparties: 5,
            tx_count: 10,
            neighborhood_size: 8,
        };

        let arr = fv.to_array();
        assert_eq!(arr.len(), 9);
        assert_eq!(arr[0], 0.5);
        assert_eq!(arr[6], 5.0);
    }

    #[test]
    fn test_neighborhood_bounds_feature_computation() {
        // Chain: alice -> bob -> carol -> dave -> eve
        // With k=1, only alice and bob are in the subgraph
        let txs = vec![
            Transaction { from: "alice".into(), to: "bob".into(), amount: 100.0, timestamp: 1 },
            Transaction { from: "bob".into(), to: "carol".into(), amount: 80.0, timestamp: 2 },
            Transaction { from: "carol".into(), to: "dave".into(), amount: 60.0, timestamp: 3 },
            Transaction { from: "dave".into(), to: "eve".into(), amount: 40.0, timestamp: 4 },
        ];
        let graph = TransactionGraph::from_transactions("alice", &txs);

        let features_k1 = extract_features(
            &graph,
            &NeighborhoodConfig { hops: 1, max_neighbors_per_hop: None },
        );
        let features_k2 = extract_features(
            &graph,
            &NeighborhoodConfig { hops: 2, max_neighbors_per_hop: None },
        );

        assert_eq!(features_k1.neighborhood_size, 2); // alice, bob
        assert_eq!(features_k2.neighborhood_size, 3); // alice, bob, carol

        // User's direct features should be the same regardless of k
        // (alice -> bob is the only edge involving alice)
        assert_eq!(features_k1.unique_counterparties, features_k2.unique_counterparties);
        assert_eq!(features_k1.total_tx_volume, features_k2.total_tx_volume);
    }
}
