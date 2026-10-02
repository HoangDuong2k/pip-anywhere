# PiP Anywhere

Keep a **browser window always on top** of your other apps, like picture-in-picture for the whole
window, and make it **see-through**. The window stays a normal, fully usable browser window: tabs,
extensions, logins and typing all work.

- **Always on top**: the window stays above other apps when you click elsewhere or open another window.
- **Opacity**: 20–100%, real transparency. You see the app behind it.
- **Clear on hover** (optional, on by default): a see-through window becomes fully opaque while the
  pointer is over it, and fades back shortly after the pointer leaves. When you change the opacity
  with the pointer over the window, the new value is shown for 1.5 s first. Linux/GNOME only for
  now: on Windows the option is hidden.

| Action | Shortcut |
|---|---|
| Pin / unpin this window | `Alt+Shift+O` |
| More transparent / more opaque | `Alt+Shift+↓` / `Alt+Shift+↑` |

Or click the toolbar icon for a switch and an opacity slider. Chrome now uses `Alt+Shift+P` itself (new
tab group), so it cannot be the pin shortcut. If a shortcut shows as `—` in the popup, another
extension already took it: set one at `chrome://extensions/shortcuts`.

## Why a helper?

Browsers do not let extensions change how the operating system stacks windows or blends them, and
Wayland does not let any app do it to its own windows either. PiP Anywhere is therefore a browser
extension plus a small helper:

```
extension (popup, shortcuts) ─native messaging─┬→ Linux:   native/host.py ─D-Bus→ GNOME Shell extension
                                               └→ Windows: pip-anywhere-host.exe → SetWindowPos(HWND_TOPMOST),
                                                                                  SetLayeredWindowAttributes
```

| Platform | Helper | Status |
|---|---|---|
| Linux, GNOME 45–50 (Wayland and X11) | `native/host.py` + GNOME Shell extension | ✅ |
| Windows 10/11 | `native/windows` (Rust, `pip-anywhere-host.exe`) | ✅ |
| Linux, KDE Plasma | | Planned |
| macOS | | Not possible without private APIs |

## Install (Windows)

1. **Helper**: get `pip-anywhere-windows.zip` (build it with `npm run package:windows`, or download
   the `pip-anywhere-windows` artifact from CI), unzip it and double-click **`install.cmd`**. No
   administrator rights needed. It installs to `%LOCALAPPDATA%\PipAnywhere` and registers the helper for
   Chrome, Chromium, Edge, Brave and Vivaldi (current user only).
2. **Browser extension**: same as below (Load unpacked → `extension/`).

To remove the helper: double-click `uninstall.cmd`.

## Install (Linux / GNOME)

1. **Helper** (no sudo). From this folder:
   ```bash
   ./scripts/install-linux.sh
   ```
   Then **log out and back in** once, and run
   `gnome-extensions enable pip-anywhere@pipanywhere.github.io`.
2. **Browser extension**: open `chrome://extensions`, turn on **Developer mode**, click **Load unpacked**
   and select the [`extension/`](extension) folder. Pin it to the toolbar.

Works with Chrome, Chromium, Brave, Edge and Vivaldi. The extension ID is fixed
(`imcnedckfcpfibpcdjpcjgjaichejlee`) so the helper only accepts this extension.

To remove the helper: `./scripts/uninstall-linux.sh`.

## Troubleshooting

- **"Copy diagnostics"** at the bottom of the popup copies a report: versions, the window stacking
  order as the desktop sees it (window titles shortened to 60 characters), and the last helper log
  lines. Paste it into a bug report.
- **The popup says the helper is out of date**: run the installer again (Linux:
  `./scripts/install-linux.sh`, Windows: `install.cmd`). On GNOME, also log out and back in. GNOME
  only loads new extension code at login, and the popup tells you when that is the missing step.
- **Ubuntu's tiling (Tiling Assistant)** unpins a window whenever it tiles it (Super+←/→, dragging to
  an edge, layouts). PiP Anywhere pins it again automatically. Diagnostics count these repairs under
  `stats.repins`.
- **Helper log**: `~/.local/state/pip-anywhere/host.log` (Linux), `%LOCALAPPDATA%\PipAnywhere\host.log`
  (Windows). One line per request with a full timestamp.

## Development

```bash
npm test             # extension (Node) and native host (Python) unit tests
npm run test:gnome   # end-to-end: helper + host in a private headless GNOME Shell (needs gnome-shell)
npm run test:windows-host   # Windows host: protocol tests anywhere, Win32 tests on Windows
npm run package:windows     # cross-compile the Windows host and build dist/pip-anywhere-windows.zip
npm run zip          # package extension/ as pip-anywhere.zip
```

Cross-compiling for Windows needs `rustup target add x86_64-pc-windows-gnu` and either
`pip install ziglang cargo-zigbuild` or mingw-w64. CI builds and tests the host on `windows-latest`,
including tests that pin a real window and change its opacity.

`npm run test:gnome` starts GNOME Shell headless with its own HOME, D-Bus session and Wayland display,
opens a window, and checks through a test-only probe extension that the compositor really pins it,
applies opacity, and keeps the opacity after minimise/restore. Your desktop is not touched.

Earlier approaches (a Tauri desktop app, then a per-element Document PiP extension) are in the git
history.

## License

MIT
