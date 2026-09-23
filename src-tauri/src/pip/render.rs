//! CPU renderer: crops, scales (nearest neighbour, aspect preserved) and draws
//! the PiP overlay into a softbuffer `0x00RRGGBB` pixel buffer.

use crate::capture::{Frame, PixelFormat};
use crate::profiles::Rect;

pub const BACKGROUND: u32 = 0x0010_1014;
pub const CLOSE_BUTTON_SIZE: u32 = 22;
const CLOSE_BG: u32 = 0x00c4_2b1c;
const CLOSE_FG: u32 = 0x00ff_ffff;

/// Largest rect with the aspect ratio of `src_w` x `src_h` centred inside `dst_w` x `dst_h`.
pub fn fit(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Rect {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return Rect { x: 0, y: 0, width: 0, height: 0 };
    }
    let scale = f64::min(dst_w as f64 / src_w as f64, dst_h as f64 / src_h as f64);
    let width = ((src_w as f64 * scale).round() as u32).clamp(1, dst_w);
    let height = ((src_h as f64 * scale).round() as u32).clamp(1, dst_h);
    Rect { x: (dst_w - width) / 2, y: (dst_h - height) / 2, width, height }
}

/// Draws `src` (a region of `frame`) letterboxed into `dst`.
pub fn blit(frame: &Frame, src: Rect, dst: &mut [u32], dst_w: u32, dst_h: u32) {
    dst.fill(BACKGROUND);
    let target = fit(src.width, src.height, dst_w, dst_h);
    if target.width == 0 || target.height == 0 {
        return;
    }

    let (r_off, b_off) = match frame.format {
        PixelFormat::Bgrx => (2, 0),
        PixelFormat::Rgbx => (0, 2),
    };
    let src_x: Vec<usize> =
        (0..target.width).map(|x| (src.x + x * src.width / target.width) as usize * 4).collect();

    for y in 0..target.height {
        let sy = (src.y + y * src.height / target.height) as usize;
        let row_start = sy * frame.stride;
        let Some(row) = frame.data.get(row_start..row_start + frame.stride) else { break };
        let out_start = ((target.y + y) * dst_w + target.x) as usize;
        let out = &mut dst[out_start..out_start + target.width as usize];
        for (px, &sx) in out.iter_mut().zip(&src_x) {
            if let Some(p) = row.get(sx..sx + 4) {
                *px = (p[r_off] as u32) << 16 | (p[1] as u32) << 8 | p[b_off] as u32;
            }
        }
    }
}

/// Bounds of the close button in the top-right corner.
pub fn close_button(dst_w: u32) -> Rect {
    let size = CLOSE_BUTTON_SIZE;
    Rect { x: dst_w.saturating_sub(size + 6), y: 6, width: size, height: size }
}

pub fn contains(r: Rect, x: f64, y: f64) -> bool {
    x >= r.x as f64 && y >= r.y as f64 && x < (r.x + r.width) as f64 && y < (r.y + r.height) as f64
}

/// Draws the hover overlay: a thin border and a close button.
pub fn draw_overlay(dst: &mut [u32], dst_w: u32, dst_h: u32) {
    if dst_w < CLOSE_BUTTON_SIZE + 12 || dst_h < CLOSE_BUTTON_SIZE + 12 {
        return;
    }
    let border = 0x0060_6878;
    for x in 0..dst_w {
        dst[x as usize] = border;
        dst[((dst_h - 1) * dst_w + x) as usize] = border;
    }
    for y in 0..dst_h {
        dst[(y * dst_w) as usize] = border;
        dst[(y * dst_w + dst_w - 1) as usize] = border;
    }

    let b = close_button(dst_w);
    for y in 0..b.height {
        for x in 0..b.width {
            // Two 2px-thick diagonals with a 6px margin form the "×".
            let (ix, iy) = (x as i32, y as i32);
            let inner = (6..b.width as i32 - 6).contains(&ix) && (6..b.height as i32 - 6).contains(&iy);
            let on_cross = inner && ((ix - iy).abs() <= 1 || (ix + iy - (b.width as i32 - 1)).abs() <= 1);
            dst[((b.y + y) * dst_w + b.x + x) as usize] = if on_cross { CLOSE_FG } else { CLOSE_BG };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(format: PixelFormat, w: u32, h: u32, px: [u8; 4]) -> Frame {
        Frame { width: w, height: h, stride: w as usize * 4, format, data: px.repeat((w * h) as usize) }
    }

    #[test]
    fn fit_letterboxes_wide_source() {
        assert_eq!(fit(200, 100, 100, 100), Rect { x: 0, y: 25, width: 100, height: 50 });
        assert_eq!(fit(100, 200, 100, 100), Rect { x: 25, y: 0, width: 50, height: 100 });
        assert_eq!(fit(0, 10, 100, 100).width, 0);
    }

    #[test]
    fn blit_converts_pixel_formats() {
        let full = Rect { x: 0, y: 0, width: 2, height: 2 };
        let mut out = vec![0u32; 4];
        blit(&frame(PixelFormat::Bgrx, 2, 2, [0x11, 0x22, 0x33, 0xff]), full, &mut out, 2, 2);
        assert!(out.iter().all(|&p| p == 0x0033_2211));
        blit(&frame(PixelFormat::Rgbx, 2, 2, [0x11, 0x22, 0x33, 0xff]), full, &mut out, 2, 2);
        assert!(out.iter().all(|&p| p == 0x0011_2233));
    }

    #[test]
    fn blit_crops_and_fills_background() {
        // Left half red, right half blue; crop the right half into a wide target.
        let mut f = frame(PixelFormat::Rgbx, 4, 2, [0; 4]);
        for y in 0..2 {
            for x in 0..4 {
                let i = y * f.stride + x * 4;
                f.data[i..i + 4].copy_from_slice(if x < 2 { &[255, 0, 0, 0] } else { &[0, 0, 255, 0] });
            }
        }
        let mut out = vec![0u32; 8 * 4];
        blit(&f, Rect { x: 2, y: 0, width: 2, height: 2 }, &mut out, 8, 4);
        assert_eq!(out[0], BACKGROUND); // letterbox bar
        assert_eq!(out[4 + 2 * 8], 0x0000_00ff); // centre is blue
        assert!(!out.contains(&0x00ff_0000)); // no red from the cropped-out half
    }

    #[test]
    fn close_button_hit_test() {
        let b = close_button(300);
        assert!(contains(b, (b.x + 3) as f64, (b.y + 3) as f64));
        assert!(!contains(b, 10.0, 10.0));
    }
}
