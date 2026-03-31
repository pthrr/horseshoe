use super::primitives::{draw_rect, draw_rect_alpha};
use super::{GridMetrics, Rect, Surface};
use crate::terminal::render::{CursorStyle, RenderColors, RenderState};

/// Draw the terminal cursor.
pub(super) fn draw_cursor(
    surface: &mut Surface<'_>,
    grid: &GridMetrics,
    render_state: &RenderState,
    colors: &RenderColors,
    blink_visible: bool,
    cursor_wide: bool,
) {
    let cursor = render_state.cursor();
    if !cursor.visible || !cursor.in_viewport {
        return;
    }
    // If cursor is blinking and currently in "off" phase, don't draw
    if cursor.blinking && !blink_visible {
        return;
    }
    let cx = u32::from(cursor.x) * grid.cell_w + grid.pad;
    let cy = u32::from(cursor.y) * grid.cell_h + grid.pad;
    let cursor_color = colors.cursor.unwrap_or(colors.foreground);
    let cursor_w = if cursor_wide {
        grid.cell_w * 2
    } else {
        grid.cell_w
    };

    match cursor.style {
        CursorStyle::Block => {
            let rect = Rect {
                x: cx,
                y: cy,
                w: cursor_w,
                h: grid.cell_h,
            };
            draw_rect_alpha(surface, &rect, cursor_color, 128);
        }
        CursorStyle::Bar => {
            let bar_w = (grid.cell_w / 8).max(2);
            let rect = Rect {
                x: cx,
                y: cy,
                w: bar_w,
                h: grid.cell_h,
            };
            draw_rect(surface, &rect, cursor_color);
        }
        CursorStyle::Underline => {
            let rect = Rect {
                x: cx,
                y: cy + grid.cell_h - 2,
                w: cursor_w,
                h: 2,
            };
            draw_rect(surface, &rect, cursor_color);
        }
        CursorStyle::BlockHollow => {
            let top = Rect {
                x: cx,
                y: cy,
                w: cursor_w,
                h: 1,
            };
            let bottom = Rect {
                x: cx,
                y: cy + grid.cell_h - 1,
                w: cursor_w,
                h: 1,
            };
            let left_side = Rect {
                x: cx,
                y: cy,
                w: 1,
                h: grid.cell_h,
            };
            let right_side = Rect {
                x: cx + cursor_w - 1,
                y: cy,
                w: 1,
                h: grid.cell_h,
            };
            draw_rect(surface, &top, cursor_color);
            draw_rect(surface, &bottom, cursor_color);
            draw_rect(surface, &left_side, cursor_color);
            draw_rect(surface, &right_side, cursor_color);
        }
    }
}
