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

## Next
- [ ] Signed Windows build and an MSI/winget package (unsigned exes can trigger SmartScreen)
- [ ] KDE Plasma helper (KWin script: `keepAbove`, `opacity`)
- [ ] Toolbar badge showing when the current window is pinned
- [ ] "Click-through" mode for a pinned, translucent window
- [ ] Remember opacity per window across browser restarts
- [ ] Chrome Web Store listing and a packaged helper (.deb) with the GNOME extension on extensions.gnome.org
