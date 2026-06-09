use std::fs;
use std::path::Path;

use feature_extractor::{
    extract_features, NeighborhoodConfig, TransactionGraph,
};

/// Helper to load a graph from the data/ directory.
fn load_graph(filename: &str) -> TransactionGraph {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(manifest_dir)
        .parent().unwrap()  // crates/
        .parent().unwrap()  // project root
        .join("data")
        .join(filename);
    let json = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    TransactionGraph::from_json(&json).unwrap()
}

// ─── Star Graph ──────────────────────────────────────────────────────────────

#[test]
fn test_star_graph_clustering_is_zero() {
    let graph = load_graph("star_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    // Star has no edges between leaves → clustering = 0
    assert!(
        features.clustering_coefficient.abs() < 1e-9,
        "Star graph clustering should be 0, got {}",
        features.clustering_coefficient
    );
}

#[test]
fn test_star_graph_degree() {
    let graph = load_graph("star_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    // alice has 5 outgoing edges, 0 incoming, 6 nodes total → out_deg = 5/5 = 1.0
    assert!(
        (features.out_degree_centrality - 1.0).abs() < 1e-9,
        "Star center out-degree centrality should be 1.0, got {}",
        features.out_degree_centrality
    );
    assert!(
        features.in_degree_centrality.abs() < 1e-9,
        "Star center in-degree centrality should be 0.0, got {}",
        features.in_degree_centrality
    );
}

#[test]
fn test_star_graph_counterparties() {
    let graph = load_graph("star_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    assert_eq!(features.unique_counterparties, 5);
    assert_eq!(features.tx_count, 5);
}

// ─── Ring Graph ──────────────────────────────────────────────────────────────

#[test]
fn test_ring_graph_clustering_is_zero() {
    let graph = load_graph("ring_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    // Pure cycle has no triangles
    assert!(
        features.clustering_coefficient.abs() < 1e-9,
        "Ring graph clustering should be 0, got {}",
        features.clustering_coefficient
    );
}

#[test]
fn test_ring_graph_degree() {
    let graph = load_graph("ring_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    // Ring: alice -> bob -> carol -> dave -> eve -> frank -> alice
    // With k=2, the neighborhood captures 5 of 6 nodes (not all 6).
    // In that subgraph alice has in_degree=1, out_degree=1.
    // Centrality = 1 / (neighborhood_size - 1)
    let expected = 1.0 / (features.neighborhood_size as f64 - 1.0);
    assert!(
        (features.in_degree_centrality - expected).abs() < 1e-9,
        "Ring in-degree centrality should be {:.4}, got {:.4}",
        expected, features.in_degree_centrality
    );
    assert!(
        (features.out_degree_centrality - expected).abs() < 1e-9,
        "Ring out-degree centrality should be {:.4}, got {:.4}",
        expected, features.out_degree_centrality
    );
}

// ─── Clique Graph ────────────────────────────────────────────────────────────

#[test]
fn test_clique_graph_clustering_is_one() {
    let graph = load_graph("clique_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    assert!(
        (features.clustering_coefficient - 1.0).abs() < 1e-9,
        "Clique clustering should be 1.0, got {}",
        features.clustering_coefficient
    );
}

#[test]
fn test_clique_graph_betweenness_near_zero() {
    let graph = load_graph("clique_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    // In a complete graph, all shortest paths are direct → betweenness ≈ 0
    assert!(
        features.betweenness_centrality.abs() < 1e-9,
        "Clique betweenness should be ~0, got {}",
        features.betweenness_centrality
    );
}

// ─── Realistic Graph ─────────────────────────────────────────────────────────

#[test]
fn test_realistic_graph_features_in_range() {
    let graph = load_graph("realistic_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    // All centrality metrics should be in [0, 1]
    assert!(features.in_degree_centrality >= 0.0 && features.in_degree_centrality <= 1.0);
    assert!(features.out_degree_centrality >= 0.0 && features.out_degree_centrality <= 1.0);
    assert!(features.betweenness_centrality >= 0.0 && features.betweenness_centrality <= 1.0);
    assert!(features.clustering_coefficient >= 0.0 && features.clustering_coefficient <= 1.0);

    // Volume should be positive
    assert!(features.total_tx_volume > 0.0);
    assert!(features.avg_tx_amount > 0.0);
    assert!(features.unique_counterparties > 0);
    assert!(features.tx_count > 0);
}

#[test]
fn test_realistic_graph_has_clustering() {
    let graph = load_graph("realistic_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    // alice's friends (bob, carol) transact with each other → some clustering
    assert!(
        features.clustering_coefficient > 0.0,
        "Realistic graph should have positive clustering, got {}",
        features.clustering_coefficient
    );
}

#[test]
fn test_realistic_graph_quantization() {
    let graph = load_graph("realistic_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());
    let quantized = features.quantize();

    // Verify quantization is consistent
    assert_eq!(
        quantized.in_degree_centrality,
        (features.in_degree_centrality * 10_000.0).round() as u64
    );
    assert_eq!(quantized.unique_counterparties, features.unique_counterparties);
    assert_eq!(quantized.tx_count, features.tx_count);
}

// ─── Large Graph — Neighborhood Bounding ─────────────────────────────────────

#[test]
fn test_large_graph_k1_neighborhood() {
    let graph = load_graph("large_graph.json");
    let features = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 1, max_neighbors_per_hop: None },
    );

    // k=1: alice + b1..b5 = 6 nodes
    assert_eq!(
        features.neighborhood_size, 6,
        "k=1 should have 6 nodes, got {}",
        features.neighborhood_size
    );
}

#[test]
fn test_large_graph_k2_neighborhood() {
    let graph = load_graph("large_graph.json");
    let features = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 2, max_neighbors_per_hop: None },
    );

    // k=2: alice + b1..b5 + c1..c8 = 14 nodes
    // But also b1->b2 and b3->b4 edges, so b2 and b4 are already at hop 1
    // c-nodes are at hop 2
    assert!(
        features.neighborhood_size > 6,
        "k=2 should have more than 6 nodes, got {}",
        features.neighborhood_size
    );
    assert!(
        features.neighborhood_size <= 14,
        "k=2 should have at most 14 nodes, got {}",
        features.neighborhood_size
    );
}

#[test]
fn test_large_graph_k3_neighborhood() {
    let graph = load_graph("large_graph.json");
    let features_k2 = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 2, max_neighbors_per_hop: None },
    );
    let features_k3 = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 3, max_neighbors_per_hop: None },
    );

    // k=3 should capture more nodes than k=2
    assert!(
        features_k3.neighborhood_size > features_k2.neighborhood_size,
        "k=3 ({}) should capture more nodes than k=2 ({})",
        features_k3.neighborhood_size,
        features_k2.neighborhood_size
    );
}

#[test]
fn test_large_graph_features_stable_with_k() {
    let graph = load_graph("large_graph.json");

    let features_k1 = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 1, max_neighbors_per_hop: None },
    );
    let features_k2 = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 2, max_neighbors_per_hop: None },
    );

    // Direct features should be the same (alice's immediate edges don't change)
    assert_eq!(features_k1.unique_counterparties, features_k2.unique_counterparties);
    assert!(
        (features_k1.total_tx_volume - features_k2.total_tx_volume).abs() < 1e-9,
        "Total volume should be the same regardless of k"
    );
}

