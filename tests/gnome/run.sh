#!/usr/bin/env bash
# End-to-end test of the GNOME helper and the native host in a private headless GNOME Shell
# (own HOME, D-Bus session and Wayland display; the desktop you are using is not touched).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ "${1:-}" != "--inner" ]]; then
  T="$(mktemp -d)"
  # The document portal mounts a FUSE file system in the runtime dir; unmount it before cleaning up.
  trap 'fusermount3 -u "$T/run/doc" 2>/dev/null || fusermount -u "$T/run/doc" 2>/dev/null || true; rm -rf "$T"' EXIT
  mkdir -p "$T/share/gnome-shell/extensions" "$T/run" && chmod 700 "$T/run"
  cp -r "$ROOT/gnome-extension/pip-anywhere@pipanywhere.github.io" "$T/share/gnome-shell/extensions/"
  cp -r "$ROOT/tests/gnome/pip-anywhere-test@pipanywhere.github.io" "$T/share/gnome-shell/extensions/"
  HOME="$T" XDG_DATA_HOME="$T/share" XDG_CONFIG_HOME="$T/config" XDG_CACHE_HOME="$T/cache" \
    XDG_RUNTIME_DIR="$T/run" GSETTINGS_BACKEND=keyfile T="$T" \
    dbus-run-session -- "$0" --inner 2>"$T/session.log" || { tail -20 "$T/session.log"; exit 1; }
  exit
fi

fail() { echo "FAIL: $*"; echo "--- shell log"; tail -30 "$T/shell.log"; exit 1; }
host() { python3 - "$@" <<'PY'
import io, json, subprocess, sys
sys.path.insert(0, sys.argv[1])
import host
buf = io.BytesIO(); host.write_message(buf, json.loads(sys.argv[2]))
out = subprocess.run([sys.executable, sys.argv[1] + "/host.py"], input=buf.getvalue(), capture_output=True).stdout
print(json.dumps(host.read_message(io.BytesIO(out))))
PY
}
probe() { gdbus call --session --dest com.pipanywhere.Test --object-path /com/pipanywhere/Test \
  --method com.pipanywhere.Test.Probe | python3 -c "import ast,sys; print(ast.literal_eval(sys.stdin.read())[0])"; }
field() { python3 -c "import json,sys; d=json.loads(sys.argv[1]); print(eval(sys.argv[2], {}, {'d': d}))" "$1" "$2"; }

unset WAYLAND_DISPLAY DISPLAY
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell enabled-extensions \
  "['pip-anywhere@pipanywhere.github.io', 'pip-anywhere-test@pipanywhere.github.io']"

gnome-shell --headless --wayland --no-x11 --virtual-monitor 1280x800 --wayland-display pipa-test \
  >"$T/shell.log" 2>&1 &
SHELL_PID=$!
trap 'kill $SHELL_PID 2>/dev/null || true; kill ${APP_PID:-0} 2>/dev/null || true' EXIT

for _ in $(seq 60); do
  gdbus introspect --session --dest com.pipanywhere.Shell --object-path /com/pipanywhere/Shell >/dev/null 2>&1 && break
  sleep 0.5
done
gdbus introspect --session --dest com.pipanywhere.Shell --object-path /com/pipanywhere/Shell >/dev/null 2>&1 \
  || fail "helper extension did not start"
echo "ok   helper extension exported com.pipanywhere.Shell"

# A normal application window: modal dialogs (e.g. zenity) cannot be minimised.
WAYLAND_DISPLAY=pipa-test GDK_BACKEND=wayland gnome-calculator >/dev/null 2>&1 &
APP_PID=$!
for _ in $(seq 40); do
  [[ "$(host "$ROOT/native" '{"cmd":"state"}')" == *'"ok": true'* ]] && break
  sleep 0.5
done
state="$(host "$ROOT/native" '{"cmd":"state"}')"
[[ "$(field "$state" "d['ok']")" == "True" ]] || fail "no focused window: $state"
echo "ok   state: focused window '$(field "$state" "d['window']['title']")'"

res="$(host "$ROOT/native" '{"cmd":"set_above","above":true}')"
[[ "$(field "$res" "d['window']['above']")" == "True" ]] || fail "set_above reply: $res"
[[ "$(field "$(probe)" "d['above']")" == "True" ]] || fail "window is not above: $(probe)"
echo "ok   set_above(true): compositor reports the window above others"

res="$(host "$ROOT/native" '{"cmd":"set_opacity","opacity":0.5}')"
[[ "$(field "$res" "d['window']['opacity']")" == "0.5" ]] || fail "set_opacity reply: $res"
[[ "$(field "$(probe)" "d['actorOpacity']")" == "128" ]] || fail "actor opacity: $(probe)"
echo "ok   set_opacity(0.5): compositor actor opacity is 128/255"

testcall() { gdbus call --session --dest com.pipanywhere.Test --object-path /com/pipanywhere/Test \
  --method "com.pipanywhere.Test.$1" "${@:2}"; }
opacity_now() { field "$(probe)" "d['actorOpacity']"; }

# GNOME Shell starts in the overview, where window animations are skipped; leave it so the real
# 400 ms minimise/unminimise animations run (they reset the opacity to 255 when they finish).
testcall HideOverview >/dev/null
sleep 1
testcall Minimize >/dev/null
sleep 0.1
[[ "$(testcall Animating)" == "(true,)" ]] || fail "GNOME did not animate the minimise; the test would prove nothing"
sleep 1
testcall Unminimize >/dev/null
sleep 0.1
[[ "$(testcall Animating)" == "(true,)" ]] || fail "GNOME did not animate the unminimise"
sleep 1.2
[[ "$(opacity_now)" == "128" ]] || fail "opacity lost after minimise/restore: $(probe)"
echo "ok   opacity survives minimise/restore with GNOME's real 400 ms animations"

testcall ForceActorOpacity 255 >/dev/null
sleep 0.3
[[ "$(opacity_now)" == "128" ]] || fail "opacity not restored after an external reset: $(probe)"
echo "ok   opacity is restored when something else resets it"

host "$ROOT/native" '{"cmd":"set_opacity","opacity":1}' >/dev/null
host "$ROOT/native" '{"cmd":"set_above","above":false}' >/dev/null
[[ "$(field "$(probe)" "d['actorOpacity']")" == "255" && "$(field "$(probe)" "d['above']")" == "False" ]] \
  || fail "reset: $(probe)"
echo "ok   reset to opaque and not above"
echo "PASS"
