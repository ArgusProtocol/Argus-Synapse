# Argus Protocol Setup

$ErrorActionPreference = "Stop"

Write-Host "Starting Argus Setup..."
Write-Host "Step 1: Building Rust CLI"
cargo build -p argus-cli --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Rust build failed."
    exit 1
}
Write-Host "Binary built successfully."

Write-Host "Step 2: Running tests"
cargo test --workspace --lib
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Tests failed."
    exit 1
}
Write-Host "Tests passed."

Write-Host "Step 3: Installing Python dependencies"
python -m pip install -r python\requirements.txt
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: pip install failed"
    exit 1
}
Write-Host "Python dependencies installed."

Write-Host "Setup complete."
Write-Host "To run Argus locally, open TWO terminals:"
Write-Host "Terminal 1:"
Write-Host "  .\target\release\argus start --k 3"
Write-Host "Terminal 2:"
Write-Host "  cd python"
Write-Host "  uvicorn argus_gateway.main:app --port 8080"
