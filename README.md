# Privacy-Preserving Credit Scoring

**Decentralized, zero-knowledge credit evaluation using Nova recursive SNARKs**

A system where users prove their creditworthiness to banks *without revealing any personal financial data*. Users generate zero-knowledge proofs locally from their transaction graph; banks verify attestations on-chain and learn only whether the applicant exceeds a credit threshold, nothing more.

---

## The Problem

Traditional credit scoring requires users to surrender their complete financial history to centralized bureaus. This creates massive privacy risks, single points of failure, and excludes billions of people without formal banking relationships.

## Solution

Two parallel privacy-preserving flows built on **blockchain** and **zero-knowledge proofs**:

| Flow | Technology | Purpose | What's Revealed |
|------|-----------|---------|-----------------|
| **ZKP Attestation** (this repo) | Nova recursive SNARKs | Individual bank queries | Only: "score ≥ threshold" |
| HE Aggregation (future) | CKKS Homomorphic Encryption | AI model training on population statistics | Only: encrypted aggregate |

This repository implements the **ZKP Attestation flow**, the green path in the architecture diagram below.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        USER LOCAL PC                            │
│                                                                 │
│  Transaction    Feature         Privacy Module                  │
│    Graph    →  Extractor   →  ┌──────────────────┐              │
│  (private)    (native Rust)   │ Step 1: Commit   │              │
│                               │  Poseidon(graph)  │             │
│  Nothing leaves               │  Pedersen(feats)  │             │
│  the device as                │                   │             │
│  raw data.                    │ Step 2: Threshold │             │
│                               │  Open commitment  │             │
│                               │  Range checks     │             │
│                               │  Score ≥ 650?     │             │
│                               └────────┬─────────┘              │
│                                        │                        │
│                                  Nova IVC Fold                  │
│                                        │                        │
│                               Spartan Compression               │
│                                   (~few KB proof)               │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                      compressed proof + public inputs
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                      BLOCKCHAIN (EVM)                           │
│                                                                 │
│   ┌─────────────────┐      ┌──────────────────────┐            │
│   │ NovaVerifier.sol │ ──→ │ AttestationRegistry   │            │
│   │ (BN254 pairing) │      │                      │            │
│   └─────────────────┘      │ attestations[addr] = │            │
│                             │  {threshold, block,  │            │
│   On-chain: only            │   valid: true}       │            │
│   ciphertext blobs          └──────────┬───────────┘            │
│   and proofs. No raw data.             │                        │
└────────────────────────────────────────┼────────────────────────┘
                                         │
                                    query attestation
                                         │
                                         ▼
                               ┌─────────────────┐
                               │      BANK        │
                               │                  │
                               │ Learns ONLY:     │
                               │ "Score ≥ 650" ✓  │
                               │                  │
                               │ Never sees:      │
                               │ • Transaction    │
                               │   graph          │
                               │ • Raw features   │
                               │ • Exact score    │
                               └─────────────────┘
```

---

## Why Nova?

| Property | Nova | Groth16 | PLONK/Halo2 |
|----------|------|---------|-------------|
| Trusted setup | **None** ✓ | Required ✗ | Varies |
| Recursive composition | Native (folding) | Expensive | Expensive |
| Prover speed |  | Fast | Medium |
| Proof size | ~few KB (cFastompressed) | ~128 bytes | ~few KB |
| EVM verification | ✓ (BN254/Grumpkin) | ✓ | ✓ |

Nova uses a **folding scheme** — instead of proving each step independently and recursively verifying proofs inside proofs (expensive), it *folds* multiple computation steps into a single accumulated instance. 

---

## Two-Step Proving Strategy

Graph traversal is expensive inside arithmetic circuits. We split proving into two steps linked by cryptographic commitments:

### Step 1 — Graph Commitment
The circuit proves: *"I possess a transaction graph whose Poseidon hash is `H`, and I committed to features `F` derived from it."*

- **Inside circuit**: Poseidon hash of serialized graph, Pedersen commitment of feature vector
- **Cost**: ~10K–15K R1CS constraints (Poseidon is circuit-friendly)

### Step 2 — Threshold Proof
The circuit proves: *"The features I committed to are valid and produce a credit score above the threshold."*

- **Inside circuit**: Open Pedersen commitment, range-check each feature, compute weighted sum, assert ≥ threshold
- **Cost**: ~15K–25K R1CS constraints

### Why Two Steps?

Computing clustering coefficient *inside* a circuit would require encoding graph neighbor enumeration and triangle counting as arithmetic constraints — potentially 500K+ constraints. So compute features locally and then commit to the circuit, greatly reducing constraint size.

---

## Project Structure

```
PrivacyPreservingCredit/
├── README.md                           # You are here
├── Cargo.toml                          # Rust workspace root
│
├── crates/
│   ├── feature-extractor/              # Graph metrics (native Rust)
│   │   └── src/
│   │       ├── lib.rs                  # FeatureVector, extract_features()
│   │       ├── graph.rs                # TransactionGraph, adjacency list
│   │       ├── centrality.rs           # Degree & betweenness centrality
│   │       └── clustering.rs           # Clustering coefficient
│   │
│   ├── privacy-module/                 # ZKP proving pipeline
│   │   └── src/
│   │       ├── lib.rs                  # generate_proof() → CompressedProof
│   │       ├── circuits/
│   │       │   ├── step1_commit.rs     # Nova StepCircuit: graph + feature commit
│   │       │   └── step2_threshold.rs  # Nova StepCircuit: threshold assertion
│   │       ├── poseidon.rs             # Poseidon hash gadget
│   │       ├── pedersen.rs             # Pedersen commitment gadget
│   │       ├── range.rs                # Bit-decomposition range proof
│   │       └── compress.rs             # Spartan compression → EVM-ready bytes
│   │
│   └── zkp-client/                     # CLI binary
│       └── src/main.rs                 # prove / verify / submit commands
│
├── contracts/                          # Solidity (Foundry)
│   ├── src/
│   │   ├── NovaVerifier.sol            # BN254 pairing verification
│   │   └── AttestationRegistry.sol     # On-chain attestation storage
│   └── test/
│       └── Attestation.t.sol           # Contract tests
│
└── docs/
    └── architecture.md                 # Detailed design document
