use super::glyph::draw_glyph;
use super::primitives::{draw_rect, draw_rect_alpha};
use super::{Bgra, GridMetrics, Rect, RenderOptions, SearchHighlight, SelectionRange, Surface};
use crate::font::FontManager;
use crate::num::{f64_to_u32_saturating, format_usize, u32_from_u64_saturating};
use crate::terminal::render::{RenderColors, RenderState};

// Scrollbar geometry
const SCROLLBAR_WIDTH: u32 = 6;
const SCROLLBAR_MARGIN: u32 = 2;
const SCROLLBAR_MIN_THUMB: u32 = 10;
const SCROLLBAR_COLOR: Bgra = (200, 200, 200);
const SCROLLBAR_ALPHA: u8 = 128;

// Selection
const SELECTION_DEFAULT_COLOR: Bgra = (100, 150, 255);
const SELECTION_CUSTOM_ALPHA: u8 = 180;
const SELECTION_DEFAULT_ALPHA: u8 = 80;

// Search highlights
const SEARCH_MATCH_COLOR: Bgra = (255, 255, 0);
const SEARCH_MATCH_ALPHA: u8 = 60;
const SEARCH_CURRENT_COLOR: Bgra = (255, 165, 0);
const SEARCH_CURRENT_ALPHA: u8 = 100;

// Search bar
const SEARCH_BAR_PADDING: u32 = 4;
const SEARCH_BAR_BG: Bgra = (40, 40, 40);
const SEARCH_BAR_BG_ALPHA: u8 = 230;
const SEARCH_CURSOR_WIDTH: u32 = 2;

// IME preedit underline
const PREEDIT_UNDERLINE_HEIGHT: u32 = 2;

/// Draw all overlay elements (selection, search, cursor, scrollbar, preedit, search bar).
pub(super) fn draw_overlays(
    surface: &mut Surface<'_>,
    grid: &GridMetrics,
    render_state: &mut RenderState,
    colors: &RenderColors,
    opts: &RenderOptions<'_>,
    font: &mut FontManager,
    cursor_wide: bool,
) {
    if let Some(((sc, sr), (ec, er))) = opts.selection {
        let sel = SelectionRange {
            start_col: sc,
            start_row: sr,
            end_col: ec,
            end_row: er,
        };
        draw_selection(
            surface,
            grid,
            &sel,
            render_state,
            (opts.selection_fg, opts.selection_bg),
        );
    }
    if !opts.search_highlights.is_empty() {
        draw_search_highlights(surface, grid, opts.search_highlights);
    }
    let cursor_info = render_state.cursor();
    super::cursor::draw_cursor(
        surface,
        grid,
        render_state,
        colors,
        opts.cursor_blink_visible,
        cursor_wide,
    );
    draw_scrollbar(surface, opts.scrollbar);
    if let Some(preedit) = opts.preedit {
        draw_preedit(
            surface,
            grid,
            font,
            (cursor_info.x, cursor_info.y),
            preedit,
            colors,
        );
    }
    if let Some(query) = opts.search_bar {
        draw_search_bar(
            surface,
            grid,
            font,
            query,
            opts.search_highlights.len(),
            opts.search_cursor,
        );
    }
}

/// Draw the selection highlight overlay.
pub(super) fn draw_selection(
    surface: &mut Surface<'_>,
    grid: &GridMetrics,
    sel: &SelectionRange,
    render_state: &RenderState,
    selection_colors: (Option<Bgra>, Option<Bgra>),
) {
    let (cols, _rows) = render_state.dimensions();
    let sel_color: Bgra = selection_colors.1.unwrap_or(SELECTION_DEFAULT_COLOR);
    let _ = selection_colors.0; // foreground only used if we re-render text (future)

    for row in sel.start_row..=sel.end_row {
        let (left, right) = if sel.start_row == sel.end_row {
            (sel.start_col, sel.end_col)
        } else if row == sel.start_row {
            (sel.start_col, cols.saturating_sub(1))
        } else if row == sel.end_row {
            (0, sel.end_col)
        } else {
            (0, cols.saturating_sub(1))
        };

        let rx = u32::from(left) * grid.cell_w + grid.pad;
        let ry = u32::from(row) * grid.cell_h + grid.pad;
        let sel_w = (u32::from(right) - u32::from(left) + 1) * grid.cell_w;

        let rect = Rect {
            x: rx,
            y: ry,
            w: sel_w,
            h: grid.cell_h,
        };
        let alpha = if selection_colors.1.is_some() {
            SELECTION_CUSTOM_ALPHA
        } else {
            SELECTION_DEFAULT_ALPHA
        };
        draw_rect_alpha(surface, &rect, sel_color, alpha);
    }
}

