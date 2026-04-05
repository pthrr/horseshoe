use super::background::opacity_to_alpha;
use super::cell_style::{draw_underline, resolve_cell_colors};
use super::glyph::{draw_box_glyph, draw_glyph};
use super::overlay::{draw_scrollbar, draw_search_highlights, draw_selection};
use super::*;
use crate::font::GlyphImage;
use crate::terminal::render::default_cell_style;

fn test_surface(w: u32, h: u32) -> (Vec<u8>, u32) {
    let stride = w * 4;
    let buf = vec![0u8; (stride * h) as usize];
    (buf, stride)
}

#[test]
fn test_set_pixel() {
    let (mut buf, stride) = test_surface(10, 10);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    set_pixel(&mut surface, 5, 5, (255, 128, 64));
    let offset = 5 * 40 + 5 * 4;
    assert_eq!(*surface.buf.get(offset).expect("B byte"), 64); // B
    assert_eq!(*surface.buf.get(offset + 1).expect("G byte"), 128); // G
    assert_eq!(*surface.buf.get(offset + 2).expect("R byte"), 255); // R
    assert_eq!(*surface.buf.get(offset + 3).expect("A byte"), 0xFF); // A
}

#[test]
fn test_draw_rect() {
    let (mut buf, stride) = test_surface(20, 20);
    let mut surface = Surface {
        buf: &mut buf,
        width: 20,
        height: 20,
        stride,
    };
    let rect = Rect {
        x: 2,
        y: 2,
        w: 3,
        h: 3,
    };
    draw_rect(&mut surface, &rect, (255, 0, 0));
    // Check a pixel inside the rect
    let offset = (2 * stride + 2 * 4) as usize;
    assert_eq!(*surface.buf.get(offset + 2).expect("R byte"), 255); // R component
}

#[test]
fn test_draw_rect_alpha() {
    let (mut buf, stride) = test_surface(10, 10);
    // Fill with white
    for chunk in buf.chunks_exact_mut(4) {
        if let [b_ch, g_ch, r_ch, a_ch] = chunk {
            *b_ch = 255;
            *g_ch = 255;
            *r_ch = 255;
            *a_ch = 255;
        }
    }
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    let rect = Rect {
        x: 0,
        y: 0,
        w: 1,
        h: 1,
    };
    draw_rect_alpha(&mut surface, &rect, (0, 0, 0), 128);
    // Should be approximately half brightness
    assert!(*surface.buf.first().expect("first byte") < 200); // blended towards black
}

#[test]
fn test_set_pixel_out_of_bounds() {
    let (mut buf, stride) = test_surface(10, 10);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    // These should not panic
    set_pixel(&mut surface, 100, 100, (255, 255, 255));
    set_pixel(&mut surface, 0, 100, (255, 255, 255));
}

#[test]
fn test_blend_channel_full_fg() {
    // alpha=255 → fg completely replaces bg
    assert_eq!(blend_channel(200, 50, 255), 200);
}

#[test]
fn test_blend_channel_zero_alpha() {
    // alpha=0 → bg unchanged
    assert_eq!(blend_channel(200, 50, 0), 50);
}

#[test]
fn test_blend_channel_half_alpha() {
    // alpha=128 → roughly midpoint
    let result = blend_channel(255, 0, 128);
    assert!(result > 100 && result < 160, "expected ~128, got {result}");
}

#[test]
fn test_blend_channel_boundary_values() {
    assert_eq!(blend_channel(0, 0, 0), 0);
    assert_eq!(blend_channel(255, 255, 255), 255);
    assert_eq!(blend_channel(0, 255, 0), 255);
    assert_eq!(blend_channel(255, 0, 255), 255);
}

#[test]
fn test_blend_channel_symmetry() {
    // blend(fg=100, bg=200, alpha=128) should be close to midpoint
    let r1 = blend_channel(100, 200, 128);
    let r2 = blend_channel(200, 100, 128);
    // These should sum to approximately 300 (but not exactly due to rounding)
    assert!((i32::from(r1) + i32::from(r2) - 300).abs() < 3);
}

#[test]
fn test_opacity_to_alpha_zero() {
    assert_eq!(opacity_to_alpha(0.0), 0);
}

#[test]
fn test_opacity_to_alpha_one() {
    assert_eq!(opacity_to_alpha(1.0), 255);
}

#[test]
fn test_opacity_to_alpha_half() {
    let result = opacity_to_alpha(0.5);
    assert!(result > 120 && result < 135, "expected ~128, got {result}");
}

#[test]
fn test_opacity_to_alpha_clamp_above() {
    assert_eq!(opacity_to_alpha(2.0), 255);
}

#[test]
fn test_opacity_to_alpha_clamp_below() {
    assert_eq!(opacity_to_alpha(-1.0), 0);
}

#[test]
fn test_set_pixel_first() {
    let (mut buf, _stride) = test_surface(10, 10);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride: 40,
    };
    set_pixel(&mut surface, 0, 0, (100, 200, 50));
    assert_eq!(*surface.buf.first().expect("B byte"), 50); // B
    assert_eq!(*surface.buf.get(1).expect("G byte"), 200); // G
    assert_eq!(*surface.buf.get(2).expect("R byte"), 100); // R
    assert_eq!(*surface.buf.get(3).expect("A byte"), 0xFF);
}

#[test]
fn test_set_pixel_last() {
    let (mut buf, _stride) = test_surface(10, 10);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride: 40,
    };
    set_pixel(&mut surface, 9, 9, (10, 20, 30));
    let offset = (9 * 40 + 9 * 4) as usize;
    let pixel = surface.buf.get(offset..offset + 4).expect("pixel slice");
    assert_eq!(*pixel.first().expect("B byte"), 30);
    assert_eq!(*pixel.get(1).expect("G byte"), 20);
    assert_eq!(*pixel.get(2).expect("R byte"), 10);
    assert_eq!(*pixel.get(3).expect("A byte"), 0xFF);
}

#[test]
fn test_draw_rect_zero_size() {
    let (mut buf, _stride) = test_surface(10, 10);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride: 40,
    };
    let rect = Rect {
        x: 5,
        y: 5,
        w: 0,
        h: 0,
    };
    draw_rect(&mut surface, &rect, (255, 255, 255));
    assert_eq!(surface.buf, &original[..]);
}

#[test]
fn test_draw_rect_full_surface() {
    let (mut buf, _stride) = test_surface(4, 4);
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    draw_rect(&mut surface, &rect, (255, 0, 0));
    // Every pixel should be red
    for chunk in surface.buf.chunks_exact(4) {
        assert_eq!(chunk, &[0, 0, 255, 0xFF]);
    }
}

#[test]
fn test_draw_rect_clipping() {
    let (mut buf, _stride) = test_surface(10, 10);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride: 40,
    };
    // Rect extends past the surface — should not panic
    let rect = Rect {
        x: 8,
        y: 8,
        w: 10,
        h: 10,
    };
    draw_rect(&mut surface, &rect, (128, 128, 128));
    // Pixel at (9,9) should be set
    let offset = (9 * 40 + 9 * 4) as usize;
    assert_eq!(*surface.buf.get(offset + 2).expect("R byte"), 128);
}

#[test]
fn test_draw_rect_alpha_transparent() {
    let (mut buf, _stride) = test_surface(4, 4);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[100, 100, 100, 255]);
    }
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    draw_rect_alpha(&mut surface, &rect, (255, 0, 0), 0);
    // Alpha=0 means no blending, background unchanged
    assert_eq!(surface.buf, &original[..]);
}

#[test]
fn test_draw_rect_alpha_opaque() {
    let (mut buf, _stride) = test_surface(4, 4);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[100, 100, 100, 255]);
    }
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    let rect = Rect {
        x: 0,
        y: 0,
        w: 1,
        h: 1,
    };
    draw_rect_alpha(&mut surface, &rect, (0, 255, 0), 255);
    // Alpha=255 → full replacement
    assert_eq!(*surface.buf.first().expect("B byte"), 0); // B=0 (from color green)
    assert_eq!(*surface.buf.get(1).expect("G byte"), 255); // G=255
    assert_eq!(*surface.buf.get(2).expect("R byte"), 0); // R=0
}

#[test]
fn test_clear_background_opaque() {
    let (mut buf, stride) = test_surface(4, 4);
    clear_background(&mut buf, 4, 4, stride, (10, 20, 30), 1.0);
    for chunk in buf.chunks_exact(4) {
        assert_eq!(*chunk.first().expect("B byte"), 30); // B
        assert_eq!(*chunk.get(1).expect("G byte"), 20); // G
        assert_eq!(*chunk.get(2).expect("R byte"), 10); // R
        assert_eq!(*chunk.get(3).expect("A byte"), 255); // A = fully opaque
    }
}

#[test]
fn test_clear_background_half_opacity() {
    let (mut buf, _stride) = test_surface(2, 2);
    clear_background(&mut buf, 2, 2, 2 * 4, (100, 100, 100), 0.5);
    let alpha_val = *buf.get(3).expect("alpha byte");
    assert!(
        alpha_val > 120 && alpha_val < 135,
        "expected ~128, got {alpha_val}"
    );
}

#[test]
fn test_clear_background_zero_opacity() {
    let (mut buf, stride) = test_surface(2, 2);
    clear_background(&mut buf, 2, 2, stride, (100, 100, 100), 0.0);
    assert_eq!(*buf.get(3).expect("alpha byte"), 0);
}

#[test]
fn test_draw_underline_single() {
    let (mut buf, stride) = test_surface(8, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 8,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 8, 16, 1, (255, 255, 255));
    // Underline at y = cell_h - 2 = 14
    let ul_y = 14;
    let mut count = 0;
    for col_px in 0..8 {
        let off = (ul_y * stride + col_px * 4) as usize;
        if *surface.buf.get(off + 2).expect("R byte") == 255 {
            count += 1;
        }
    }
    assert_eq!(count, 8, "single underline should span full cell width");
}

