//! Windows capture: not implemented yet (Phase 3, DWM thumbnails / Windows.Graphics.Capture). See docs/ROADMAP.md.

use super::{CaptureHandle, CaptureRequest, FrameSink};

pub fn start(_req: CaptureRequest, _sink: FrameSink) -> Result<CaptureHandle, String> {
    Err("window capture is not implemented on this platform yet".into())
}
