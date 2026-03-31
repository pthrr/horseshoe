use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::Buffer;
use wayland_client::protocol::wl_shm;

use horseshoe::num::u32_to_i32;
use horseshoe::renderer;
use horseshoe::selection::SearchMatch;
use horseshoe::terminal::TerminalOps;

use super::App;
use super::profiling;

use libghostty_vt::terminal;

impl App {
    pub(super) fn enable_ime(&self) {
        let cursor = self.render_state.cursor();
        self.ime.enable(
            &cursor,
            self.geometry.scale_120,
            self.font.cell_width,
            self.font.cell_height,
            self.display.padding,
        );
    }

    /// Disable IME text input (called on keyboard focus leave).
    pub(super) fn disable_ime(&mut self) {
        self.ime.disable();
    }

    /// Render one frame.  Returns `true` if a frame was actually drawn,
    /// `false` if rendering was skipped (e.g. zero-size surface or
    /// synchronized output).  Callers should only update `dirty` and
    /// `last_render_time` when this returns `true`.
    pub fn draw(&mut self) -> bool {
        let draw_start = std::time::Instant::now();
        if self.geometry.width == 0 || self.geometry.height == 0 {
            return false;
        }

        // Synchronized output (DEC mode 2026): defer rendering while active,
        // but only up to 100ms to prevent stalls from rapid mode toggles
        // (e.g. holding Enter while bash re-draws the prompt each time).
        if self
            .terminal
            .mode_get(terminal::Mode::SYNC_OUTPUT)
            .unwrap_or(false)
            && self.last_render_time.is_some_and(|t| {
                draw_start.duration_since(t) < std::time::Duration::from_millis(100)
            })
        {
            return false;
        }

        // Physical (buffer) dimensions = logical * scale
        let phys_w = self.geometry.phys(self.geometry.width);
        let phys_h = self.geometry.phys(self.geometry.height);
        let stride = phys_w * 4;
        let buf_size = usize::try_from(stride * phys_h).expect("buffer size fits usize");

        // Ensure pool is large enough for double-buffering (2 frames)
        let pool_needed = buf_size * 2;
        if self.wl.pool.len() < pool_needed {
            let _ = self.wl.pool.resize(pool_needed);
        }

        // Compute values before borrowing pool mutably
        let selection = self.selection_range();
        let scrollbar = self.terminal.scrollbar();
        let search_highlights = if self.search.active {
            self.build_search_highlights()
        } else {
            Vec::new()
        };
        let search_query: Option<&str> = if self.search.active {
            Some(&self.search.query)
        } else {
            None
        };
        let preedit = self.ime.preedit_text.as_deref();
        let phys_pad = self.geometry.phys(self.display.padding);

        let buf_width = u32_to_i32(phys_w);
        let buf_height = u32_to_i32(phys_h);
        let stride_i32 = u32_to_i32(stride);

        let (buffer, canvas) = self
            .wl
            .pool
            .create_buffer(buf_width, buf_height, stride_i32, wl_shm::Format::Argb8888)
            .expect("create buffer");

        // Update render state from terminal only when terminal content changed.
        // Cursor-blink frames skip this (~1.2ms saved at 1080p).
        // SAFETY: self.terminal owns the pointer and is alive for the duration of App.
        if self.terminal_changed {
            let _ = self.render_state.update(self.terminal.inner());
            self.cached_colors = Some(self.render_state.colors());
            self.terminal_changed = false;
        }

        // Use cached colors (refresh on terminal change, reuse for blink frames).
        let colors = self
            .cached_colors
            .get_or_insert_with(|| self.render_state.colors());

        // Render to the buffer at physical resolution
        let render_opts = renderer::RenderOptions {
            scrollbar: scrollbar.as_ref(),
            bold_is_bright: self.display.flags.bold_is_bright(),
            cursor_blink_visible: self.display.cursor_blink_visible,
            selection,
            padding: phys_pad,
            opacity: self.display.opacity,
            search_highlights: &search_highlights,
            search_bar: search_query,
            search_cursor: self.search.cursor_pos,
            preedit,
            selection_fg: self.display.selection_fg,
            selection_bg: self.display.selection_bg,
        };
        let mut target = renderer::RenderTarget {
            buf: canvas,
            width: phys_w,
            height: phys_h,
            stride,
            retained: &mut self.retained_buf,
        };
        renderer::render_frame(
            &mut target,
            &mut self.render_state,
            &mut self.font,
            &render_opts,
            colors,
        );

        // Clear dirty state on render side so next update() only reports
        // actual terminal changes.
        self.render_state.clear_dirty();

        self.commit_buffer(buffer, phys_w, phys_h);
        self.dirty = false;

        if profiling() {
            let draw_elapsed = draw_start.elapsed();
            let since_data = draw_start.duration_since(self.last_data_time);
            eprintln!(
                "[profile] draw: {:.2}ms (since_data: {:.2}ms, buf: {}x{})",
                draw_elapsed.as_secs_f64() * 1000.0,
                since_data.as_secs_f64() * 1000.0,
                phys_w,
                phys_h,
            );
        }
        true
    }

