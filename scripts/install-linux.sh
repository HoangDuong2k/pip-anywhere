#!/usr/bin/env bash
# Installs the PiP Anywhere helper for the current user (no sudo):
#   1. the native messaging host that the browser extension talks to,
#   2. the GNOME Shell extension that pins windows and changes their opacity.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UUID="pip-anywhere@pipanywhere.github.io"
HOST_NAME="com.pipanywhere.host"
# Fixed by the "key" in extension/manifest.json.
EXTENSION_ID="imcnedckfcpfibpcdjpcjgjaichejlee"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"

command -v python3 >/dev/null || { echo "python3 is required"; exit 1; }

# 1. Native messaging host
HOST_PATH="$DATA/pip-anywhere/host.py"
install -Dm755 "$ROOT/native/host.py" "$HOST_PATH"
manifest() {
  cat <<JSON
{
  "name": "$HOST_NAME",
  "description": "PiP Anywhere helper: keeps the browser window on top and changes its opacity",
  "path": "$HOST_PATH",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$EXTENSION_ID/"]
}
JSON
}
installed=0
for browser in google-chrome google-chrome-beta google-chrome-unstable chromium \
               BraveSoftware/Brave-Browser microsoft-edge vivaldi; do
  if [[ -d "$CONFIG/$browser" ]]; then
    mkdir -p "$CONFIG/$browser/NativeMessagingHosts"
    manifest > "$CONFIG/$browser/NativeMessagingHosts/$HOST_NAME.json"
    echo "✓ native host registered for $browser"
    installed=1
  fi
done
[[ $installed == 1 ]] || echo "! no Chromium-based browser profile found in $CONFIG (start the browser once, then rerun)"

# 2. GNOME Shell extension
if [[ "${XDG_CURRENT_DESKTOP:-}" == *GNOME* ]] || command -v gnome-shell >/dev/null; then
  DEST="$DATA/gnome-shell/extensions/$UUID"
  mkdir -p "$DEST"
  cp "$ROOT/gnome-extension/$UUID/metadata.json" "$ROOT/gnome-extension/$UUID/extension.js" "$DEST/"
  echo "✓ GNOME extension installed to $DEST"
  if gnome-extensions enable "$UUID" 2>/dev/null; then
    echo "✓ GNOME extension enabled"
  else
    echo
    echo "Last step: log out and back in (GNOME on Wayland only discovers new extensions at login), then run:"
    echo "  gnome-extensions enable $UUID"
  fi
else
  echo "! This desktop is not GNOME. Only GNOME is supported for now (see docs/ROADMAP.md)."
fi
