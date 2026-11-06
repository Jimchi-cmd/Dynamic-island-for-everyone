#!/usr/bin/env bash
# Copy the built island (and, on macOS, its MediaRemote helper) to where the
# Tauri bundler picks them up as the Settings app's sidecar.
#   scripts/stage-island.sh [target-triple]   (default: the host triple)
# Expects `cargo build --release -p islet [-p islet-mediaremote]` first; for
# universal-apple-darwin, both Apple targets built with --target.
set -euo pipefail
cd "$(dirname "$0")/.."
triple="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
mkdir -p src-tauri/binaries
exe=""
[[ "$triple" == *windows* ]] && exe=".exe"
out="src-tauri/binaries/islet-$triple$exe"
if [[ "$triple" == universal-apple-darwin ]]; then
  # Tauri builds each architecture separately (needing a sidecar per arch),
  # then bundles the universal one.
  for arch in aarch64 x86_64; do
    cp "target/$arch-apple-darwin/release/islet" "src-tauri/binaries/islet-$arch-apple-darwin"
  done
  lipo -create -output "$out" target/{aarch64,x86_64}-apple-darwin/release/islet
  lipo -create -output src-tauri/binaries/libislet_mediaremote.dylib \
    target/{aarch64,x86_64}-apple-darwin/release/libislet_mediaremote.dylib
else
  dir=target/release
  [[ -d "target/$triple/release" ]] && dir="target/$triple/release"
  cp "$dir/islet$exe" "$out"
  if [[ "$triple" == *apple-darwin ]]; then
    cp "$dir/libislet_mediaremote.dylib" src-tauri/binaries/
  fi
fi
# A real binary, not a placeholder.
[[ $(wc -c < "$out") -gt 100000 ]] || { echo "stage-island: $out looks wrong" >&2; exit 1; }
echo "staged $out"