#[test]
fn test_draw_underline_double() {
    let (mut buf, stride) = test_surface(8, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 8,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 8, 16, 2, (255, 255, 255));
    // Should have pixels at two distinct rows
    let mut filled_rows = 0;
    for row_y in 10..16 {
        let off = (row_y * stride) as usize;
        if *surface.buf.get(off + 2).expect("R byte") == 255 {
            filled_rows += 1;
        }
    }
    assert!(
        filled_rows >= 2,
        "double underline should have >=2 rows, got {filled_rows}"
    );
}

#[test]
fn test_draw_underline_curly() {
    let (mut buf, stride) = test_surface(8, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 8,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 8, 16, 3, (255, 255, 255));
    // Curly: should have some pixels set
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R byte") == 255)
        .count();
    assert!(
        filled >= 4,
        "curly underline should have some pixels, got {filled}"
    );
}

#[test]
fn test_draw_underline_dotted() {
    let (mut buf, stride) = test_surface(8, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 8,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 8, 16, 4, (255, 255, 255));
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R byte") == 255)
        .count();
    // Dotted: every other pixel → 4 pixels for width=8
    assert_eq!(
        filled, 4,
        "dotted underline: expected 4 pixels, got {filled}"
    );
}

#[test]
fn test_draw_underline_dashed() {
    let (mut buf, stride) = test_surface(12, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 12,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 12, 16, 5, (255, 255, 255));
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R byte") == 255)
        .count();
    // Dashed: 3-on 3-off pattern → ~50%
    assert!(
        filled >= 4,
        "dashed underline should have pixels, got {filled}"
    );
    assert!(
        filled < 12,
        "dashed underline should have gaps, got {filled}"
    );
}

#[test]
fn test_draw_box_glyph_basic() {
    let (mut buf, stride) = test_surface(4, 4);
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride,
    };
    let alpha_buf = vec![255u8; 16]; // 4x4 fully opaque
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    draw_box_glyph(&mut surface, &rect, &alpha_buf, (100, 200, 50));
    // Every pixel should be the fg color
    for chunk in surface.buf.chunks_exact(4) {
        assert_eq!(*chunk.first().expect("B byte"), 50); // B
        assert_eq!(*chunk.get(1).expect("G byte"), 200); // G
        assert_eq!(*chunk.get(2).expect("R byte"), 100); // R
        assert_eq!(*chunk.get(3).expect("A byte"), 0xFF);
    }
}

#[test]
fn test_draw_box_glyph_transparent() {
    let (mut buf, stride) = test_surface(4, 4);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride,
    };
    let alpha_buf = vec![0u8; 16]; // all transparent
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    draw_box_glyph(&mut surface, &rect, &alpha_buf, (100, 200, 50));
    assert_eq!(surface.buf, &original[..]);
}

#[test]
fn test_draw_box_glyph_partial_alpha() {
    let (mut buf, _stride) = test_surface(2, 2);
    // Fill with white background
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[255, 255, 255, 255]);
    }
    let mut surface = Surface {
        buf: &mut buf,
        width: 2,
        height: 2,
        stride: 8,
    };
    let alpha_buf = vec![128u8; 4]; // half-transparent
    let rect = Rect {
        x: 0,
        y: 0,
        w: 2,
        h: 2,
    };
    draw_box_glyph(&mut surface, &rect, &alpha_buf, (0, 0, 0));
    // Should be blended toward black
    let red = *surface.buf.get(2).expect("R byte");
    assert!(red < 200 && red > 50, "expected blended value, got {red}");
}

#[test]
fn test_draw_box_glyph_clipping() {
    let (mut buf, _stride) = test_surface(4, 4);
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    let alpha_buf = vec![255u8; 16];
    // Rect starts at (3,3) with 4x4 — extends past boundary
    let rect = Rect {
        x: 3,
        y: 3,
        w: 4,
        h: 4,
    };
    // Should not panic
    draw_box_glyph(&mut surface, &rect, &alpha_buf, (255, 0, 0));
    // Pixel at (3,3) should be set
    let offset = (3 * 16 + 3 * 4) as usize;
    assert_eq!(*surface.buf.get(offset + 2).expect("R byte"), 255);
}

#[test]
fn test_draw_glyph_basic() {
    let (mut buf, stride) = test_surface(10, 20);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 20,
        stride,
    };
    let glyph = GlyphImage {
        width: 2,
        height: 2,
        left: 0,
        top: 1,
        alpha: vec![255, 255, 255, 255],
    };
    draw_glyph(&mut surface, 0, 0, 20, &glyph, (255, 0, 0));
    // Some pixels should be drawn (exact position depends on baseline calc)
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R byte") == 255)
        .count();
    assert!(filled > 0, "glyph should draw at least some pixels");
}

#[test]
fn test_draw_glyph_out_of_bounds() {
    let (mut buf, stride) = test_surface(4, 4);
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride,
    };
    let glyph = GlyphImage {
        width: 2,
        height: 2,
        left: -10, // way off-screen
        top: 1,
        alpha: vec![255, 255, 255, 255],
    };
    // Should not panic
    draw_glyph(&mut surface, 0, 0, 4, &glyph, (255, 0, 0));
}

use crate::terminal::render::RenderState;
use crate::terminal::vt::{Terminal, TerminalOps};

/// Standalone render pipeline for integration testing without Wayland.
pub(crate) struct RenderCapture {
    pub terminal: Terminal,
    pub render_state: RenderState,
    pub font: FontManager,
    pub buf: Vec<u8>,
    pub retained: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub cols: u16,
    pub rows: u16,
}

impl RenderCapture {
    /// Create a new capture with the given grid dimensions.
    pub fn new(cols: u16, rows: u16) -> Self {
        let terminal = Terminal::new(cols, rows, 100).expect("terminal creation");
        let render_state = RenderState::new().expect("render state creation");
        let font = FontManager::new(16.0);
        let width = u32::from(cols) * font.cell_width;
        let height = u32::from(rows) * font.cell_height;
        let stride = width * 4;
        let buf_len = (stride * height) as usize;
        Self {
            terminal,
            render_state,
            font,
            buf: vec![0u8; buf_len],
            retained: Vec::new(),
            width,
            height,
            stride,
            cols,
            rows,
        }
    }

    /// Feed VT-encoded data to the terminal.
    pub fn write_vt(&mut self, data: &[u8]) {
        self.terminal.vt_write(data);
    }

    /// Render the terminal into the buffer.
    pub fn render(&mut self) {
        // SAFETY: self.terminal owns the pointer and is valid.
        self.render_state
            .update(self.terminal.inner())
            .expect("update");
        let opts = RenderOptions {
            scrollbar: None,
            bold_is_bright: true,
            cursor_blink_visible: true,
            selection: None,
            padding: 0,
            opacity: 1.0,
            search_highlights: &[],
            search_bar: None,
            search_cursor: 0,
            preedit: None,
            selection_fg: None,
            selection_bg: None,
        };
        let mut target = RenderTarget {
            buf: &mut self.buf,
            width: self.width,
            height: self.height,
            stride: self.stride,
            retained: &mut self.retained,
        };
        let colors = self.render_state.colors();
        render_frame(
            &mut target,
            &mut self.render_state,
            &mut self.font,
            &opts,
            &colors,
        );
    }

    /// Render with a selection overlay.
    pub fn render_with_selection(&mut self, sel: ((u16, u16), (u16, u16))) {
        // SAFETY: self.terminal owns the pointer and is valid.
        self.render_state
            .update(self.terminal.inner())
            .expect("update");
        let opts = RenderOptions {
            scrollbar: None,
            bold_is_bright: true,
            cursor_blink_visible: true,
            selection: Some(sel),
            padding: 0,
            opacity: 1.0,
            search_highlights: &[],
            search_bar: None,
            search_cursor: 0,
            preedit: None,
            selection_fg: None,
            selection_bg: None,
        };
        let mut target = RenderTarget {
            buf: &mut self.buf,
            width: self.width,
            height: self.height,
            stride: self.stride,
            retained: &mut self.retained,
        };
        let colors = self.render_state.colors();
        render_frame(
            &mut target,
            &mut self.render_state,
            &mut self.font,
            &opts,
            &colors,
        );
    }

    /// Read a BGRA pixel at the given coordinates.
    pub fn pixel_at(&self, px: u32, py: u32) -> (u8, u8, u8, u8) {
        let offset = (py * self.stride + px * 4) as usize;
        let pixel = self
            .buf
            .get(offset..offset + 4)
            .expect("pixel_at in bounds");
        let blue = *pixel.first().expect("blue channel");
        let green = *pixel.get(1).expect("green channel");
        let red = *pixel.get(2).expect("red channel");
        let alpha = *pixel.get(3).expect("alpha channel");
        (red, green, blue, alpha)
    }

    /// Get the pixel origin of a cell.
    pub fn cell_origin(&self, col: u16, row: u16) -> (u32, u32) {
        (
            u32::from(col) * self.font.cell_width,
            u32::from(row) * self.font.cell_height,
        )
    }

    /// Check if a cell has any pixels differing from the background.
    pub fn cell_has_nonbg_pixels(&self, col: u16, row: u16) -> bool {
        let (ox, oy) = self.cell_origin(col, row);
        let cw = self.font.cell_width;
        let ch = self.font.cell_height;
        // Sample the background from a cell that should be empty (bottom-right corner)
        let bg = self.pixel_at(ox, oy);
        for dy in 0..ch {
            for dx in 0..cw {
                let px = self.pixel_at(ox + dx, oy + dy);
                if px != bg {
                    return true;
                }
            }
        }
        false
    }

