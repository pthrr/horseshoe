/// Wayland protocol and infrastructure state.
pub struct WaylandState {
    pub registry_state: smithay_client_toolkit::registry::RegistryState,
    pub seat_state: smithay_client_toolkit::seat::SeatState,
    pub output_state: smithay_client_toolkit::output::OutputState,
    pub compositor_state: smithay_client_toolkit::compositor::CompositorState,
    pub shm: smithay_client_toolkit::shm::Shm,
    pub _xdg_shell: smithay_client_toolkit::shell::xdg::XdgShell,
    pub data_device_manager: smithay_client_toolkit::data_device_manager::DataDeviceManagerState,
    pub primary_selection_manager: Option<smithay_client_toolkit::primary_selection::PrimarySelectionManagerState>,
    pub loop_handle: calloop::LoopHandle<'static, super::App>,
    pub window: smithay_client_toolkit::shell::xdg::window::Window,
    pub pool: smithay_client_toolkit::shm::slot::SlotPool,
    pub buffer: Option<smithay_client_toolkit::shm::slot::Buffer>,
    pub viewport: Option<smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport>,
    pub fractional_scale_obj: Option<smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1>,
    pub conn: wayland_client::Connection,
    pub activation_state: Option<smithay_client_toolkit::activation::ActivationState>,
    pub qh: wayland_client::QueueHandle<super::App>,
}

/// Input-related state: encoders, modifiers, mouse, cursor, and bindings.
pub struct InputState {
    pub key_encoder: horseshoe::terminal::input::KeyEncoder,
    pub mouse_encoder: horseshoe::terminal::input::MouseEncoder,
    pub mods: horseshoe::keymap::ModifierState,
    pub mouse: MouseState,
    pub cursor: Option<super::cursor::Cursor>,
    pub bindings: horseshoe::config::Bindings,
    /// Whether the window currently has keyboard focus.
    pub focused: bool,
}

impl InputState {
    pub const fn ghostty_mods(&self) -> libghostty_vt::key::Mods {
        self.mods.to_mods()
    }

    /// Check whether the shift modifier is currently held.
    pub const fn mods_has_shift(&self) -> bool {
        self.mods
            .to_mods()
            .contains(libghostty_vt::key::Mods::SHIFT)
    }

    /// Check whether the ctrl modifier is currently held.
    pub const fn mods_has_ctrl(&self) -> bool {
        self.mods.to_mods().contains(libghostty_vt::key::Mods::CTRL)
    }

    /// Look up a keybinding from the configurable bindings table.
    pub fn check_binding(&self, keysym: u32) -> Option<horseshoe::config::KeyAction> {
        self.bindings
            .lookup(keysym, self.mods_has_ctrl(), self.mods_has_shift())
    }

    /// Return mouse coordinates scaled to physical (buffer) pixels.
    pub fn scaled_mouse(&self, scale_f64: f64) -> (f32, f32) {
        let mx = horseshoe::num::pixel_f64_to_f32(self.mouse.x * scale_f64);
        let my = horseshoe::num::pixel_f64_to_f32(self.mouse.y * scale_f64);
        (mx, my)
    }
}

/// Mouse tracking state.
#[derive(Default)]
pub struct MouseState {
    pub x: f64,
    pub y: f64,
    pub buttons_pressed: u32,
    pub scrollbar_dragging: bool,
    /// Whether the pointer is currently hidden (hide-when-typing).
    pub pointer_hidden: bool,
    /// Accumulator for continuous scroll events (sub-step residual).
    pub scroll_accum: f64,
}

/// IME (text-input-v3) state.
#[derive(Default)]
pub struct ImeState {
    pub text_input_manager: Option<smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3>,
    pub text_input: Option<smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_v3::ZwpTextInputV3>,
    pub preedit_text: Option<String>,
    pub pending_preedit: Option<String>,
    pub pending_commit: Option<String>,
}

