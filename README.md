# Argus Protocol

**The Zero-Ops Agentic Gateway for GhostDAG**

![Argus Banner](https://img.shields.io/badge/Argus-Orchestration-blueviolet?style=for-the-badge&logo=rust)
![GhostDAG](https://img.shields.io/badge/PHANTOM-GhostDAG-blue?style=for-the-badge)
![Agentic](https://img.shields.io/badge/Agentic-Infrastructure-green?style=for-the-badge)
![License](https://img.shields.io/badge/License-MIT-orange.svg)
![Version](https://img.shields.io/badge/Version-0.1.0-lightgrey.svg)
![FastAPI](https://img.shields.io/badge/FastAPI-005571?style=for-the-badge&logo=fastapi)
![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)

> **"From Tangled DAGs to Deterministic Streams."**  
> **Official Website:** [argus-protocol.xyz](https://argus-protocol.xyz)  
> **Repository:** [Argus Protocol / Argus-Synapse](https://github.com/ArgusProtocol/Argus-Synapse.git)

---

## 1. Executive Summary

Welcome to the official repository for the **Argus Protocol**. Argus is a senior-grade, high-performance orchestration layer engineered to sit atop a GhostDAG (Directed Acyclic Graph) consensus node, specifically targeting architectures similar to Kaspa. 

Traditional blockchain architectures (e.g., Bitcoin) rely on single-chain structures, causing severe bottlenecks in transaction throughput and leading to high latency and fees. GhostDAG solves this by allowing blocks to be created in parallel, forming a 3D block-DAG. While GhostDAG pushes the boundaries of scalability (reaching thousands of blocks per second), it introduces immense structural complexity. The graph theory required to traverse, linearize, and understand parallel blocks in real-time is computationally demanding and difficult to integrate with traditional downstream software, particularly Machine Learning models.

**Argus fundamentally bridges that gap.**

Argus operates as an autonomous, self-healing agent. It fundamentally requires **zero manual intervention**. Once deployed, the Argus Orchestration Layer automatically:
1. **Connects** to the underlying GhostDAG node via JSON-RPC.
2. **Monitors** its topological position within the block-DAG's $k$-cluster (the "Blue Set").
3. **Detects** network partitions, malicious forks, and topological drift.
4. **Heals** itself by executing a state machine recovery loop to request missing historical blocks and re-color the local DAG representation.
5. **Linearizes** the complex 3D non-deterministic block graph into a deterministic, real-time 1D data stream.
6. **Optimizes** its own parameters (specifically the $k$-parameter) dynamically using a real-time Proximal Policy Optimization (PPO) Reinforcement Learning Agent.

The output is pristine, flattened JSON streams completely optimized for ingestion by **Graph Neural Networks (GNNs)**, high-frequency trading bots, and enterprise API consumers, abstracting away the mathematical complexity of PHANTOM ordering and $k$-cluster coloring.

---

## 2. Macro System Architecture

The Argus Protocol is conceptually split into two hemispheres:
1. **The Rust Engine (Orchestration & Linearization):** Handles high-computation graph algorithms, bitwise mathematics, memory-safe data structures, and raw socket connectivity.
2. **The Python Gateway (Zero-Ops API & RL):** Handles REST interfaces, WebSocket routing, API abstractions, and Reinforcement Learning matrix operations using PyTorch.

```mermaid
graph TD
    classDef rust fill:#dea584,stroke:#333,stroke-width:2px;
    classDef py fill:#3572A5,stroke:#333,stroke-width:2px;
    classDef node fill:#444,stroke:#333,stroke-width:2px,color:#fff;

    GNN[Frontend / GNN / AI Client]

    subgraph "Python Zero-Ops Gateway"
    API[FastAPI Router (Port 8080)]:::py
    RL[PPO RL Agent (k-Tuning)]:::py
    end

    subgraph "Argus Orchestration Layer (Rust)"
    LIN[Linearizer WebSocket + RPC (Port 9292/9293)]:::rust
    AGT[Autonomous Agent FSM (Recovery Loop)]:::rust
    GDAG[GhostDAG Store & Colorizer]:::rust
    end

    NODE((GhostDAG Node / kaspad)):::node

    GNN <==>|REST JSON + WebSocket Streams| API
    API <==>|TCP JSON-RPC| LIN
    RL  -.->|Recommends k-updates| LIN
    LIN <--> AGT
    AGT <--> GDAG
    AGT <==>|Raw TCP / P2P| NODE
```

### 2.1 Component Breakdown (The Core Stack)

Argus utilizes a multi-crate workspace architecture to enforce strict separation of concerns, parallel compilation, and mathematical isolation.

| Component / Crate | Language | Role / Responsibility |
|---|---|---|
| [`argus-ghostdag`](./crates/argus-ghostdag) | Rust | The absolute mathematical core. In-memory `DagStore` using Kahn's topological sort. Implements the PHANTOM §3 block coloring and total ordering algorithms. |
| [`argus-agent`](./crates/argus-agent) | Rust | The brain of the orchestration layer. A 4-state Finite State Machine (SYNCED, DRIFTING, RECOVERING, PARTITIONED) responsible for monitoring divergence using Lowest Common Ancestor (LCA) algorithms. |
| [`argus-linearizer`](./crates/argus-linearizer) | Rust | The flattening engine. Exposes a TCP JSON-RPC server for the Python Gateway and broadcasts 1D linearized `StreamFrames` over native WebSockets to connected client sinks. |
| [`argus-pybridge`](./crates/argus-pybridge) | Rust | PyO3 C-bindings. Exposes highly optimized Rust DAG methods natively to Python 3 instances bypassing socket overhead where absolute latency is critical. |
| [`argus_rl`](./python/argus_rl) | Python | Stable-Baselines3 integration. Simulates sub-graph topology to train a Proximal Policy Optimization (PPO) agent to balance network throughput against partition security automatically dynamically adjusting $k$. |
| [`argus_gateway`](./python/argus_gateway) | Python | The developer-facing membrane. An asynchronous FastAPI server providing REST endpoints, automatic dependency injection, CORS settings, OpenAPI schema generation, and "Smart Submit" routing. |

---

## 3. The Mathematical Foundations

Argus is not a simple proxy. It locally models, evaluates, and colors a partial subset of the network Directed Acyclic Graph. To understand Argus, one must understand the mathematical primitives it implements.

### 3.1 Directed Acyclic Graphs (DAGs) in Blockchains

A blockchain is a linked list: $B_0 \leftarrow B_1 \leftarrow B_2$. If two miners mine a block simultaneously, a fork occurs, and network consensus must discard one (an "orphan" block). This structural limit caps throughput drastically.

A DAG (specifically replacing the blockchain) allows a block to point to *multiple* parents:
$Parents(B) = \{P_1, P_2, \dots, P_n\}$

If two miners mine simultaneously, the next block simply references *both* of them. No work is wasted. However, this creates a partial ordering. Without a single chain, how do we establish a definitive chronological history of transactions to prevent double-spending?

### 3.2 The GhostDAG Protocol & $K$-Cluster Algorithm

To solve partial ordering, we use the GhostDAG algorithm utilizing the PHANTOM specification. The goal is to identify the "honest" core of the graph (the **Blue Cluster**) and penalize delayed or malicious blocks (the **Red Set**).

The parameter $k$ represents the maximum expected network delay. If block creation is very fast, $k$ must be high to tolerate the high number of naturally occurring parallel, unconnected blocks ("anticone").

**The Core Operations (Memoized in `argus-ghostdag`):**
- $Past(B)$: The set of all blocks reachable by traversing parent links backwards from $B$.
- $Future(B)$: The set of all blocks that point to $B$ directly or indirectly.
- $Anticone(B)$: The set of blocks that are neither in $Past(B)$ nor $Future(B)$. These are parallel blocks created roughly at the same time as $B$.

**The Coloring Rule:**
For any sub-DAG $Past(B)$, Argus iteratively colors blocks. A block $h \in Past(B)$ is colored **Blue** if it maintains the rule that no blue block has an anticone containing more than $k$ other blue blocks.
$$ |Anticone(h) \cap BlueSet(B)| \le k $$

If the inclusion of $h$ violates the $k$-limit, $h$ is colored **Red** (excluded from the primary honest cluster). 

### 3.3 PHANTOM Total Ordering

Once all blocks are colored, Argus must linearize them. For downstream GNNs and traditional REST clients, 3D data is useless. Argus guarantees deterministic 1D execution ordering.

1. **Calculate Blue Score**: Each block receives a $BlueScore(B)$ equal to the total number of Blue blocks in its $Past(B)$.
2. **Sort Primary**: Sort the entire DAG ascending by $BlueScore$.
3. **Sort Secondary (Tie-Break)**: If two blocks have identical $BlueScore$s (they are siblings in parallel), break the tie using a cryptographically secure XOR distance metric relative to the structural selected parent:
   $$ TieBreak(B) = Hash(B) \oplus Hash(SelectedParent(B)) $$

This results in a perfectly flat, deterministically ordered list of blocks. `argus-linearizer` pushes these over WebSockets as `StreamFrames`.

---

## 4. The Self-Healing Autonomous Agent

Argus features a robotic state machine running on a perpetual asynchronous Tokio event loop. It maintains its own internal representation of the network state.

### 4.1 The Finite State Machine (FSM)
The agent transitions across four states:

1. **SYNCED (Green)**: The local DAG tip's $SelectedParent$ matches the global network API's $SelectedParent$. No action required.
2. **DRIFTING (Yellow)**: The agent detected that the global network's $BlueScore$ is advancing faster than the local state. The agent begins querying the underlying kaspad node for missing $Future()$ hashes.
3. **RECOVERING (Orange)**: A topological fork was detected. Utilizing the **Greedy Lowest Common Ancestor (LCA)** algorithm, Argus walks backwards from the local tip and network tip until it finds the structural intersection point (LCA). It then requests all blocks downstream of the LCA, re-colors the entire DAG incrementally, and re-broadcasts corrected frames.
4. **PARTITIONED (Red)**: The LCA search depth exceeded safety limits (e.g., > 10,000 blocks divergence) or the TCP heartbeat failed continuously. The agent pauses orchestration, throttles REST API requests with 503s, and initiates exponential backoff reconnects to prevent downstream data corruption.

### 4.2 Divergence Detection (LCA)
Traditional chains use simple block height. GhostDAG divergence requires identifying the LCA in a multi-parent topology. Argus uses a greedy path intersection algorithm defined in `argus-agent/src/lca.rs`, efficiently traversing the maximal blue-work path to locate partition roots in $O(log N)$ amputated time.

---

## 5. Machine Learning Integration

Argus is uniquely designed for AI/ML consumers.

### 5.1 GNN Edge Streaming
Graph Neural Networks (GNNs) require node/edge structures. `argus-linearizer` explicitly tags WebSocket frames with edge semantics:
- `PARENT_OF`: Structural topology. Useful for structural anomaly detection.
- `BLUE_PAST`: Indication of protocol-level trust.
- `RED_PAST`: Indication of high network latency or potential malicious isolation.

### 5.2 Reinforcement Learning: K-Optimizer
The $k$-parameter in GhostDAG dictates how many orphan blocks are structurally tolerated before being completely isolated into the Red Set.
- If $k$ is too low: Honest miners are unfairly orphaned (Red Set inflation).
- If $k$ is too high: Security degrades, and the system becomes vulnerable to 51% attacks from parallel chains.

`argus_rl` utilizes a custom Gymnasium environment attached to a PyTorch **Proximal Policy Optimization (PPO)** model. The agent observes the current network `TPS` and `orphan_rate` to output continuous step adjustments to $k$, dynamically hot-swapping the $k$-parameter inside the Rust core without dropping socket connections.

---

## 6. Comprehensive Installation & Setup Guide

Argus requires setting up the Rust binaries first, followed by the Python Gateway. 

### 6.1 System Requirements

| Platform | Minimum RAM | Recommended CPUs | Compilers / Linkers Needed |
|---|---|---|---|
| Windows 10/11 | 8 GB | 4+ Cores | **MSVC Build Tools (Required)** |
| Ubuntu/Debian | 4 GB | 2+ Cores | `build-essential`, `gcc` |
| macOS (M1/M2) | 8 GB | Apple Silicon | Xcode Command Line Tools |

**Critical Windows Prerequisite:**
Rust on Windows explicitly requires the Microsoft C++ linker (`link.exe`). Before proceeding on Windows, you **must**:
1. Download the [Visual Studio Build Tools](https://aka.ms/vs/17/release/vs_buildtools.exe).
2. Run the installer and check the **"Desktop development with C++"** payload.
3. Restart your terminal.

---

### 6.2 Step-by-Step Build Instructions

We have provided automated scripts for a frictionless start.

**For Windows (PowerShell):**
```powershell
git clone https://github.com/ArgusProtocol/Argus-Synapse.git
cd Argus-Synapse

# The script compiles the CLI, runs tests, and installs Python deps
.\setup.ps1
```

**For Linux / macOS:**
```bash
git clone https://github.com/ArgusProtocol/Argus-Synapse.git
cd Argus-Synapse

# The script compiles the CLI, runs tests, and installs Python deps
chmod +x setup.sh
./setup.sh
```

---

## 7. Operations & Usage

Argus is run as a tandem architecture. You must run both the Rust core engine and the Python API Gateway.

### Step 1: Start the Rust Orchestrator
The Rust agent holds the DAG in memory, connects to the P2P node, and hosts the high-speed TCP JSON-RPC interface.

```bash
# Terminal 1
./target/release/argus start --rpc-port 9293 --ws-port 9292 --k 3
```

### Step 2: Start the Python Gateway
The Python gateway exposes the Zero-Ops REST API for external developers and web interfaces. It abstracts the TCP communication into simple HTTP endpoints.

```bash
# Terminal 2 - Note: Requires Python 3.10+
cd python
# Start the asynchronous ASGI Uvicorn server
uvicorn argus_gateway.main:app --host 0.0.0.0 --port 8080
```

### Step 3: Verify Health
You can utilize the built-in CLI connectivity checker to verify that the Rust core is operating correctly.

```bash
# Terminal 3
./target/release/argus check --endpoint 127.0.0.1:9293
```
*Expected Output:*
```text
Checking Argus connectivity at 127.0.0.1:9293 (TCP JSON-RPC)...
Argus Gateway: [OK]
Agent State:   [SYNCED]
Current K:     [3]
Total Blocks:  [1]
```

---

## 8. Complete API Documentation

Once the Python Gateway is active, interactive Swagger / OpenAPI documentation is automatically available at:
👉 **[http://localhost:8080/docs](http://localhost:8080/docs)**

### 8.1 REST Endpoints

#### `GET /agent/health`
Returns the dashboard telemetry for the orchestration layer.

*Response:*
```json
{
  "status": "SYNCED",
  "current_k": 3,
  "orphan_rate": 0.001,
  "rl_confidence": 0.98,
  "local_blue_score": 105,
  "network_blue_score": 105,
  "version": "0.1.0"
}
```

#### `GET /dag/snapshot?n=100`
Returns the most recent `n` blocks, fully linearized, colored, and sorted via PHANTOM total ordering. This is the primary consumption endpoint for stateless clients.

#### `POST /tx/submit-smart`

---

## Under the Hood: The Math

### PHANTOM Total Ordering
Argus resolves parallel block conflicts by applying the PHANTOM sorting rule:
1. **Primary**: Sort by `BlueScore(B)` ascending.
2. **Secondary**: XOR tie-break: $Hash(B) \oplus Hash(SelectedParent(B))$.

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
**Rick** - Senior Principal Protocol Engineer  
**Project**: Argus Orchestration Layer  
**Website**: [argus-protocol.xyz](https://argus-protocol.xyz)