    /// Resize the capture buffers and terminal.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.terminal.resize(cols, rows).expect("resize");
        self.cols = cols;
        self.rows = rows;
        self.width = u32::from(cols) * self.font.cell_width;
        self.height = u32::from(rows) * self.font.cell_height;
        self.stride = self.width * 4;
        let buf_len = (self.stride * self.height) as usize;
        self.buf = vec![0u8; buf_len];
        self.retained.clear();
    }

    /// Render with a scrollbar overlay.
    pub fn render_with_scrollbar(&mut self, scrollbar: &ffi::GhosttyTerminalScrollbar) {
        self.render_state
            .update(self.terminal.inner())
            .expect("update");
        let opts = RenderOptions {
            scrollbar: Some(scrollbar),
            bold_is_bright: true,
            cursor_blink_visible: true,
            selection: None,
            padding: 0,
            opacity: 1.0,
            search_highlights: &[],
            search_bar: None,
            search_cursor: 0,
            preedit: None,
            selection_fg: None,
            selection_bg: None,
        };
        let mut target = RenderTarget {
            buf: &mut self.buf,
            width: self.width,
            height: self.height,
            stride: self.stride,
            retained: &mut self.retained,
        };
        let colors = self.render_state.colors();
        render_frame(
            &mut target,
            &mut self.render_state,
            &mut self.font,
            &opts,
            &colors,
        );
    }

    /// Render with a specific `cursor_blink_visible` value.
    pub fn render_with_blink(&mut self, cursor_blink_visible: bool) {
        self.render_state
            .update(self.terminal.inner())
            .expect("update");
        let opts = RenderOptions {
            scrollbar: None,
            bold_is_bright: true,
            cursor_blink_visible,
            selection: None,
            padding: 0,
            opacity: 1.0,
            search_highlights: &[],
            search_bar: None,
            search_cursor: 0,
            preedit: None,
            selection_fg: None,
            selection_bg: None,
        };
        let mut target = RenderTarget {
            buf: &mut self.buf,
            width: self.width,
            height: self.height,
            stride: self.stride,
            retained: &mut self.retained,
        };
        let colors = self.render_state.colors();
        render_frame(
            &mut target,
            &mut self.render_state,
            &mut self.font,
            &opts,
            &colors,
        );
    }

    /// Get a snapshot of the retained buffer.
    pub fn retained_snapshot(&self) -> Vec<u8> {
        self.retained.clone()
    }
}

#[test]
fn test_overlay_cleanup_on_rerender() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Hello");
    cap.render_with_selection(((0, 0), (4, 0)));
    let (ox, oy) = cap.cell_origin(2, 0);
    let with_sel = cap.pixel_at(ox + 1, oy + 1);
    cap.render();
    let without_sel = cap.pixel_at(ox + 1, oy + 1);
    cap.render();
    let stable = cap.pixel_at(ox + 1, oy + 1);
    assert_ne!(
        with_sel, without_sel,
        "removing selection should change pixels"
    );
    assert_eq!(without_sel, stable, "stable frames should be identical");
}

#[test]
fn test_retained_not_polluted_by_overlays() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"content");
    cap.render();
    let retained1 = cap.retained_snapshot();
    cap.render_with_selection(((0, 0), (6, 0)));
    let retained2 = cap.retained_snapshot();
    assert_eq!(
        retained1, retained2,
        "overlays must not pollute retained buffer"
    );
}

#[test]
fn test_render_capture_blank() {
    let mut cap = RenderCapture::new(80, 24);
    cap.render();
    assert!(!cap.buf.is_empty());
    assert_eq!(*cap.buf.get(3).expect("alpha byte"), 0xFF);
}

#[test]
fn test_render_capture_text_pixels() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"X");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "cell with 'X' should have non-bg pixels"
    );
}

#[test]
fn test_render_capture_cursor_at_origin() {
    let mut cap = RenderCapture::new(80, 24);
    cap.render();
    let (ox0, oy0) = cap.cell_origin(0, 0);
    let (ox1, oy1) = cap.cell_origin(5, 5);
    let px_cursor = cap.pixel_at(ox0 + 1, oy0 + 1);
    let px_empty = cap.pixel_at(ox1 + 1, oy1 + 1);
    assert_ne!(
        px_cursor, px_empty,
        "cursor cell should differ from empty cell"
    );
}

#[test]
fn test_render_capture_multiple_frames() {
    let mut cap = RenderCapture::new(80, 24);
    cap.render();
    cap.write_vt(b"Hello");
    cap.render();
    assert_eq!(*cap.buf.get(3).expect("alpha byte"), 0xFF);
}

#[test]
fn test_render_capture_after_resize() {
    let mut cap = RenderCapture::new(80, 24);
    cap.render();
    cap.resize(40, 10);
    cap.render();
    let expected_len = (cap.stride * cap.height) as usize;
    assert_eq!(cap.buf.len(), expected_len);
    assert_eq!(cap.cols, 40);
    assert_eq!(cap.rows, 10);
}

#[test]
fn test_render_capture_with_selection() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Hello World");
    cap.render();
    let (ox, oy) = cap.cell_origin(0, 0);
    let before = cap.pixel_at(ox + 1, oy + 1);
    cap.render_with_selection(((0, 0), (4, 0)));
    let after = cap.pixel_at(ox + 1, oy + 1);
    assert_ne!(before, after, "selection overlay should change pixels");
}

#[test]
fn test_render_colored_prompt() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[32muser@host\x1b[0m:\x1b[34m~/projects\x1b[0m$ echo hello\r\n");
    cap.write_vt(b"hello\r\n");
    cap.write_vt(b"\x1b[32muser@host\x1b[0m:\x1b[34m~/projects\x1b[0m$ ");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "first char should have text"
    );
    assert!(
        cap.cell_has_nonbg_pixels(1, 0),
        "second char should have text"
    );
    assert!(
        cap.cell_has_nonbg_pixels(0, 1),
        "hello line should have text"
    );
    assert!(
        !cap.cell_has_nonbg_pixels(79, 23),
        "far corner should be empty bg"
    );
}

#[test]
fn test_search_highlight_no_u16_overflow() {
    let (mut buf, stride) = test_surface(100, 100);
    let mut surface = Surface {
        buf: &mut buf,
        width: 100,
        height: 100,
        stride,
    };
    let grid = GridMetrics {
        cell_w: 1,
        cell_h: 1,
        pad: 0,
    };
    let hl = SearchHighlight {
        row: 0,
        start_col: 10,
        end_col: u16::MAX,
        is_current: false,
    };
    draw_search_highlights(&mut surface, &grid, &[hl]);
}

#[test]
fn test_search_highlight_visible() {
    let (mut buf, stride) = test_surface(200, 50);
    let mut surface = Surface {
        buf: &mut buf,
        width: 200,
        height: 50,
        stride,
    };
    let grid = GridMetrics {
        cell_w: 8,
        cell_h: 16,
        pad: 0,
    };
    let hl = SearchHighlight {
        row: 0,
        start_col: 0,
        end_col: 3,
        is_current: true,
    };
    draw_search_highlights(&mut surface, &grid, &[hl]);
    let mut nonzero = 0;
    for y in 0..16u32 {
        for x in 0..32u32 {
            let off = (y * stride + x * 4) as usize;
            if buf.get(off + 1).copied().unwrap_or(0) != 0 {
                nonzero += 1;
            }
        }
    }
    assert!(nonzero > 0, "search highlight should paint visible pixels");
}

#[test]
fn test_set_pixel_out_of_bounds_noop() {
    let (mut buf, stride) = test_surface(10, 10);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    set_pixel(&mut surface, 10, 5, (255, 0, 0));
    set_pixel(&mut surface, 5, 10, (255, 0, 0));
    set_pixel(&mut surface, u32::MAX, u32::MAX, (255, 0, 0));
    assert_eq!(
        &*surface.buf, &original,
        "out-of-bounds set_pixel should not modify buffer"
    );
}

#[test]
fn test_draw_rect_clips_to_surface() {
    let (mut buf, stride) = test_surface(20, 20);
    let mut surface = Surface {
        buf: &mut buf,
        width: 20,
        height: 20,
        stride,
    };
    let rect = Rect {
        x: 15,
        y: 15,
        w: 100,
        h: 100,
    };
    draw_rect(&mut surface, &rect, (255, 0, 0));
}

#[test]
fn test_draw_rect_alpha_clips_to_surface() {
    let (mut buf, stride) = test_surface(20, 20);
    let mut surface = Surface {
        buf: &mut buf,
        width: 20,
        height: 20,
        stride,
    };
    let rect = Rect {
        x: 15,
        y: 15,
        w: 100,
        h: 100,
    };
    draw_rect_alpha(&mut surface, &rect, (0, 255, 0), 128);
}

#[test]
fn test_blend_channel_extremes() {
    assert_eq!(blend_channel(255, 0, 255), 255, "full alpha: src dominates");
    assert_eq!(blend_channel(0, 255, 0), 255, "zero alpha: dst unchanged");
    assert_eq!(blend_channel(128, 128, 128), 128, "equal blend: mid");
}

#[test]
fn test_clear_background_fills_entire_buffer() {
    let w = 40u32;
    let h = 30u32;
    let stride = w * 4;
    let mut buf = vec![0u8; (stride * h) as usize];
    let bg = (10, 20, 30);
    clear_background(&mut buf, w, h, stride, bg, 1.0);
    for y in 0..h {
        for x in 0..w {
            let off = (y * stride + x * 4) as usize;
            let pixel: &[u8; 4] = buf
                .get(off..off + 4)
                .expect("pixel in bounds")
                .try_into()
                .expect("4 bytes");
            assert_eq!(pixel, &[bg.2, bg.1, bg.0, 0xFF], "pixel at ({x},{y})");
        }
    }
}

#[test]
fn test_clear_background_with_opacity() {
    let w = 10u32;
    let h = 10u32;
    let stride = w * 4;
    let mut buf = vec![0u8; (stride * h) as usize];
    clear_background(&mut buf, w, h, stride, (100, 100, 100), 0.5);
    let a = *buf.get(3).expect("alpha byte");
    assert!((126..=129).contains(&a), "alpha should be ~128, got {a}");
}

#[test]
fn test_render_frame_full_dirty() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Hello");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "cell (0,0) should have non-bg pixels after writing 'Hello'"
    );
}

