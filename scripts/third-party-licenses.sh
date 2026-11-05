#!/usr/bin/env bash
# Regenerate THIRD_PARTY_LICENSES.md (needs `cargo install cargo-about`).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo about generate --workspace --fail about.hbs -o THIRD_PARTY_LICENSES.md
echo "wrote THIRD_PARTY_LICENSES.md ($(wc -l < THIRD_PARTY_LICENSES.md) lines)"
