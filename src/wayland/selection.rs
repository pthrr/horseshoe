use horseshoe::terminal::TerminalOps;
use libghostty_vt::terminal;
use wayland_client::QueueHandle;

use horseshoe::num::clamped_f64_to_u32;
use horseshoe::paste;
use horseshoe::selection;

use super::{App, dbg_log, read_pipe_with_timeout};

impl App {
    pub(super) fn paste_to_pty(&mut self, text: &str) {
        if super::profiling() {
            eprintln!("[profile] paste: {}", &text[..text.len().min(80)]);
        }
        dbg_log!("[paste] paste_to_pty called, len={}", text.len());
        let bracketed = self
            .terminal
            .mode_get(terminal::Mode::BRACKETED_PASTE)
            .unwrap_or(false);
        dbg_log!("[paste] bracketed={bracketed}");
        let Some(data) = paste::prepare_paste(text, bracketed) else {
            eprintln!("Paste blocked: content contains bracket-end escape sequence");
            return;
        };
        dbg_log!("[paste] writing {} bytes to pty", data.len());
        if !data.is_empty() {
            let _ = self.pty.write_all(&data);
        }
        self.dirty = true;
    }

    pub(super) fn paste_clipboard(&mut self) {
        dbg_log!(
            "[paste] paste_clipboard called, own_source={}, content_len={}",
            self.clipboard.copy_paste_source.is_some(),
            self.clipboard.clipboard_content.len()
        );
        // If we own the clipboard, paste directly (avoids Wayland roundtrip deadlock)
        if self.clipboard.copy_paste_source.is_some()
            && !self.clipboard.clipboard_content.is_empty()
        {
            let text = self.clipboard.clipboard_content.clone();
            self.paste_to_pty(&text);
            return;
        }

        let selection = self
            .clipboard
            .data_device
            .as_ref()
            .and_then(|dd| dd.data().selection_offer());

        let Some(offer) = selection else {
            dbg_log!("[paste] No clipboard selection offer available");
            return;
        };

        dbg_log!("[paste] receiving from wayland offer");
        match offer.receive("text/plain;charset=utf-8".to_string()) {
            Ok(mut pipe) => {
                // Flush so the compositor actually receives our receive request
                let _ = self.wl.conn.flush();
                if let Some(text) = read_pipe_with_timeout(&mut pipe) {
                    dbg_log!("[paste] pipe read {} bytes", text.len());
                    if !text.is_empty() {
                        self.paste_to_pty(&text);
                    }
                } else {
                    dbg_log!("[paste] pipe read returned None (timeout or error)");
                }
            }
            Err(err) => {
                dbg_log!("[paste] Failed to receive clipboard: {err}");
            }
        }
    }

    /// Convert pixel coordinates to grid (col, row).
    pub(super) fn pixel_to_grid(&self, px: f64, py: f64) -> (u16, u16) {
        let cw = self.font.cell_width;
        let ch = self.font.cell_height;
        if cw == 0 || ch == 0 {
            return (0, 0);
        }
        // Convert logical mouse coordinates to physical pixel coordinates
        let scale_f = self.geometry.scale_f64();
        let phys_pad = f64::from(self.display.padding) * scale_f;
        let col_px = clamped_f64_to_u32((px * scale_f - phys_pad).max(0.0));
        let row_px = clamped_f64_to_u32((py * scale_f - phys_pad).max(0.0));
        let max_col = u32::from(self.geometry.term_cols.saturating_sub(1));
        let max_row = u32::from(self.geometry.term_rows.saturating_sub(1));
        let col = u16::try_from((col_px / cw).min(max_col)).expect("column fits u16");
        let row = u16::try_from((row_px / ch).min(max_row)).expect("row fits u16");
        (col, row)
    }

    /// Get the normalized selection range (start <= end), if any.
    pub(super) const fn selection_range(&self) -> Option<((u16, u16), (u16, u16))> {
        self.selection.normalized_range()
    }

