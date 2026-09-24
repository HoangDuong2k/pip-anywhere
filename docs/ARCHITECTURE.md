# Architecture

```
extension/                     Chrome MV3 extension (permission: nativeMessaging)
├─ manifest.json               "key" pins the extension ID the helper accepts
├─ native.js                   sendNativeMessage wrapper, opacity stepping
├─ background.js               keyboard shortcuts
└─ popup/                      switch + opacity slider for the current window
native/host.py                 Linux native messaging host (Python 3, stdlib only)
native/windows/                Windows native messaging host (Rust): protocol.rs + win32.rs
gnome-extension/<uuid>/        GNOME Shell extension exporting com.pipanywhere.Shell
scripts/install-linux.sh       installs host + browser manifests + GNOME extension (per user)
scripts/windows/               install.cmd/.ps1, uninstall.cmd/.ps1 (per user, HKCU registry)
scripts/build-windows.sh       cross-compiles the Windows host and packages dist/pip-anywhere-windows.zip
tests/                         unit tests + headless GNOME Shell end-to-end test
```

## Which window?

Every request acts on the **focused top-level window**. When you use the popup or a shortcut, that is
the browser window you are in. If a popup or dialog of the browser has focus (for example the
extension popup on X11), the GNOME extension uses `find_root_ancestor()` to act on the window that
owns it. This avoids matching browser windows by title or position, which Wayland does not expose.

## Native messaging

Chrome starts `host.py` for each `chrome.runtime.sendNativeMessage` call. Messages are framed as a
32-bit native-endian length followed by UTF-8 JSON.

| Request | D-Bus call |
|---|---|
| `{"cmd":"ping"}` | none (returns host version) |
| `{"cmd":"state"}` | `GetFocused()` |
| `{"cmd":"set_above","above":true}` | `SetAbove(b)` |
| `{"cmd":"set_opacity","opacity":0.6}` | `SetOpacity(d)` (clamped to 0.2–1) |

Replies are `{"ok":true,"window":{"id","title","wm_class","above","opacity"}}` or
`{"ok":false,"error","code"}`. `code` is one of `helper_missing`, `unsupported`, `no_window`,
`bad_request`, `error`, and the extension adds `host_missing` when the host is not registered. The host
calls D-Bus through `gdbus` and parses its GVariant text output with `ast.literal_eval`, since a
one-string GVariant tuple is also a valid Python literal.

The popup sends at most one opacity request at a time, always with the latest slider value, because
every request starts a new host process.

## GNOME Shell extension

- **Always on top**: `MetaWindow.make_above()` / `unmake_above()`, the same as the window menu's
  "Always on Top".
- **Opacity**: sets `opacity` (0–255) on the window's compositor actor. GNOME resets it to 255 at the
  end of its window animations (`_minimizeWindowDone`, `_unminimizeWindowDone`, map, ...), so the
  extension watches `notify::opacity` on each translucent window's actor. When the value changes and
  no opacity transition is running, it restores the stored value in a `BEFORE_REDRAW` later, before
  the next frame is drawn. This does not depend on how long the animations take. The actor is
  re-watched on `map` in case it was replaced. Disabling the extension restores full opacity.

`tests/gnome/run.sh` leaves the overview (where GNOME skips animations) and minimises and restores a
normal window with GNOME's real 400 ms animations. It also forces the actor opacity to 255 directly,
and checks that the value comes back both times.

## Windows host

`native/windows` implements the same protocol as `host.py` (`protocol.rs`, unit-tested on any OS).
The Win32 backend (`win32.rs`):

- **Target window**: `GetAncestor(GetForegroundWindow(), GA_ROOTOWNER)`. The extension popup is a
  window owned by the browser window, so this resolves to the browser window.
- **Always on top**: `SetWindowPos(HWND_TOPMOST | HWND_NOTOPMOST, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)`.
- **Opacity**: adds `WS_EX_LAYERED` and calls `SetLayeredWindowAttributes(LWA_ALPHA)`. Windows keeps the
  value across minimise/restore, so nothing needs re-applying. At 100% the layered style is removed
  again (cheaper to draw), but only if we added it. That is recorded with the window property
  `PipAnywhere.Layered`.
- **Registration**: `install.ps1` copies the exe to `%LOCALAPPDATA%\PipAnywhere`, writes the host
  manifest (UTF-8 without BOM) and points
  `HKCU\Software\<browser>\NativeMessagingHosts\com.pipanywhere.host` at it.

The Win32 tests create their own window (so they do not depend on focus), pin and release it, and
check the layered style and alpha. They run in CI on `windows-latest`.
