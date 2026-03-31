use smithay_client_toolkit::seat::keyboard::KeyEvent;
use smithay_client_toolkit::seat::pointer::CursorIcon;
use wayland_client::{Connection, QueueHandle};

use horseshoe::config;
use horseshoe::keymap;
use horseshoe::num::{clamped_f64_to_u32, float_to_i64, scroll_delta_to_isize};
use horseshoe::terminal::TerminalOps;
use libghostty_vt::{key, mouse, terminal};
use xkeysym::key as xkb;

use super::cursor::Cursor;
use super::osc::{
    next_char_boundary, prev_char_boundary, search_word_boundary_left, search_word_boundary_right,
};
use super::process::{resolve_editor, spawn_editor_terminal, spawn_terminal_in};
use super::{App, dbg_log, profiling, wayland_button_to_ghostty};

/// Width of the scrollbar click/hover hit region in logical pixels.
const SCROLLBAR_HIT_WIDTH: u32 = 20;

impl App {
    pub(super) const fn ghostty_mods(&self) -> key::Mods {
        self.input.ghostty_mods()
    }

    /// Return mouse coordinates scaled to physical (buffer) pixels.
    fn scaled_mouse(&self) -> (f32, f32) {
        self.input.scaled_mouse(self.geometry.scale_f64())
    }

    /// Check whether the shift modifier is currently held.
    const fn mods_has_shift(&self) -> bool {
        self.input.mods_has_shift()
    }

    /// Check whether the ctrl modifier is currently held.
    const fn mods_has_ctrl(&self) -> bool {
        self.input.mods_has_ctrl()
    }

    /// Look up a keybinding from the configurable bindings table.
    fn check_binding(&self, keysym: u32) -> Option<config::KeyAction> {
        self.input.check_binding(keysym)
    }

    fn is_mouse_tracking_active(&self) -> bool {
        // DEC private modes for mouse tracking
        let modes = [
            terminal::Mode::X10_MOUSE,    // X10 mouse
            terminal::Mode::NORMAL_MOUSE, // Normal mouse
            terminal::Mode::BUTTON_MOUSE, // Button event mouse
            terminal::Mode::ANY_MOUSE,    // Any event mouse
        ];
        for &mode in &modes {
            if let Some(true) = self.terminal.mode_get(mode) {
                return true;
            }
        }
        false
    }

    fn handle_scrollbar_drag(&mut self, y: f64) -> bool {
        if let Some(sb) = self.terminal.scrollbar() {
            if sb.total <= sb.len {
                self.input.mouse.scrollbar_dragging = false;
                return false;
            }

            if self.input.mouse.scrollbar_dragging {
                let scrollable = sb.total - sb.len;
                // Compute the drag fraction in integer arithmetic to avoid
                // float precision lints.  y is clamped to [0, height].
                let y_clamped = y.clamp(0.0, f64::from(self.geometry.height));
                let y_int = float_to_i64(y_clamped);
                let height_i64 = i64::from(self.geometry.height);
                let scrollable_i = i64::try_from(scrollable).expect("scrollable fits i64");
                // target = y_int * scrollable / height  (integer division)
                let target = if height_i64 == 0 {
                    0
                } else {
                    y_int.saturating_mul(scrollable_i) / height_i64
                };
                let offset_i64 = i64::try_from(sb.offset).expect("scrollbar offset fits i64");
                let delta = target - offset_i64;
                if delta != 0 {
                    let delta_isize = isize::try_from(delta).expect("scroll delta fits isize");
                    self.terminal.scroll_viewport_delta(delta_isize);
                    self.dirty = true;
                }
                return true;
            }
        }
        false
    }

    /// Compute the consumed-modifiers mask for key encoding.
    pub const fn consumed_mods_for_key(&self, unshifted: u32) -> key::Mods {
        if unshifted != 0 && self.mods_has_shift() {
            key::Mods::SHIFT
        } else {
            key::Mods::empty()
        }
    }

