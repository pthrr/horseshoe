mod background;
mod cell_style;
mod cells;
mod cursor;
mod glyph;
mod overlay;
mod primitives;

pub use background::{clear_background, clear_dirty_rows};
pub use primitives::{blend_channel, draw_rect, draw_rect_alpha, set_pixel};

use crate::font::FontManager;
use crate::terminal::render::{RenderColors, RenderState};
use libghostty_vt::ffi;

static RENDER_PROFILE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var_os("HAND_PROFILE").is_some());

/// RGB color tuple.
pub(super) type Bgra = (u8, u8, u8);

/// Shared reference to the pixel buffer and its dimensions.
pub struct Surface<'a> {
    pub buf: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

/// Axis-aligned rectangle within the surface.
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Cell grid metrics: cell size and padding.
pub struct GridMetrics {
    pub cell_w: u32,
    pub cell_h: u32,
    pub pad: u32,
}

/// Normalized selection range.
pub(super) struct SelectionRange {
    pub(super) start_col: u16,
    pub(super) start_row: u16,
    pub(super) end_col: u16,
    pub(super) end_row: u16,
}

/// Parameters for rendering the text content of a single cell.
pub(super) struct CellTextParams<'a, 'b> {
    pub(super) surface: Surface<'a>,
    pub(super) origin: Rect,
    pub(super) codepoints: &'b [u32],
    pub(super) style: &'b crate::terminal::render::CellStyle,
    pub(super) font: &'b mut FontManager,
    pub(super) fg_color: Bgra,
}

/// Immutable rendering context shared across all cells in a frame.
pub(super) struct FrameContext<'a> {
    pub(super) grid: &'a GridMetrics,
    pub(super) colors: &'a RenderColors,
    pub(super) bold_is_bright: bool,
    pub(super) cursor_blink_visible: bool,
    pub(super) cursor_pos: (u16, u16),
}

/// A search match highlight region.
#[derive(Debug, Clone, Copy)]
pub struct SearchHighlight {
    pub row: u16,
    pub start_col: u16,
    pub end_col: u16,
    pub is_current: bool,
}

/// Options that control rendering behavior.
pub struct RenderOptions<'a> {
    pub scrollbar: Option<&'a ffi::GhosttyTerminalScrollbar>,
    pub bold_is_bright: bool,
    pub cursor_blink_visible: bool,
    /// Selection range as `((start_col, start_row), (end_col, end_row))`, normalized.
    pub selection: Option<((u16, u16), (u16, u16))>,
    /// Padding in pixels around the terminal grid.
    pub padding: u32,
    /// Background opacity (0.0 = fully transparent, 1.0 = fully opaque).
    pub opacity: f32,
    /// Search match highlights to draw.
    pub search_highlights: &'a [SearchHighlight],
    /// Search bar text to draw at the bottom (None if search inactive).
    pub search_bar: Option<&'a str>,
    /// Byte position of cursor within search bar query (after "Search: " prefix).
    pub search_cursor: usize,
    /// IME preedit text to draw at the cursor position (None if no active composition).
    pub preedit: Option<&'a str>,
    /// Custom selection foreground color (None = use inversion).
    pub selection_fg: Option<Bgra>,
    /// Custom selection background color (None = use default blue overlay).
    pub selection_bg: Option<Bgra>,
}

/// Output target for the renderer: SHM canvas + retained buffer.
pub struct RenderTarget<'a> {
    pub buf: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub retained: &'a mut Vec<u8>,
}

