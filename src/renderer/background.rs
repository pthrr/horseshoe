use super::{Bgra, GridMetrics};
use crate::num::f64_to_u32_saturating;

/// Convert an f32 opacity (clamped to 0.0..=1.0) to a u8 alpha value.
#[inline]
pub(super) fn opacity_to_alpha(opacity: f32) -> u8 {
    // Clamp to valid range, scale to 0..255, round via +0.5 then floor.
    // Result is in [0.5, 255.5]; floored integer is in [0, 255].
    let scaled = f64::from(opacity.clamp(0.0, 1.0)) * 255.0 + 0.5;
    let val = f64_to_u32_saturating(scaled.floor());
    // val is at most 255 since opacity is clamped to [0, 1].
    u8::try_from(val).expect("opacity alpha in 0..=255")
}

pub fn clear_background(
    buf: &mut [u8],
    width: u32,
    height: u32,
    stride: u32,
    bg: Bgra,
    opacity: f32,
) {
    let (bg_r, bg_g, bg_b) = bg;
    let bg_alpha = opacity_to_alpha(opacity);
    let pixel_u32 = u32::from_ne_bytes([bg_b, bg_g, bg_r, bg_alpha]);

    if stride == width * 4 {
        // Fast path: no padding between rows — fill entire buffer as u32 slice.
        // SAFETY: buf is u8-aligned; align_to_mut handles prefix/suffix bytes.
        // The SHM buffer from SlotPool is always 4-byte aligned.
        let (prefix, aligned, suffix) = unsafe { buf.align_to_mut::<u32>() };
        for b in prefix.chunks_exact_mut(4) {
            b.copy_from_slice(&pixel_u32.to_ne_bytes());
        }
        aligned.fill(pixel_u32);
        for b in suffix.chunks_exact_mut(4) {
            b.copy_from_slice(&pixel_u32.to_ne_bytes());
        }
    } else {
        // Slow path: stride != width*4, fill row by row.
        let row_bytes = (width * 4) as usize;
        let stride_bytes = stride as usize;
        for row in 0..height as usize {
            let start = row * stride_bytes;
            if let Some(row_slice) = buf.get_mut(start..start + row_bytes) {
                for chunk in row_slice.chunks_exact_mut(4) {
                    chunk.copy_from_slice(&pixel_u32.to_ne_bytes());
                }
            }
        }
    }
}

/// Clear only the pixel rows that belong to dirty terminal rows.
pub fn clear_dirty_rows(
    buf: &mut [u8],
    width: u32,
    stride: u32,
    grid: &GridMetrics,
    dirty_indices: &[u16],
    bg: Bgra,
    opacity: f32,
) {
    let (bg_r, bg_g, bg_b) = bg;
    let bg_alpha = opacity_to_alpha(opacity);
    let pixel_u32 = u32::from_ne_bytes([bg_b, bg_g, bg_r, bg_alpha]);
    let stride_bytes = stride as usize;
    let row_bytes = (width * 4) as usize;

    for &row_idx in dirty_indices {
        let py_start = u32::from(row_idx) * grid.cell_h + grid.pad;
        let py_end = py_start + grid.cell_h;
        for py in py_start..py_end {
            let start = py as usize * stride_bytes;
            if let Some(row_slice) = buf.get_mut(start..start + row_bytes) {
                let (prefix, aligned, suffix) = unsafe { row_slice.align_to_mut::<u32>() };
                for b in prefix.chunks_exact_mut(4) {
                    b.copy_from_slice(&pixel_u32.to_ne_bytes());
                }
                aligned.fill(pixel_u32);
                for b in suffix.chunks_exact_mut(4) {
                    b.copy_from_slice(&pixel_u32.to_ne_bytes());
                }
            }
        }
    }
}