#[test]
fn test_render_frame_partial_dirty() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"ABCDE");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "first render should show 'A'"
    );
    cap.write_vt(b"X");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(5, 0),
        "partial render should show 'X' at col 5"
    );
}

#[test]
fn test_render_frame_not_dirty() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Test");
    cap.render();
    let first_pixel = cap.pixel_at(0, 0);
    cap.render();
    let second_pixel = cap.pixel_at(0, 0);
    assert_eq!(
        first_pixel, second_pixel,
        "render without changes should produce identical output"
    );
}

#[test]
fn test_render_frame_retained_buffer_copied() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"ABCDE");
    cap.render();
    assert!(cap.cell_has_nonbg_pixels(0, 0), "'A' should be visible");
    assert!(cap.cell_has_nonbg_pixels(4, 0), "'E' should be visible");
    cap.write_vt(b"\x1b[3;1H");
    cap.write_vt(b"Z");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "retained buffer should preserve 'A' on row 0 after partial render"
    );
    assert!(
        cap.cell_has_nonbg_pixels(4, 0),
        "retained buffer should preserve 'E' on row 0 after partial render"
    );
    assert!(
        cap.cell_has_nonbg_pixels(0, 2),
        "'Z' should be visible at (0,2)"
    );
}

#[test]
fn test_render_frame_scrollbar() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"content");
    let mut sb =
        unsafe { std::mem::MaybeUninit::<ffi::GhosttyTerminalScrollbar>::zeroed().assume_init() };
    sb.total = 200;
    sb.len = 24;
    sb.offset = 50;
    cap.render_with_scrollbar(&sb);
    let bar_x = cap.width.saturating_sub(8);
    let mut found_scrollbar_pixel = false;
    let bg = cap.pixel_at(0, cap.height / 2);
    for y in 0..cap.height {
        for dx in 0..6 {
            let px = cap.pixel_at(bar_x + dx, y);
            if px != bg {
                found_scrollbar_pixel = true;
                break;
            }
        }
        if found_scrollbar_pixel {
            break;
        }
    }
    assert!(
        found_scrollbar_pixel,
        "scrollbar thumb should produce non-background pixels"
    );
}

#[test]
fn test_render_frame_selection_overlay() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Hello World ABCDEF");
    cap.render();
    let (ox, oy) = cap.cell_origin(2, 0);
    let before = cap.pixel_at(ox + 1, oy + 1);
    cap.render_with_selection(((0, 0), (5, 0)));
    let after = cap.pixel_at(ox + 1, oy + 1);
    assert_ne!(
        before, after,
        "selected cell should have different pixels than unselected"
    );
    let (unsel_x, unsel_y) = cap.cell_origin(10, 0);
    let outside_sel = cap.pixel_at(unsel_x + 1, unsel_y + 1);
    cap.render();
    let outside_ref = cap.pixel_at(unsel_x + 1, unsel_y + 1);
    assert_eq!(
        outside_sel, outside_ref,
        "cell outside selection should be unchanged"
    );
}

#[test]
fn test_render_frame_cursor_visible() {
    let mut cap = RenderCapture::new(80, 24);
    cap.render();
    let (ox, oy) = cap.cell_origin(0, 0);
    let cursor_px = cap.pixel_at(ox + 1, oy + 1);
    let (ex, ey) = cap.cell_origin(40, 12);
    let empty_px = cap.pixel_at(ex + 1, ey + 1);
    assert_ne!(
        cursor_px, empty_px,
        "cursor at (0,0) should produce non-background pixels"
    );
}

#[test]
fn test_render_frame_wide_char() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt("\u{4e2d}".as_bytes());
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "wide character cell (0,0) should have non-bg pixels"
    );
}

#[test]
fn test_render_frame_after_resize() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Before resize");
    cap.render();
    let old_len = cap.buf.len();
    cap.resize(40, 12);
    cap.render();
    let new_len = cap.buf.len();
    assert_ne!(old_len, new_len, "buffer size should change after resize");
    let expected = (cap.stride * cap.height) as usize;
    assert_eq!(
        cap.buf.len(),
        expected,
        "buffer should match new dimensions"
    );
    assert_eq!(*cap.buf.get(3).expect("alpha byte"), 0xFF);
}

#[test]
fn test_draw_glyph_renders_pixels() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"A");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "'A' glyph should render non-background pixels at cell (0,0)"
    );
    assert!(
        !cap.cell_has_nonbg_pixels(79, 23),
        "empty cell should have only background pixels"
    );
}

#[test]
fn test_clear_background_nonblack_fills_every_pixel() {
    let width = 10u32;
    let height = 10u32;
    let stride = width * 4;
    let mut buf = vec![0u8; (stride * height) as usize];
    let bg = (0xAA, 0xBB, 0xCC);
    clear_background(&mut buf, width, height, stride, bg, 1.0);
    for offset in (0..buf.len()).step_by(4) {
        let pixel = (
            *buf.get(offset).expect("B"),
            *buf.get(offset + 1).expect("G"),
            *buf.get(offset + 2).expect("R"),
            *buf.get(offset + 3).expect("A"),
        );
        assert_eq!(
            pixel,
            (0xCC, 0xBB, 0xAA, 0xFF),
            "pixel at byte offset {offset} should be BGRA (0xCC,0xBB,0xAA,0xFF)"
        );
    }
}

#[test]
fn test_draw_rect_clamps_to_surface() {
    let (mut buf, stride) = test_surface(10, 10);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    let rect = Rect {
        x: 5,
        y: 0,
        w: 100,
        h: 1,
    };
    draw_rect(&mut surface, &rect, (255, 0, 0));
    for x in 0..5u32 {
        let offset = (x * 4) as usize;
        let pixel = surface
            .buf
            .get(offset..offset + 4)
            .expect("pixel in bounds");
        assert_eq!(pixel, &[0, 0, 0, 0], "column {x} should be untouched");
    }
    for x in 5..10u32 {
        let offset = (x * 4) as usize;
        let pixel = surface
            .buf
            .get(offset..offset + 4)
            .expect("pixel in bounds");
        assert_eq!(pixel, &[0, 0, 255, 0xFF], "column {x} should be filled red");
    }
}

#[test]
fn test_blend_channel_all_boundary_pairs() {
    assert_eq!(blend_channel(200, 50, 0), 50, "alpha=0: bg passthrough");
    assert_eq!(
        blend_channel(0, 100, 0),
        100,
        "alpha=0: bg passthrough (fg=0)"
    );
    assert_eq!(
        blend_channel(200, 50, 255),
        200,
        "alpha=255: fg passthrough"
    );
    assert_eq!(
        blend_channel(0, 100, 255),
        0,
        "alpha=255: fg passthrough (fg=0)"
    );
    let low_alpha = blend_channel(255, 0, 1);
    assert_eq!(low_alpha, 1, "alpha=1: nearly background");
    let high_alpha = blend_channel(255, 0, 254);
    assert_eq!(high_alpha, 254, "alpha=254: nearly foreground");
}

#[test]
fn test_render_capture_cursor_blink_toggle() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[?12h");
    cap.render_with_blink(true);
    let (ox, oy) = cap.cell_origin(0, 0);
    let px_on = cap.pixel_at(ox + 1, oy + 1);
    cap.render_with_blink(false);
    let px_off = cap.pixel_at(ox + 1, oy + 1);
    assert_ne!(
        px_on, px_off,
        "cursor pixels should differ between blink on ({px_on:?}) and blink off ({px_off:?})"
    );
}

#[test]
fn test_blend_channel_exhaustive_equivalence() {
    for val in 0u16..=65025 {
        #[expect(clippy::cast_possible_truncation, reason = "proven ≤255")]
        let fast = (((u32::from(val) + 1) * 257) >> 16) as u8;
        let slow = (val / 255).min(255) as u8;
        assert_eq!(fast, slow, "mismatch at val={val}");
    }
}

#[test]
fn test_blend_channel_all_alpha_boundary() {
    for a in 0u16..=255 {
        let a_u8 = u8::try_from(a).expect("a ≤ 255");
        let inv_u8 = u8::try_from(255 - a).expect("255-a ≤ 255");
        assert_eq!(
            blend_channel(255, 0, a),
            a_u8,
            "blend(255,0,{a}) should be {a}"
        );
        assert_eq!(
            blend_channel(0, 255, a),
            inv_u8,
            "blend(0,255,{a}) should be {}",
            255 - a
        );
    }
}

#[test]
fn test_blend_channel_identity() {
    for fg in (0u8..=255).step_by(17) {
        for bg in (0u8..=255).step_by(17) {
            assert_eq!(blend_channel(fg, bg, 255), fg, "alpha=255: fg={fg} bg={bg}");
            assert_eq!(blend_channel(fg, bg, 0), bg, "alpha=0: fg={fg} bg={bg}");
        }
    }
}

#[test]
fn test_draw_rect_alpha_matches_manual_blend() {
    let (mut buf, _stride) = test_surface(4, 4);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[100, 150, 200, 255]);
    }
    let expected: Vec<u8> = buf
        .chunks_exact(4)
        .flat_map(|c| {
            if let [b_val, g_val, r_val, ..] = *c {
                let b_ch = blend_channel(50, b_val, 128);
                let g_ch = blend_channel(60, g_val, 128);
                let r_ch = blend_channel(70, r_val, 128);
                [b_ch, g_ch, r_ch, 0xFF]
            } else {
                [0, 0, 0, 0]
            }
        })
        .collect();
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    draw_rect_alpha(&mut surface, &rect, (70, 60, 50), 128);
    assert_eq!(
        surface.buf,
        &expected[..],
        "inlined blend should match per-channel blend"
    );
}

