use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use horseshoe::font::FontManager;
use horseshoe::renderer::{
    self, Rect, RenderOptions, RenderTarget, SearchHighlight, Surface, blend_channel,
    clear_background, draw_rect, draw_rect_alpha,
};
use horseshoe::terminal::render::RenderState;
use horseshoe::terminal::vt::{Terminal, TerminalCb, TerminalOps};

// ---------------------------------------------------------------------------
// Rendering pipeline benchmarks
// ---------------------------------------------------------------------------

fn bench_clear_background_1080p(c: &mut Criterion) {
    let width: u32 = 1920;
    let height: u32 = 1080;
    let stride = width * 4;
    let mut buf = vec![0u8; (stride * height) as usize];
    let _ = c.bench_function("clear_background_1080p", |b| {
        b.iter(|| {
            clear_background(
                black_box(&mut buf),
                black_box(width),
                black_box(height),
                black_box(stride),
                black_box((30, 30, 30)),
                black_box(1.0),
            );
        });
    });
}

fn bench_render_frame_empty_80x24(c: &mut Criterion) {
    let terminal = Terminal::new(80, 24, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_frame_empty_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

fn bench_render_frame_text_80x24(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 24, 100).expect("terminal");
    // Fill terminal with ASCII text
    for _ in 0..24 {
        let line: String = (0..80u8).map(|i| (b'A' + (i % 26)) as char).collect();
        terminal.vt_write(line.as_bytes());
        terminal.vt_write(b"\r\n");
    }
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_frame_text_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

fn bench_render_frame_dirty_single_row(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 24, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    // Initial full render (scoped to avoid shadow_unrelated lint)
    {
        let _ = render_state.update(terminal.inner());
        let init_opts = RenderOptions {
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
        let mut init_target = RenderTarget {
            buf: &mut buf,
            width,
            height,
            stride,
            retained: &mut retained,
        };
        let colors = render_state.colors();
        renderer::render_frame(
            &mut init_target,
            &mut render_state,
            &mut font,
            &init_opts,
            &colors,
        );
        render_state.clear_dirty();
    }

    let _ = c.bench_function("render_frame_dirty_single_row", |b| {
        b.iter(|| {
            terminal.vt_write(b"X");
            let _ = render_state.update(terminal.inner());
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

// ---------------------------------------------------------------------------
// Font subsystem benchmarks
// ---------------------------------------------------------------------------

fn bench_glyph_rasterize_cache_miss(c: &mut Criterion) {
    let mut font = FontManager::new_with_family(16.0, None);
    let _ = c.bench_function("glyph_rasterize_cache_miss", |b| {
        b.iter(|| {
            font.clear_cache();
            let _ = black_box(font.rasterize("A", false, false));
        });
    });
}

fn bench_glyph_rasterize_cache_hit(c: &mut Criterion) {
    let mut font = FontManager::new_with_family(16.0, None);
    // Warm the cache
    let _ = font.rasterize("A", false, false);
    let _ = c.bench_function("glyph_rasterize_cache_hit", |b| {
        b.iter(|| {
            let _ = black_box(font.rasterize("A", false, false));
        });
    });
}

// ---------------------------------------------------------------------------
// VT parsing benchmarks
// ---------------------------------------------------------------------------

fn bench_vt_write_16kb_ascii(c: &mut Criterion) {
    // 200 cols × 100 rows = 20 000 cells > 16 384 bytes.
    // This prevents scroll operations from dominating the benchmark,
    // so we measure pure VT parsing + cell write throughput.
    let mut terminal = Terminal::new(200, 100, 0).expect("terminal");
    let data: Vec<u8> = (0u16..16384)
        .map(|i| b'A' + u8::try_from(i % 26).expect("mod 26 fits"))
        .collect();
    let _ = c.bench_function("vt_write_16kb_ascii", |b| {
        b.iter(|| {
            // Reset cursor to top-left so no scrolling occurs.
            terminal.vt_write(b"\x1b[H");
            terminal.vt_write(black_box(&data));
        });
    });
}

fn bench_vt_write_16kb_csi_heavy(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 24, 100).expect("terminal");
    let mut data = Vec::with_capacity(16384);
    while data.len() < 16384 {
        data.extend_from_slice(b"\x1b[31m");
        data.extend_from_slice(b"Hello");
        data.extend_from_slice(b"\x1b[1;1H");
        data.extend_from_slice(b"World");
        data.extend_from_slice(b"\x1b[0m");
    }
    data.truncate(16384);
    let _ = c.bench_function("vt_write_16kb_csi_heavy", |b| {
        b.iter(|| {
            terminal.vt_write(black_box(&data));
        });
    });
}

fn bench_vt_cb_write_16kb(c: &mut Criterion) {
    // Large grid to avoid scroll overhead, same as ascii benchmark.
    let mut terminal = TerminalCb::new(200, 100, 0).expect("create terminal");
    let mut data = Vec::with_capacity(16384);
    while data.len() < 16384 {
        data.extend_from_slice(b"Normal text here. ");
        data.extend_from_slice(b"\x1b[c");
        data.extend_from_slice(b"More text. ");
        data.extend_from_slice(b"\x1b[6n");
    }
    data.truncate(16384);
    let _ = c.bench_function("vt_cb_write_16kb", |b| {
        b.iter(|| {
            terminal.vt_write(b"\x1b[H");
            terminal.vt_write(black_box(&data));
            let _ = black_box(terminal.take_pty_responses());
        });
    });
}

// ---------------------------------------------------------------------------
// Pixel primitive benchmarks
// ---------------------------------------------------------------------------

fn bench_draw_rect_100x100(c: &mut Criterion) {
    let width: u32 = 200;
    let height: u32 = 200;
    let stride = width * 4;
    let mut buf = vec![0u8; (stride * height) as usize];
    let _ = c.bench_function("draw_rect_100x100", |b| {
        b.iter(|| {
            let mut surface = Surface {
                buf: &mut buf,
                width,
                height,
                stride,
            };
            let rect = Rect {
                x: 50,
                y: 50,
                w: 100,
                h: 100,
            };
            draw_rect(&mut surface, &rect, black_box((255, 128, 64)));
        });
    });
}

fn bench_draw_rect_alpha_100x100(c: &mut Criterion) {
    let width: u32 = 200;
    let height: u32 = 200;
    let stride = width * 4;
    let mut buf = vec![128u8; (stride * height) as usize];
    let _ = c.bench_function("draw_rect_alpha_100x100", |b| {
        b.iter(|| {
            let mut surface = Surface {
                buf: &mut buf,
                width,
                height,
                stride,
            };
            let rect = Rect {
                x: 50,
                y: 50,
                w: 100,
                h: 100,
            };
            draw_rect_alpha(&mut surface, &rect, black_box((255, 128, 64)), 128);
        });
    });
}

fn bench_blend_channel_1m(c: &mut Criterion) {
    let _ = c.bench_function("blend_channel_1m", |b| {
        b.iter(|| {
            for i in 0u16..1000 {
                let fg = u8::try_from(i % 256).expect("fits");
                let bg = 255u8.wrapping_sub(fg);
                let alpha = i % 256;
                let _ = black_box(blend_channel(fg, bg, alpha));
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Selective copy benchmarks (cursor blink, no content change)
// ---------------------------------------------------------------------------

fn bench_render_frame_cursor_blink_80x24(c: &mut Criterion) {
    let terminal = Terminal::new(80, 24, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    // Initial full render.
    {
        let _ = render_state.update(terminal.inner());
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
            buf: &mut buf,
            width,
            height,
            stride,
            retained: &mut retained,
        };
        let colors = render_state.colors();
        renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
        render_state.clear_dirty();
    }

    // Benchmark: no content change, just re-render (simulates cursor blink).
    let _ = c.bench_function("render_frame_cursor_blink_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

fn bench_render_frame_cursor_blink_1080p(c: &mut Criterion) {
    let cols: u16 = 240;
    let rows: u16 = 67;
    let terminal = Terminal::new(cols, rows, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = u32::from(cols) * font.cell_width;
    let height = u32::from(rows) * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    {
        let _ = render_state.update(terminal.inner());
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
            buf: &mut buf,
            width,
            height,
            stride,
            retained: &mut retained,
        };
        let colors = render_state.colors();
        renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
        render_state.clear_dirty();
    }

    let _ = c.bench_function("render_frame_cursor_blink_1080p", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

// ---------------------------------------------------------------------------
// Selection & search overlay benchmarks
// ---------------------------------------------------------------------------

fn bench_draw_rect_alpha_selection_row(c: &mut Criterion) {
    // A full-width 1920×16 row alpha-blend — typical selection overlay.
    let width: u32 = 1920;
    let height: u32 = 16;
    let stride = width * 4;
    let mut buf = vec![128u8; (stride * height) as usize];
    let _ = c.bench_function("draw_rect_alpha_selection_row_1920x16", |b| {
        b.iter(|| {
            let mut surface = Surface {
                buf: &mut buf,
                width,
                height,
                stride,
            };
            let rect = Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 16,
            };
            draw_rect_alpha(&mut surface, &rect, black_box((50, 100, 200)), 80);
        });
    });
}

fn bench_render_frame_with_selection(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 24, 100).expect("terminal");
    // Fill with text
    for _ in 0..24 {
        let line: String = (0..80u8).map(|i| (b'A' + (i % 26)) as char).collect();
        terminal.vt_write(line.as_bytes());
        terminal.vt_write(b"\r\n");
    }
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    // 10-row selection range
    let selection = Some(((0u16, 5u16), (79u16, 14u16)));

    let _ = c.bench_function("render_frame_with_selection_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
            let opts = RenderOptions {
                scrollbar: None,
                bold_is_bright: true,
                cursor_blink_visible: true,
                selection,
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

fn bench_search_matches_large_grid(c: &mut Criterion) {
    use horseshoe::selection::SearchMatch;
    // Create a 200×100 terminal with repeated text containing a search target.
    let cols: u16 = 200;
    let rows: u16 = 100;
    let mut terminal = Terminal::new(cols, rows, 0).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");

    // Fill grid: each row has "Hello World " repeated, giving ~50 "Hello" matches.
    for _ in 0..rows {
        let line = "Hello World ".repeat(17); // 12*17 = 204 chars, truncated to 200 cols
        terminal.vt_write(line.as_bytes());
    }
    let _ = render_state.update(terminal.inner());

    let query_lower = "hello";

    let _ = c.bench_function("search_matches_200x100_grid", |b| {
        b.iter(|| {
            let cols_usize = usize::from(cols);
            let rows_usize = usize::from(rows);
            let mut row_texts: Vec<String> = (0..rows_usize)
                .map(|_| String::with_capacity(cols_usize))
                .collect();

            render_state.for_each_cell(|row, _col, codepoints, _style, _is_wide| {
                if let Some(buf) = row_texts.get_mut(row) {
                    if codepoints.is_empty() {
                        buf.push(' ');
                    } else {
                        for &cp in codepoints {
                            if let Some(ch) = char::from_u32(cp) {
                                buf.push(ch);
                            }
                        }
                    }
                }
            });

            let mut matches = Vec::new();
            for (row_idx, text) in row_texts.iter().enumerate() {
                let row_u16 = u16::try_from(row_idx).unwrap_or(u16::MAX);
                let text_lower = text.to_lowercase();
                let query_len = query_lower.len();
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(query_lower) {
                    let abs_pos = start + pos;
                    let start_col = u16::try_from(abs_pos).unwrap_or(u16::MAX);
                    let end_col = u16::try_from(abs_pos + query_len - 1).unwrap_or(u16::MAX);
                    matches.push(SearchMatch {
                        row: row_u16,
                        start_col,
                        end_col,
                    });
                    start = abs_pos + 1;
                }
            }
            let _ = black_box(&matches);
        });
    });
}

fn bench_font_rebuild_at_size(c: &mut Criterion) {
    let mut font = FontManager::new_with_family(16.0, None);
    // Warm the cache with some glyphs
    for ch in b'A'..=b'Z' {
        let bytes = [ch];
        let s = std::str::from_utf8(&bytes).expect("ascii");
        let _ = font.rasterize(s, false, false);
    }

    let _ = c.bench_function("font_rebuild_at_size", |b| {
        b.iter(|| {
            // Alternate between two sizes to force actual rebuilds
            font.rebuild_at_size(black_box(18.0));
            font.rebuild_at_size(black_box(16.0));
        });
    });
}

// ---------------------------------------------------------------------------
// FFI overhead: measure render_state.update() alone
// ---------------------------------------------------------------------------

fn bench_render_state_update_80x24(c: &mut Criterion) {
    let terminal = Terminal::new(80, 24, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    // Warm up
    let _ = render_state.update(terminal.inner());

    let _ = c.bench_function("render_state_update_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(black_box(terminal.inner()));
        });
    });
}

fn bench_render_state_update_1080p(c: &mut Criterion) {
    let terminal = Terminal::new(240, 67, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let _ = render_state.update(terminal.inner());

    let _ = c.bench_function("render_state_update_1080p", |b| {
        b.iter(|| {
            let _ = render_state.update(black_box(terminal.inner()));
        });
    });
}

// ---------------------------------------------------------------------------
// Glyph-level and color resolution benchmarks
// ---------------------------------------------------------------------------

/// Render a single glyph in a 1-cell terminal — isolates `draw_glyph` cost.
fn bench_render_single_glyph(c: &mut Criterion) {
    let mut terminal = Terminal::new(1, 1, 0).expect("terminal");
    terminal.vt_write(b"X");
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = font.cell_width;
    let height = font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_single_glyph_1x1", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
            let opts = RenderOptions {
                scrollbar: None,
                bold_is_bright: true,
                cursor_blink_visible: false,
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

/// Render 80 glyphs in a single row — measures glyph blitting throughput.
fn bench_render_full_row_80(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 1, 0).expect("terminal");
    let line: String = (0..80u8).map(|i| (b'A' + (i % 26)) as char).collect();
    terminal.vt_write(line.as_bytes());
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_full_row_80_glyphs", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
            let opts = RenderOptions {
                scrollbar: None,
                bold_is_bright: true,
                cursor_blink_visible: false,
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

/// Box-drawing char render — measures procedural path (stack-allocated).
fn bench_box_drawing_single_cell(c: &mut Criterion) {
    use horseshoe::boxdraw::draw_box_char_into;

    let cell_w: u32 = 8;
    let cell_h: u32 = 16;
    let buf_len = (cell_w * cell_h) as usize;
    let mut buf = vec![0u8; buf_len];

    let _ = c.bench_function("box_drawing_single_cell_8x16", |b| {
        b.iter(|| {
            buf.fill(0);
            let _ = black_box(draw_box_char_into(
                black_box(0x2500), // BOX DRAWINGS LIGHT HORIZONTAL
                cell_w,
                cell_h,
                &mut buf,
            ));
        });
    });
}

/// Color resolution via `for_each_cell` iteration — exercises `resolve_cell_colors`.
fn bench_color_resolution_via_iteration(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 24, 100).expect("terminal");
    // Fill with colored text to exercise various color paths
    for _ in 0..24 {
        terminal.vt_write(b"\x1b[1;31m"); // bold red
        terminal.vt_write(b"ABCDEFGHIJ");
        terminal.vt_write(b"\x1b[2;32m"); // faint green
        terminal.vt_write(b"KLMNOPQRST");
        terminal.vt_write(b"\x1b[7;34m"); // inverse blue
        terminal.vt_write(b"UVWXYZ0123");
        terminal.vt_write(b"\x1b[0m");
        terminal.vt_write(b"4567890abc");
        terminal.vt_write(b"defghijklmnopqrstuvwxyz012345678\r\n");
    }
    let mut render_state = RenderState::new().expect("render state");
    let _ = render_state.update(terminal.inner());

    let _ = c.bench_function("color_resolution_for_each_cell_80x24", |b| {
        b.iter(|| {
            let mut count = 0u32;
            render_state.for_each_cell(|_row, _col, _codepoints, _style, _wide| {
                count += 1;
            });
            let _ = black_box(count);
        });
    });
}

/// Pure memcpy retained→SHM — measures copy bandwidth for 1080p buffer.
fn bench_copy_retained_to_shm_1080p(c: &mut Criterion) {
    let width: u32 = 1920;
    let height: u32 = 1080;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let src = vec![42u8; buf_len];
    let mut dst = vec![0u8; buf_len];

    let _ = c.bench_function("copy_retained_to_shm_1080p", |b| {
        b.iter(|| {
            dst.copy_from_slice(black_box(&src));
            let _ = black_box(&dst);
        });
    });
}

/// Rasterize all 95 printable ASCII chars (cache miss each time).
fn bench_font_rasterize_all_ascii_miss(c: &mut Criterion) {
    let mut font = FontManager::new_with_family(16.0, None);

    let _ = c.bench_function("font_rasterize_all_ascii_cache_miss", |b| {
        b.iter(|| {
            font.clear_cache();
            for ch in 0x20u8..=0x7E {
                let bytes = [ch];
                let s = std::str::from_utf8(&bytes).expect("ascii");
                let _ = black_box(font.rasterize(s, false, false));
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Box-drawing / fill_rect benchmark (via full-block U+2588)
// ---------------------------------------------------------------------------

fn bench_fill_rect_boxdraw(c: &mut Criterion) {
    use horseshoe::boxdraw::draw_box_char_into;

    let cell_w: u32 = 8;
    let cell_h: u32 = 16;
    let buf_len = (cell_w * cell_h) as usize;
    let mut buf = vec![0u8; buf_len];

    let _ = c.bench_function("fill_rect_boxdraw_full_block_8x16", |b| {
        b.iter(|| {
            buf.fill(0);
            let _ = black_box(draw_box_char_into(
                black_box(0x2588), // FULL BLOCK
                cell_w,
                cell_h,
                &mut buf,
            ));
        });
    });
}

// ---------------------------------------------------------------------------
// Box-drawing grid benchmark: 80x24 of U+2500 horizontal lines
// ---------------------------------------------------------------------------

fn bench_draw_box_glyph_grid(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 24, 0).expect("terminal");
    // U+2500 = UTF-8 bytes 0xE2, 0x94, 0x80
    let horiz_line: Vec<u8> = [0xE2u8, 0x94, 0x80].repeat(80);
    for _ in 0..24 {
        terminal.vt_write(&horiz_line);
        terminal.vt_write(b"\r\n");
    }
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("draw_box_glyph_grid_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
            let opts = RenderOptions {
                scrollbar: None,
                bold_is_bright: true,
                cursor_blink_visible: false,
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

// ---------------------------------------------------------------------------
// clear_dirty_rows with 5 out of 24 rows dirty
// ---------------------------------------------------------------------------

fn bench_clear_dirty_rows_partial(c: &mut Criterion) {
    use horseshoe::renderer::{GridMetrics, clear_dirty_rows};

    let cell_w: u32 = 8;
    let cell_h: u32 = 16;
    let cols: u32 = 80;
    let rows: u32 = 24;
    let width = cols * cell_w;
    let height = rows * cell_h;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let grid = GridMetrics {
        cell_w,
        cell_h,
        pad: 0,
    };
    let dirty: [u16; 5] = [2, 5, 10, 15, 20];

    let _ = c.bench_function("clear_dirty_rows_5_of_24", |b| {
        b.iter(|| {
            clear_dirty_rows(
                black_box(&mut buf),
                black_box(width),
                black_box(stride),
                black_box(&grid),
                black_box(&dirty),
                black_box((30, 30, 30)),
                black_box(1.0),
            );
        });
    });
}

// ---------------------------------------------------------------------------
// render_frame with search_bar overlay
// ---------------------------------------------------------------------------

fn bench_render_frame_search_bar(c: &mut Criterion) {
    let terminal = Terminal::new(80, 24, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_frame_search_bar_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
            let opts = RenderOptions {
                scrollbar: None,
                bold_is_bright: true,
                cursor_blink_visible: true,
                selection: None,
                padding: 0,
                opacity: 1.0,
                search_highlights: &[],
                search_bar: Some("hello"),
                search_cursor: 3,
                preedit: None,
                selection_fg: None,
                selection_bg: None,
            };
            let mut target = RenderTarget {
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

// ---------------------------------------------------------------------------
// render_frame with opacity=0.5
// ---------------------------------------------------------------------------

fn bench_render_frame_opacity_half(c: &mut Criterion) {
    let terminal = Terminal::new(80, 24, 100).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 80 * font.cell_width;
    let height = 24 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_frame_opacity_half_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
            let opts = RenderOptions {
                scrollbar: None,
                bold_is_bright: true,
                cursor_blink_visible: true,
                selection: None,
                padding: 0,
                opacity: 0.5,
                search_highlights: &[],
                search_bar: None,
                search_cursor: 0,
                preedit: None,
                selection_fg: None,
                selection_bg: None,
            };
            let mut target = RenderTarget {
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

// ---------------------------------------------------------------------------
// VT scroll stress: 16KB of newlines
// ---------------------------------------------------------------------------

fn bench_vt_write_scroll_heavy(c: &mut Criterion) {
    let mut terminal = Terminal::new(80, 24, 100).expect("terminal");
    let newlines = vec![b'\n'; 16_384];
    let _ = c.bench_function("vt_write_scroll_heavy_16kb_newlines", |b| {
        b.iter(|| {
            terminal.vt_write(black_box(&newlines));
        });
    });
}

// ---------------------------------------------------------------------------
// Scaling & stress benchmarks
// ---------------------------------------------------------------------------

/// Full-frame render of a 240x67 grid filled with ASCII text (1920x1080-ish).
/// Unlike the cursor-blink 1080p benchmark, this measures actual glyph rendering
/// cost at large terminal sizes.
fn bench_render_large_terminal(c: &mut Criterion) {
    let cols: u16 = 240;
    let rows: u16 = 67;
    let mut terminal = Terminal::new(cols, rows, 100).expect("terminal");
    // Fill every row with ASCII text
    for _ in 0..rows {
        let line: String = (0..cols)
            .map(|i| {
                let byte = b'A' + u8::try_from(i % 26).expect("mod 26 fits");
                byte as char
            })
            .collect();
        terminal.vt_write(line.as_bytes());
        terminal.vt_write(b"\r\n");
    }
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = u32::from(cols) * font.cell_width;
    let height = u32::from(rows) * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_large_terminal_240x67_text", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

/// Render an 80x24 grid with 500+ search highlight overlays.
/// Measures the rendering cost of many overlapping highlight rectangles.
fn bench_search_many_matches(c: &mut Criterion) {
    let cols: u16 = 80;
    let rows: u16 = 24;
    let mut terminal = Terminal::new(cols, rows, 100).expect("terminal");
    // Fill every cell with 'A' so every character matches a single-char query
    for _ in 0..rows {
        let line: String = (0..cols).map(|_| 'A').collect();
        terminal.vt_write(line.as_bytes());
        terminal.vt_write(b"\r\n");
    }
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = u32::from(cols) * font.cell_width;
    let height = u32::from(rows) * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    // Build 80*24 = 1920 single-cell highlights (well above 500)
    let mut highlights = Vec::with_capacity(usize::from(cols) * usize::from(rows));
    for r in 0..rows {
        for col in 0..cols {
            highlights.push(SearchHighlight {
                row: r,
                start_col: col,
                end_col: col,
                is_current: r == 0 && col == 0,
            });
        }
    }

    let _ = c.bench_function("render_search_many_matches_1920_highlights", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
            let opts = RenderOptions {
                scrollbar: None,
                bold_is_bright: true,
                cursor_blink_visible: true,
                selection: None,
                padding: 0,
                opacity: 1.0,
                search_highlights: &highlights,
                search_bar: Some("A"),
                search_cursor: 1,
                preedit: None,
                selection_fg: None,
                selection_bg: None,
            };
            let mut target = RenderTarget {
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

/// Render 80x24 grid where each cell uses a different 256-color palette entry.
/// Stresses the color resolution path for indexed colors (SGR 38;5;N).
fn bench_color_256_palette(c: &mut Criterion) {
    let cols: u16 = 80;
    let rows: u16 = 24;
    let mut terminal = Terminal::new(cols, rows, 100).expect("terminal");
    // Write cells with rotating 256-color foreground and background
    let mut idx: u16 = 0;
    for _ in 0..rows {
        for _ in 0..cols {
            let fg = idx % 256;
            let bg = (idx + 128) % 256;
            // SGR 38;5;N = set foreground to palette color N
            // SGR 48;5;N = set background to palette color N
            let seq = format!("\x1b[38;5;{fg}m\x1b[48;5;{bg}mX");
            terminal.vt_write(seq.as_bytes());
            idx += 1;
        }
        terminal.vt_write(b"\r\n");
    }
    let mut render_state = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = u32::from(cols) * font.cell_width;
    let height = u32::from(rows) * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    let _ = c.bench_function("render_color_256_palette_80x24", |b| {
        b.iter(|| {
            let _ = render_state.update(terminal.inner());
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
                buf: &mut buf,
                width,
                height,
                stride,
                retained: &mut retained,
            };
            let colors = render_state.colors();
            renderer::render_frame(&mut target, &mut render_state, &mut font, &opts, &colors);
            render_state.clear_dirty();
        });
    });
}

/// Measure the cost of resizing from 80x24 to 120x40 and back.
/// Each cycle rebuilds the font at a different size and reconstructs the grid,
/// simulating rapid window resize events.
fn bench_rapid_resize(c: &mut Criterion) {
    let mut font = FontManager::new_with_family(16.0, None);
    // Pre-warm glyph cache at default size
    for ch in 0x20u8..=0x7E {
        let bytes = [ch];
        let s = std::str::from_utf8(&bytes).expect("ascii");
        let _ = font.rasterize(s, false, false);
    }

    let _ = c.bench_function("rapid_resize_80x24_to_120x40_cycle", |b| {
        b.iter(|| {
            // Resize to larger: new font size, new terminal + render state
            font.rebuild_at_size(black_box(12.0));
            let cols_large: u16 = 120;
            let rows_large: u16 = 40;
            let terminal_large = Terminal::new(cols_large, rows_large, 100).expect("terminal");
            let mut rs_large = RenderState::new().expect("render state");
            let _ = rs_large.update(terminal_large.inner());

            // Resize back to original
            font.rebuild_at_size(black_box(16.0));
            let cols_small: u16 = 80;
            let rows_small: u16 = 24;
            let terminal_small = Terminal::new(cols_small, rows_small, 100).expect("terminal");
            let mut rs_small = RenderState::new().expect("render state");
            let _ = rs_small.update(terminal_small.inner());

            let _ = black_box(&rs_large);
            let _ = black_box(&rs_small);
        });
    });
}

// ---------------------------------------------------------------------------
// Criterion harness
// ---------------------------------------------------------------------------

criterion_group!(
    rendering,
    bench_clear_background_1080p,
    bench_render_frame_empty_80x24,
    bench_render_frame_text_80x24,
    bench_render_frame_dirty_single_row,
    bench_render_frame_cursor_blink_80x24,
    bench_render_frame_cursor_blink_1080p,
    bench_render_single_glyph,
    bench_render_full_row_80,
    bench_draw_box_glyph_grid,
    bench_clear_dirty_rows_partial,
    bench_render_frame_opacity_half,
    bench_render_large_terminal,
    bench_color_256_palette,
);

criterion_group!(
    font_bench,
    bench_glyph_rasterize_cache_miss,
    bench_glyph_rasterize_cache_hit,
    bench_font_rasterize_all_ascii_miss,
);

criterion_group!(
    vt_parsing,
    bench_vt_write_16kb_ascii,
    bench_vt_write_16kb_csi_heavy,
    bench_vt_cb_write_16kb,
    bench_vt_write_scroll_heavy,
);

criterion_group!(
    primitives,
    bench_draw_rect_100x100,
    bench_draw_rect_alpha_100x100,
    bench_draw_rect_alpha_selection_row,
    bench_blend_channel_1m,
    bench_box_drawing_single_cell,
    bench_copy_retained_to_shm_1080p,
    bench_fill_rect_boxdraw,
);

criterion_group!(
    overlays,
    bench_render_frame_with_selection,
    bench_search_matches_large_grid,
    bench_font_rebuild_at_size,
    bench_render_frame_search_bar,
    bench_search_many_matches,
    bench_rapid_resize,
);

criterion_group!(
    ffi_overhead,
    bench_render_state_update_80x24,
    bench_render_state_update_1080p,
    bench_color_resolution_via_iteration,
);

criterion_main!(
    rendering,
    font_bench,
    vt_parsing,
    primitives,
    overlays,
    ffi_overhead
);
