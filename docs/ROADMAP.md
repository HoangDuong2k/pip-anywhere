# Roadmap

## ✅ v0.3: Whole-window PiP on GNOME
- [x] Always on top for the current browser window (popup switch, Alt+Shift+P)
- [x] Real window opacity 20–100% (popup slider, Alt+Shift+↑/↓), kept across minimise/restore
- [x] Native messaging host + GNOME Shell extension, per-user installer and uninstaller
- [x] Unit tests; end-to-end test in a headless GNOME Shell

## ✅ v0.4: Windows
- [x] Native host in Rust (`pip-anywhere-host.exe`): always on top + opacity via Win32
- [x] Per-user installer (`install.cmd`), uninstaller, CI build and Win32 tests on windows-latest
- [x] Popup shows setup steps for the current OS

## ✅ v0.4.1: Reliability
- [x] Re-pin windows that another component unpinned (e.g. Ubuntu's Tiling Assistant when tiling)
- [x] Recompute the layer of a pinned window that GNOME put below the top layer
- [x] "Copy diagnostics" in the popup: versions, window stack, repairs, recent log
- [x] Helper version check: the popup says when to reinstall or to log out and back in
- [x] Full ISO timestamps in the helper logs

## ✅ v0.5: Clear on hover
- [x] Optional "Clear on hover" (popup switch, on by default) on GNOME
- [x] Opacity changes previewed for 1.5 s under the pointer
- [ ] Windows: needs a small resident watcher process (the native host exits after each request)

## Next
- [ ] Signed Windows build and an MSI/winget package (unsigned exes can trigger SmartScreen)
- [ ] KDE Plasma helper (KWin script: `keepAbove`, `opacity`)
- [ ] Toolbar badge showing when the current window is pinned
- [ ] "Click-through" mode for a pinned, translucent window
- [ ] Remember opacity per window across browser restarts
- [ ] Chrome Web Store listing and a packaged helper (.deb) with the GNOME extension on extensions.gnome.org