    /// Extract text from the selected region using render state cells.
    fn extract_selected_text(&mut self) -> String {
        let Some((start, end)) = self.selection_range() else {
            dbg_log!("[sel] extract_selected_text: no selection range");
            return String::new();
        };
        dbg_log!(
            "[sel] extract_selected_text: range ({},{}) -> ({},{}) [col,row]",
            start.0,
            start.1,
            end.0,
            end.1
        );

        let _ = self.render_state.update(self.terminal.inner());

        let mut out = String::new();
        let mut current_row: Option<usize> = None;
        let mut line_start = 0usize; // byte offset where current line begins

        self.render_state
            .for_each_cell(|row, col, codepoints, _style, _is_wide| {
                let r = u16::try_from(row).expect("row index fits u16");
                let c = u16::try_from(col).expect("col index fits u16");

                if !selection::cell_in_selection(r, c, start, end) {
                    return;
                }

                if let Some(prev_row) = current_row
                    && row != prev_row
                {
                    // Trim trailing whitespace from the line we just finished
                    let trimmed = out[line_start..].trim_end().len();
                    out.truncate(line_start + trimmed);
                    out.push('\n');
                    line_start = out.len();
                }
                current_row = Some(row);

                if codepoints.is_empty() {
                    out.push(' ');
                } else {
                    for &cp in codepoints {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                    }
                }
            });

        // Trim trailing whitespace from the last line
        let trimmed = out[line_start..].trim_end().len();
        out.truncate(line_start + trimmed);
        out
    }

    /// Find word boundaries around a given grid position.
    /// Returns (`start_col`, `end_col`) for the word on the given row.
    pub(super) fn word_boundaries_at(&mut self, col: u16, row: u16) -> (u16, u16) {
        // SAFETY: self.terminal owns the pointer and is alive for the duration of App.
        let _ = self.render_state.update(self.terminal.inner());

        // Collect codepoints for the entire row
        let mut row_chars: Vec<(u16, char)> = Vec::new();
        self.render_state
            .for_each_cell(|r, c, codepoints, _style, _is_wide| {
                let r16 = u16::try_from(r).expect("row index fits u16");
                if r16 != row {
                    return;
                }
                let c16 = u16::try_from(c).expect("col index fits u16");
                if codepoints.is_empty() {
                    row_chars.push((c16, ' '));
                } else if let Some(ch) = codepoints.first().and_then(|&cp| char::from_u32(cp)) {
                    row_chars.push((c16, ch));
                } else {
                    row_chars.push((c16, ' '));
                }
            });

        selection::word_boundaries(&row_chars, col)
    }

    /// Select the entire line at the given row.
    pub(super) const fn select_line(&mut self, row: u16) {
        self.selection.select_line(row, self.geometry.term_cols);
        self.dirty = true;
    }

    /// Clear the current selection.
    pub(super) const fn clear_selection(&mut self) {
        if self.selection.clear() {
            self.dirty = true;
        }
    }

    /// Set the primary selection to the currently selected text.
    pub(super) fn set_primary_selection(&mut self, qh: &QueueHandle<Self>) {
        let text = self.extract_selected_text();
        if text.is_empty() {
            dbg_log!("[sel] set_primary_selection: extracted text is empty, skipping");
            return;
        }
        dbg_log!("[sel] set_primary_selection: {} bytes", text.len());
        self.clipboard.primary_selection_content = text;

        if let Some(ref mgr) = self.wl.primary_selection_manager {
            let source = mgr.create_selection_source(
                qh,
                [
                    "text/plain;charset=utf-8",
                    "UTF8_STRING",
                    "TEXT",
                    "STRING",
                    "text/plain",
                ],
            );
            if let Some(ref dev) = self.clipboard.primary_selection_device {
                source.set_selection(dev, self.clipboard.last_serial);
                dbg_log!(
                    "[sel] primary selection set with serial {}",
                    self.clipboard.last_serial
                );
            } else {
                dbg_log!("[sel] no primary_selection_device, cannot set selection");
            }
            self.clipboard.primary_selection_source = Some(source);
        } else {
            dbg_log!("[sel] no primary_selection_manager, cannot set selection");
        }
    }

