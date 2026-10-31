$ErrorActionPreference = "Stop"
cargo fmt --all
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --all-targets
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo clippy --all-targets --all-features
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