/// Draw the scrollbar thumb.
pub(super) fn draw_scrollbar(
    surface: &mut Surface<'_>,
    scrollbar: Option<&libghostty_vt::ffi::GhosttyTerminalScrollbar>,
) {
    let Some(sb) = scrollbar else { return };
    if sb.total <= sb.len {
        return;
    }
    // Auto-hide: only show scrollbar when scrolled away from bottom
    if sb.offset + sb.len >= sb.total {
        return;
    }

    let bar_x = surface
        .width
        .saturating_sub(SCROLLBAR_WIDTH + SCROLLBAR_MARGIN);

    // visible_frac: ratio of visible area to total. Both are u64;
    // for row counts the values are always well below 2^53 so f64
    // provides sufficient precision. We truncate explicitly to u32
    // after clamping the result to the valid range.
    let total_f = u32_from_u64_saturating(sb.total);
    let len_f = u32_from_u64_saturating(sb.len);
    let visible_frac = f64::from(len_f) / f64::from(total_f);

    // thumb_height: fraction of surface height, clamped to minimum.
    let raw_thumb = f64::from(surface.height) * visible_frac;
    let mut thumb_height = f64_to_u32_saturating(raw_thumb);
    if thumb_height < SCROLLBAR_MIN_THUMB {
        thumb_height = SCROLLBAR_MIN_THUMB;
    }

    let scroll_frac = if sb.total > sb.len {
        let offset_clamped = u32_from_u64_saturating(sb.offset);
        let range_clamped = u32_from_u64_saturating(sb.total - sb.len);
        f64::from(offset_clamped) / f64::from(range_clamped)
    } else {
        1.0
    };

    let available = surface.height.saturating_sub(thumb_height);
    let thumb_y = f64_to_u32_saturating(scroll_frac * f64::from(available));

    let rect = Rect {
        x: bar_x,
        y: thumb_y,
        w: SCROLLBAR_WIDTH,
        h: thumb_height,
    };
    draw_rect_alpha(surface, &rect, SCROLLBAR_COLOR, SCROLLBAR_ALPHA);
}

pub(super) fn draw_search_highlights(
    surface: &mut Surface<'_>,
    grid: &GridMetrics,
    highlights: &[SearchHighlight],
) {
    for hl in highlights {
        let y = u32::from(hl.row) * grid.cell_h + grid.pad;
        let x_start = u32::from(hl.start_col) * grid.cell_w + grid.pad;
        let x_end = (u32::from(hl.end_col) + 1) * grid.cell_w + grid.pad;
        let w = x_end.saturating_sub(x_start);
        let rect = Rect {
            x: x_start,
            y,
            w,
            h: grid.cell_h,
        };
        // Yellow overlay for matches, orange for current match
        let (color, alpha) = if hl.is_current {
            (SEARCH_CURRENT_COLOR, SEARCH_CURRENT_ALPHA)
        } else {
            (SEARCH_MATCH_COLOR, SEARCH_MATCH_ALPHA)
        };
        draw_rect_alpha(surface, &rect, color, alpha);
    }
}

