use super::cell_style::{draw_cell_decorations, draw_cell_text, resolve_cell_colors};
use super::{CellTextParams, FrameContext, Rect, Surface};
use crate::font::FontManager;
use crate::terminal::render::RenderState;
use libghostty_vt::ffi;

/// Render cells into the buffer. When `dirty_only` is true, only dirty rows
/// are processed; otherwise all rows are rendered.
/// Returns true if the cell under the cursor is a wide character.
pub(super) fn draw_cells_impl(
    target: &mut Surface<'_>,
    render_state: &mut RenderState,
    font: &mut FontManager,
    ctx: &FrameContext<'_>,
    dirty_only: bool,
) -> bool {
    let buf = &mut *target.buf;
    let width = target.width;
    let height = target.height;
    let stride = target.stride;
    let bold_is_bright = ctx.bold_is_bright;
    let cursor_blink_visible = ctx.cursor_blink_visible;
    let cell_w = ctx.grid.cell_w;
    let cell_h = ctx.grid.cell_h;
    let pad = ctx.grid.pad;
    let colors = ctx.colors;
    let cursor_col = usize::from(ctx.cursor_pos.0);
    let cursor_row = usize::from(ctx.cursor_pos.1);
    let cursor_wide = std::cell::Cell::new(false);

    let mut callback = |row: usize,
                        col: usize,
                        codepoints: &[u32],
                        style: &crate::terminal::render::CellStyle,
                        is_wide: bool| {
        if is_wide && col == cursor_col && row == cursor_row {
            cursor_wide.set(true);
        }

        let origin_x = u32::try_from(col).unwrap_or(0) * cell_w + pad;
        let origin_y = u32::try_from(row).unwrap_or(0) * cell_h + pad;
        let span_w = if is_wide { cell_w * 2 } else { cell_w };

        let (fg_color, bg_color) = resolve_cell_colors(style, colors, bold_is_bright);

        if style.bg_tag != ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE
            || style.attrs.inverse()
        {
            let mut surface = Surface {
                buf,
                width,
                height,
                stride,
            };
            let rect = Rect {
                x: origin_x,
                y: origin_y,
                w: span_w,
                h: cell_h,
            };
            super::primitives::draw_rect(&mut surface, &rect, bg_color);
        }

        if !codepoints.is_empty()
            && !style.attrs.invisible()
            && (!style.attrs.blink() || cursor_blink_visible)
        {
            let params = CellTextParams {
                surface: Surface {
                    buf,
                    width,
                    height,
                    stride,
                },
                origin: Rect {
                    x: origin_x,
                    y: origin_y,
                    w: span_w,
                    h: cell_h,
                },
                codepoints,
                style,
                font,
                fg_color,
            };
            draw_cell_text(params);
        }

        let mut surface = Surface {
            buf,
            width,
            height,
            stride,
        };
        let deco_area = Rect {
            x: origin_x,
            y: origin_y,
            w: span_w,
            h: cell_h,
        };
        draw_cell_decorations(&mut surface, style, colors, &deco_area, fg_color);
    };

    render_state.for_each_cell_filtered(dirty_only, &mut callback);
    cursor_wide.get()
}
