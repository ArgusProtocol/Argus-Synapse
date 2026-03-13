# Argus Protocol — Local Setup & Run Script (Windows PowerShell)
# Usage: .\setup.ps1

$ErrorActionPreference = "Stop"

Write-Host "`n╔══════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║        Argus Protocol — Local Setup     ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════╝`n" -ForegroundColor Cyan

# ──────────────────────────────────────────────────────────────────────────────
# Step 1: Build Rust CLI
# ──────────────────────────────────────────────────────────────────────────────
Write-Host "[1/3] Building Rust CLI (argus-cli)..." -ForegroundColor Yellow
cargo build -p argus-cli --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Rust build failed. Make sure Rust >= 1.75 is installed (https://rustup.rs)." -ForegroundColor Red
    exit 1
}
Write-Host "      Binary: .\target\release\argus.exe" -ForegroundColor Green

# ──────────────────────────────────────────────────────────────────────────────
# Step 2: Run Rust tests
# ──────────────────────────────────────────────────────────────────────────────
Write-Host "`n[2/3] Running workspace tests..." -ForegroundColor Yellow
cargo test --workspace --lib
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Tests failed." -ForegroundColor Red
    exit 1
}
Write-Host "      All tests passed." -ForegroundColor Green

# ──────────────────────────────────────────────────────────────────────────────
# Step 3: Install Python dependencies
# ──────────────────────────────────────────────────────────────────────────────
Write-Host "`n[3/3] Installing Python dependencies..." -ForegroundColor Yellow
python -m pip install -r python\requirements.txt
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: pip install failed. Make sure Python >= 3.10 is on your PATH." -ForegroundColor Red
    exit 1
}
Write-Host "      Python dependencies installed." -ForegroundColor Green

# ──────────────────────────────────────────────────────────────────────────────
# Done — print run instructions
# ──────────────────────────────────────────────────────────────────────────────
Write-Host "`n╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  Setup complete! To run Argus locally, open TWO terminals:  ║" -ForegroundColor Cyan
Write-Host "╠══════════════════════════════════════════════════════════════╣" -ForegroundColor Cyan
Write-Host "║                                                              ║" -ForegroundColor Cyan
Write-Host "║  Terminal 1 — Start the Rust Orchestration Layer:           ║" -ForegroundColor Cyan
Write-Host "║    .\target\release\argus start --k 3                       ║" -ForegroundColor Cyan
Write-Host "║    (JSON-RPC on :9293, WebSocket stream on :9292)           ║" -ForegroundColor Cyan
Write-Host "║                                                              ║" -ForegroundColor Cyan
Write-Host "║  Terminal 2 — Start the Python Gateway:                     ║" -ForegroundColor Cyan
Write-Host "║    cd python                                                 ║" -ForegroundColor Cyan
Write-Host "║    uvicorn argus_gateway.main:app --port 8080               ║" -ForegroundColor Cyan
Write-Host "║    (REST API + Swagger docs at http://localhost:8080/docs)   ║" -ForegroundColor Cyan
Write-Host "║                                                              ║" -ForegroundColor Cyan
Write-Host "║  Check health (after both are running):                     ║" -ForegroundColor Cyan
Write-Host "║    .\target\release\argus check --endpoint 127.0.0.1:9293   ║" -ForegroundColor Cyan
Write-Host "║                                                              ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
