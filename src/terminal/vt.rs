use std::cell::RefCell;
use std::rc::Rc;

use libghostty_vt::TerminalOptions;
use libghostty_vt::ffi::GhosttySizeReportSize;
use libghostty_vt::terminal::{
    ConformanceLevel, DeviceAttributeFeature, DeviceAttributes, DeviceType, Mode,
    PrimaryDeviceAttributes, ScrollViewport, SecondaryDeviceAttributes, TertiaryDeviceAttributes,
};

/// Shared terminal operations delegated to the inner `libghostty_vt::Terminal`.
///
/// Both [`TerminalCb`] and [`Terminal`] implement this trait by forwarding
/// calls to their inner terminal instance.
pub trait TerminalOps {
    /// Access the inner terminal (immutable).
    fn vt(&self) -> &libghostty_vt::Terminal<'static, 'static>;
    /// Access the inner terminal (mutable).
    fn vt_mut(&mut self) -> &mut libghostty_vt::Terminal<'static, 'static>;

    /// Feed VT-encoded data to the terminal.
    fn vt_write(&mut self, data: &[u8]) {
        self.vt_mut().vt_write(data);
    }

    /// Scroll the viewport by a delta (negative = up, positive = down).
    fn scroll_viewport_delta(&mut self, delta: isize) {
        self.vt_mut().scroll_viewport(ScrollViewport::Delta(delta));
    }

    /// Scroll the viewport to the bottom (active area).
    fn scroll_viewport_bottom(&mut self) {
        self.vt_mut().scroll_viewport(ScrollViewport::Bottom);
    }

    /// Scroll the viewport to the top of scrollback.
    fn scroll_viewport_top(&mut self) {
        self.vt_mut().scroll_viewport(ScrollViewport::Top);
    }

    /// Set a terminal mode value.
    fn mode_set(&mut self, mode: Mode, value: bool) -> Result<(), &'static str> {
        self.vt_mut()
            .set_mode(mode, value)
            .map_err(|_| "failed to set terminal mode")
    }

    /// Get a terminal mode value.
    fn mode_get(&self, mode: Mode) -> Option<bool> {
        self.vt().mode(mode).ok()
    }

    /// Get the scrollbar state.
    fn scrollbar(&self) -> Option<libghostty_vt::ffi::GhosttyTerminalScrollbar> {
        self.vt().scrollbar().ok()
    }

    /// Check if the alternate screen is active.
    fn is_alternate_screen(&self) -> bool {
        self.vt().active_screen().ok().is_some_and(|s| {
            s == libghostty_vt::ffi::GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_ALTERNATE
        })
    }

    /// Get the cursor position as (col, row), both 0-based.
    fn cursor_position(&self) -> (u16, u16) {
        let cx = self.vt().cursor_x().unwrap_or(0);
        let cy = self.vt().cursor_y().unwrap_or(0);
        (cx, cy)
    }

    /// Hard-reset the terminal.
    fn reset(&mut self) {
        self.vt_mut().reset();
    }
}

impl TerminalOps for TerminalCb {
    fn vt(&self) -> &libghostty_vt::Terminal<'static, 'static> {
        &self.inner
    }
    fn vt_mut(&mut self) -> &mut libghostty_vt::Terminal<'static, 'static> {
        &mut self.inner
    }
}

impl TerminalOps for Terminal {
    fn vt(&self) -> &libghostty_vt::Terminal<'static, 'static> {
        &self.inner
    }
    fn vt_mut(&mut self) -> &mut libghostty_vt::Terminal<'static, 'static> {
        &mut self.inner
    }
}

/// Mutable state accumulated by terminal callbacks.
pub struct CallbackState {
    /// PTY response bytes accumulated by `on_pty_write`.
    pub pty_responses: Vec<u8>,
    /// Set when BEL (0x07) is received.
    pub bell_pending: bool,
    /// Set when the window title changes (OSC 0 / OSC 2).
    pub title: Option<String>,
    /// Current grid dimensions (updated on resize).
    pub cols: u16,
    /// Current grid rows (updated on resize).
    pub rows: u16,
}

impl CallbackState {
    const fn new(cols: u16, rows: u16) -> Self {
        Self {
            pty_responses: Vec::new(),
            bell_pending: false,
            title: None,
            cols,
            rows,
        }
    }
}

/// Terminal with callback-based side-effect dispatch.
///
/// Wraps `libghostty_vt::Terminal` with registered callbacks for PTY writes,
/// bell, title changes, device attributes, and XTVERSION queries.
///
/// The inner `Terminal` is heap-allocated (`Box`) because the upstream crate
/// stores a raw pointer to its inline `VTable` in the C layer during callback
/// registration. Moving the `Terminal` (e.g. returning it from a constructor)
/// would invalidate that pointer, causing SIGSEGV when callbacks fire. Boxing
/// ensures the `VTable` address is stable regardless of how `TerminalCb` moves.
pub struct TerminalCb {
    inner: Box<libghostty_vt::Terminal<'static, 'static>>,
    state: Rc<RefCell<CallbackState>>,
}

// The terminal is only accessed from a single thread (the calloop event loop).
// libghostty_vt marks Terminal as !Send, but we guarantee single-thread access.
unsafe impl Send for TerminalCb {}

