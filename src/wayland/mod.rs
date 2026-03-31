mod cursor;
mod drawing;
pub(crate) mod event_loop;
mod handlers;
pub(crate) mod init;
mod input;
mod osc;
mod process;
mod selection;
mod state;

// Re-exports for the binary crate (main.rs)
pub use osc::{Osc52State, OscAccum, OscEvent, scan_osc52};
pub use state::{
    ClipboardState, DisplayConfig, DisplayFlags, ImeState, InputState, MouseState, OscTracking,
    RepeatState, WaylandState, WindowGeometry,
};

/// Re-export selection types from the library crate.
pub use horseshoe::selection::{SearchState, SelectionState};

// Items brought into mod scope so siblings can import via `use super::*`
use handlers::wayland_button_to_ghostty;
use process::read_pipe_with_timeout;

use smithay_client_toolkit::{activation::RequestData, shell::WaylandSurface};

use horseshoe::font::FontManager;
use horseshoe::num::pixel_f64_to_f32;
use horseshoe::pty::Pty;
use horseshoe::terminal::render::RenderState;
use horseshoe::terminal::vt::TerminalCb;

/// Profiling flag, enabled by `HAND_PROFILE=1` environment variable.
static PROFILE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var_os("HAND_PROFILE").is_some());

/// Debug flag, enabled by `HAND_DEBUG=1` environment variable.
static DEBUG: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var_os("HAND_DEBUG").is_some());

pub fn profiling() -> bool {
    *PROFILE
}

pub(crate) fn debugging() -> bool {
    *DEBUG
}

/// Log a debug message only when `HAND_DEBUG=1` is set.
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        if $crate::wayland::debugging() {
            eprintln!($($arg)*);
        }
    };
}
pub(crate) use dbg_log;

/// Main application state.
///
/// ## Method locations
/// - **Font/scale**: `mod.rs` — `rebuild_font`, `adjust_font_size`, `reset_font_size`, `apply_scale`
/// - **Drawing**: `drawing.rs` — `draw`, `commit_buffer`, `recalculate_grid`, `build_search_highlights`
/// - **Input dispatch**: `input.rs` — `handle_key`, `dispatch_binding`, pointer/scroll handlers
/// - **Selection**: `selection.rs` — `copy_selection`, `paste_clipboard`, `extract_selected_text`
/// - **Clipboard/OSC**: `handlers.rs` — `DataDeviceHandler`, `PrimarySelectionHandler`
/// - **Init/event loop**: `init.rs` — `create_app`; `event_loop.rs` — `register_event_sources`, `drain_startup_pty`
pub struct App {
    // --- Wayland infrastructure ---
    pub wl: WaylandState,

    // --- Terminal core ---
    pub terminal: TerminalCb,
    pub render_state: RenderState,
    pub cached_colors: Option<horseshoe::terminal::render::RenderColors>,
    pub pty: Pty,
    pub font: FontManager,
    pub retained_buf: Vec<u8>,

    // --- Substates ---
    pub input: InputState,
    pub clipboard: ClipboardState,
    pub ime: ImeState,
    pub osc: OscTracking,
    pub geometry: WindowGeometry,
    pub repeat: RepeatState,

    // --- Already-existing substates ---
    pub display: DisplayConfig,
    pub selection: SelectionState,
    pub search: SearchState,

    // --- Global flags & timing ---
    pub dirty: bool,
    /// Whether the terminal state has changed since the last `render_state.update()`.
    /// Set by `process_pty_chunk()` after `vt_write()`; cleared after `update()` in `draw()`.
    pub terminal_changed: bool,
    pub running: bool,
    pub last_data_time: std::time::Instant,
    pub last_render_time: Option<std::time::Instant>,
}

impl App {
    /// Request an xdg-activation token (triggers urgency hint on most compositors).
    pub fn request_activation(&self) {
        if let Some(ref activation) = self.wl.activation_state {
            activation.request_token(
                &self.wl.qh,
                RequestData {
                    app_id: None,
                    seat_and_serial: None,
                    surface: Some(self.wl.window.wl_surface().clone()),
                },
            );
        }
    }

    /// Recreate fonts at the current base size and scale, then recalculate grid.
    fn rebuild_font(&mut self) {
        let scale_f32 = pixel_f64_to_f32(self.geometry.scale_f64());
        let phys_size = self.geometry.base_font_size * scale_f32;
        self.font.rebuild_at_size(phys_size);
        self.recalculate_grid();
        self.dirty = true;
    }

    /// Adjust the logical font size by the given delta (e.g. +1.0 or -1.0 points).
    fn adjust_font_size(&mut self, delta: f32) {
        if self.geometry.adjust_font_size(delta) {
            self.rebuild_font();
        }
    }

    /// Reset font size to the original configured size.
    fn reset_font_size(&mut self) {
        if self.geometry.reset_font_size() {
            self.rebuild_font();
        }
    }

    /// Apply a new scale (numerator/120) and recreate fonts.
    fn apply_scale(&mut self, new_120: u32) {
        self.geometry.apply_scale(new_120);
        self.rebuild_font();
    }
}

#[cfg(test)]
mod tests;
