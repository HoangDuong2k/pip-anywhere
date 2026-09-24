//! Windows backend: acts on the foreground window's top-level owner, i.e. the browser window
//! even while the extension popup (an owned window) has focus.
//!
//! - Always on top: `SetWindowPos(HWND_TOPMOST / HWND_NOTOPMOST)`.
//! - Opacity: `WS_EX_LAYERED` + `SetLayeredWindowAttributes(LWA_ALPHA)`. Windows keeps it across
//!   minimise/restore, so nothing has to be re-applied.

use windows_sys::Win32::Foundation::{CloseHandle, COLORREF, HANDLE, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, GetLayeredWindowAttributes, GetPropW, GetWindowLongPtrW,
    GetWindowTextW, GetWindowThreadProcessId, RemovePropW, SetLayeredWindowAttributes, SetPropW,
    SetWindowLongPtrW, SetWindowPos, GA_ROOTOWNER, GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST,
    LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_LAYERED, WS_EX_TOPMOST,
};

use crate::protocol::{Backend, HostError, WindowInfo};

/// Window property marking that we added `WS_EX_LAYERED`, so we only ever remove our own change.
const LAYERED_BY_US: &str = "PipAnywhere.Layered";

pub struct Win32;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn target() -> Option<HWND> {
    // SAFETY: plain Win32 queries with no pointers passed in.
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_null() {
            return None;
        }
        let root = GetAncestor(foreground, GA_ROOTOWNER);
        Some(if root.is_null() { foreground } else { root })
    }
}

fn ex_style(hwnd: HWND) -> u32 {
    // SAFETY: hwnd came from the window manager; an invalid handle just returns 0.
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

fn title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: the buffer length is passed along with the pointer.
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn process_name(hwnd: HWND) -> String {
    let mut pid = 0u32;
    // SAFETY: pid is a valid out pointer; the process handle is closed below.
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process: HANDLE = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().unwrap_or_default().to_string()
    }
}

fn opacity(hwnd: HWND) -> f64 {
    if ex_style(hwnd) & WS_EX_LAYERED == 0 {
        return 1.0;
    }
    let (mut key, mut alpha, mut flags): (COLORREF, u8, u32) = (0, 255, 0);
    // SAFETY: all three out pointers are valid locals.
    let ok = unsafe { GetLayeredWindowAttributes(hwnd, &mut key, &mut alpha, &mut flags) };
    if ok != 0 && flags & LWA_ALPHA != 0 {
        alpha as f64 / 255.0
    } else {
        1.0
    }
}

fn describe(hwnd: HWND) -> WindowInfo {
    WindowInfo {
        id: hwnd as usize as u64,
        title: title(hwnd),
        wm_class: process_name(hwnd),
        above: ex_style(hwnd) & WS_EX_TOPMOST != 0,
        opacity: opacity(hwnd),
    }
}

fn last_error(what: &str) -> HostError {
    HostError::new(
        "error",
        format!("{what} failed: {}", std::io::Error::last_os_error()),
    )
}

/// Pins `hwnd` above all non-topmost windows, or releases it.
pub fn set_above_on(hwnd: HWND, above: bool) -> Result<(), HostError> {
    let after = if above { HWND_TOPMOST } else { HWND_NOTOPMOST };
    // SAFETY: hwnd is a top-level window handle; flags keep size, position and activation.
    let ok = unsafe {
        SetWindowPos(
            hwnd,
            after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
    if ok == 0 {
        return Err(last_error("SetWindowPos"));
    }
    Ok(())
}

/// Makes `hwnd` translucent (0.2–1.0). At 1.0 the window becomes a normal, non-layered window
/// again, unless it was layered before we touched it.
pub fn set_opacity_on(hwnd: HWND, opacity: f64) -> Result<(), HostError> {
    let prop = wide(LAYERED_BY_US);
    // SAFETY: hwnd is a window handle and `prop` a NUL-terminated UTF-16 string that outlives the calls.
    unsafe {
        let style = ex_style(hwnd);
        if opacity >= 1.0 {
            if !GetPropW(hwnd, prop.as_ptr()).is_null() {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (style & !WS_EX_LAYERED) as isize);
                RemovePropW(hwnd, prop.as_ptr());
            } else if style & WS_EX_LAYERED != 0 {
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            }
        } else {
            if style & WS_EX_LAYERED == 0 {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (style | WS_EX_LAYERED) as isize);
                SetPropW(hwnd, prop.as_ptr(), 1 as HANDLE);
            }
            let alpha = (opacity * 255.0).round() as u8;
            if SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA) == 0 {
                return Err(last_error("SetLayeredWindowAttributes"));
            }
        }
    }
    Ok(())
}

