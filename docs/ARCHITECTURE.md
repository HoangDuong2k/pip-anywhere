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
| `{"cmd":"set_opacity","opacity":0.6,"hover_reveal":true}` | `SetOpacity(d)` (clamped to 0.2–1), then `SetHoverReveal(b)` if `hover_reveal` is given |
| `{"cmd":"set_hover_reveal","enabled":true}` | `SetHoverReveal(b)` ("Clear on hover"; Windows: `unsupported`) |
| `{"cmd":"diagnostics"}` | `Diagnostics()` + host details and the last 40 log lines |

Replies are `{"ok":true,"window":{"id","title","wm_class","above","opacity"},"versions":{...}}` or
`{"ok":false,"error","code","versions":{...}}`. `versions` holds `host`, `helper` (the running
GNOME extension) and `helper_installed` (the one on disk). The popup compares them with
`REQUIRED_HELPER_VERSION` (`extension/native.js`). It asks the user to reinstall when something is too
old, or to log out and back in when only the running GNOME extension is old. `code` is one of `helper_missing`, `unsupported`, `no_window`,
`bad_request`, `error`, and the extension adds `host_missing` when the host is not registered. The host
calls D-Bus through `gdbus` and parses its GVariant text output with `ast.literal_eval`, since a
one-string GVariant tuple is also a valid Python literal.

The popup sends at most one opacity request at a time, always with the latest slider value, because
every request starts a new host process.

## GNOME Shell extension

- **Always on top**: `MetaWindow.make_above()` / `unmake_above()`, the same as the window menu's
  "Always on Top". Windows pinned through PiP Anywhere are tracked. Other components may unpin them:
  Ubuntu's Tiling Assistant calls `unmake_above()` whenever it tiles a window. So on `notify::above`
  the extension pins the window again, before the next frame, until the user unpins it through PiP
  Anywhere. On every `restacked`, a pinned window that is not in the top layer has its layer
  recomputed. Both repairs are counted in `Diagnostics().stats` with the last 20 events.
- **Clear on hover**: one global switch (`SetHoverReveal`). The extension stores the preference in
  `chrome.storage.sync` and re-sends it with every opacity change, because the GNOME extension keeps
  no settings across logins. GNOME does not report the pointer entering another app's window, so
  while the switch is on and at least one translucent window exists, the GNOME extension checks
  `global.get_pointer()` every 100 ms. It looks for the topmost window under the pointer on the
  active workspace; popups count as their owner window (`find_root_ancestor()`), so menus and tooltips
  do not make it flicker. A hovered window eases to 255 in 150 ms and eases back after the pointer
  has been away for 300 ms. After each `SetOpacity` the chosen value is shown for 1.5 s even under
  the pointer, so changes made from the browser popup or the shortcuts are visible.
- **Diagnostics**: shell version, enabled extensions, focus, repair stats, and the full window stack
  (top first, with layer, above, maximised, fullscreen, workspace, monitor, X11/Wayland), plus the
  window actors' drawing order. If a pinned window is covered, comparing `stack` with `actors` shows
  whether GNOME stacks the other window higher or only draws it on top.
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
- **One request at a time**: each request holds the named mutex `Local\PipAnywhere.Host`. Chrome
  starts a host per request, and a pin change racing an opacity change on a Chrome window sometimes
  left it pinned (about 1 in 100 when sent together). The log lines no longer interleave either.
- **Registration**: `install.ps1` copies the exe to `%LOCALAPPDATA%\PipAnywhere`, writes the host
  manifest (UTF-8 without BOM) and points
  `HKCU\Software\<browser>\NativeMessagingHosts\com.pipanywhere.host` at it.

The Win32 tests create their own window (so they do not depend on focus), pin and release it, and
check the layered style and alpha. They run in CI on `windows-latest`.