#[test]
fn test_large_graph_max_neighbors_pruning() {
    let graph = load_graph("large_graph.json");
    let features_full = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 1, max_neighbors_per_hop: None },
    );
    let features_pruned = extract_features(
        &graph,
        &NeighborhoodConfig { hops: 1, max_neighbors_per_hop: Some(2) },
    );

    // Pruning to 2 neighbors per hop should reduce neighborhood size
    assert!(
        features_pruned.neighborhood_size < features_full.neighborhood_size,
        "Pruned ({}) should have fewer nodes than full ({})",
        features_pruned.neighborhood_size,
        features_full.neighborhood_size
    );
    // alice + top 2 neighbors = 3 nodes
    assert_eq!(features_pruned.neighborhood_size, 3);
}

// ─── Edge Cases ──────────────────────────────────────────────────────────────

#[test]
fn test_single_node_graph() {
    let graph = TransactionGraph::from_transactions("alice", &[]);
    let features = extract_features(&graph, &NeighborhoodConfig::default());

    assert_eq!(features.neighborhood_size, 1);
    assert_eq!(features.unique_counterparties, 0);
    assert_eq!(features.tx_count, 0);
    assert_eq!(features.total_tx_volume, 0.0);
    assert_eq!(features.in_degree_centrality, 0.0);
    assert_eq!(features.out_degree_centrality, 0.0);
    assert_eq!(features.clustering_coefficient, 0.0);
}

#[test]
fn test_display_feature_vector() {
    let graph = load_graph("realistic_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());
    let display = format!("{}", features);
    assert!(display.contains("Feature Vector:"));
    assert!(display.contains("In-degree centrality"));
}

#[test]
fn test_to_array_length() {
    let graph = load_graph("realistic_graph.json");
    let features = extract_features(&graph, &NeighborhoodConfig::default());
    assert_eq!(features.to_array().len(), 9);
    assert_eq!(features.quantize().to_array().len(), 9);
}
