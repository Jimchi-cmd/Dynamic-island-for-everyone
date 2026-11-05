#!/usr/bin/env bash
# Package the GNOME Shell extension as a zip that
# `gnome-extensions install --force <zip>` accepts.
# Usage: scripts/gnome-extension.sh [out-dir]   (default: dist-gnome)
set -euo pipefail
cd "$(dirname "$0")/.."
UUID=islet@jimchi-cmd.github.io
OUT=${1:-dist-gnome}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cp -r "gnome-extension/$UUID/." "$WORK/"
cp LICENSE "$WORK/LICENSE"
glib-compile-schemas --strict "$WORK/schemas"
mkdir -p "$OUT"
rm -f "$OUT/$UUID.shell-extension.zip"
(cd "$WORK" && zip -qr - .) > "$OUT/$UUID.shell-extension.zip"
echo "$OUT/$UUID.shell-extension.zip"
