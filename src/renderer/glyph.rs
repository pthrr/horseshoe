use super::{Bgra, Rect, Surface};
use crate::font::GlyphImage;

/// Draw a box-drawing glyph directly at cell position (no baseline offset).
#[expect(clippy::cast_possible_truncation, reason = "blend result proven ≤255")]
pub(super) fn draw_box_glyph(
    surface: &mut Surface<'_>,
    rect: &Rect,
    alpha_buf: &[u8],
    fg_color: Bgra,
) {
    let row_end = rect.h.min(surface.height.saturating_sub(rect.y));
    let col_end = rect.w.min(surface.width.saturating_sub(rect.x));
    if row_end == 0 || col_end == 0 {
        return;
    }
    let fg_opaque = [fg_color.2, fg_color.1, fg_color.0, 0xFF];
    let fg_blue = u32::from(fg_color.2);
    let fg_green = u32::from(fg_color.1);
    let fg_red = u32::from(fg_color.0);
    let stride = surface.stride as usize;
    let rect_x = rect.x as usize;
    let rect_w = rect.w as usize;

    for row in 0..row_end {
        let py = (rect.y + row) as usize;
        let buf_row_base = py * stride + rect_x * 4;
        let alpha_row_base = row as usize * rect_w;

        for col in 0..col_end as usize {
            // SAFETY: row < row_end ≤ rect.h and col < col_end ≤ rect.w,
            // so alpha_row_base + col < rect.h * rect.w ≤ alpha_buf.len().
            let alpha_val = unsafe { *alpha_buf.get_unchecked(alpha_row_base + col) };
            if alpha_val == 0 {
                continue;
            }
            // SAFETY: py = rect.y + row < rect.y + row_end ≤ surface.height,
            // and rect_x + col < rect.x + col_end ≤ surface.width,
            // so offset + 4 ≤ (py + 1) * stride ≤ buf.len().
            let offset = buf_row_base + col * 4;
            let pixel = unsafe { surface.buf.get_unchecked_mut(offset..offset + 4) };
            if alpha_val == 255 {
                pixel.copy_from_slice(&fg_opaque);
            } else if let [b_ch, g_ch, r_ch, a_ch] = pixel {
                let a32 = u32::from(alpha_val);
                let inv32 = 255 - a32;
                *b_ch = ((((fg_blue * a32 + u32::from(*b_ch) * inv32) + 1) * 257) >> 16) as u8;
                *g_ch = ((((fg_green * a32 + u32::from(*g_ch) * inv32) + 1) * 257) >> 16) as u8;
                *r_ch = ((((fg_red * a32 + u32::from(*r_ch) * inv32) + 1) * 257) >> 16) as u8;
                *a_ch = 0xFF;
            }
        }
    }
}

#[expect(clippy::cast_possible_truncation, reason = "blend result proven ≤255")]
pub(super) fn draw_glyph(
    surface: &mut Surface<'_>,
    cell_x: u32,
    cell_y: u32,
    cell_h: u32,
    glyph: &GlyphImage,
    fg_color: Bgra,
) {
    // Position the glyph within the cell using the placement offsets.
    // top is typically the distance from the baseline to the top of the glyph.
    // We place the baseline at roughly 80% of the cell height.
    let x_signed = i32::try_from(cell_x).expect("cell_x fits in i32");
    let y_signed = i32::try_from(cell_y).expect("cell_y fits in i32");
    let h_signed = i32::try_from(cell_h).expect("cell_h fits in i32");
    let baseline = y_signed + (h_signed * 4 / 5);
    let gx0 = x_signed + glyph.left;
    let gy0 = baseline - glyph.top;

    let gw = i32::try_from(glyph.width).expect("glyph width fits in i32");
    let gh = i32::try_from(glyph.height).expect("glyph height fits in i32");
    let sw = i32::try_from(surface.width).expect("width fits in i32");
    let sh = i32::try_from(surface.height).expect("height fits in i32");

    // Pre-clamp iteration range to surface bounds.
    // row_start/row_end select the visible portion of the glyph:
    //   row_start = max(0, -gy0): skip rows above the surface
    //   row_end   = min(gh, sh - gy0): stop at surface bottom
    // Screen Y for glyph row `row` = gy0 + row, which is in [0, sh).
    let row_start = usize::try_from(0i32.max(-gy0)).unwrap_or(0);
    let row_end = usize::try_from(gh.min(sh - gy0)).unwrap_or(0);
    let col_start = usize::try_from(0i32.max(-gx0)).unwrap_or(0);
    let col_end = usize::try_from(gw.min(sw - gx0)).unwrap_or(0);
    if row_start >= row_end || col_start >= col_end {
        return;
    }

    let fg_opaque = [fg_color.2, fg_color.1, fg_color.0, 0xFF];
    let fg_blue = u32::from(fg_color.2);
    let fg_green = u32::from(fg_color.1);
    let fg_red = u32::from(fg_color.0);
    let glyph_w = usize::try_from(gw).unwrap_or(0);
    let stride_bytes = surface.stride as usize;
    // origin_row/col = max(0, gy0/gx0). Combined with row - row_start,
    // this computes the correct screen coordinate: gy0 + row.
    let origin_row = usize::try_from(gy0.max(0)).unwrap_or(0);
    let origin_col = usize::try_from(gx0.max(0)).unwrap_or(0);

    for row in row_start..row_end {
        // Screen Y = gy0 + row = origin_row + (row - row_start).
        let py = origin_row + row - row_start;
        let alpha_row_base = row * glyph_w;
        let buf_row_base = py * stride_bytes;

        for col in col_start..col_end {
            // SAFETY: row < row_end ≤ gh and col < col_end ≤ gw,
            // so alpha_row_base + col = row * gw + col < gh * gw = alpha.len().
            let alpha_val = unsafe { *glyph.alpha.get_unchecked(alpha_row_base + col) };
            if alpha_val == 0 {
                continue;
            }

            // Screen X = gx0 + col = origin_col + (col - col_start).
            let px = origin_col + col - col_start;
            // SAFETY: py ∈ [0, sh) and px ∈ [0, sw) by construction of
            // row_start/row_end/col_start/col_end. offset + 4 ≤ (py+1) * stride ≤ buf.len().
            let offset = buf_row_base + px * 4;
            let pixel = unsafe { surface.buf.get_unchecked_mut(offset..offset + 4) };

            if alpha_val == 255 {
                pixel.copy_from_slice(&fg_opaque);
            } else if let [b_ch, g_ch, r_ch, a_ch] = pixel {
                let a32 = u32::from(alpha_val);
                let inv32 = 255 - a32;
                *b_ch = ((((fg_blue * a32 + u32::from(*b_ch) * inv32) + 1) * 257) >> 16) as u8;
                *g_ch = ((((fg_green * a32 + u32::from(*g_ch) * inv32) + 1) * 257) >> 16) as u8;
                *r_ch = ((((fg_red * a32 + u32::from(*r_ch) * inv32) + 1) * 257) >> 16) as u8;
                *a_ch = 0xFF;
            }
        }
    }
}