impl ImeState {
    /// Enable IME text input and position the popup at the terminal cursor.
    pub fn enable(
        &self,
        cursor: &horseshoe::terminal::render::CursorState,
        scale_120: u32,
        cell_width: u32,
        cell_height: u32,
        padding: u32,
    ) {
        if let Some(ref ti) = self.text_input {
            use smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_v3;
            ti.enable();
            ti.set_content_type(
                zwp_text_input_v3::ContentHint::empty(),
                zwp_text_input_v3::ContentPurpose::Terminal,
            );
            let scale = u64::from(scale_120);
            let cw = u32::try_from(u64::from(cell_width) * 120 / scale).unwrap_or(1);
            let ch = u32::try_from(u64::from(cell_height) * 120 / scale).unwrap_or(1);
            let x = padding + u32::from(cursor.x) * cw;
            let y = padding + u32::from(cursor.y) * ch;
            ti.set_cursor_rectangle(
                horseshoe::num::u32_to_i32(x),
                horseshoe::num::u32_to_i32(y),
                horseshoe::num::u32_to_i32(cw),
                horseshoe::num::u32_to_i32(ch),
            );
            ti.commit();
        }
    }

    /// Disable IME text input (called on keyboard focus leave).
    pub fn disable(&mut self) {
        if let Some(ref ti) = self.text_input {
            ti.disable();
            ti.commit();
        }
        self.preedit_text = None;
        self.pending_preedit = None;
        self.pending_commit = None;
    }
}

/// OSC sequence tracking state (OSC 52 clipboard, OSC 7 cwd, OSC 133 prompts).
#[derive(Default)]
pub struct OscTracking {
    pub osc52_state: super::Osc52State,
    pub osc52_buf: Vec<u8>,
    pub osc_accum: super::OscAccum,
    pub cwd: Option<std::path::PathBuf>,
    pub prompt_marks: Vec<u16>,
}

const FONT_SIZE_MIN: f32 = 6.0;
const FONT_SIZE_MAX: f32 = 72.0;

/// Window geometry: logical dimensions, grid size, and scale factor.
pub struct WindowGeometry {
    pub width: u32,
    pub height: u32,
    pub term_cols: u16,
    pub term_rows: u16,
    /// Scale factor as numerator with denominator 120 (120 = 1.0x, 240 = 2.0x).
    pub scale_120: u32,
    /// Base font size (logical, before scale multiplication).
    pub base_font_size: f32,
    /// Original font size from config (for reset).
    pub initial_font_size: f32,
}

impl WindowGeometry {
    /// Convert a logical pixel value to physical (buffer) pixels using the current scale.
    pub fn phys(&self, logical: u32) -> u32 {
        horseshoe::num::phys_from_scale(logical, self.scale_120)
    }

    /// Return the current scale factor as an `f64`.
    pub fn scale_f64(&self) -> f64 {
        f64::from(self.scale_120) / 120.0
    }

    /// Adjust the logical font size by the given delta (e.g. +1.0 or -1.0 points).
    /// Returns `true` if the size changed.
    pub fn adjust_font_size(&mut self, delta: f32) -> bool {
        let new_size = (self.base_font_size + delta).clamp(FONT_SIZE_MIN, FONT_SIZE_MAX);
        if (new_size - self.base_font_size).abs() < f32::EPSILON {
            return false;
        }
        self.base_font_size = new_size;
        true
    }

    /// Reset font size to the original configured size.
    /// Returns `true` if the size changed.
    pub fn reset_font_size(&mut self) -> bool {
        if (self.base_font_size - self.initial_font_size).abs() < f32::EPSILON {
            return false;
        }
        self.base_font_size = self.initial_font_size;
        true
    }

    /// Apply a new scale (numerator/120).
    pub const fn apply_scale(&mut self, new_120: u32) {
        self.scale_120 = new_120;
    }
}

/// Clipboard and primary selection state.
pub struct ClipboardState {
    pub data_device: Option<smithay_client_toolkit::data_device_manager::data_device::DataDevice>,
    pub primary_selection_device:
        Option<smithay_client_toolkit::primary_selection::device::PrimarySelectionDevice>,
    pub copy_paste_source:
        Option<smithay_client_toolkit::data_device_manager::data_source::CopyPasteSource>,
    pub clipboard_content: String,
    pub primary_selection_source:
        Option<smithay_client_toolkit::primary_selection::selection::PrimarySelectionSource>,
    pub primary_selection_content: String,
    pub has_wl_copy: bool,
    pub last_serial: u32,
}

impl ClipboardState {
    /// Persist clipboard content via `wl-copy` so it survives after exit.
    pub fn persist(&self) {
        if !self.has_wl_copy
            || self.copy_paste_source.is_none()
            || self.clipboard_content.is_empty()
        {
            return;
        }
        let Ok(mut child) = std::process::Command::new("wl-copy")
            .arg("--type")
            .arg("text/plain;charset=utf-8")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = std::io::Write::write_all(&mut stdin, self.clipboard_content.as_bytes());
        }
        // Don't wait — wl-copy daemonizes and serves the clipboard independently.
    }
}