#[test]
fn test_draw_rect_zero_dimensions_noop() {
    let (mut buf, _stride) = test_surface(10, 10);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride: 40,
    };
    draw_rect(
        &mut surface,
        &Rect {
            x: 5,
            y: 5,
            w: 0,
            h: 3,
        },
        (255, 0, 0),
    );
    assert_eq!(surface.buf, &original[..], "w=0 should be noop");
    draw_rect(
        &mut surface,
        &Rect {
            x: 5,
            y: 5,
            w: 3,
            h: 0,
        },
        (255, 0, 0),
    );
    assert_eq!(surface.buf, &original[..], "h=0 should be noop");
}

#[test]
fn test_draw_rect_alpha_full_opacity_matches_draw_rect() {
    let (mut buf_alpha, _) = test_surface(4, 4);
    let (mut buf_opaque, _) = test_surface(4, 4);
    for chunk in buf_alpha.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[50, 100, 150, 255]);
    }
    buf_opaque.copy_from_slice(&buf_alpha);
    let color = (200, 100, 50);
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    {
        let mut s = Surface {
            buf: &mut buf_alpha,
            width: 4,
            height: 4,
            stride: 16,
        };
        draw_rect_alpha(&mut s, &rect, color, 255);
    }
    {
        let mut s = Surface {
            buf: &mut buf_opaque,
            width: 4,
            height: 4,
            stride: 16,
        };
        draw_rect(&mut s, &rect, color);
    }
    assert_eq!(buf_alpha, buf_opaque, "alpha=255 should match draw_rect");
}

#[test]
fn test_draw_rect_alpha_zero_opacity_noop() {
    let (mut buf, _) = test_surface(4, 4);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[50, 100, 150, 255]);
    }
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    draw_rect_alpha(
        &mut surface,
        &Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        },
        (255, 0, 0),
        0,
    );
    assert_eq!(
        surface.buf,
        &original[..],
        "alpha=0 should leave buffer unchanged"
    );
}

#[test]
fn test_clear_background_opacity_produces_alpha() {
    let (mut buf, stride) = test_surface(2, 2);
    clear_background(&mut buf, 2, 2, stride, (100, 100, 100), 0.75);
    let alpha = *buf.get(3).expect("alpha byte");
    assert!(
        (189..=193).contains(&alpha),
        "opacity 0.75 should produce alpha ~191, got {alpha}"
    );
}

#[test]
fn test_render_capture_search_bar_at_bottom() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"content");
    cap.render_state
        .update(cap.terminal.inner())
        .expect("update");
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: Some("find me"),
        search_cursor: 4,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
        retained: &mut cap.retained,
    };
    let colors = cap.render_state.colors();
    render_frame(
        &mut target,
        &mut cap.render_state,
        &mut cap.font,
        &opts,
        &colors,
    );
    let bg = cap.pixel_at(cap.width / 2, 0);
    let mut found_search_bar = false;
    let last_y = cap.height.saturating_sub(1);
    for x in 0..cap.width.min(200) {
        let px = cap.pixel_at(x, last_y);
        if px != bg {
            found_search_bar = true;
            break;
        }
    }
    assert!(
        found_search_bar,
        "search bar should render pixels at bottom"
    );
}

#[test]
fn test_draw_glyph_full_alpha_exact_colors() {
    let (mut buf, stride) = test_surface(10, 10);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[255, 255, 255, 255]);
    }
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    let glyph = GlyphImage {
        alpha: vec![255; 4],
        width: 2,
        height: 2,
        left: 0,
        top: 2,
    };
    let fg = (200, 100, 50);
    draw_glyph(&mut surface, 0, 0, 10, &glyph, fg);
    let py = 6u32;
    for dy in 0..2u32 {
        for dx in 0..2u32 {
            let off = ((py + dy) * stride + dx * 4) as usize;
            assert_eq!(*surface.buf.get(off).expect("B"), 50, "B at ({dx},{dy})");
            assert_eq!(
                *surface.buf.get(off + 1).expect("G"),
                100,
                "G at ({dx},{dy})"
            );
            assert_eq!(
                *surface.buf.get(off + 2).expect("R"),
                200,
                "R at ({dx},{dy})"
            );
            assert_eq!(
                *surface.buf.get(off + 3).expect("A"),
                0xFF,
                "A at ({dx},{dy})"
            );
        }
    }
}

#[test]
fn test_draw_glyph_zero_alpha_preserves_bg() {
    let (mut buf, stride) = test_surface(10, 10);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[50, 100, 150, 255]);
    }
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    let glyph = GlyphImage {
        alpha: vec![0; 4],
        width: 2,
        height: 2,
        left: 0,
        top: 2,
    };
    draw_glyph(&mut surface, 0, 0, 10, &glyph, (255, 0, 0));
    assert_eq!(
        surface.buf,
        &original[..],
        "zero-alpha glyph should not modify buffer"
    );
}

#[test]
fn test_draw_glyph_partial_alpha_blend_value() {
    let (mut buf, stride) = test_surface(10, 10);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[255, 255, 255, 255]);
    }
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride,
    };
    let glyph = GlyphImage {
        alpha: vec![128],
        width: 1,
        height: 1,
        left: 0,
        top: 1,
    };
    draw_glyph(&mut surface, 0, 0, 10, &glyph, (0, 0, 0));
    let py = 7u32;
    let off = (py * stride) as usize;
    let blended_b = *surface.buf.get(off).expect("B");
    assert!(
        (100..=160).contains(&blended_b),
        "expected ~128, got {blended_b}"
    );
}

#[test]
fn test_resolve_cell_colors_normal() {
    let style = default_cell_style();
    let colors = RenderColors {
        foreground: (200, 150, 100),
        background: (10, 20, 30),
        cursor: None,
        palette: [(0, 0, 0); 256],
    };
    let (fg, bg) = resolve_cell_colors(&style, &colors, true);
    assert_eq!(fg, (200, 150, 100));
    assert_eq!(bg, (10, 20, 30));
}

#[test]
fn test_resolve_cell_colors_inverse_swaps() {
    use crate::terminal::render::CellStyleAttrs;
    let mut style = default_cell_style();
    style.attrs = CellStyleAttrs::from_bits(CellStyleAttrs::INVERSE);
    let colors = RenderColors {
        foreground: (200, 150, 100),
        background: (10, 20, 30),
        cursor: None,
        palette: [(0, 0, 0); 256],
    };
    let (fg, bg) = resolve_cell_colors(&style, &colors, true);
    assert_eq!(fg, (10, 20, 30), "inverse should swap fg to bg");
    assert_eq!(bg, (200, 150, 100), "inverse should swap bg to fg");
}

#[test]
fn test_resolve_cell_colors_bold_bright_remap() {
    use crate::terminal::render::CellStyleAttrs;
    let mut style = default_cell_style();
    style.attrs = CellStyleAttrs::from_bits(CellStyleAttrs::BOLD);
    style.fg_tag = ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE;
    style.fg_palette = 1;
    let mut palette = [(0u8, 0u8, 0u8); 256];
    palette[1] = (170, 0, 0);
    palette[9] = (255, 85, 85);
    let colors = RenderColors {
        foreground: (200, 200, 200),
        background: (0, 0, 0),
        cursor: None,
        palette,
    };
    let (fg, _bg) = resolve_cell_colors(&style, &colors, true);
    assert_eq!(
        fg,
        (255, 85, 85),
        "bold + palette 0-7 should remap to bright"
    );
}

#[test]
fn test_resolve_cell_colors_faint_halves() {
    use crate::terminal::render::CellStyleAttrs;
    let mut style = default_cell_style();
    style.attrs = CellStyleAttrs::from_bits(CellStyleAttrs::FAINT);
    let colors = RenderColors {
        foreground: (200, 100, 50),
        background: (0, 0, 0),
        cursor: None,
        palette: [(0, 0, 0); 256],
    };
    let (fg, _bg) = resolve_cell_colors(&style, &colors, true);
    assert_eq!(fg, (100, 50, 25), "faint should halve fg channels via >>1");
}

#[test]
fn test_draw_cell_text_box_drawing_path() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt("─".as_bytes());
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "box-drawing char should produce visible pixels"
    );
}

#[test]
fn test_render_capture_preedit_at_cursor() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"before ");
    cap.render_state
        .update(cap.terminal.inner())
        .expect("update");
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: Some("abc"),
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
        retained: &mut cap.retained,
    };
    let colors = cap.render_state.colors();
    render_frame(
        &mut target,
        &mut cap.render_state,
        &mut cap.font,
        &opts,
        &colors,
    );
    let (cx, cy) = cap.cell_origin(7, 0);
    let bg = cap.pixel_at(cap.width - 1, cap.height - 1);
    let mut found_preedit = false;
    for dx in 0..cap.font.cell_width * 3 {
        for dy in 0..cap.font.cell_height {
            if cx + dx < cap.width && cy + dy < cap.height {
                let px = cap.pixel_at(cx + dx, cy + dy);
                if px != bg {
                    found_preedit = true;
                    break;
                }
            }
        }
        if found_preedit {
            break;
        }
    }
    assert!(
        found_preedit,
        "preedit text should render pixels near cursor"
    );
}

#[test]
fn test_opacity_to_alpha_nan() {
    assert_eq!(opacity_to_alpha(f32::NAN), 0);
}

#[test]
fn test_draw_box_glyph_all_opaque() {
    let (mut buf, stride) = test_surface(4, 4);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[10, 20, 30, 255]);
    }
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride,
    };
    let alpha_buf = vec![255u8; 16];
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    draw_box_glyph(&mut surface, &rect, &alpha_buf, (200, 100, 50));
    for chunk in surface.buf.chunks_exact(4) {
        assert_eq!(chunk, &[50, 100, 200, 0xFF]);
    }
}

