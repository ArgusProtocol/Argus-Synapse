# Argus Protocol

**Zero-Ops Agentic Gateway for GhostDAG**

![Argus Banner](https://img.shields.io/badge/Argus-Orchestration-blueviolet?style=for-the-badge&logo=rust)
![GhostDAG](https://img.shields.io/badge/PHANTOM-GhostDAG-blue?style=for-the-badge)
![Agentic](https://img.shields.io/badge/Agentic-Infrastructure-green?style=for-the-badge)

> **"From Tangled DAGs to Deterministic Streams."**  
> **Official Website:** [argus-protocol.xyz](https://argus-protocol.xyz)  
> **Repository:** [Argus Protocol / Argus-Synapse](https://github.com/ArgusProtocol/Argus-Synapse.git)

Argus is a **senior-grade orchestration layer** that sits on top of a GhostDAG / Kaspa node. It requires zero manual intervention — it monitors its own position in the k-cluster, recovers from blue-set divergence autonomously, linearizes the 3D block-DAG into a real-time edge stream for GNNs, and dynamically tunes the k-parameter using a live PPO reinforcement learning agent.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Frontend / GNN Client                        │
│       REST (JSON)  +  WebSocket (JSON stream)                    │
└────────────────────────┬────────────────────────────────────────┘
                         │  Zero-Ops API  (port 8080)
┌────────────────────────▼────────────────────────────────────────┐
│                  ARGUS ORCHESTRATION LAYER                       │
│                                                                  │
│  ┌──────────────────┐  ┌─────────────────────────────────────┐  │
│  │  Self-Healing    │  │     Linearization Engine            │  │
│  │  Agent (FSM)     │  │  GhostDAG 3D → flat edge stream    │  │
│  │  + Recovery Loop │  │  PARENT_OF / BLUE_PAST / RED_PAST  │  │
│  └────────┬─────────┘  └──────────────────┬──────────────────┘  │
│           │                               │                      │
│  ┌────────▼───────────────────────────────▼──────────────────┐  │
│  │          PPO k-Optimizer  (orphan rate → k suggestion)    │  │
│  └────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────┬───────────────────────────────┘
                                  │  JSON-RPC + WebSocket
┌─────────────────────────────────▼───────────────────────────────┐
│                  GhostDAG Node  (unmodified kaspad)              │
│   GHOSTDAG(G,k) consensus — blue_score, past(), anticone()       │
└─────────────────────────────────────────────────────────────────┘
```

### Crate Map (Core Stack)

| Crate | Role |
|---|---|
| [`argus-ghostdag`](./crates/argus-ghostdag) | Mathematical core. Implements bit-perfect $k$-cluster coloring. |
| [`argus-agent`](./crates/argus-agent) | Autonomous 4-state FSM for self-healing and recovery. |
| [`argus-linearizer`](./crates/argus-linearizer) | Flattening engine for GNN-ready JSON streams. |
| [`argus-pybridge`](./crates/argus-pybridge) | high-performance PyO3 bridge for Python orchestration. |
| [`argus_rl`](./python/argus_rl) | RL environment + PPO training for $k$-parameter tuning. |
| [`argus_gateway`](./python/argus_gateway) | Zero-Ops FastAPI gateway for developers. |

---

## Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Rust | ≥ 1.75 | Install via [rustup](https://rustup.rs) |
| Python | ≥ 3.10 | Required for RL and Gateway components |
| kaspad | latest | Node binary with `--utxoindex` |
| OpenSSL | any | For secure RPC communication |

---

## Quick Start

### 1. Build and Initialize (Rust CLI)
```bash
git clone https://github.com/ArgusProtocol/Argus-Synapse.git
cd Argus-Synapse

# Build the unified CLI binary
cargo build -p argus-cli --release
```

### 2. Activate the CLI Node
Once built, the `argus` binary is the primary entry point for the orchestration layer.

```bash
# Check connectivity to your local kaspad node
./target/release/argus check --endpoint http://localhost:9293

# Start the Argus Orchestration Layer
./target/release/argus start --rpc-port 9293 --ws-port 9292 --k 3
```

### 3. Setup Orchestrator (Python Gateway)
```bash
cd python
pip install -r requirements.txt
uvicorn argus_gateway.main:app --host 0.0.0.0 --port 8080
```

Argus is now accessible at **http://localhost:8080**.

---

## API Reference

### REST Endpoints

#### `GET /agent/health`
Returns the primary operational dashboard for the agent.

```json
{
  "status": "SYNCED",
  "current_k": 18,
  "orphan_rate": 0.042,
  "rl_confidence": 0.91,
  "local_blue_score": 81204931,
  "network_blue_score": 81204931,
  "version": "0.1.0"
}
```

#### `GET /dag/snapshot?n=100`
Returns the recent sub-graph formatted for **Graph Neural Networks (GNNs)**.

#### `POST /tx/submit-smart`
Guarantees the fastest inclusion by pointing new transactions to the 3-5 "Bluest" tips autonomously.

---

### WebSocket Streams

#### `WS /v1/stream/blocks`
Real-time linearized block stream with PHANTOM total ordering.

| Edge Type | Meaning | GNN Utility |
|---|---|---|
| `PARENT_OF` | Structural parent link | Topology analysis |
| `BLUE_PAST` | Block is in blue cluster | Trust signal |
| `RED_PAST` | Block is orphaned (red) | Latency signal |

---

## Under the Hood: The Math

### PHANTOM Total Ordering
Argus resolves parallel block conflicts by applying the PHANTOM sorting rule:
1. **Primary**: Sort by `BlueScore(B)` ascending.
2. **Secondary**: XOR tie-break: $Hash(B) \oplus Hash(SelectedParent(B))$

### RL k-Optimization
The RL agent (Stable-Baselines3 PPO) monitors network metrics and adjusts $k$ to maximize:
$$R = \omega_1(TPS) - \omega_2(OrphanRate) - \omega_3(SecurityMargin)$$

---

## Security and Reliability
- **Memoized Traversals**: `past(B)` and `anticone(B)` use internal caching. Amortized $O(1)$.
- **Thread Safety**: RwLock-protected DAG storage for parallel Read/Write throughput.
- **RPC Resiliency**: Automatic retries and robust chunked JSON parsing.

---

## Contributing
We welcome Protocol Engineers and Data Scientists. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

---

## License
Distributed under the MIT License. See [LICENSE](LICENSE).

---

## Contact
**Alex** - Senior Principal Protocol Engineer  
**Project**: Argus Orchestration Layer  
**Website**: [argus-protocol.xyz](https://argus-protocol.xyz)
