use libghostty_vt::{focus, key, mouse};

/// Safe wrapper around `libghostty_vt::key::Encoder` and `key::Event`.
///
/// The event object is pre-allocated and reused for every call to
/// [`encode`](KeyEncoder::encode) so that only the encoder and a single event
/// allocation exist for the lifetime of the wrapper.
pub struct KeyEncoder {
    encoder: key::Encoder<'static>,
    event: key::Event<'static>,
    buf: Vec<u8>,
}

unsafe impl Send for KeyEncoder {}

impl KeyEncoder {
    /// Create a new key encoder (and its companion key event).
    pub fn new() -> Result<Self, &'static str> {
        let encoder = key::Encoder::new().map_err(|_| "failed to create key encoder")?;
        let event = key::Event::new().map_err(|_| "failed to create key event")?;
        Ok(Self {
            encoder,
            event,
            buf: Vec::with_capacity(128),
        })
    }

    /// Synchronize encoder options from the current terminal state.
    ///
    /// This should be called before encoding so that mode-dependent encoding
    /// (e.g. application cursor keys, kitty keyboard protocol) is up to date.
    pub fn sync_from_terminal(&mut self, terminal: &libghostty_vt::Terminal<'_, '_>) {
        let _ = self.encoder.set_options_from_terminal(terminal);
    }

    /// Encode a key press / release / repeat event.
    ///
    /// Returns `Some(bytes)` containing the escape sequence to send to the
    /// PTY, or `None` if the encoder produced no output (e.g. for a modifier-
    /// only event or an unrecognized key).
    pub fn encode(
        &mut self,
        k: key::Key,
        action: key::Action,
        mods: key::Mods,
        consumed_mods: key::Mods,
        utf8_text: Option<&str>,
        unshifted_codepoint: u32,
    ) -> Option<Vec<u8>> {
        let _ = self.event.set_key(k);
        let _ = self.event.set_action(action);
        let _ = self.event.set_mods(mods);
        let _ = self.event.set_consumed_mods(consumed_mods);
        let _ = self.event.set_utf8(utf8_text);

        if let Some(cp) = char::from_u32(unshifted_codepoint) {
            let _ = self.event.set_unshifted_codepoint(cp);
        } else {
            let _ = self.event.set_unshifted_codepoint('\0');
        }

        self.buf.clear();
        match self.encoder.encode_to_vec(&self.event, &mut self.buf) {
            Ok(()) if !self.buf.is_empty() => Some(self.buf.clone()),
            _ => None,
        }
    }
}

/// Safe wrapper around `libghostty_vt::mouse::Encoder` and `mouse::Event`.
pub struct MouseEncoder {
    encoder: mouse::Encoder<'static>,
    event: mouse::Event<'static>,
    buf: Vec<u8>,
}

unsafe impl Send for MouseEncoder {}

impl MouseEncoder {
    /// Create a new mouse encoder (and its companion mouse event).
    pub fn new() -> Result<Self, &'static str> {
        let encoder = mouse::Encoder::new().map_err(|_| "failed to create mouse encoder")?;
        let event = mouse::Event::new().map_err(|_| "failed to create mouse event")?;
        Ok(Self {
            encoder,
            event,
            buf: Vec::with_capacity(128),
        })
    }

    /// Synchronize encoder options from the current terminal state.
    pub fn sync_from_terminal(&mut self, terminal: &libghostty_vt::Terminal<'_, '_>) {
        let _ = self.encoder.set_options_from_terminal(terminal);
    }

    /// Set the size context used for pixel-to-cell coordinate conversion.
    ///
    /// `padding` is applied uniformly to all four edges.
    pub fn set_size(
        &mut self,
        screen_width: u32,
        screen_height: u32,
        cell_width: u32,
        cell_height: u32,
        padding: u32,
    ) {
        let _ = self.encoder.set_size(mouse::EncoderSize {
            screen_width,
            screen_height,
            cell_width,
            cell_height,
            padding_top: padding,
            padding_bottom: padding,
            padding_left: padding,
            padding_right: padding,
        });
    }

    /// Inform the encoder whether any mouse button is currently held.
    pub fn set_any_button_pressed(&mut self, pressed: bool) {
        let _ = self.encoder.set_any_button_pressed(pressed);
    }

    /// Enable or disable motion deduplication by last-reported cell.
    pub fn set_track_last_cell(&mut self, track: bool) {
        let _ = self.encoder.set_track_last_cell(track);
    }

    /// Encode a mouse event and return the resulting escape sequence bytes.
    ///
    /// `button` may be `None` for motion-only events where no button is held.
    /// Pixel coordinates `(x, y)` are converted to cell coordinates by the
    /// encoder using the size context set via [`set_size`](Self::set_size).
    pub fn encode(
        &mut self,
        action: mouse::Action,
        button: Option<mouse::Button>,
        mods: key::Mods,
        x: f32,
        y: f32,
    ) -> Option<Vec<u8>> {
        let _ = self.event.set_action(action);
        let _ = self.event.set_button(button);
        let _ = self.event.set_mods(mods);
        let _ = self.event.set_position(mouse::Position { x, y });

        self.buf.clear();
        match self.encoder.encode_to_vec(&self.event, &mut self.buf) {
            Ok(()) if !self.buf.is_empty() => Some(self.buf.clone()),
            _ => None,
        }
    }
}

/// Encode a focus gained (`CSI I`) or focus lost (`CSI O`) event.
///
/// Returns `None` when the encoder produces no output (should not happen for
/// valid focus events).
pub fn encode_focus(gained: bool) -> Option<Vec<u8>> {
    let event = if gained {
        focus::Event::Gained
    } else {
        focus::Event::Lost
    };

    let mut buf = [0u8; 8];
    match event.encode(&mut buf) {
        Ok(written) if written > 0 => Some(buf.get(..written)?.to_vec()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
