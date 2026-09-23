# PiP Anywhere

Picture-in-Picture for **any** desktop window: float a terminal, a browser tab, a file manager or VS Code
in a small always-on-top window while you work in another app.

Built with [Tauri 2](https://tauri.app) + Rust (capture and native PiP window) and React + TypeScript (control panel).

| Platform | Status |
|---|---|
| Linux (Wayland: GNOME, KDE; X11 with xdg-desktop-portal) | ✅ MVP |
| Windows | 🚧 Phase 3 (DWM thumbnails) |
| macOS | 🚧 Phase 4 (ScreenCaptureKit) |

## Features (MVP)

- Pick any window through the system screen-share dialog; each PiP remembers its window, so reopening skips the dialog
- Borderless, always-on-top PiP: drag to move, drag the edges to resize, `Esc`, middle-click or × to close; opacity slider in the control panel
- **App profiles** for popular apps: Browser (hides tabs and address bar), Terminal (follows the latest output, low fps),
  Files, VS Code (hides the side bar). See [`profiles/`](profiles)
- Several PiPs at once; PiPs that were open when you quit come back on next launch, with the same position, size and opacity
- Tray menu and a `--pop [profile]` command line flag for keyboard shortcuts

## Getting started (Ubuntu / Debian)

```bash
# System libraries (Tauri + PipeWire)
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libssl-dev libpipewire-0.3-dev libclang-dev

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

npm install
npm run tauri dev
```

Other platforms: see the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

### Keyboard shortcut (GNOME)

Global shortcuts are not available to apps on Wayland, so bind one in the desktop settings instead:
**Settings → Keyboard → View and Customize Shortcuts → Custom Shortcuts → +**, with the command
`pip-anywhere --pop terminal` (or `default`, `chrome`, `files`, `vscode`) and e.g. `Ctrl+Alt+P`.
The running app receives it; a second copy is never started.

## Development

```bash
npm run tauri dev                        # run the app
cd src-tauri && cargo test               # unit tests (crop, scaling, profiles, state)
cd src-tauri && cargo clippy --all-targets
```

- [Architecture](docs/ARCHITECTURE.md)
- [Roadmap](docs/ROADMAP.md)
- [Platform notes and limitations](docs/platform-notes.md)

## License

MIT
