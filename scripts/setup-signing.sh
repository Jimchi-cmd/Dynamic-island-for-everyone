#!/usr/bin/env bash
# Make Islet's release keys on YOUR computer and hand them to GitHub.
#
#   scripts/setup-signing.sh            make the keys, then add them to the
#                                       repository's secrets with `gh` (if
#                                       it is installed and signed in)
#   scripts/setup-signing.sh --no-upload  only make the keys
#
# It makes two keys, in ~/islet-keys (never inside the repository):
#   - the phone app's signing key, so each new APK installs over the last
#     one (and Android knows it comes from the same developer);
#   - the update key, so Islet on a computer can update itself in place.
# Back the folder up (a password manager or an encrypted drive): if a key
# is lost, phones must uninstall and reinstall, and computers must
# download the next version by hand.
#
# Needs: keytool (from any Java 17+, e.g. Android Studio's) and Node.js.
set -euo pipefail
cd "$(dirname "$0")/.." 2>/dev/null || true

upload=1
[[ "${1:-}" == "--no-upload" ]] && upload=0
dir="${ISLET_KEYS_DIR:-$HOME/islet-keys}"
repo="${ISLET_REPO:-Jimchi-cmd/Dynamic-island-for-everyone}"

die() { echo "setup-signing: $*" >&2; exit 1; }
# 32 letters and digits (cut reads all its input, so nothing breaks the pipe).
password() { head -c 600 /dev/urandom | LC_ALL=C tr -dc 'A-Za-z0-9' | cut -c1-32; }

command -v keytool >/dev/null || die "keytool not found: install Java 17 or later (or Android Studio) and try again"
command -v npx >/dev/null || die "npx not found: install Node.js 20 or later and try again"

mkdir -p "$dir"
chmod 700 "$dir"

# 1. The phone app's signing key.
jks="$dir/islet-release.jks"
if [[ -e "$jks" ]]; then
  echo "Keeping the phone key already in $jks"
  store_pw=$(sed -n 's/^ANDROID_KEYSTORE_PASSWORD=//p' "$dir/passwords.txt")
else
  store_pw=$(password)
  keytool -genkeypair -keystore "$jks" -storetype PKCS12 -alias islet -keyalg RSA -keysize 4096 \
    -validity 10000 -dname "CN=Islet" -storepass "$store_pw" -keypass "$store_pw" >/dev/null
  echo "Made the phone key: $jks"
fi

# 2. The update key (Tauri's, Ed25519).
key="$dir/islet-updater.key"
if [[ -e "$key" ]]; then
  echo "Keeping the update key already in $key"
  upd_pw=$(sed -n 's/^TAURI_SIGNING_PRIVATE_KEY_PASSWORD=//p' "$dir/passwords.txt")
else
  upd_pw=$(password)
  # The Tauri version the project builds with (this file may be run on its own, too).
  cli=$(node -p "require('./package.json').devDependencies['@tauri-apps/cli'].replace(/^[^0-9]*/, '')" 2>/dev/null || echo 2)
  npx --yes "@tauri-apps/cli@$cli" signer generate --ci -w "$key" -p "$upd_pw" >/dev/null
  echo "Made the update key: $key"
fi

umask 077
cat > "$dir/passwords.txt" <<EOF
ANDROID_KEYSTORE_PASSWORD=$store_pw
ANDROID_KEY_ALIAS=islet
TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$upd_pw
EOF
chmod 600 "$dir"/*

b64() { base64 <"$1" | tr -d '\n'; }

if [[ $upload == 1 ]] && command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
  echo "Adding them to $repo (Settings → Secrets and variables → Actions)…"
  b64 "$jks" | gh secret set ANDROID_KEYSTORE_BASE64 --repo "$repo"
  printf '%s' "$store_pw" | gh secret set ANDROID_KEYSTORE_PASSWORD --repo "$repo"
  printf '%s' "islet" | gh secret set ANDROID_KEY_ALIAS --repo "$repo"
  printf '%s' "$store_pw" | gh secret set ANDROID_KEY_PASSWORD --repo "$repo"
  gh secret set TAURI_SIGNING_PRIVATE_KEY --repo "$repo" <"$key"
  printf '%s' "$upd_pw" | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo "$repo"
  gh variable set ISLET_UPDATER_PUBKEY --repo "$repo" --body "$(cat "$key.pub")"
  echo "Done. The next release is signed with these keys."
else
  # No gh: write what to paste, for the website.
  out="$dir/paste-into-github.txt"
  {
    echo "Paste each of these at https://github.com/$repo/settings/secrets/actions"
    echo "(Secrets: \"New repository secret\"; the last one under the Variables tab.)"
    echo
    echo "Secret ANDROID_KEYSTORE_BASE64:"; b64 "$jks"; echo; echo
    echo "Secret ANDROID_KEYSTORE_PASSWORD:"; echo "$store_pw"; echo
    echo "Secret ANDROID_KEY_ALIAS:"; echo "islet"; echo
    echo "Secret ANDROID_KEY_PASSWORD:"; echo "$store_pw"; echo
    echo "Secret TAURI_SIGNING_PRIVATE_KEY:"; cat "$key"; echo; echo
    echo "Secret TAURI_SIGNING_PRIVATE_KEY_PASSWORD:"; echo "$upd_pw"; echo
    echo "Variable ISLET_UPDATER_PUBKEY:"; cat "$key.pub"; echo
  } >"$out"
  chmod 600 "$out"
  echo "Wrote what to paste into GitHub: $out (delete it once pasted)."
fi
echo
echo "Back up $dir now: without it, phones must reinstall and computers must update by hand."
