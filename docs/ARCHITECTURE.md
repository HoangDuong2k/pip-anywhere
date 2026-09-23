# Architecture

```
┌────────────────────── pip-anywhere (control app) ──────────────────────┐
│  React control panel (webview)  ⇄  Tauri commands / events             │
│  Tray menu, single-instance (`--pop`), state.json                      │
│  manager.rs: spawns one child process per PiP, talks JSON over stdio   │
└───────────────┬─────────────────────────────────────────┬──────────────┘
                │ pip-anywhere pip --config <json>         │ …
┌───────────────▼──────────────── PiP child ──────────────────────────────┐
│  capture thread                          main thread (winit)           │
│  portal ScreenCast → PipeWire stream ──► FrameSink ──► softbuffer blit │
│  (throttled to profile fps)              (latest frame)  crop + scale  │
└─────────────────────────────────────────────────────────────────────────┘
```

## Why a child process per PiP?

- On GNOME Wayland, Mutter ignores "always on top" requests from Wayland clients. The child is started without
  `WAYLAND_DISPLAY`, so winit uses X11 through XWayland, where `_NET_WM_STATE_ABOVE` and `_NET_WM_WINDOW_OPACITY`
  work. The control panel stays a native Wayland app.
- A capture or renderer failure only takes down one PiP.
- The PiP is a plain winit window rendered with softbuffer, with no webview involved, so frames never go through IPC.

## Source layout (`src-tauri/src`)

| File | Role |
|---|---|
| `main.rs` | Dispatches `pip` subcommand (child) vs. control app |
| `lib.rs` | Tauri setup, commands, CLI args, window/quit behaviour |
| `manager.rs` | Child process lifecycle, saved sources, state events |
| `tray.rs` | Tray menu |
| `settings.rs` | `state.json` (saved sources: restore token, geometry, opacity) |
| `profiles.rs` | App profiles (`/profiles/*.json`, compiled in) and crop maths |
| `capture/` | Platform capture backends (`linux.rs`: portal + PipeWire; Windows/macOS: stubs) |
| `pip/window.rs` | The PiP window: input handling, always-on-top, opacity |
| `pip/render.rs` | CPU renderer: crop, aspect-preserving scale, overlay |

## Child protocol

Control app → child (stdin): `close`, `opacity <0.2-1.0>`. EOF also closes the child.

Child → control app (stdout, one JSON object per line):

```json
{"type":"started","restore_token":"…"}
{"type":"closed","reason":"closed","by_user":true,"geometry":{"x":10,"y":10,"width":480,"height":300},"opacity":0.9}
```

`by_user: false` (the app asked it to close, e.g. on quit) marks the source to reopen on next launch.