impl TerminalCb {
    /// Create a new callback-aware terminal.
    pub fn new(cols: u16, rows: u16, max_scrollback: usize) -> Result<Self, &'static str> {
        let state = Rc::new(RefCell::new(CallbackState::new(cols, rows)));

        let mut inner = Box::new(
            libghostty_vt::Terminal::new(TerminalOptions {
                cols,
                rows,
                max_scrollback,
            })
            .map_err(|_| "failed to create terminal")?,
        );

        // Register callbacks using Rc<RefCell<CallbackState>> for shared mutable state.
        let pty_state = state.clone();
        let _ = inner
            .on_pty_write(move |_term, data| {
                pty_state.borrow_mut().pty_responses.extend_from_slice(data);
            })
            .map_err(|_| "failed to register on_pty_write")?;

        let bell_state = state.clone();
        let _ = inner
            .on_bell(move |_term| {
                bell_state.borrow_mut().bell_pending = true;
            })
            .map_err(|_| "failed to register on_bell")?;

        let title_state = state.clone();
        let _ = inner
            .on_title_changed(move |term| {
                if let Ok(title) = term.title() {
                    title_state.borrow_mut().title = Some(title.to_owned());
                }
            })
            .map_err(|_| "failed to register on_title_changed")?;

        let _ = inner
            .on_xtversion(|_term| Some(concat!("horseshoe(", env!("CARGO_PKG_VERSION"), ")")))
            .map_err(|_| "failed to register on_xtversion")?;

        let _ = inner
            .on_device_attributes(|_term| {
                Some(DeviceAttributes {
                    primary: PrimaryDeviceAttributes::new(
                        ConformanceLevel::LEVEL_2,
                        [DeviceAttributeFeature::ANSI_COLOR],
                    ),
                    secondary: SecondaryDeviceAttributes {
                        device_type: DeviceType::VT220,
                        firmware_version: 10,
                        rom_cartridge: 0,
                    },
                    tertiary: TertiaryDeviceAttributes {
                        // "HRS" in hex = 0x485253
                        unit_id: 0x48_52_53,
                    },
                })
            })
            .map_err(|_| "failed to register on_device_attributes")?;

        let _ = inner
            .on_enquiry(|_term| None)
            .map_err(|_| "failed to register on_enquiry")?;

        let size_state = state.clone();
        let _ = inner
            .on_size(move |_term| {
                let s = size_state.borrow();
                Some(GhosttySizeReportSize {
                    rows: s.rows,
                    columns: s.cols,
                    cell_width: 0,
                    cell_height: 0,
                })
            })
            .map_err(|_| "failed to register on_size")?;

        Ok(Self { inner, state })
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), &'static str> {
        // Pass 0 for pixel dims — we don't have pixel info at resize time.
        self.inner
            .resize(cols, rows, 0, 0)
            .map_err(|_| "failed to resize terminal")?;
        let mut s = self.state.borrow_mut();
        s.cols = cols;
        s.rows = rows;
        Ok(())
    }

    /// Get a reference to the inner `libghostty_vt::Terminal`.
    pub fn inner(&self) -> &libghostty_vt::Terminal<'static, 'static> {
        &self.inner
    }

    /// Get a mutable reference to the inner `libghostty_vt::Terminal`.
    pub fn inner_mut(&mut self) -> &mut libghostty_vt::Terminal<'static, 'static> {
        &mut self.inner
    }

    // -- Callback state accessors --

    /// Take accumulated PTY response bytes, leaving the buffer empty.
    pub fn take_pty_responses(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.state.borrow_mut().pty_responses)
    }

    /// Check and clear the bell-pending flag.
    pub fn take_bell(&mut self) -> bool {
        std::mem::replace(&mut self.state.borrow_mut().bell_pending, false)
    }

    /// Take the latest title if it changed, leaving `None`.
    pub fn take_title(&mut self) -> Option<String> {
        self.state.borrow_mut().title.take()
    }

    /// Read-only access to the callback state.
    pub fn callback_state(&self) -> std::cell::Ref<'_, CallbackState> {
        self.state.borrow()
    }
}

/// Safe wrapper around a `libghostty_vt::Terminal` (without callbacks).
///
/// Used in tests and benchmarks where callback dispatch is not needed.
pub struct Terminal {
    inner: libghostty_vt::Terminal<'static, 'static>,
}

unsafe impl Send for Terminal {}

impl Terminal {
    /// Create a new terminal with the given dimensions and scrollback.
    pub fn new(cols: u16, rows: u16, max_scrollback: usize) -> Result<Self, &'static str> {
        let inner = libghostty_vt::Terminal::new(TerminalOptions {
            cols,
            rows,
            max_scrollback,
        })
        .map_err(|_| "failed to create terminal")?;
        Ok(Self { inner })
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), &'static str> {
        self.inner
            .resize(cols, rows, 0, 0)
            .map_err(|_| "failed to resize terminal")
    }

    /// Get a reference to the inner `libghostty_vt::Terminal`.
    pub const fn inner(&self) -> &libghostty_vt::Terminal<'static, 'static> {
        &self.inner
    }

    /// Get a mutable reference to the inner `libghostty_vt::Terminal`.
    pub const fn inner_mut(&mut self) -> &mut libghostty_vt::Terminal<'static, 'static> {
        &mut self.inner
    }
}

#[cfg(test)]
#[path = "vt_tests.rs"]
mod tests;
