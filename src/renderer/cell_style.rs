use super::glyph::{draw_box_glyph, draw_glyph};
use super::primitives::{draw_rect, set_pixel};
use super::{Bgra, CellTextParams, Rect, Surface};
use crate::boxdraw;
use crate::terminal::render::{RenderColors, resolve_color};
use libghostty_vt::ffi;

#[inline]
pub(super) fn resolve_cell_colors(
    style: &crate::terminal::render::CellStyle,
    colors: &RenderColors,
    bold_is_bright: bool,
) -> (Bgra, Bgra) {
    let mut fg_color = resolve_color(
        style.fg_tag,
        style.fg_palette,
        style.fg_rgb,
        colors,
        colors.foreground,
    );
    let mut bg_color = resolve_color(
        style.bg_tag,
        style.bg_palette,
        style.bg_rgb,
        colors,
        colors.background,
    );

    // Handle inverse
    if style.attrs.inverse() {
        std::mem::swap(&mut fg_color, &mut bg_color);
    }

    // Bold as bright: remap ANSI colors 0-7 to bright 8-15
    if bold_is_bright
        && style.attrs.bold()
        && style.fg_tag == ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE
        && style.fg_palette < 8
        && let Some(&bright) = colors.palette.get(usize::from(style.fg_palette) + 8)
    {
        fg_color = bright;
    }

    // Handle faint
    if style.attrs.faint() {
        fg_color = (fg_color.0 >> 1, fg_color.1 >> 1, fg_color.2 >> 1);
    }

    (fg_color, bg_color)
}

pub(super) fn draw_cell_decorations(
    surface: &mut Surface<'_>,
    style: &crate::terminal::render::CellStyle,
    colors: &RenderColors,
    area: &Rect,
    fg_color: Bgra,
) {
    if style.underline > 0 {
        let ul_color = resolve_color(
            style.underline_color_tag,
            style.underline_color_palette,
            style.underline_color_rgb,
            colors,
            fg_color,
        );
        draw_underline(
            surface,
            area.x,
            area.y,
            area.w,
            area.h,
            style.underline,
            ul_color,
        );
    }

    if style.attrs.strikethrough() {
        let rect = Rect {
            x: area.x,
            y: area.y + area.h / 2,
            w: area.w,
            h: 1,
        };
        draw_rect(surface, &rect, fg_color);
    }

    if style.attrs.overline() {
        let rect = Rect {
            x: area.x,
            y: area.y,
            w: area.w,
            h: 1,
        };
        draw_rect(surface, &rect, fg_color);
    }
}

pub(super) fn draw_cell_text(params: CellTextParams<'_, '_>) {
    let CellTextParams {
        mut surface,
        origin,
        codepoints,
        style,
        font,
        fg_color,
    } = params;

    // Check for box drawing / block elements (procedural, pixel-perfect)
    if let Some(&first_cp) = codepoints.first()
        && codepoints.len() == 1
        && boxdraw::is_box_drawing(first_cp)
    {
        // Stack-allocate the alpha buffer (max 64x64 = 4096 bytes).
        let mut stack_alpha = [0u8; 4096];
        let needed = (origin.w as usize) * (origin.h as usize);
        if needed <= stack_alpha.len()
            && boxdraw::draw_box_char_into(first_cp, origin.w, origin.h, &mut stack_alpha)
            && let Some(alpha_slice) = stack_alpha.get(..needed)
        {
            draw_box_glyph(&mut surface, &origin, alpha_slice, fg_color);
        }
        return;
    }

    // Normal text rendering via font.
    // Fast path: single codepoint (common case) avoids heap allocation.
    let mut stack_buf = [0u8; 4];
    let text: &str = if codepoints.len() == 1 {
        if let Some(&cp) = codepoints.first() {
            if let Some(ch) = char::from_u32(cp) {
                ch.encode_utf8(&mut stack_buf)
            } else {
                return;
            }
        } else {
            return;
        }
    } else {
        let s: String = codepoints
            .iter()
            .filter_map(|&cp| char::from_u32(cp))
            .collect();
        if s.is_empty() {
            return;
        }
        if let Some(glyph) = font.rasterize(&s, style.attrs.bold(), style.attrs.italic()) {
            draw_glyph(&mut surface, origin.x, origin.y, origin.h, glyph, fg_color);
        }
        return;
    };

    if let Some(glyph) = font.rasterize(text, style.attrs.bold(), style.attrs.italic()) {
        draw_glyph(&mut surface, origin.x, origin.y, origin.h, glyph, fg_color);
    }
}

pub(super) fn draw_underline(
    surface: &mut Surface<'_>,
    px: u32,
    py: u32,
    cell_w: u32,
    cell_h: u32,
    variant: i32,
    color: Bgra,
) {
    if variant == 0 {
        return;
    }
    let ul_y = py + cell_h.saturating_sub(2);
    match variant {
        1 => {
            // Single underline
            let rect = Rect {
                x: px,
                y: ul_y,
                w: cell_w,
                h: 1,
            };
            draw_rect(surface, &rect, color);
        }
        2 if cell_h >= 3 => {
            // Double underline (needs at least 3px cell height for both lines)
            let rect_top = Rect {
                x: px,
                y: ul_y.saturating_sub(1),
                w: cell_w,
                h: 1,
            };
            draw_rect(surface, &rect_top, color);
            let rect_bot = Rect {
                x: px,
                y: ul_y + 1,
                w: cell_w,
                h: 1,
            };
            draw_rect(surface, &rect_bot, color);
        }
        2 => {
            // Double underline fallback for tiny cells
            let rect = Rect {
                x: px,
                y: ul_y,
                w: cell_w,
                h: 1,
            };
            draw_rect(surface, &rect, color);
        }
        3 => {
            // Curly underline (approximated with wavy pattern)
            for x_off in 0..cell_w {
                let wave = u32::from((x_off / 2) % 2 != 0);
                let wy = ul_y.saturating_add(wave);
                set_pixel(surface, px + x_off, wy, color);
            }
        }
        4 => {
            // Dotted underline
            for x_off in (0..cell_w).step_by(2) {
                set_pixel(surface, px + x_off, ul_y, color);
            }
        }
        5 => {
            // Dashed underline
            for x_off in 0..cell_w {
                if (x_off / 3) % 2 == 0 {
                    set_pixel(surface, px + x_off, ul_y, color);
                }
            }
        }
        _ => {
            let rect = Rect {
                x: px,
                y: ul_y,
                w: cell_w,
                h: 1,
            };
            draw_rect(surface, &rect, color);
        }
    }
}