#[test]
fn test_draw_box_glyph_partial_surface() {
    let (mut buf, stride) = test_surface(4, 4);
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride,
    };
    let alpha_buf = vec![255u8; 100];
    let rect = Rect {
        x: 2,
        y: 2,
        w: 10,
        h: 10,
    };
    draw_box_glyph(&mut surface, &rect, &alpha_buf, (255, 0, 0));
    let offset = (3 * stride + 3 * 4) as usize;
    assert_eq!(*surface.buf.get(offset + 2).expect("R"), 255);
}

#[test]
fn test_clear_dirty_rows_empty_slice() {
    let (mut buf, stride) = test_surface(4, 4);
    let original = buf.clone();
    let grid = GridMetrics {
        cell_w: 4,
        cell_h: 4,
        pad: 0,
    };
    clear_dirty_rows(&mut buf, 4, stride, &grid, &[], (0, 0, 0), 1.0);
    assert_eq!(buf, original);
}

#[test]
fn test_clear_dirty_rows_single_row() {
    let (mut buf, stride) = test_surface(4, 8);
    let grid = GridMetrics {
        cell_w: 4,
        cell_h: 4,
        pad: 0,
    };
    clear_dirty_rows(&mut buf, 4, stride, &grid, &[1], (100, 100, 100), 1.0);
    for px in 0..4 {
        let off = px * 4;
        assert_eq!(*buf.get(off).expect("byte"), 0, "row 0 should be untouched");
    }
    let row1_start = 4 * stride as usize;
    assert_eq!(*buf.get(row1_start).expect("byte"), 100);
}

#[test]
fn test_clear_dirty_rows_multiple() {
    let width = 4u32;
    let cell_h = 4u32;
    let rows = 5u32;
    let height = rows * cell_h;
    let stride = width * 4;
    let (mut buf, _) = test_surface(width, height);
    let grid = GridMetrics {
        cell_w: width,
        cell_h,
        pad: 0,
    };
    let bg: Bgra = (50, 100, 150);
    clear_dirty_rows(&mut buf, width, stride, &grid, &[1, 2, 4], bg, 1.0);
    let pixel_bgra = [bg.2, bg.1, bg.0, 0xFF];
    for py in 0..cell_h {
        for px in 0..width {
            let off = (py * stride + px * 4) as usize;
            let p = buf.get(off..off + 4).expect("pixel in bounds");
            assert_eq!(
                p,
                &[0, 0, 0, 0],
                "row 0 pixel ({px},{py}) should be untouched"
            );
        }
    }
    for py in cell_h..cell_h * 2 {
        for px in 0..width {
            let off = (py * stride + px * 4) as usize;
            let p = buf.get(off..off + 4).expect("pixel in bounds");
            assert_eq!(p, &pixel_bgra, "row 1 pixel ({px},{py}) should be bg");
        }
    }
}

#[test]
fn test_clear_dirty_rows_first_and_last() {
    let width = 4u32;
    let cell_h = 3u32;
    let rows = 4u32;
    let height = rows * cell_h;
    let stride = width * 4;
    let (mut buf, _) = test_surface(width, height);
    let grid = GridMetrics {
        cell_w: width,
        cell_h,
        pad: 0,
    };
    let bg: Bgra = (200, 100, 50);
    clear_dirty_rows(&mut buf, width, stride, &grid, &[0, 3], bg, 1.0);
    let pixel_bgra = [bg.2, bg.1, bg.0, 0xFF];
    for py in 0..cell_h {
        for px in 0..width {
            let off = (py * stride + px * 4) as usize;
            let p = buf.get(off..off + 4).expect("pixel in bounds");
            assert_eq!(p, &pixel_bgra, "first row pixel ({px},{py}) should be bg");
        }
    }
    for py in cell_h..cell_h * 2 {
        for px in 0..width {
            let off = (py * stride + px * 4) as usize;
            let p = buf.get(off..off + 4).expect("pixel in bounds");
            assert_eq!(
                p,
                &[0, 0, 0, 0],
                "middle row pixel ({px},{py}) should be untouched"
            );
        }
    }
}

#[test]
fn test_clear_dirty_rows_empty_list() {
    let width = 8u32;
    let height = 16u32;
    let stride = width * 4;
    let mut buf = vec![0xABu8; (stride * height) as usize];
    let original = buf.clone();
    let grid = GridMetrics {
        cell_w: width,
        cell_h: 8,
        pad: 0,
    };
    clear_dirty_rows(&mut buf, width, stride, &grid, &[], (0, 0, 0), 1.0);
    assert_eq!(buf, original, "empty dirty list should not modify buffer");
}

#[test]
fn test_blend_channel_commutativity_counterexample() {
    let r1 = blend_channel(200, 50, 100);
    let r2 = blend_channel(50, 200, 100);
    assert_ne!(
        r1, r2,
        "blend_channel should NOT be commutative: blend(200,50,100)={r1} vs blend(50,200,100)={r2}"
    );
}

#[test]
fn test_draw_rect_zero_width() {
    let (mut buf, _) = test_surface(10, 10);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride: 40,
    };
    let rect = Rect {
        x: 3,
        y: 3,
        w: 0,
        h: 5,
    };
    draw_rect(&mut surface, &rect, (255, 128, 64));
    assert_eq!(
        surface.buf,
        &original[..],
        "zero-width rect should not change any pixels"
    );
}

#[test]
fn test_draw_rect_zero_height() {
    let (mut buf, _) = test_surface(10, 10);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 10,
        stride: 40,
    };
    let rect = Rect {
        x: 3,
        y: 3,
        w: 5,
        h: 0,
    };
    draw_rect(&mut surface, &rect, (255, 128, 64));
    assert_eq!(
        surface.buf,
        &original[..],
        "zero-height rect should not change any pixels"
    );
}

#[test]
fn test_set_pixel_bgra_order() {
    let (mut buf, _) = test_surface(4, 4);
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    set_pixel(&mut surface, 0, 0, (0xAA, 0xBB, 0xCC));
    assert_eq!(
        *surface.buf.first().expect("byte 0 = B"),
        0xCC,
        "byte 0 should be B"
    );
    assert_eq!(
        *surface.buf.get(1).expect("byte 1 = G"),
        0xBB,
        "byte 1 should be G"
    );
    assert_eq!(
        *surface.buf.get(2).expect("byte 2 = R"),
        0xAA,
        "byte 2 should be R"
    );
    assert_eq!(
        *surface.buf.get(3).expect("byte 3 = A"),
        0xFF,
        "byte 3 should be A"
    );
}

#[test]
fn test_draw_rect_alpha_half_blending() {
    let (mut buf, _) = test_surface(4, 4);
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[255, 255, 255, 255]);
    }
    let mut surface = Surface {
        buf: &mut buf,
        width: 4,
        height: 4,
        stride: 16,
    };
    let rect = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    draw_rect_alpha(&mut surface, &rect, (0, 0, 0), 128);
    for chunk in surface.buf.chunks_exact(4) {
        let b_val = *chunk.first().expect("B");
        let g_val = *chunk.get(1).expect("G");
        let r_val = *chunk.get(2).expect("R");
        let a_val = *chunk.get(3).expect("A");
        assert!(
            (100..=160).contains(&b_val),
            "B channel should be ~128, got {b_val}"
        );
        assert!(
            (100..=160).contains(&g_val),
            "G channel should be ~128, got {g_val}"
        );
        assert!(
            (100..=160).contains(&r_val),
            "R channel should be ~128, got {r_val}"
        );
        assert_eq!(a_val, 0xFF, "alpha should be 0xFF");
    }
}

#[test]
fn test_clear_background_fast_path_various_sizes() {
    for width in [1u32, 2, 3, 7, 16, 100] {
        let height = 3u32;
        let stride = width * 4;
        let total = (stride * height) as usize;
        let mut buf = vec![0u8; total];
        let bg: Bgra = (0xAA, 0xBB, 0xCC);
        clear_background(&mut buf, width, height, stride, bg, 1.0);
        for offset in (0..total).step_by(4) {
            let pixel = buf.get(offset..offset + 4).expect("pixel slice");
            assert_eq!(
                pixel,
                &[0xCC, 0xBB, 0xAA, 0xFF],
                "width={width} pixel at byte offset {offset}"
            );
        }
    }
}

#[test]
fn test_clear_background_stride_padding() {
    let width = 3u32;
    let height = 2u32;
    let stride = 16u32;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0xFFu8; buf_len];
    let bg: Bgra = (10, 20, 30);
    clear_background(&mut buf, width, height, stride, bg, 1.0);
    let expected_pixel = [30u8, 20, 10, 0xFF];
    for row in 0..height as usize {
        let row_start = row * stride as usize;
        let row_end = row_start + (width * 4) as usize;
        for offset in (row_start..row_end).step_by(4) {
            let pixel = buf.get(offset..offset + 4).expect("pixel");
            assert_eq!(pixel, &expected_pixel, "row {row} active pixel");
        }
        let pad_start = row_end;
        let pad_end = row_start + stride as usize;
        for offset in pad_start..pad_end {
            assert_eq!(
                *buf.get(offset).expect("padding byte"),
                0xFF,
                "row {row} padding byte at offset {offset}"
            );
        }
    }
}