/// Key repeat timing state for accumulated tick calculation.
pub struct RepeatState {
    /// Last time the repeat callback fired (for elapsed-time tick calculation).
    pub last_callback: Option<std::time::Instant>,
    /// Repeat interval in ms from compositor `repeat_info`. Default 40ms (25 Hz).
    pub interval_ms: u32,
}

impl Default for RepeatState {
    fn default() -> Self {
        Self {
            last_callback: None,
            interval_ms: 40,
        }
    }
}

impl RepeatState {
    /// Compute how many repeat ticks elapsed since the last callback,
    /// then update `last_callback` to now.
    ///
    /// Returns at least 1 (the current tick), plus any missed ticks if
    /// the event loop stalled. Capped at 8 to limit burst after long gaps.
    pub fn accumulated_ticks(&mut self) -> u32 {
        let now = std::time::Instant::now();
        let count = match self.last_callback {
            Some(last) => {
                let elapsed =
                    u64::try_from(now.duration_since(last).as_millis()).unwrap_or(u64::MAX);
                let interval = u64::from(self.interval_ms).max(1);
                let ticks = elapsed / interval;
                u32::try_from(ticks.clamp(1, 8)).unwrap_or(8)
            }
            None => 1,
        };
        self.last_callback = Some(now);
        count
    }
}

/// Packed configuration flags for display behavior (avoids excessive bools on `DisplayConfig`).
#[derive(Clone, Copy)]
pub struct DisplayFlags(u8);

impl DisplayFlags {
    const BOLD_IS_BRIGHT: u8 = 1 << 0;
    const LOCKED_TITLE: u8 = 1 << 1;
    const HOLD: u8 = 1 << 2;
    const HIDE_WHEN_TYPING: u8 = 1 << 3;
    const ALTERNATE_SCROLL_MODE: u8 = 1 << 4;

    pub const fn new() -> Self {
        Self(Self::ALTERNATE_SCROLL_MODE)
    }

    pub const fn bold_is_bright(self) -> bool {
        self.0 & Self::BOLD_IS_BRIGHT != 0
    }
    pub const fn locked_title(self) -> bool {
        self.0 & Self::LOCKED_TITLE != 0
    }
    pub const fn hold(self) -> bool {
        self.0 & Self::HOLD != 0
    }
    pub const fn hide_when_typing(self) -> bool {
        self.0 & Self::HIDE_WHEN_TYPING != 0
    }
    pub const fn alternate_scroll_mode(self) -> bool {
        self.0 & Self::ALTERNATE_SCROLL_MODE != 0
    }

    pub const fn with_bold_is_bright(self, v: bool) -> Self {
        Self(if v {
            self.0 | Self::BOLD_IS_BRIGHT
        } else {
            self.0 & !Self::BOLD_IS_BRIGHT
        })
    }
    pub const fn with_locked_title(self, v: bool) -> Self {
        Self(if v {
            self.0 | Self::LOCKED_TITLE
        } else {
            self.0 & !Self::LOCKED_TITLE
        })
    }
    pub const fn with_hold(self, v: bool) -> Self {
        Self(if v {
            self.0 | Self::HOLD
        } else {
            self.0 & !Self::HOLD
        })
    }
    pub const fn with_hide_when_typing(self, v: bool) -> Self {
        Self(if v {
            self.0 | Self::HIDE_WHEN_TYPING
        } else {
            self.0 & !Self::HIDE_WHEN_TYPING
        })
    }
    pub const fn with_alternate_scroll_mode(self, v: bool) -> Self {
        Self(if v {
            self.0 | Self::ALTERNATE_SCROLL_MODE
        } else {
            self.0 & !Self::ALTERNATE_SCROLL_MODE
        })
    }
}

/// Display and rendering configuration.
pub struct DisplayConfig {
    pub cursor_blink_visible: bool,
    pub fullscreen: bool,
    pub padding: u32,
    pub opacity: f32,
    pub selection_fg: Option<(u8, u8, u8)>,
    pub selection_bg: Option<(u8, u8, u8)>,
    pub scroll_multiplier: f32,
    pub flags: DisplayFlags,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            cursor_blink_visible: true,
            fullscreen: false,
            padding: 0,
            opacity: 1.0,
            selection_fg: None,
            selection_bg: None,
            scroll_multiplier: 3.0,
            flags: DisplayFlags::new(),
        }
    }
}