```

---

## Tech Stack

| Layer | Technology | Version |
|-------|-----------|---------|
| ZKP Engine | [`nova-snark`](https://github.com/microsoft/Nova) | latest (BN254/Grumpkin) |
| Circuit Gadgets | `bellpepper-core` | latest |
| Hash (in-circuit) | Poseidon via `neptune` | latest |
| Graph Library | `petgraph` | 0.6+ |
| Smart Contracts | Solidity + Foundry | 0.8.20+ |
| Curve Cycle | BN254 / Grumpkin | — |
| Compression | Spartan (built into Nova) | — |
| CLI | `clap` | 4.x |

---

## Getting Started

### Prerequisites

- **Rust** 1.75+ (edition 2021)
- **Foundry** (`forge`, `anvil`) for smart contract development
- ~8 GB RAM for proof generation

### Build

```bash
# Clone
git clone https://github.com/<your-org>/PrivacyPreservingCredit.git
cd PrivacyPreservingCredit

# Build all Rust crates
cargo build --release

# Build contracts
cd contracts && forge build
```

### Generate a Proof

```bash
# From a sample transaction graph
cargo run --release -p zkp-client -- prove \
  --graph data/sample_graph.json \
  --threshold 650 \
  --output proof.bin

# Verify locally (no blockchain needed)
cargo run --release -p zkp-client -- verify --proof proof.bin
```

### Deploy & Submit On-Chain

```bash
# Start local Anvil node
anvil &

# Deploy contracts
cd contracts && forge script script/Deploy.s.sol --broadcast --rpc-url http://localhost:8545

# Submit proof to chain
cargo run --release -p zkp-client -- submit \
  --proof proof.bin \
  --rpc http://localhost:8545 \
  --registry <REGISTRY_ADDRESS>
```

### Query Attestation (as a Bank)

```bash
# Read attestation for an address
cast call <REGISTRY_ADDRESS> \
  "getAttestation(address)" \
  <USER_ADDRESS> \
  --rpc-url http://localhost:8545
```

---

## How It Preserves Privacy

| What | Who sees it | Cryptographic guarantee |
|------|-------------|------------------------|
| Transaction graph | **Only the user** | Never leaves the device |
| Raw feature values | **Only the user** | Hidden inside Pedersen commitment + ZK proof |
| Exact credit score | **Only the user** | ZKP reveals only pass/fail against threshold |
| Proof on-chain | **Everyone** | Reveals nothing beyond "score ≥ T" (zero-knowledge) |
| Attestation | **Everyone** | Signed assertion: "address X passed threshold T at block N" |

---

## Security Model

- **No trusted setup**: Nova's folding scheme + Spartan compression require no ceremony. Any compromise of a trusted setup would allow forged proofs — we avoid this entirely.
- **Soundness**: A computationally bounded adversary cannot produce a valid proof for a score below threshold. Guaranteed by the hardness of the discrete log problem on BN254.
- **Zero-knowledge**: The proof reveals nothing beyond the proven statement. Even the verifier (blockchain + bank) learns only the threshold assertion.
- **Binding**: The Pedersen commitment binds the prover to specific feature values. They cannot prove a threshold with one set of features and claim another.

---

## License

MIT

---

## References

- [Nova: Recursive Zero-Knowledge Arguments from Folding Schemes](https://eprint.iacr.org/2021/370) — Kothapalli, Setty, Tzialla
- [microsoft/Nova](https://github.com/microsoft/Nova) — Reference implementation
- [Poseidon: A New Hash Function for Zero-Knowledge Proof Systems](https://eprint.iacr.org/2019/458)
- [EIP-197: Precompiled contracts for elliptic curve pairing operations](https://eips.ethereum.org/EIPS/eip-197)
