# Roadmap

## ✅ Phase 0: Setup
Tauri 2 + React/TS scaffold, repo layout, docs, CI.

## ✅ Phase 1: Linux MVP (Wayland)
- [x] Window picker via xdg-desktop-portal ScreenCast, PipeWire frames (BGRx/BGRA/RGBx/RGBA, shared memory)
- [x] Borderless always-on-top PiP (XWayland), drag, edge resize, opacity (control panel slider), close button / Esc / middle-click
- [x] Restore tokens: reopen the same window without the dialog; restore PiPs open at quit with geometry and opacity
- [x] Profiles: Any window, Browser, Terminal, Files, VS Code
- [x] Tray menu, control panel, `--pop <profile>` through single instance

## Phase 2: Profiles & UX
- [ ] Drag-select a crop region directly on the PiP, save as a custom profile
- [ ] Profile editor in the control panel (user profiles in the config dir)
- [ ] Snap to screen corners, "hide when hovered" mode
- [ ] Pause capture while the PiP is minimised or fully transparent
- [ ] Click-through mode

## Phase 3: Windows
- [ ] Window list (`EnumWindows`) with icons and process names, automatic profile matching
- [ ] `DwmRegisterThumbnail` rendering (crop via `rcSource`), near-zero CPU
- [ ] Opacity (`SetLayeredWindowAttributes`), double-click to focus the source (`SetForegroundWindow`)
- [ ] Global shortcut (`tauri-plugin-global-shortcut`)

## Phase 4: macOS
- [ ] ScreenCaptureKit (`SCShareableContent`, `SCStream`), Screen Recording permission flow
- [ ] Automatic profile matching by bundle id, focus the source via `NSRunningApplication.activate`

## Phase 5: Polish
- [ ] wgpu renderer, zero-copy DMA-BUF import on Linux
- [ ] Native X11 window list (no portal dialog on X11 sessions)
- [ ] Packages (.deb, AppImage, .msi, .dmg), auto-update, signing
- [ ] Forward clicks and scroll to the source window where the platform allows it
