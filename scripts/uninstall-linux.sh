#!/usr/bin/env bash
# Removes everything install-linux.sh added.
set -euo pipefail
UUID="pip-anywhere@pipanywhere.github.io"
HOST_NAME="com.pipanywhere.host"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"

gnome-extensions disable "$UUID" 2>/dev/null || true
rm -rf "$DATA/gnome-shell/extensions/$UUID" "$DATA/pip-anywhere"
for browser in google-chrome google-chrome-beta google-chrome-unstable chromium \
               BraveSoftware/Brave-Browser microsoft-edge vivaldi; do
  rm -f "$CONFIG/$browser/NativeMessagingHosts/$HOST_NAME.json"
done
echo "✓ PiP Anywhere helper removed (log out and back in to unload the GNOME extension)."