    /// Extract the full scrollback buffer as plain text.
    ///
    /// Scrolls the viewport from top to bottom, reading each page of cells,
    /// then restores the original viewport position.
    fn extract_scrollback_text(&mut self) -> String {
        let Some(sb) = self.terminal.scrollbar() else {
            return String::new();
        };
        let rows = usize::from(self.geometry.term_rows.max(1));
        let cols = usize::from(self.geometry.term_cols.max(1));

        // Save current viewport offset so we can restore it
        let saved_offset = sb.offset;

        // Scroll to top
        self.terminal.scroll_viewport_top();

        // Calculate how many pages we need to iterate
        let total = usize::try_from(sb.total).unwrap_or(usize::MAX);
        let pages = if total == 0 { 0 } else { total.div_ceil(rows) };

        let mut result = String::new();
        let mut row_texts: Vec<String> = (0..rows).map(|_| String::with_capacity(cols)).collect();

        for page in 0..pages {
            // Update render state for current viewport position
            let _ = self.render_state.update(self.terminal.inner());

            for buf in &mut row_texts {
                buf.clear();
            }

            self.render_state
                .for_each_cell(|row, _col, codepoints, _style, _is_wide| {
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

            for text in &row_texts {
                result.push_str(text.trim_end());
                result.push('\n');
            }

            // Advance one page (except on last page)
            if page + 1 < pages {
                let delta = isize::try_from(rows).unwrap_or(isize::MAX);
                self.terminal.scroll_viewport_delta(delta);
            }
        }

        // Restore viewport to saved offset
        // Scroll to top first, then delta forward to saved offset
        self.terminal.scroll_viewport_top();
        if saved_offset > 0 {
            let delta = isize::try_from(saved_offset).unwrap_or(isize::MAX);
            self.terminal.scroll_viewport_delta(delta);
        }

        // Trim trailing empty lines
        let trimmed = result.trim_end_matches('\n');
        if trimmed.is_empty() {
            String::new()
        } else {
            let mut s = trimmed.to_string();
            s.push('\n');
            s
        }
    }

    /// Open the scrollback buffer in an external editor.
    ///
    /// Extracts all scrollback text, writes it to a temp file, and spawns a new
    /// horseshoe window running the editor on that file.
    fn open_scrollback_editor(&mut self) {
        let Some(editor) = resolve_editor() else {
            eprintln!("scrollback-editor: no editor found (set $VISUAL or $EDITOR)");
            return;
        };

        let text = self.extract_scrollback_text();
        if text.is_empty() {
            return;
        }

        // Write to temp file
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let pid = std::process::id();
        let path = format!("/tmp/hs-scrollback-{pid}-{timestamp}");

        if let Err(e) = std::fs::write(&path, &text) {
            eprintln!("scrollback-editor: failed to write temp file: {e}");
            return;
        }

        spawn_editor_terminal(&editor, &path);
    }

    fn dispatch_binding(&mut self, key_action: config::KeyAction, qh: &QueueHandle<Self>) {
        match key_action {
            config::KeyAction::ToggleFullscreen => {
                self.display.fullscreen = !self.display.fullscreen;
                if self.display.fullscreen {
                    self.wl.window.set_fullscreen(None);
                } else {
                    self.wl.window.unset_fullscreen();
                }
            }
            config::KeyAction::Copy => self.copy_selection(qh),
            config::KeyAction::Paste => self.paste_clipboard(),
            config::KeyAction::Search => {
                self.search.active = true;
                self.search.query.clear();
                self.search.cursor_pos = 0;
                self.search.matches.clear();
                self.search.current_match = 0;
                self.dirty = true;
            }
            config::KeyAction::FontSizeUp => self.adjust_font_size(1.0),
            config::KeyAction::FontSizeDown => self.adjust_font_size(-1.0),
            config::KeyAction::FontSizeReset => self.reset_font_size(),
            config::KeyAction::PrimaryPaste => self.paste_primary_selection(),
            config::KeyAction::ScrollPageUp => {
                let rows = i32::from(self.geometry.term_rows.max(1));
                self.terminal
                    .scroll_viewport_delta(scroll_delta_to_isize(-rows));
                self.dirty = true;
            }
            config::KeyAction::ScrollPageDown => {
                let rows = i32::from(self.geometry.term_rows.max(1));
                self.terminal
                    .scroll_viewport_delta(scroll_delta_to_isize(rows));
                self.dirty = true;
            }
            config::KeyAction::ScrollHalfPageUp => {
                let half = i32::from(self.geometry.term_rows.max(1)) / 2;
                self.terminal
                    .scroll_viewport_delta(scroll_delta_to_isize(-half));
                self.dirty = true;
            }
            config::KeyAction::ScrollHalfPageDown => {
                let half = i32::from(self.geometry.term_rows.max(1)) / 2;
                self.terminal
                    .scroll_viewport_delta(scroll_delta_to_isize(half));
                self.dirty = true;
            }
            config::KeyAction::ScrollLineUp => {
                self.terminal.scroll_viewport_delta(-1);
                self.dirty = true;
            }
            config::KeyAction::ScrollLineDown => {
                self.terminal.scroll_viewport_delta(1);
                self.dirty = true;
            }
            config::KeyAction::SpawnTerminal => spawn_terminal_in(self.osc.cwd.as_deref()),
            config::KeyAction::ScrollbackEditor => self.open_scrollback_editor(),
            config::KeyAction::Noop => {} // Filtered by Bindings::lookup, unreachable
        }
    }

    pub(super) fn handle_key(
        &mut self,
        event: &KeyEvent,
        action: key::Action,
        qh: &QueueHandle<Self>,
    ) {
        // Hide pointer on keypress/repeat (hide-when-typing)
        if action != key::Action::Release
            && self.display.flags.hide_when_typing()
            && !self.input.mouse.pointer_hidden
        {
            if let Some(Cursor::Shm(sc)) = &self.input.cursor {
                sc.pointer.set_cursor(sc.enter_serial, None, 0, 0);
            }
            self.input.mouse.pointer_hidden = true;
        }
        // Check keybindings before forwarding to terminal.
        // Repeat goes through the same path as press (matching foot).
        if action != key::Action::Release
            && let Some(key_action) = self.check_binding(event.keysym.raw())
        {
            if profiling() {
                dbg_log!("[key] binding matched: {key_action:?}");
            }
            self.dispatch_binding(key_action, qh);
            return;
        }

        // Handle search mode input (repeat enables holding Backspace/arrows)
        if self.search.active {
            if action != key::Action::Release {
                self.handle_search_key(event);
            }
            return;
        }

        // Clear selection on regular key input
        if action != key::Action::Release {
            self.clear_selection();
        }

        // Sync encoder from terminal state
        // SAFETY: self.terminal owns the pointer and is alive for the duration of App.
        self.input
            .key_encoder
            .sync_from_terminal(self.terminal.inner());

        let keysym = event.keysym.raw();
        let gkey = keymap::xkb_to_ghostty_key(keysym);

        let mods = self.ghostty_mods();
        let unshifted = keymap::unshifted_codepoint(keysym);

        // Build UTF-8 text from the event.
        // When Ctrl is held, XKB may produce control characters (bytes < 0x20)
        // as utf8 text (e.g. "\x04" for Ctrl+D). The ghostty encoder's ctrlSeq()
        // expects ASCII letters ('d'=0x64) in its switch table, not the control
        // byte (0x04). Stripping control chars forces the encoder to fall back to
        // the logical key codepoint path which correctly maps 'd' → 0x04.
        let utf8_text = if action == key::Action::Release {
            None
        } else {
            let text = event.utf8.as_deref();
            text.filter(|s| !s.is_empty())
                .filter(|s| !s.bytes().all(|b| b < 0x20))
        };

        // Skip keys that are unidentified AND have no text (e.g. unknown
        // modifier-only keys). Keys with text (shifted symbols like @, #, etc.)
        // are still sent to the encoder which handles them via UTF-8 text.
        if gkey == key::Key::Unidentified && utf8_text.is_none() {
            return;
        }

        let consumed_mods = self.consumed_mods_for_key(unshifted);

        if let Some(data) =
            self.input
                .key_encoder
                .encode(gkey, action, mods, consumed_mods, utf8_text, unshifted)
        {
            let _ = self.pty.write_all(&data);
            // Scroll to bottom on keypress or repeat
            if action == key::Action::Press || action == key::Action::Repeat {
                self.terminal.scroll_viewport_bottom();
                if profiling() {
                    let tag = if action == key::Action::Press {
                        "key"
                    } else {
                        "repeat"
                    };
                    eprintln!(
                        "[profile] {tag}: wrote {} bytes to pty (keysym=0x{:04x})",
                        data.len(),
                        keysym
                    );
                }
            }
            self.dirty = true;
            // Prevent the render timer from drawing immediately with stale
            // terminal state.  Without this, the timer sees an old
            // last_data_time and triggers a useless render before the shell
            // has responded, wasting a full draw cycle.
            self.last_data_time = std::time::Instant::now();
        }
    }

    fn handle_search_key(&mut self, event: &KeyEvent) {
        let keysym = event.keysym.raw();
        let ctrl = self.mods_has_ctrl();
        match keysym {
            xkb::Escape => {
                // Escape: exit search
                self.search.active = false;
                self.search.query.clear();
                self.search.cursor_pos = 0;
                self.search.matches.clear();
                self.dirty = true;
            }
            xkb::Return => {
                // Enter: next match (Shift+Enter: previous)
                self.search_navigate(!self.mods_has_shift());
            }
            xkb::BackSpace => {
                // Backspace: delete char before cursor (Ctrl: delete word)
                if self.search.cursor_pos > 0 {
                    let target = if ctrl {
                        search_word_boundary_left(&self.search.query, self.search.cursor_pos)
                    } else {
                        prev_char_boundary(&self.search.query, self.search.cursor_pos)
                    };
                    drop(self.search.query.drain(target..self.search.cursor_pos));
                    self.search.cursor_pos = target;
                    self.update_search_matches();
                    self.dirty = true;
                }
            }
            xkb::Delete => {
                // Delete: delete char after cursor
                if self.search.cursor_pos < self.search.query.len() {
                    let end = next_char_boundary(&self.search.query, self.search.cursor_pos);
                    drop(self.search.query.drain(self.search.cursor_pos..end));
                    self.update_search_matches();
                    self.dirty = true;
                }
            }
            xkb::Left => {
                // Left: move cursor left (Ctrl: word left)
                if self.search.cursor_pos > 0 {
                    self.search.cursor_pos = if ctrl {
                        search_word_boundary_left(&self.search.query, self.search.cursor_pos)
                    } else {
                        prev_char_boundary(&self.search.query, self.search.cursor_pos)
                    };
                    self.dirty = true;
                }
            }
            xkb::Right => {
                // Right: move cursor right (Ctrl: word right)
                if self.search.cursor_pos < self.search.query.len() {
                    self.search.cursor_pos = if ctrl {
                        search_word_boundary_right(&self.search.query, self.search.cursor_pos)
                    } else {
                        next_char_boundary(&self.search.query, self.search.cursor_pos)
                    };
                    self.dirty = true;
                }
            }
            xkb::Home => {
                // Home: move cursor to start
                if self.search.cursor_pos > 0 {
                    self.search.cursor_pos = 0;
                    self.dirty = true;
                }
            }
            xkb::End => {
                // End: move cursor to end
                let end = self.search.query.len();
                if self.search.cursor_pos < end {
                    self.search.cursor_pos = end;
                    self.dirty = true;
                }
            }
            _ => {
                // Insert printable text at cursor position
                if let Some(text) = event.utf8.as_deref()
                    && !text.is_empty()
                    && text.chars().all(|c| !c.is_control())
                {
                    self.search.query.insert_str(self.search.cursor_pos, text);
                    self.search.cursor_pos += text.len();
                    self.update_search_matches();
                    self.dirty = true;
                }
            }
        }
    }

    // -- Pointer event handlers (broken out of pointer_frame) --

    /// Choose the appropriate cursor icon based on mouse position.
    /// Returns `CursorIcon::Default` (arrow) when over the scrollbar,
    /// `CursorIcon::Text` (I-beam) otherwise.
    fn cursor_icon_for_position(&self) -> CursorIcon {
        let bar_width = SCROLLBAR_HIT_WIDTH;
        let mouse_x_u32 = clamped_f64_to_u32(self.input.mouse.x.max(0.0));
        let over_scrollbar = mouse_x_u32 > self.geometry.width.saturating_sub(bar_width);
        let has_scrollback = self
            .terminal
            .scrollbar()
            .is_some_and(|sb| sb.total > sb.len);
        if over_scrollbar && has_scrollback {
            CursorIcon::Default
        } else {
            CursorIcon::Text
        }
    }

    pub(super) fn handle_pointer_enter(
        &mut self,
        conn: &Connection,
        serial: u32,
        position: (f64, f64),
    ) {
        self.input.mouse.x = position.0;
        self.input.mouse.y = position.1;
        let icon = self.cursor_icon_for_position();
        if let Some(ref mut cursor) = self.input.cursor {
            cursor.set_enter_serial(serial);
            cursor.set_cursor(conn, icon);
        }
    }

    pub(super) fn handle_pointer_motion(
        &mut self,
        conn: &Connection,
        position: (f64, f64),
        mods: key::Mods,
    ) {
        self.input.mouse.x = position.0;
        self.input.mouse.y = position.1;

        // Restore cursor if it was hidden by hide-when-typing
        if self.input.mouse.pointer_hidden {
            if let Some(ref cursor) = self.input.cursor {
                let icon = self.cursor_icon_for_position();
                cursor.set_cursor(conn, icon);
            }
            self.input.mouse.pointer_hidden = false;
        }

        // Handle scrollbar drag
        if self.input.mouse.scrollbar_dragging {
            let y = self.input.mouse.y;
            let _ = self.handle_scrollbar_drag(y);
            return;
        }

        // Handle selection drag
        if self.selection.active {
            let (col, row) = self.pixel_to_grid(self.input.mouse.x, self.input.mouse.y);
            self.selection.end = Some((col, row));
            self.dirty = true;
            return;
        }

        // Update cursor icon (arrow over scrollbar, I-beam over text)
        if let Some(ref cursor) = self.input.cursor {
            let icon = self.cursor_icon_for_position();
            cursor.set_cursor(conn, icon);
        }

        // Forward to mouse encoder
        let button = if self.input.mouse.buttons_pressed > 0 {
            Some(mouse::Button::Left)
        } else {
            None
        };
        let (mx, my) = self.scaled_mouse();
        if let Some(data) =
            self.input
                .mouse_encoder
                .encode(mouse::Action::Motion, button, mods, mx, my)
        {
            let _ = self.pty.write_all(&data);
        }
    }

    pub(super) fn handle_pointer_press(
        &mut self,
        button: u32,
        mods: key::Mods,
        qh: &QueueHandle<Self>,
    ) {
        self.input.mouse.buttons_pressed += 1;

        // Check if clicking in scrollbar area
        let bar_width = SCROLLBAR_HIT_WIDTH;
        let mouse_x_u32 = clamped_f64_to_u32(self.input.mouse.x.max(0.0));
        if mouse_x_u32 > self.geometry.width.saturating_sub(bar_width) {
            self.input.mouse.scrollbar_dragging = true;
            let y = self.input.mouse.y;
            let _ = self.handle_scrollbar_drag(y);
            return;
        }

        // Middle-click: paste primary selection
        if button == 0x112 && !self.is_mouse_tracking_active() {
            self.paste_primary_selection();
            return;
        }

        // Left-click without mouse tracking: selection handling
        if button == 0x110 && !self.is_mouse_tracking_active() {
            self.handle_left_click(qh);
            return;
        }

        // Clear any existing selection on other clicks
        self.clear_selection();

        let gbtn = wayland_button_to_ghostty(button);
        let (mx, my) = self.scaled_mouse();
        if let Some(data) =
            self.input
                .mouse_encoder
                .encode(mouse::Action::Press, Some(gbtn), mods, mx, my)
        {
            let _ = self.pty.write_all(&data);
        }
        self.dirty = true;
    }

    fn handle_left_click(&mut self, qh: &QueueHandle<Self>) {
        let (col, row) = self.pixel_to_grid(self.input.mouse.x, self.input.mouse.y);
        let click_count = self.selection.register_click(col, row);

        match click_count {
            2 => {
                // Double-click: word selection
                let (start_col, end_col) = self.word_boundaries_at(col, row);
                self.selection.start = Some((start_col, row));
                self.selection.end = Some((end_col, row));
                self.selection.active = false;
                self.dirty = true;
                self.copy_selection(qh);
                self.set_primary_selection(qh);
            }
            n if n >= 3 => {
                // Triple-click: line selection
                self.select_line(row);
                self.selection.active = false;
                self.selection.click_count = 3; // cap to avoid overflow
                self.copy_selection(qh);
                self.set_primary_selection(qh);
            }
            _ => {
                // Single click: start drag selection
                self.selection.start = Some((col, row));
                self.selection.end = Some((col, row));
                self.selection.active = true;
                self.dirty = true;
            }
        }
    }

    pub(super) fn handle_pointer_release(
        &mut self,
        button: u32,
        mods: key::Mods,
        qh: &QueueHandle<Self>,
    ) {
        self.input.mouse.buttons_pressed = self.input.mouse.buttons_pressed.saturating_sub(1);
        self.input.mouse.scrollbar_dragging = false;

        // Finish selection on left button release
        dbg_log!(
            "[sel] pointer_release btn=0x{button:x} active={} start={:?} end={:?}",
            self.selection.active,
            self.selection.start,
            self.selection.end
        );
        if button == 0x110 && self.selection.active {
            self.selection.active = false;
            // If start == end, clear selection (just a click, not a drag)
            if self.selection.start == self.selection.end {
                dbg_log!("[sel] start==end, clearing selection");
                self.clear_selection();
            } else {
                dbg_log!(
                    "[sel] selection range: {:?} -> {:?}, copying",
                    self.selection.start,
                    self.selection.end
                );
                self.copy_selection(qh);
                self.set_primary_selection(qh);
            }
            return;
        }

        let gbtn = wayland_button_to_ghostty(button);
        let (mx, my) = self.scaled_mouse();
        if let Some(data) =
            self.input
                .mouse_encoder
                .encode(mouse::Action::Release, Some(gbtn), mods, mx, my)
        {
            let _ = self.pty.write_all(&data);
        }
        self.dirty = true;
    }

    pub(super) fn handle_pointer_axis(
        &mut self,
        vertical: &smithay_client_toolkit::seat::pointer::AxisScroll,
        mods: key::Mods,
    ) {
        // Scroll handling: prefer discrete steps, fall back to absolute with accumulator
        let scroll_amount = if vertical.discrete != 0 {
            // Reset accumulator on discrete events (e.g. notched scroll wheel)
            self.input.mouse.scroll_accum = 0.0;
            vertical.discrete
        } else if vertical.absolute != 0.0 {
            // Accumulate continuous scroll and fire when a full line is reached.
            // Use the actual cell height (matching foot's behavior) so scroll
            // speed is consistent regardless of font size.
            let line_height = f64::from(self.font.cell_height.max(1));
            self.input.mouse.scroll_accum += vertical.absolute;
            let steps_f64 = (self.input.mouse.scroll_accum / line_height).trunc();
            let steps = i32::try_from(float_to_i64(steps_f64)).expect("scroll steps fits i32");
            self.input.mouse.scroll_accum -= f64::from(steps) * line_height;
            steps
        } else {
            0
        };

        if scroll_amount == 0 {
            return;
        }

        // Ctrl+scroll: font zoom
        if mods.contains(key::Mods::CTRL) {
            let delta = if scroll_amount < 0 { 1.0 } else { -1.0 };
            self.adjust_font_size(delta);
            return;
        }

        if self.is_mouse_tracking_active() {
            self.handle_scroll_mouse_tracking(scroll_amount, mods);
        } else if self.terminal.is_alternate_screen() && self.display.flags.alternate_scroll_mode()
        {
            // In alternate screen without mouse tracking,
            // send arrow keys so apps like less/man scroll.
            // Use KeyEncoder to respect DECCKM (application cursor mode).
            let gkey = if scroll_amount < 0 {
                key::Key::ArrowUp
            } else {
                key::Key::ArrowDown
            };
            self.input
                .key_encoder
                .sync_from_terminal(self.terminal.inner());
            if let Some(data) = self.input.key_encoder.encode(
                gkey,
                key::Action::Press,
                key::Mods::empty(),
                key::Mods::empty(),
                None,
                0,
            ) {
                let count = scroll_amount.unsigned_abs();
                for _ in 0..count {
                    let _ = self.pty.write_all(&data);
                }
            }
        } else {
            // Scroll viewport through scrollback, applying multiplier
            let multiplier = self.display.scroll_multiplier;
            let scaled = float_to_i64(f64::from(scroll_amount) * f64::from(multiplier));
            let delta = i32::try_from(scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)))
                .unwrap_or(scroll_amount);
            self.terminal
                .scroll_viewport_delta(scroll_delta_to_isize(delta));
        }
        self.dirty = true;
    }

    fn handle_scroll_mouse_tracking(&mut self, scroll_amount: i32, mods: key::Mods) {
        let scroll_btn = if scroll_amount < 0 {
            mouse::Button::Four
        } else {
            mouse::Button::Five
        };
        let count = scroll_amount.unsigned_abs();
        let (mx, my) = self.scaled_mouse();
        for _ in 0..count {
            if let Some(data) = self.input.mouse_encoder.encode(
                mouse::Action::Press,
                Some(scroll_btn),
                mods,
                mx,
                my,
            ) {
                let _ = self.pty.write_all(&data);
            }
            if let Some(data) = self.input.mouse_encoder.encode(
                mouse::Action::Release,
                Some(scroll_btn),
                mods,
                mx,
                my,
            ) {
                let _ = self.pty.write_all(&data);
            }
        }
    }
}
