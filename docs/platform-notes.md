# Platform notes and limitations

## Linux, Wayland (GNOME)
- **Window selection**: Wayland does not let apps list other windows. The PiP asks through the system
  screen-share dialog. The portal returns a *restore token* (persist mode "until revoked"), so reopening the
  same source skips the dialog. Revoke in **Settings → Privacy → Screen sharing** (GNOME 46+).
- **Always on top**: Mutter ignores the request for Wayland clients, so PiP windows run through XWayland
  (`WAYLAND_DISPLAY` removed for the child process). XWayland must be available (it is by default).
- **Screen-share indicator**: GNOME shows its screen-sharing icon in the top bar while a PiP is open. This is
  expected and cannot be hidden.
- **App identity**: the portal does not report which app the window belongs to, so you choose the profile.
- **Focusing the source window** from the PiP is not possible on Wayland.
- **Global shortcuts**: use a desktop custom shortcut running `pip-anywhere --pop <profile>` (see README).
- **Tray icon**: needs the AppIndicator extension (enabled by default on Ubuntu).
- **Buffers**: only shared-memory buffers are negotiated for now (no DMA-BUF), which costs a copy per frame.
  Profiles cap the frame rate to keep CPU usage low.

## Linux, X11
Works through the same portal (requires `xdg-desktop-portal` with a ScreenCast backend). A native window list
without the dialog is planned (Phase 5).

## Windows (Phase 3), macOS (Phase 4)
Capture backends are stubs that return "not implemented"; the rest of the app builds and runs.
