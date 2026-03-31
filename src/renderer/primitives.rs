use super::{Bgra, Rect, Surface};

/// Blend a single channel: `(fg * alpha + bg * (255 - alpha)) / 255`.
/// Both inputs and the result are in 0..=255, so the intermediate `u16`
/// arithmetic never overflows and the final division always yields a value
/// that fits in `u8`.
#[inline]
pub fn blend_channel(fg: u8, bg: u8, alpha: u16) -> u8 {
    let inv = 255 - alpha;
    let val = u16::from(fg) * alpha + u16::from(bg) * inv;
    // Exact division by 255: for val in 0..=65025, ((val+1)*257)>>16 == val/255.
    // Proof: val = 255q + r (0 <= r < 255). (val+1)*257 = 65535q + (r+1)*257.
    // Since (r+1)*257 in [257, 65535] and 65535 = 2^16-1, the >>16 yields q.
    // Max val = 65025 (255*255), max q = 255, so result always fits in u8.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "proven ≤255 for val in 0..=65025"
    )]
    let result = (((u32::from(val) + 1) * 257) >> 16) as u8;
    result
}

pub fn set_pixel(surface: &mut Surface<'_>, px: u32, py: u32, color: Bgra) {
    if px >= surface.width || py >= surface.height {
        return;
    }
    let offset = (py * surface.stride + px * 4) as usize;
    if let Some(pixel) = surface.buf.get_mut(offset..offset + 4) {
        pixel.copy_from_slice(&[color.2, color.1, color.0, 0xFF]);
    }
}

pub fn draw_rect(surface: &mut Surface<'_>, rect: &Rect, color: Bgra) {
    let pixel_u32 = u32::from_ne_bytes([color.2, color.1, color.0, 0xFF]);
    // Clamp to surface bounds
    let x_end = (rect.x + rect.w).min(surface.width);
    let y_end = (rect.y + rect.h).min(surface.height);
    if rect.x >= x_end || rect.y >= y_end {
        return;
    }
    let col_start = (rect.x * 4) as usize;
    let col_bytes = ((x_end - rect.x) * 4) as usize;
    let buf_len = surface.buf.len();
    for py in rect.y..y_end {
        let row_base = (py * surface.stride) as usize;
        let start = row_base + col_start;
        let end = start + col_bytes;
        debug_assert!(end <= buf_len, "draw_rect: row slice out of bounds");
        // SAFETY: x_end/y_end are clamped to surface.width/height, and
        // stride * height <= buf.len() is a buffer invariant, so
        // start..end is always within bounds.
        let row = unsafe { surface.buf.get_unchecked_mut(start..end) };
        // SAFETY: row is a &mut [u8] slice of 4-byte BGRA pixels.
        let (prefix, aligned, suffix) = unsafe { row.align_to_mut::<u32>() };
        for b in prefix.chunks_exact_mut(4) {
            b.copy_from_slice(&pixel_u32.to_ne_bytes());
        }
        aligned.fill(pixel_u32);
        for b in suffix.chunks_exact_mut(4) {
            b.copy_from_slice(&pixel_u32.to_ne_bytes());
        }
    }
}

/// Blend `color` over the existing buffer contents at the given rectangle,
/// using the supplied alpha value (0 = fully transparent, 255 = fully opaque).
#[expect(
    clippy::cast_possible_truncation,
    reason = "fast div-255: result proven ≤255"
)]
pub fn draw_rect_alpha(surface: &mut Surface<'_>, rect: &Rect, color: Bgra, alpha: u8) {
    // Fast path: fully opaque — skip blend math entirely.
    if alpha == 255 {
        draw_rect(surface, rect, color);
        return;
    }
    let x_end = (rect.x + rect.w).min(surface.width);
    let y_end = (rect.y + rect.h).min(surface.height);
    if rect.x >= x_end || rect.y >= y_end {
        return;
    }
    // Pre-compute fg * alpha per channel (loop-invariant hoisting).
    let a32 = u32::from(alpha);
    let fg_b = u32::from(color.2) * a32;
    let fg_g = u32::from(color.1) * a32;
    let fg_r = u32::from(color.0) * a32;
    let inv32 = u32::from(255 - alpha);
    let col_start = (rect.x * 4) as usize;
    let col_bytes = ((x_end - rect.x) * 4) as usize;
    let buf_len = surface.buf.len();
    for py in rect.y..y_end {
        let row_base = (py * surface.stride) as usize;
        let start = row_base + col_start;
        let end = start + col_bytes;
        debug_assert!(end <= buf_len, "draw_rect_alpha: row slice out of bounds");
        // SAFETY: x_end/y_end are clamped to surface.width/height, and
        // stride * height <= buf.len() is a buffer invariant, so
        // start..end is always within bounds.
        let row = unsafe { surface.buf.get_unchecked_mut(start..end) };
        for chunk in row.chunks_exact_mut(4) {
            if let [b_ch, g_ch, r_ch, a_ch] = chunk {
                let val_b = fg_b + u32::from(*b_ch) * inv32;
                *b_ch = (((val_b + 1) * 257) >> 16) as u8;
                let val_g = fg_g + u32::from(*g_ch) * inv32;
                *g_ch = (((val_g + 1) * 257) >> 16) as u8;
                let val_r = fg_r + u32::from(*r_ch) * inv32;
                *r_ch = (((val_r + 1) * 257) >> 16) as u8;
                *a_ch = 0xFF;
            }
        }
    }
}
