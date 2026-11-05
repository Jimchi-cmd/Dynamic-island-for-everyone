#!/usr/bin/env bash
# Load the GNOME Shell extension in a headless GNOME Shell (45+) and drive
# it with a virtual keyboard. A stand-in `islet` records what it's asked.
# Needs gnome-shell and dbus-run-session; run it as-is (it starts its own
# session and a private stand-in system bus).
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" != --inside ]]; then
  ROOT=$(mktemp -d)
  mkdir -p "$ROOT/home/bin" "$ROOT/run"
  chmod 700 "$ROOT/run"
  printf '#!/bin/sh\necho "$@" >> "%s/home/islet-calls.log"\n' "$ROOT" > "$ROOT/home/bin/islet"
  chmod +x "$ROOT/home/bin/islet"
  zip=$(scripts/gnome-extension.sh "$ROOT/dist")
  HOME="$ROOT/home" PATH="$ROOT/home/bin:$PATH" XDG_RUNTIME_DIR="$ROOT/run" ZIP="$zip" \
    dbus-run-session -- "$0" --inside
  exit $?
fi

UUID=islet@jimchi-cmd.github.io
EXT="$HOME/.local/share/gnome-shell/extensions/$UUID"
mkdir -p "$EXT"
(cd "$EXT" && unzip -q "$ZIP")
# gnome-shell wants logind on the system bus; a private bus stands in.
DBUS_SYSTEM_BUS_ADDRESS=$(dbus-daemon --session --print-address --fork)
export DBUS_SYSTEM_BUS_ADDRESS
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell enabled-extensions "['$UUID']"
gnome-shell --headless --wayland --virtual-monitor 1280x800 --unsafe-mode >"$HOME/shell.log" 2>&1 &
SHELL_PID=$!
trap 'kill $SHELL_PID 2>/dev/null || true' EXIT

call() { gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method "$@"; }
for _ in $(seq 60); do call org.gnome.Shell.Extensions.GetExtensionInfo "$UUID" >/dev/null 2>&1 && break; sleep 0.5; done
sleep 3

fail() { echo "FAIL: $*"; echo "--- shell log"; grep -iE "islet|JS ERROR" "$HOME/shell.log" || true; exit 1; }
info=$(call org.gnome.Shell.Extensions.GetExtensionInfo "$UUID")
grep -q "'state': <1.0>" <<<"$info" || fail "extension not enabled: $info"
echo "ok: extension enabled"

keys() {
  call org.gnome.Shell.Eval "const C = imports.gi.Clutter, G = imports.gi.GLib;
    const d = C.get_default_backend().get_default_seat().create_virtual_device(C.InputDeviceType.KEYBOARD_DEVICE);
    const k = (v, s) => d.notify_keyval(G.get_monotonic_time(), v, s);
    $1; 'ok'" >/dev/null
}
keys 'k(C.KEY_Caps_Lock, C.KeyState.PRESSED); k(C.KEY_Caps_Lock, C.KeyState.RELEASED)'
sleep 1.5
grep -qx 'caps-lock on' "$HOME/islet-calls.log" 2>/dev/null || fail "Caps Lock didn't reach islet"
echo "ok: Caps Lock → islet caps-lock on"

keys 'k(C.KEY_Control_L, C.KeyState.PRESSED); k(C.KEY_Alt_L, C.KeyState.PRESSED);
      k(C.KEY_i, C.KeyState.PRESSED); k(C.KEY_i, C.KeyState.RELEASED);
      k(C.KEY_Alt_L, C.KeyState.RELEASED); k(C.KEY_Control_L, C.KeyState.RELEASED)'
sleep 1.5
grep -qx 'toggle' "$HOME/islet-calls.log" || fail "the shortcut didn't run islet toggle"
echo "ok: Ctrl+Alt+I → islet toggle"

clock() {
  call org.gnome.Shell.Eval 'function f(a) { if (a.constructor && a.constructor.name === "DateMenuButton") return a;
    for (const c of a.get_children()) { const r = f(c); if (r) return r; } return null; }
    const b = f(global.stage); b ? String(b.get_parent().visible) : "missing"'
}
gset() { gsettings --schemadir "$EXT/schemas" set org.gnome.shell.extensions.islet "$@"; }
gset hide-clock true; sleep 1
clock | grep -q '"false"' || fail "hide-clock didn't hide the clock"
gset hide-clock false; sleep 1
clock | grep -q '"true"' || fail "the clock didn't come back"
echo "ok: hide-clock hides and restores the clock"

call org.gnome.Shell.Extensions.DisableExtension "$UUID" >/dev/null; sleep 1
info=$(call org.gnome.Shell.Extensions.GetExtensionInfo "$UUID")
grep -q "'error': <''>" <<<"$info" || fail "error on disable: $info"
echo "ok: disables cleanly"
