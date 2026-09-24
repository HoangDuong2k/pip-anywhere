#!/usr/bin/env bash
# Cross-compiles the Windows native host and packages dist/pip-anywhere-windows.zip.
# Needs Rust with the x86_64-pc-windows-gnu target, plus either cargo-zigbuild (pip install
# ziglang cargo-zigbuild) or the mingw-w64 toolchain.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET=x86_64-pc-windows-gnu
cd "$ROOT/native/windows"
if command -v cargo-zigbuild >/dev/null; then
  cargo zigbuild --release --target "$TARGET"
else
  cargo build --release --target "$TARGET"
fi

OUT="$ROOT/dist/pip-anywhere-windows"
rm -rf "$OUT" "$OUT.zip"
mkdir -p "$OUT"
cp "target/$TARGET/release/pip-anywhere-host.exe" "$OUT/"
cp "$ROOT"/scripts/windows/{install.ps1,install.cmd,uninstall.ps1,uninstall.cmd,README.txt} "$OUT/"
(cd "$ROOT/dist" && zip -qr pip-anywhere-windows.zip pip-anywhere-windows)
echo "Built $OUT.zip"