impl Backend for Win32 {
    fn focused(&mut self) -> Result<Option<WindowInfo>, HostError> {
        Ok(target().map(describe))
    }

    fn set_above(&mut self, above: bool) -> Result<Option<WindowInfo>, HostError> {
        let Some(hwnd) = target() else {
            return Ok(None);
        };
        set_above_on(hwnd, above)?;
        Ok(Some(describe(hwnd)))
    }

    fn set_opacity(&mut self, opacity: f64) -> Result<Option<WindowInfo>, HostError> {
        let Some(hwnd) = target() else {
            return Ok(None);
        };
        set_opacity_on(hwnd, opacity)?;
        Ok(Some(describe(hwnd)))
    }
}

/// Runs on real Windows (CI: windows-latest) against a window this test creates, so it does not
/// depend on which window has focus.
#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WS_OVERLAPPEDWINDOW,
    };

    struct TestWindow(HWND);

    impl TestWindow {
        fn new() -> Self {
            let class = wide("STATIC"); // predefined class, no registration needed
            let title = wide("PiP Anywhere test");
            // SAFETY: valid NUL-terminated strings; null parent/menu/instance/param are allowed.
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW,
                    100,
                    100,
                    400,
                    300,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(
                !hwnd.is_null(),
                "CreateWindowExW failed: {}",
                std::io::Error::last_os_error()
            );
            Self(hwnd)
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: the window was created by this test.
            unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn describes_a_window() {
        let w = TestWindow::new();
        let info = describe(w.0);
        assert_eq!(info.title, "PiP Anywhere test");
        assert!(
            info.wm_class.ends_with(".exe"),
            "process name: {}",
            info.wm_class
        );
        assert!(!info.above);
        assert_eq!(info.opacity, 1.0);
    }

    #[test]
    fn pins_and_releases() {
        let w = TestWindow::new();
        set_above_on(w.0, true).unwrap();
        assert!(describe(w.0).above);
        set_above_on(w.0, false).unwrap();
        assert!(!describe(w.0).above);
    }

    #[test]
    fn opacity_round_trip_restores_a_normal_window() {
        let w = TestWindow::new();
        set_opacity_on(w.0, 0.5).unwrap();
        assert_eq!(ex_style(w.0) & WS_EX_LAYERED, WS_EX_LAYERED);
        assert!((describe(w.0).opacity - 128.0 / 255.0).abs() < 1e-9);
        set_opacity_on(w.0, 0.8).unwrap();
        assert!((describe(w.0).opacity - 204.0 / 255.0).abs() < 1e-9);
        set_opacity_on(w.0, 1.0).unwrap();
        assert_eq!(
            ex_style(w.0) & WS_EX_LAYERED,
            0,
            "layered style should be removed"
        );
        assert_eq!(describe(w.0).opacity, 1.0);
    }

    #[test]
    fn keeps_windows_that_were_already_layered() {
        let w = TestWindow::new();
        // SAFETY: test-owned window.
        unsafe { SetWindowLongPtrW(w.0, GWL_EXSTYLE, (ex_style(w.0) | WS_EX_LAYERED) as isize) };
        set_opacity_on(w.0, 0.5).unwrap();
        set_opacity_on(w.0, 1.0).unwrap();
        assert_eq!(
            ex_style(w.0) & WS_EX_LAYERED,
            WS_EX_LAYERED,
            "must not strip the app's own layering"
        );
        assert_eq!(describe(w.0).opacity, 1.0);
    }
}
