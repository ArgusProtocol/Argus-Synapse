#!/usr/bin/env bash
# Argus Protocol — Local Setup & Run Script (Linux/macOS)
# Usage: chmod +x setup.sh && ./setup.sh
set -e

CYAN='\033[0;36m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'; RED='\033[0;31m'; NC='\033[0m'

echo -e "${CYAN}"
echo "╔══════════════════════════════════════════╗"
echo "║        Argus Protocol — Local Setup     ║"
echo "╚══════════════════════════════════════════╝"
echo -e "${NC}"

# ──────────────────────────────────────────────────────────────────────────────
# Step 1: Build Rust CLI
# ──────────────────────────────────────────────────────────────────────────────
echo -e "${YELLOW}[1/3] Building Rust CLI (argus-cli)...${NC}"
cargo build -p argus-cli --release
echo -e "${GREEN}      Binary: ./target/release/argus${NC}"

# ──────────────────────────────────────────────────────────────────────────────
# Step 2: Run Rust tests
# ──────────────────────────────────────────────────────────────────────────────
echo -e "\n${YELLOW}[2/3] Running workspace tests...${NC}"
cargo test --workspace --lib
echo -e "${GREEN}      All tests passed.${NC}"

# ──────────────────────────────────────────────────────────────────────────────
# Step 3: Install Python dependencies
# ──────────────────────────────────────────────────────────────────────────────
echo -e "\n${YELLOW}[3/3] Installing Python dependencies...${NC}"
python3 -m pip install -r python/requirements.txt
echo -e "${GREEN}      Python dependencies installed.${NC}"

# ──────────────────────────────────────────────────────────────────────────────
# Done — print run instructions
# ──────────────────────────────────────────────────────────────────────────────
echo -e "\n${CYAN}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Setup complete! To run Argus locally, open TWO terminals:  ║"
echo "╠══════════════════════════════════════════════════════════════╣"
echo "║                                                              ║"
echo "║  Terminal 1 — Start the Rust Orchestration Layer:           ║"
echo "║    ./target/release/argus start --k 3                       ║"
echo "║    (JSON-RPC on :9293, WebSocket stream on :9292)           ║"
echo "║                                                              ║"
echo "║  Terminal 2 — Start the Python Gateway:                     ║"
echo "║    cd python                                                 ║"
echo "║    uvicorn argus_gateway.main:app --port 8080               ║"
echo "║    (REST API + Swagger at http://localhost:8080/docs)        ║"
echo "║                                                              ║"
echo "║  Check health (after both are running):                     ║"
echo "║    ./target/release/argus check --endpoint 127.0.0.1:9293   ║"
echo "║                                                              ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo -e "${NC}"
