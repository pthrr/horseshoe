use super::types::WindowMode;

/// Window configuration.
pub struct WindowConfig {
    /// Window title. Defaults to "hs".
    pub title: String,
    /// XDG app-id for the window. Defaults to "hs".
    pub app_id: String,
    pub initial_cols: u16,
    pub initial_rows: u16,
    /// Initial window mode (windowed, maximized, fullscreen).
    pub initial_window_mode: WindowMode,
    /// Initial window size in pixels (width, height). Overrides cols/rows.
    pub initial_size_pixels: Option<(u32, u32)>,
    /// Initial window size in characters (cols, rows). Overrides `initial_cols`/`initial_rows`.
    pub initial_size_chars: Option<(u16, u16)>,
    /// Terminal padding in pixels.
    pub padding: u32,
    /// Resize delay in milliseconds (coalesce resize events).
    pub resize_delay_ms: u32,
    pub locked_title: bool,
    pub hold: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "hs".to_string(),
            app_id: "hs".to_string(),
            initial_cols: 0,
            initial_rows: 0,
            initial_window_mode: WindowMode::default(),
            initial_size_pixels: None,
            initial_size_chars: None,
            padding: 0,
            resize_delay_ms: 100,
            locked_title: false,
            hold: false,
        }
    }
}