    fn commit_buffer(&mut self, buffer: Buffer, phys_w: u32, phys_h: u32) {
        let buf_width = u32_to_i32(phys_w);
        let buf_height = u32_to_i32(phys_h);

        // Tell compositor about scaling
        if let Some(ref viewport) = self.wl.viewport {
            self.wl.window.wl_surface().set_buffer_scale(1);
            viewport.set_destination(
                u32_to_i32(self.geometry.width),
                u32_to_i32(self.geometry.height),
            );
        } else {
            let int_scale = (self.geometry.scale_120 + 60) / 120;
            self.wl
                .window
                .wl_surface()
                .set_buffer_scale(u32_to_i32(int_scale));
        }

        buffer
            .attach_to(self.wl.window.wl_surface())
            .expect("attach");
        // Always damage the entire buffer.  We create a new wl_buffer from the
        // SHM pool on every frame, so partial damage would leave the compositor
        // with stale pixels for undamaged regions (the compositor only copies
        // damaged pixels from the SHM buffer).
        self.wl
            .window
            .wl_surface()
            .damage_buffer(0, 0, buf_width, buf_height);
        self.wl.window.wl_surface().commit();
        if let Err(e) = self.wl.conn.flush() {
            eprintln!("Wayland flush failed: {e}");
        }
        self.wl.buffer = Some(buffer);
    }

    pub(super) fn recalculate_grid(&mut self) {
        let cw = self.font.cell_width;
        let ch = self.font.cell_height;
        if cw == 0 || ch == 0 {
            return;
        }
        // Physical buffer dimensions
        let phys_w = self.geometry.phys(self.geometry.width);
        let phys_h = self.geometry.phys(self.geometry.height);
        let phys_pad = self.geometry.phys(self.display.padding);
        let usable_w = phys_w.saturating_sub(phys_pad * 2);
        let usable_h = phys_h.saturating_sub(phys_pad * 2);
        let cols = u16::try_from((usable_w / cw).max(1)).expect("grid columns fit u16");
        let rows = u16::try_from((usable_h / ch).max(1)).expect("grid rows fit u16");

        if cols != self.geometry.term_cols || rows != self.geometry.term_rows {
            self.geometry.term_cols = cols;
            self.geometry.term_rows = rows;
            let _ = self.terminal.resize(cols, rows);
            let px_w = u16::try_from(phys_w.min(u32::from(u16::MAX))).unwrap_or(u16::MAX);
            let px_h = u16::try_from(phys_h.min(u32::from(u16::MAX))).unwrap_or(u16::MAX);
            let _ = self.pty.resize(cols, rows, px_w, px_h);
            // Force full redraw by invalidating the retained buffer.
            self.retained_buf.clear();
            self.dirty = true;
        }
    }

    fn build_search_highlights(&self) -> Vec<renderer::SearchHighlight> {
        self.search
            .matches
            .iter()
            .enumerate()
            .map(|(i, m)| renderer::SearchHighlight {
                row: m.row,
                start_col: m.start_col,
                end_col: m.end_col,
                is_current: i == self.search.current_match,
            })
            .collect()
    }

    /// Search visible rows for occurrences of the search query.
    pub(super) fn update_search_matches(&mut self) {
        self.search.matches.clear();
        if self.search.query.is_empty() {
            self.search.query_lower.clear();
            return;
        }
        self.search.query_lower = self.search.query.to_lowercase();

        let cols = usize::from(self.geometry.term_cols);
        let rows = usize::from(self.geometry.term_rows);
        self.search
            .row_texts
            .resize_with(rows, || String::with_capacity(cols));
        for buf in &mut self.search.row_texts {
            buf.clear();
        }

        self.render_state
            .for_each_cell(|row, _col, codepoints, _style, _is_wide| {
                if let Some(buf) = self.search.row_texts.get_mut(row) {
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

        let query_lower = &self.search.query_lower;
        for (row_idx, text) in self.search.row_texts.iter().enumerate() {
            let row_u16 = u16::try_from(row_idx).unwrap_or(u16::MAX);
            let text_lower = text.to_lowercase();
            let query_len = query_lower.len();
            let mut start = 0;
            while let Some(pos) = text_lower
                .get(start..)
                .and_then(|s| s.find(query_lower.as_str()))
            {
                let abs_pos = start + pos;
                let start_col = u16::try_from(abs_pos).unwrap_or(u16::MAX);
                let end_col = u16::try_from(abs_pos + query_len)
                    .unwrap_or(u16::MAX)
                    .saturating_sub(1);
                self.search.matches.push(SearchMatch {
                    row: row_u16,
                    start_col,
                    end_col,
                });
                start = abs_pos + 1;
            }
        }

        // Clamp current_match
        if self.search.matches.is_empty() || self.search.current_match >= self.search.matches.len()
        {
            self.search.current_match = 0;
        }
    }

    /// Navigate to next or previous search match.
    pub(super) const fn search_navigate(&mut self, forward: bool) {
        if self.search.navigate(forward) {
            self.dirty = true;
        }
    }
}