#[test]
fn test_clear_dirty_rows_various_widths() {
    for width in [1u32, 3, 7, 16, 100] {
        let cell_h = 2u32;
        let stride = width * 4;
        let height = cell_h * 2;
        let total = (stride * height) as usize;
        let mut buf = vec![0u8; total];
        let grid = GridMetrics {
            cell_w: width,
            cell_h,
            pad: 0,
        };
        let bg: Bgra = (50, 100, 150);
        clear_dirty_rows(&mut buf, width, stride, &grid, &[0], bg, 1.0);
        let expected = [150u8, 100, 50, 0xFF];
        for py in 0..cell_h as usize {
            for px_idx in 0..width as usize {
                let off = py * stride as usize + px_idx * 4;
                let pixel = buf.get(off..off + 4).expect("pixel");
                assert_eq!(pixel, &expected, "width={width} dirty row ({px_idx},{py})");
            }
        }
        for py in cell_h as usize..(cell_h * 2) as usize {
            for px_idx in 0..width as usize {
                let off = py * stride as usize + px_idx * 4;
                let pixel = buf.get(off..off + 4).expect("pixel");
                assert_eq!(
                    pixel,
                    &[0, 0, 0, 0],
                    "width={width} clean row ({px_idx},{py})"
                );
            }
        }
    }
}

#[test]
fn test_draw_cells_explicit_bg_color() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[48;5;1mX\x1b[0m");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "cell with explicit bg should have non-bg pixels"
    );
}

#[test]
fn test_draw_cells_inverse_attribute() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[7mINV\x1b[0m");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "inverse cell should have non-bg pixels"
    );
}

#[test]
fn test_draw_cells_underline_attribute() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[4mUL\x1b[0m");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "underlined cell should have non-bg pixels"
    );
}

#[test]
fn test_draw_cells_strikethrough_attribute() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[9mST\x1b[0m");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "strikethrough cell should have non-bg pixels"
    );
}

#[test]
fn test_draw_cells_overline_attribute() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[53mOL\x1b[0m");
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "overline cell should have non-bg pixels"
    );
}

#[test]
fn test_draw_cell_text_multi_codepoint() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt("e\u{0301}".as_bytes());
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "multi-codepoint cell should produce visible pixels"
    );
}

#[test]
fn test_draw_cell_text_emoji_multi_codepoint() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt("\u{1F468}\u{200D}\u{1F469}".as_bytes());
    cap.render();
    assert!(
        cap.cell_has_nonbg_pixels(0, 0),
        "emoji sequence should produce visible pixels"
    );
}

#[test]
fn test_draw_underline_variant_zero_noop() {
    let (mut buf, stride) = test_surface(8, 16);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 8,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 8, 16, 0, (255, 255, 255));
    assert_eq!(surface.buf, &original[..], "variant 0 should be noop");
}

#[test]
fn test_draw_underline_double_small_cell() {
    let (mut buf, stride) = test_surface(8, 2);
    let mut surface = Surface {
        buf: &mut buf,
        width: 8,
        height: 2,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 8, 2, 2, (255, 255, 255));
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R byte") == 255)
        .count();
    assert!(
        filled > 0,
        "double underline on tiny cell should fall back to single"
    );
}

#[test]
fn test_draw_underline_default_fallback() {
    let (mut buf, stride) = test_surface(8, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 8,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 8, 16, 99, (255, 255, 255));
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R byte") == 255)
        .count();
    assert_eq!(
        filled, 8,
        "default fallback should draw full-width single line"
    );
}

#[test]
fn test_draw_underline_all_variants_no_panic() {
    for variant in 0..=10 {
        let (mut buf, stride) = test_surface(12, 20);
        let mut surface = Surface {
            buf: &mut buf,
            width: 12,
            height: 20,
            stride,
        };
        draw_underline(&mut surface, 0, 0, 12, 20, variant, (200, 100, 50));
    }
}

#[test]
fn test_draw_selection_multi_row() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Line one content\r\nLine two content\r\nLine three content");
    cap.render();
    let (mid_x, mid_y) = cap.cell_origin(5, 1);
    let baseline = cap.pixel_at(mid_x + 1, mid_y + 1);
    cap.render_with_selection(((3, 0), (5, 2)));
    let selected = cap.pixel_at(mid_x + 1, mid_y + 1);
    assert_ne!(
        baseline, selected,
        "middle row of multi-row selection should change pixels"
    );
}

#[test]
fn test_draw_selection_single_row() {
    let (mut buf, stride) = test_surface(200, 50);
    let mut surface = Surface {
        buf: &mut buf,
        width: 200,
        height: 50,
        stride,
    };
    let grid = GridMetrics {
        cell_w: 8,
        cell_h: 16,
        pad: 0,
    };
    let term = Terminal::new(20, 3, 10).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    let sel = SelectionRange {
        start_col: 2,
        start_row: 0,
        end_col: 5,
        end_row: 0,
    };
    draw_selection(&mut surface, &grid, &sel, &rs, (None, None));
    let mut nonzero = 0usize;
    for py in 0..16u32 {
        for px in 16..48u32 {
            let off = (py * stride + px * 4) as usize;
            if buf.get(off + 1).copied().unwrap_or(0) != 0 {
                nonzero += 1;
            }
        }
    }
    assert!(nonzero > 0, "single-row selection should paint pixels");
}

#[test]
fn test_draw_cursor_bar_style() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[6 q");
    cap.render();
    let (ox, oy) = cap.cell_origin(0, 0);
    let cursor_px = cap.pixel_at(ox, oy + 1);
    let bg_origin = cap.cell_origin(40, 12);
    let nocursor_px = cap.pixel_at(bg_origin.0, bg_origin.1 + 1);
    assert_ne!(
        cursor_px, nocursor_px,
        "bar cursor should produce non-background pixels"
    );
}

#[test]
fn test_draw_cursor_underline_style() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"\x1b[4 q");
    cap.render();
    let (ox, oy) = cap.cell_origin(0, 0);
    let ch = cap.font.cell_height;
    let cursor_px = cap.pixel_at(ox + 1, oy + ch - 1);
    let bg_origin = cap.cell_origin(40, 12);
    let nocursor_px = cap.pixel_at(bg_origin.0 + 1, bg_origin.1 + cap.font.cell_height - 1);
    assert_ne!(
        cursor_px, nocursor_px,
        "underline cursor should produce non-background pixels at bottom"
    );
}

#[test]
fn test_draw_cursor_block_hollow_style() {
    let mut cap = RenderCapture::new(80, 24);
    cap.render();
    let cw = cap.font.cell_width;
    let ch = cap.font.cell_height;
    for py in 0..ch {
        for px_col in 0..cw {
            let off = (py * cap.stride + px_col * 4) as usize;
            if let Some(pixel) = cap.buf.get_mut(off..off + 4) {
                pixel.copy_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    let mut surface = Surface {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
    };
    let cursor_color = (200, 200, 200);
    let top = Rect {
        x: 0,
        y: 0,
        w: cw,
        h: 1,
    };
    let bottom = Rect {
        x: 0,
        y: ch - 1,
        w: cw,
        h: 1,
    };
    let left = Rect {
        x: 0,
        y: 0,
        w: 1,
        h: ch,
    };
    let right = Rect {
        x: cw - 1,
        y: 0,
        w: 1,
        h: ch,
    };
    draw_rect(&mut surface, &top, cursor_color);
    draw_rect(&mut surface, &bottom, cursor_color);
    draw_rect(&mut surface, &left, cursor_color);
    draw_rect(&mut surface, &right, cursor_color);
    assert_eq!(
        *surface.buf.get(2).expect("R"),
        200,
        "hollow block top-left should have cursor color"
    );
    if cw > 2 && ch > 2 {
        let interior_off = (cap.stride + 4) as usize;
        assert_eq!(
            *surface.buf.get(interior_off + 2).expect("R"),
            0,
            "hollow block interior should be empty"
        );
    }
}

#[test]
fn test_draw_scrollbar_visible_thumb() {
    let (mut buf, stride) = test_surface(100, 200);
    let mut surface = Surface {
        buf: &mut buf,
        width: 100,
        height: 200,
        stride,
    };
    let sb = ffi::GhosttyTerminalScrollbar {
        total: 500,
        len: 50,
        offset: 100,
    };
    draw_scrollbar(&mut surface, Some(&sb));
    let bar_x_start = 100 - 6 - 2;
    let mut found_thumb = false;
    for py in 0..200u32 {
        for dx in 0..6u32 {
            let off = (py * stride + (bar_x_start + dx) * 4) as usize;
            if surface.buf.get(off + 2).copied().unwrap_or(0) != 0 {
                found_thumb = true;
                break;
            }
        }
        if found_thumb {
            break;
        }
    }
    assert!(found_thumb, "scrollbar thumb should be visible");
}

#[test]
fn test_draw_scrollbar_at_bottom_hidden() {
    let (mut buf, stride) = test_surface(100, 200);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 100,
        height: 200,
        stride,
    };
    let sb = ffi::GhosttyTerminalScrollbar {
        total: 100,
        len: 50,
        offset: 50,
    };
    draw_scrollbar(&mut surface, Some(&sb));
    assert_eq!(
        surface.buf,
        &original[..],
        "scrollbar at bottom should be hidden"
    );
}

#[test]
fn test_draw_scrollbar_total_lte_len_hidden() {
    let (mut buf, stride) = test_surface(100, 200);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 100,
        height: 200,
        stride,
    };
    let sb = ffi::GhosttyTerminalScrollbar {
        total: 24,
        len: 24,
        offset: 0,
    };
    draw_scrollbar(&mut surface, Some(&sb));
    assert_eq!(
        surface.buf,
        &original[..],
        "scrollbar with total <= len should be hidden"
    );
}

#[test]
fn test_draw_scrollbar_none() {
    let (mut buf, stride) = test_surface(100, 200);
    let original = buf.clone();
    let mut surface = Surface {
        buf: &mut buf,
        width: 100,
        height: 200,
        stride,
    };
    draw_scrollbar(&mut surface, None);
    assert_eq!(surface.buf, &original[..], "no scrollbar should be noop");
}

#[test]
fn test_draw_preedit_nonempty() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"AB");
    cap.render_state
        .update(cap.terminal.inner())
        .expect("update");
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: Some("xyz"),
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
        retained: &mut cap.retained,
    };
    let colors = cap.render_state.colors();
    render_frame(
        &mut target,
        &mut cap.render_state,
        &mut cap.font,
        &opts,
        &colors,
    );
    let (cx, cy) = cap.cell_origin(2, 0);
    let bg = cap.pixel_at(cap.width - 1, cap.height - 1);
    let mut found = false;
    for dx in 0..cap.font.cell_width * 3 {
        for dy in 0..cap.font.cell_height {
            if cx + dx < cap.width && cy + dy < cap.height {
                let px = cap.pixel_at(cx + dx, cy + dy);
                if px != bg {
                    found = true;
                    break;
                }
            }
        }
        if found {
            break;
        }
    }
    assert!(found, "preedit text should render visible pixels");
}

