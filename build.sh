#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all
cargo test --all-targets
cargo clippy --all-targets --all-features
cargo build --release