fn draw_preedit(
    surface: &mut Surface<'_>,
    grid: &GridMetrics,
    font: &mut FontManager,
    cursor_pos: (u16, u16),
    preedit: &str,
    colors: &RenderColors,
) {
    if preedit.is_empty() {
        return;
    }
    let col_start = u32::from(cursor_pos.0);
    let row = u32::from(cursor_pos.1);
    let fg = colors.foreground;
    let bg = colors.background;

    let mut preedit_buf = [0u8; 4];
    for (i, ch) in preedit.chars().enumerate() {
        let col = col_start + u32::try_from(i).unwrap_or(u32::MAX);
        let x = grid.pad + col * grid.cell_w;
        let y = grid.pad + row * grid.cell_h;

        // Draw inverted background for preedit cells
        let cell_rect = Rect {
            x,
            y,
            w: grid.cell_w,
            h: grid.cell_h,
        };
        draw_rect(surface, &cell_rect, fg);

        // Rasterize and draw using same baseline as normal text
        let text = ch.encode_utf8(&mut preedit_buf);
        if let Some(glyph) = font.rasterize(text, false, false) {
            draw_glyph(surface, x, y, grid.cell_h, glyph, bg);
        }

        // Draw underline at the bottom of the cell
        let underline_rect = Rect {
            x,
            y: y + grid.cell_h - PREEDIT_UNDERLINE_HEIGHT,
            w: grid.cell_w,
            h: PREEDIT_UNDERLINE_HEIGHT,
        };
        draw_rect(surface, &underline_rect, bg);
    }
}

/// Render a single character for the search bar and advance the x cursor.
fn search_bar_char(
    ch: char,
    cx: &mut u32,
    cy: u32,
    utf8_buf: &mut [u8; 4],
    surf: &mut Surface<'_>,
    fm: &mut FontManager,
    grid: &GridMetrics,
) {
    let s = ch.encode_utf8(utf8_buf);
    if let Some(glyph) = fm.rasterize(s, false, false) {
        draw_glyph(surf, *cx, cy, grid.cell_h, glyph, (255, 255, 255));
    }
    *cx += grid.cell_w;
}

fn draw_search_bar(
    surface: &mut Surface<'_>,
    grid: &GridMetrics,
    font: &mut FontManager,
    query: &str,
    match_count: usize,
    cursor_byte_pos: usize,
) {
    // Draw a bar at the very bottom of the surface
    let bar_height = grid.cell_h + SEARCH_BAR_PADDING;
    let bar_y = surface.height.saturating_sub(bar_height);
    let bar_rect = Rect {
        x: 0,
        y: bar_y,
        w: surface.width,
        h: bar_height,
    };
    draw_rect_alpha(surface, &bar_rect, SEARCH_BAR_BG, SEARCH_BAR_BG_ALPHA);

    // Render characters directly without intermediate String allocation.
    let text_y = bar_y + 2;
    let mut text_x = grid.pad.max(SEARCH_BAR_PADDING);
    let mut char_buf = [0u8; 4];

    // "Search: " prefix
    for ch in "Search: ".chars() {
        search_bar_char(ch, &mut text_x, text_y, &mut char_buf, surface, font, grid);
    }
    // Query text with cursor tracking
    let mut byte_offset = 0;
    let mut cursor_x = text_x; // x position of cursor (before any query chars)
    for ch in query.chars() {
        if byte_offset == cursor_byte_pos {
            cursor_x = text_x;
        }
        search_bar_char(ch, &mut text_x, text_y, &mut char_buf, surface, font, grid);
        byte_offset += ch.len_utf8();
    }
    // If cursor is at end of query
    if byte_offset == cursor_byte_pos {
        cursor_x = text_x;
    }
    // Draw cursor (thin vertical line)
    let cursor_rect = Rect {
        x: cursor_x,
        y: text_y,
        w: SEARCH_CURSOR_WIDTH,
        h: grid.cell_h,
    };
    draw_rect_alpha(surface, &cursor_rect, (220, 220, 220), 255);

    // Suffix with match count (only when query is non-empty)
    if !query.is_empty() {
        for ch in "  (".chars() {
            search_bar_char(ch, &mut text_x, text_y, &mut char_buf, surface, font, grid);
        }
        let mut num_buf = [0u8; 20];
        let num_str = format_usize(match_count, &mut num_buf);
        for ch in num_str.chars() {
            search_bar_char(ch, &mut text_x, text_y, &mut char_buf, surface, font, grid);
        }
        for ch in " matches)".chars() {
            search_bar_char(ch, &mut text_x, text_y, &mut char_buf, surface, font, grid);
        }
    }
}