#[test]
fn test_draw_search_bar_cursor_at_end() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"text");
    cap.render_state
        .update(cap.terminal.inner())
        .expect("update");
    let query = "hello";
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: Some(query),
        search_cursor: query.len(),
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
        retained: &mut cap.retained,
    };
    let colors = cap.render_state.colors();
    render_frame(
        &mut target,
        &mut cap.render_state,
        &mut cap.font,
        &opts,
        &colors,
    );
    let bg = cap.pixel_at(cap.width / 2, 0);
    let last_y = cap.height.saturating_sub(1);
    let mut found = false;
    for px_x in 0..cap.width.min(300) {
        let px = cap.pixel_at(px_x, last_y);
        if px != bg {
            found = true;
            break;
        }
    }
    assert!(found, "search bar with cursor at end should render");
}

#[test]
fn test_draw_search_bar_match_count_suffix() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"content");
    cap.render_state
        .update(cap.terminal.inner())
        .expect("update");
    let highlights = [
        SearchHighlight {
            row: 0,
            start_col: 0,
            end_col: 2,
            is_current: true,
        },
        SearchHighlight {
            row: 0,
            start_col: 5,
            end_col: 7,
            is_current: false,
        },
    ];
    let query = "abc";
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &highlights,
        search_bar: Some(query),
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
        retained: &mut cap.retained,
    };
    let colors = cap.render_state.colors();
    render_frame(
        &mut target,
        &mut cap.render_state,
        &mut cap.font,
        &opts,
        &colors,
    );
    let bg = cap.pixel_at(cap.width / 2, 0);
    let last_y = cap.height.saturating_sub(1);
    let mut found = false;
    for px_x in 0..cap.width.min(300) {
        let px = cap.pixel_at(px_x, last_y);
        if px != bg {
            found = true;
            break;
        }
    }
    assert!(found, "search bar with match count should render");
}

#[test]
fn test_draw_search_bar_empty_query_no_suffix() {
    let mut cap = RenderCapture::new(80, 24);
    cap.render_state
        .update(cap.terminal.inner())
        .expect("update");
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: Some(""),
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
        retained: &mut cap.retained,
    };
    let colors = cap.render_state.colors();
    render_frame(
        &mut target,
        &mut cap.render_state,
        &mut cap.font,
        &opts,
        &colors,
    );
    let bg = cap.pixel_at(cap.width / 2, 0);
    let last_y = cap.height.saturating_sub(1);
    let mut found = false;
    for px_x in 0..cap.width.min(300) {
        let px = cap.pixel_at(px_x, last_y);
        if px != bg {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "search bar with empty query should still render prefix"
    );
}

#[test]
fn test_draw_underline_curly_has_wavy_pattern() {
    let (mut buf, stride) = test_surface(16, 20);
    let mut surface = Surface {
        buf: &mut buf,
        width: 16,
        height: 20,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 16, 20, 3, (255, 255, 255));
    let ul_y = 18u32;
    let mut rows_hit = [false; 2];
    for dx in 0..16u32 {
        for (idx, dy) in [0u32, 1].iter().enumerate() {
            let py = ul_y + dy;
            if py < 20 {
                let off = (py * stride + dx * 4) as usize;
                if *surface.buf.get(off + 2).expect("R") == 255
                    && let Some(slot) = rows_hit.get_mut(idx)
                {
                    *slot = true;
                }
            }
        }
    }
    assert!(
        rows_hit.iter().all(|&h| h),
        "curly underline should have pixels on two rows (wave pattern)"
    );
}

#[test]
fn test_draw_underline_dotted_exact_count() {
    let (mut buf, stride) = test_surface(10, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 10,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 10, 16, 4, (255, 255, 255));
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R") == 255)
        .count();
    assert_eq!(
        filled, 5,
        "dotted underline for width=10 should have 5 pixels"
    );
}

#[test]
fn test_draw_underline_dashed_pattern() {
    let (mut buf, stride) = test_surface(18, 16);
    let mut surface = Surface {
        buf: &mut buf,
        width: 18,
        height: 16,
        stride,
    };
    draw_underline(&mut surface, 0, 0, 18, 16, 5, (255, 255, 255));
    let filled: usize = surface
        .buf
        .chunks_exact(4)
        .filter(|c| *c.get(2).expect("R") == 255)
        .count();
    assert_eq!(
        filled, 9,
        "dashed underline for width=18 should have 9 on-pixels"
    );
}

#[test]
fn test_draw_selection_custom_colors() {
    let mut cap = RenderCapture::new(80, 24);
    cap.write_vt(b"Some text here");
    cap.render_state
        .update(cap.terminal.inner())
        .expect("update");
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: Some(((0, 0), (5, 0))),
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: Some((255, 0, 0)),
        selection_bg: Some((0, 255, 0)),
    };
    let mut target = RenderTarget {
        buf: &mut cap.buf,
        width: cap.width,
        height: cap.height,
        stride: cap.stride,
        retained: &mut cap.retained,
    };
    let colors = cap.render_state.colors();
    render_frame(
        &mut target,
        &mut cap.render_state,
        &mut cap.font,
        &opts,
        &colors,
    );
    let (ox, oy) = cap.cell_origin(2, 0);
    let selected_pixel = cap.pixel_at(ox + 1, oy + 1);
    let bg_origin = cap.cell_origin(20, 10);
    let unselected_pixel = cap.pixel_at(bg_origin.0 + 1, bg_origin.1 + 1);
    assert_ne!(
        selected_pixel, unselected_pixel,
        "custom selection color should alter pixels"
    );
}

#[test]
fn test_draw_scrollbar_small_viewport() {
    let (mut buf, stride) = test_surface(100, 200);
    let mut surface = Surface {
        buf: &mut buf,
        width: 100,
        height: 200,
        stride,
    };
    let sb = ffi::GhosttyTerminalScrollbar {
        total: 100_000,
        len: 10,
        offset: 50_000,
    };
    draw_scrollbar(&mut surface, Some(&sb));
    let bar_x_start = 100 - 6 - 2;
    let mut thumb_rows = 0u32;
    for py in 0..200u32 {
        let off = (py * stride + bar_x_start * 4) as usize;
        if surface.buf.get(off + 2).copied().unwrap_or(0) != 0 {
            thumb_rows += 1;
        }
    }
    assert!(
        thumb_rows >= 10,
        "scrollbar thumb should be at least 10px, got {thumb_rows}"
    );
}

// ---- copy_retained_to_shm length mismatch guard tests ----

#[test]
fn test_copy_retained_to_shm_equal_lengths() {
    let mut shm = vec![0u8; 100];
    let mut retained = vec![42u8; 100];
    let mut target = RenderTarget {
        buf: &mut shm,
        width: 5,
        height: 5,
        stride: 20,
        retained: &mut retained,
    };
    copy_retained_to_shm(&mut target);
    assert!(
        target.buf.iter().all(|&b| b == 42),
        "equal-length copy should transfer all bytes"
    );
}

#[test]
fn test_copy_retained_to_shm_retained_shorter() {
    // Retained buffer is smaller than SHM (e.g. resize grew the window).
    // Should copy what's available and zero-fill the rest.
    let mut shm = vec![0xFFu8; 200];
    let mut retained = vec![42u8; 100];
    let mut target = RenderTarget {
        buf: &mut shm,
        width: 10,
        height: 5,
        stride: 40,
        retained: &mut retained,
    };
    copy_retained_to_shm(&mut target);
    // First 100 bytes should be from retained
    assert!(
        target
            .buf
            .get(..100)
            .expect("first 100 bytes")
            .iter()
            .all(|&b| b == 42),
        "first portion should be copied from retained"
    );
    // Remaining bytes should be zeroed
    assert!(
        target
            .buf
            .get(100..)
            .expect("remaining bytes")
            .iter()
            .all(|&b| b == 0),
        "remainder should be zero-filled"
    );
}

#[test]
fn test_copy_retained_to_shm_retained_longer() {
    // Retained buffer is larger than SHM (e.g. resize shrank the window).
    // Should copy only up to SHM length without panic.
    let mut shm = vec![0u8; 100];
    let mut retained = vec![42u8; 200];
    let mut target = RenderTarget {
        buf: &mut shm,
        width: 5,
        height: 5,
        stride: 20,
        retained: &mut retained,
    };
    copy_retained_to_shm(&mut target);
    assert!(
        target.buf.iter().all(|&b| b == 42),
        "SHM buffer should be fully filled from retained"
    );
}

#[test]
fn test_copy_retained_to_shm_empty_retained() {
    // Empty retained buffer (first frame before any render).
    let mut shm = vec![0xFFu8; 100];
    let mut retained: Vec<u8> = Vec::new();
    let mut target = RenderTarget {
        buf: &mut shm,
        width: 5,
        height: 5,
        stride: 20,
        retained: &mut retained,
    };
    copy_retained_to_shm(&mut target);
    assert!(
        target.buf.iter().all(|&b| b == 0),
        "SHM buffer should be zero-filled when retained is empty"
    );
}