/// Render the terminal state into a retained buffer, then copy to the
/// SHM output buffer.  Only dirty rows are re-rendered; clean rows are
/// preserved from the previous frame in `retained`.  Overlays (cursor,
/// selection, scrollbar, etc.) are drawn on the SHM buffer only, so
/// they never pollute the retained cell content.
pub fn render_frame(
    target: &mut RenderTarget<'_>,
    render_state: &mut RenderState,
    font: &mut FontManager,
    opts: &RenderOptions<'_>,
    colors: &RenderColors,
) {
    let width = target.width;
    let height = target.height;
    let stride = target.stride;
    let buf_len = target.buf.len();
    let grid = GridMetrics {
        cell_w: font.cell_width,
        cell_h: font.cell_height,
        pad: opts.padding,
    };

    let dirty = render_state.dirty();
    let size_changed = target.retained.len() != buf_len;
    let full_redraw =
        size_changed || dirty == ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_FULL;

    // Update retained buffer with cell content only (no overlays).
    let cursor_wide = if full_redraw {
        target.retained.resize(buf_len, 0);
        clear_background(
            target.retained,
            width,
            height,
            stride,
            colors.background,
            opts.opacity,
        );
        let cursor = render_state.cursor();
        let ctx = FrameContext {
            grid: &grid,
            colors,
            bold_is_bright: opts.bold_is_bright,
            cursor_blink_visible: opts.cursor_blink_visible,
            cursor_pos: (cursor.x, cursor.y),
        };
        let mut s = Surface {
            buf: target.retained,
            width,
            height,
            stride,
        };
        cells::draw_cells_impl(&mut s, render_state, font, &ctx, false)
    } else if dirty == ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_PARTIAL {
        let dirty_rows = render_state.dirty_row_indices(u16::MAX);
        clear_dirty_rows(
            target.retained,
            width,
            stride,
            &grid,
            dirty_rows.as_slice(),
            colors.background,
            opts.opacity,
        );
        let cursor = render_state.cursor();
        let ctx = FrameContext {
            grid: &grid,
            colors,
            bold_is_bright: opts.bold_is_bright,
            cursor_blink_visible: opts.cursor_blink_visible,
            cursor_pos: (cursor.x, cursor.y),
        };
        let mut s = Surface {
            buf: target.retained,
            width,
            height,
            stride,
        };
        cells::draw_cells_impl(&mut s, render_state, font, &ctx, true)
    } else {
        false
    };

    // Copy retained → SHM (full copy — SHM buffer is fresh each frame).
    let t0 = if *RENDER_PROFILE {
        Some(std::time::Instant::now())
    } else {
        None
    };
    copy_retained_to_shm(target);
    let t1 = if *RENDER_PROFILE {
        Some(std::time::Instant::now())
    } else {
        None
    };

    // Overlays drawn on SHM only.
    let mut surface = Surface {
        buf: target.buf,
        width,
        height,
        stride,
    };
    overlay::draw_overlays(
        &mut surface,
        &grid,
        render_state,
        colors,
        opts,
        font,
        cursor_wide,
    );

    if let (Some(copy_start), Some(copy_end)) = (t0, t1) {
        let overlay_end = std::time::Instant::now();
        log_render_profile(dirty, copy_start, copy_end, overlay_end, buf_len);
    }
}

fn log_render_profile(
    dirty: u32,
    t0: std::time::Instant,
    t1: std::time::Instant,
    t2: std::time::Instant,
    buf_len: usize,
) {
    if *RENDER_PROFILE {
        eprintln!(
            "[profile] render: cells={} copy={:.1}ms overlay={:.1}ms buf={}KB",
            match dirty {
                x if x == ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_FULL => "full",
                x if x == ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_PARTIAL =>
                    "partial",
                _ => "none",
            },
            t1.duration_since(t0).as_secs_f64() * 1000.0,
            t2.duration_since(t1).as_secs_f64() * 1000.0,
            buf_len / 1024,
        );
    }
}

/// Copy retained buffer to SHM.
///
/// The SHM buffer is freshly allocated from `SlotPool` each frame, so it
/// does NOT contain the previous frame's content.  We must always copy the
/// full retained buffer — selective/range-based copy is not possible here.
fn copy_retained_to_shm(target: &mut RenderTarget<'_>) {
    let shm_len = target.buf.len();
    let ret_len = target.retained.len();
    if shm_len != ret_len {
        // Length mismatch can happen transiently during resize if the
        // retained buffer was sized for a previous frame.  Truncate the
        // copy to the shorter length and zero-fill any remainder in the
        // SHM buffer to avoid stale pixels.
        let copy_len = shm_len.min(ret_len);
        if let (Some(dst), Some(src)) = (
            target.buf.get_mut(..copy_len),
            target.retained.get(..copy_len),
        ) {
            dst.copy_from_slice(src);
        }
        if let Some(rest) = target.buf.get_mut(copy_len..) {
            for byte in rest {
                *byte = 0;
            }
        }
        return;
    }
    target.buf.copy_from_slice(target.retained);
}

#[cfg(test)]
mod tests;