    /// Paste from primary selection (middle-click).
    pub(super) fn paste_primary_selection(&mut self) {
        // If we own the primary selection, paste directly (avoids Wayland roundtrip deadlock)
        if self.clipboard.primary_selection_source.is_some()
            && !self.clipboard.primary_selection_content.is_empty()
        {
            dbg_log!(
                "[sel] paste_primary_selection: self-paste {} bytes",
                self.clipboard.primary_selection_content.len()
            );
            let text = self.clipboard.primary_selection_content.clone();
            self.paste_to_pty(&text);
            return;
        }

        dbg_log!(
            "[sel] paste_primary_selection: external (source={}, dev={})",
            self.clipboard.primary_selection_source.is_some(),
            self.clipboard.primary_selection_device.is_some(),
        );

        let maybe_offer = self
            .clipboard
            .primary_selection_device
            .as_ref()
            .and_then(|dev| dev.data().selection_offer());

        let Some(offer) = maybe_offer else {
            dbg_log!("[sel] No primary selection offer available");
            return;
        };

        dbg_log!("[sel] receiving from primary selection offer");
        match offer.receive("text/plain;charset=utf-8".to_string()) {
            Ok(mut pipe) => {
                // Flush so the compositor actually receives our receive request
                let _ = self.wl.conn.flush();
                if let Some(text) = read_pipe_with_timeout(&mut pipe) {
                    dbg_log!("[sel] pipe read {} bytes", text.len());
                    if !text.is_empty() {
                        self.paste_to_pty(&text);
                    }
                } else {
                    dbg_log!("[sel] pipe read returned None (timeout or error)");
                }
            }
            Err(err) => {
                dbg_log!("[sel] Failed to receive primary selection: {err}");
            }
        }
    }

    /// Copy selected text to the Wayland clipboard.
    pub(super) fn copy_selection(&mut self, qh: &QueueHandle<Self>) {
        let text = self.extract_selected_text();
        if text.is_empty() {
            dbg_log!("[sel] copy_selection: extracted text is empty, skipping");
            return;
        }
        if super::profiling() {
            eprintln!("[profile] clipboard_set: {}", &text[..text.len().min(80)]);
        }
        dbg_log!(
            "[sel] copy_selection: {} bytes: {:?}",
            text.len(),
            &text[..text.len().min(80)]
        );
        self.clipboard.clipboard_content = text;

        // Create a copy-paste source offering text MIME types
        let source = self.wl.data_device_manager.create_copy_paste_source(
            qh,
            [
                "text/plain;charset=utf-8",
                "UTF8_STRING",
                "TEXT",
                "STRING",
                "text/plain",
            ],
        );

        // Set selection on the data device
        if let Some(dd) = &self.clipboard.data_device {
            source.set_selection(dd, self.clipboard.last_serial);
        }

        // Keep source alive (dropped = cancelled)
        self.clipboard.copy_paste_source = Some(source);
    }

    /// Set clipboard content from an OSC 52 escape sequence.
    pub fn set_clipboard_osc52(&mut self, text: String, qh: &QueueHandle<Self>) {
        if text.is_empty() {
            return;
        }
        if super::profiling() {
            eprintln!("[profile] clipboard_set: {}", &text[..text.len().min(80)]);
        }
        self.clipboard.clipboard_content = text;
        let source = self.wl.data_device_manager.create_copy_paste_source(
            qh,
            [
                "text/plain;charset=utf-8",
                "UTF8_STRING",
                "TEXT",
                "STRING",
                "text/plain",
            ],
        );
        if let Some(dd) = &self.clipboard.data_device {
            source.set_selection(dd, self.clipboard.last_serial);
        }
        self.clipboard.copy_paste_source = Some(source);
    }

    /// Persist clipboard content via `wl-copy` so it survives after exit.
    pub fn persist_clipboard(&self) {
        self.clipboard.persist();
    }
}
