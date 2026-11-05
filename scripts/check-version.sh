#!/usr/bin/env bash
# Fail unless the tag (vX.Y.Z), Cargo.toml, package.json and
# src-tauri/tauri.conf.json all carry the same version.
#   scripts/check-version.sh v0.1.0
set -euo pipefail
cd "$(dirname "$0")/.."
tag="${1:?usage: check-version.sh vX.Y.Z}"
want="${tag#v}"
cargo=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
npm=$(node -p "require('./package.json').version")
tauri=$(node -p "require('./src-tauri/tauri.conf.json').version")
ok=1
for pair in "Cargo.toml:$cargo" "package.json:$npm" "tauri.conf.json:$tauri"; do
  if [[ "${pair#*:}" != "$want" ]]; then
    echo "::error::${pair%%:*} has version ${pair#*:}, but the tag is $tag" >&2
    ok=0
  fi
done
[[ $ok == 1 ]] && echo "version $want matches"
[[ $ok == 1 ]]
