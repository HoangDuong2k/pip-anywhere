//! Window capture backends. Each platform exposes `start()` which delivers frames
//! into a [`FrameSink`] from a background thread.

use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::start;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::start;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::start;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Bytes B, G, R, X (alpha ignored).
    Bgrx,
    /// Bytes R, G, B, X (alpha ignored).
    Rgbx,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Bytes per row in `data`.
    pub stride: usize,
    pub format: PixelFormat,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// Token from a previous session, lets the portal skip the picker dialog.
    pub restore_token: Option<String>,
    pub fps: u32,
}

#[derive(Debug, Clone)]
pub enum CaptureEvent {
    Started { restore_token: Option<String> },
    Frame,
    Ended { reason: String },
}

/// Latest-frame slot shared with the renderer, plus a wake-up callback.
#[derive(Clone)]
pub struct FrameSink {
    latest: Arc<Mutex<Option<Frame>>>,
    notify: Arc<dyn Fn(CaptureEvent) + Send + Sync>,
}

impl FrameSink {
    pub fn new(notify: impl Fn(CaptureEvent) + Send + Sync + 'static) -> Self {
        Self { latest: Arc::default(), notify: Arc::new(notify) }
    }

    pub fn notify(&self, event: CaptureEvent) {
        (self.notify)(event);
    }

    /// Copies a frame into the shared slot, reusing the pending buffer when the
    /// renderer has not consumed it yet.
    pub fn publish(&self, width: u32, height: u32, stride: usize, format: PixelFormat, bytes: &[u8]) {
        {
            let mut slot = self.latest.lock().unwrap();
            match slot.as_mut() {
                Some(f) => {
                    f.data.clear();
                    f.data.extend_from_slice(bytes);
                    f.width = width;
                    f.height = height;
                    f.stride = stride;
                    f.format = format;
                }
                None => {
                    *slot = Some(Frame { width, height, stride, format, data: bytes.to_vec() });
                }
            }
        }
        self.notify(CaptureEvent::Frame);
    }

    pub fn take(&self) -> Option<Frame> {
        self.latest.lock().unwrap().take()
    }
}

/// Stops the capture when [`CaptureHandle::stop`] is called.
pub struct CaptureHandle {
    stop: Option<Box<dyn FnOnce() + Send>>,
}

impl CaptureHandle {
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Self {
        Self { stop: Some(Box::new(stop)) }
    }

    pub fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
