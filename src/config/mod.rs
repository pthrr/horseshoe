mod bindings;
mod colors;
pub mod font;
pub mod input;
pub mod terminal_cfg;
mod types;
pub mod window;

pub use bindings::{BindingKey, Bindings, KeyAction};
pub use colors::{Color, ColorConfig};
pub use font::FontConfig;
pub use input::InputConfig;
pub use terminal_cfg::TerminalConfig;
pub use types::{PT_TO_PX, SelectionTarget, WindowMode};
pub use window::WindowConfig;

use bindings::{parse_binding_action, parse_binding_combo};
use colors::{parse_foot_cursor_color, parse_hex_color, parse_palette_key};

use std::path::PathBuf;

/// Application configuration loaded from `~/.config/foot/foot.ini`.
#[derive(Default)]
pub struct Config {
    pub font: FontConfig,
    pub colors: ColorConfig,
    pub window: WindowConfig,
    pub terminal: TerminalConfig,
    pub input: InputConfig,
    /// Configurable key bindings.
    pub bindings: Bindings,
}

impl Config {
    /// Load config from `~/.config/foot/foot.ini`, falling back to defaults.
    pub fn load() -> Self {
        Self::load_from(&config_path())
    }

    /// Load config from a specific path, falling back to defaults.
    pub fn load_from(path: &std::path::Path) -> Self {
        if !path.exists() {
            eprintln!("No config file at {}, using defaults", path.display());
            return Self::default();
        }

        match std::fs::read_to_string(path) {
            Ok(contents) => Self::parse(&contents),
            Err(err) => {
                eprintln!("Failed to read config file: {err}");
                Self::default()
            }
        }
    }

    /// Validate the config file at the default path.
    /// Returns `Ok(())` if valid, or `Err(errors)` with a list of issues.
    pub fn check(path: &std::path::Path) -> Result<(), Vec<String>> {
        if !path.exists() {
            return Err(vec![format!("Config file not found: {}", path.display())]);
        }
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let mut errors = Vec::new();
                for (line_no, raw_line) in contents.lines().enumerate() {
                    let trimmed = raw_line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
                        continue;
                    }
                    if !trimmed.contains('=') {
                        errors.push(format!(
                            "line {}: missing '=' separator: {trimmed}",
                            line_no + 1
                        ));
                    }
                }
                // Try full parse to catch any issues
                let _ = Self::parse(&contents);
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors)
                }
            }
            Err(err) => Err(vec![format!("Cannot read config: {err}")]),
        }
    }

    /// Apply a single `[section.]key=value` override (from `-o` CLI flag).
    pub fn apply_override(&mut self, kv: &str) {
        let Some((raw_key, raw_val)) = kv.split_once('=') else {
            eprintln!("Invalid override (missing '='): {kv}");
            return;
        };
        // Strip optional section prefix (e.g. "colors.foreground" → "foreground")
        let key_part = raw_key.trim();
        let key = if let Some((_section, k)) = key_part.split_once('.') {
            k.to_lowercase()
        } else {
            key_part.to_lowercase()
        };
        let value = raw_val.trim();
        Self::apply_setting(self, &key, value);
    }

    /// Return the default config file path.
    pub fn default_path() -> PathBuf {
        config_path()
    }

    /// Parse foot.ini config using `configparser` for structural INI parsing.
    /// Unrecognized keys are silently ignored.
    fn parse(contents: &str) -> Self {
        let mut cfg = Self::default();
        let mut ini = configparser::ini::Ini::new();
        // Disable inline comments so `#` in color values like `#d4be98` is preserved.
        ini.set_inline_comment_symbols(Some(&[]));
        if ini.read(contents.to_string()).is_err() {
            return cfg;
        }

        for (section, kvs) in ini.get_map_ref() {
            let is_key_bindings = section.eq_ignore_ascii_case("key-bindings");
            for (key, val) in kvs {
                let Some(value) = val else { continue };
                if is_key_bindings {
                    Self::apply_key_binding(&mut cfg, key, value);
                } else {
                    Self::apply_setting(&mut cfg, key, value);
                }
            }
        }

        cfg
    }

    /// Parse a `[key-bindings]` entry: `action-name=modifier+key` or `none` to unbind.
    fn apply_key_binding(cfg: &mut Self, action_name: &str, combo: &str) {
        let Some(action) = parse_binding_action(action_name) else {
            return;
        };
        if combo.eq_ignore_ascii_case("none") {
            cfg.bindings.unbind_action(action);
            return;
        }
        if let Some(bk) = parse_binding_combo(combo) {
            cfg.bindings.set(action, bk);
        }
    }

    fn apply_setting(cfg: &mut Self, key: &str, value: &str) {
        if !Self::apply_core(cfg, key, value)
            && !Self::apply_window(cfg, key, value)
            && !Self::apply_font_scroll(cfg, key, value)
        {
            Self::apply_appearance(cfg, key, value);
        }
    }

    /// Core terminal behavior settings (font, shell, scrollback, cursor).
    fn apply_core(cfg: &mut Self, key: &str, value: &str) -> bool {
        match key {
            "font-size" | "font_size" => {
                if let Ok(pt) = value.parse::<f32>() {
                    cfg.font.size = (pt * PT_TO_PX).clamp(8.0, 96.0);
                }
            }
            "font" => {
                if let Some(family) = parse_foot_font_family(value) {
                    cfg.font.family = Some(family);
                }
                if let Some(pt) = parse_foot_font_size(value) {
                    cfg.font.size = (pt * PT_TO_PX).clamp(8.0, 96.0);
                }
            }
            "scrollback" | "lines" => {
                if let Ok(v) = value.parse::<usize>() {
                    cfg.terminal.scrollback = v;
                }
            }
            "shell" => {
                if !value.is_empty() {
                    cfg.terminal.shell = Some(value.to_string());
                }
            }
            "cols" | "columns" => {
                if let Ok(v) = value.parse::<u16>() {
                    cfg.window.initial_cols = v;
                }
            }
            "rows" => {
                if let Ok(v) = value.parse::<u16>() {
                    cfg.window.initial_rows = v;
                }
            }
            "cursor-blink" | "cursor_blink" => {
                cfg.terminal.cursor_blink = matches!(value, "true" | "yes" | "1");
            }
            "cursor-blink-interval" | "cursor_blink_interval" => {
                if let Ok(v) = value.parse::<u64>() {
                    cfg.terminal.cursor_blink_interval_ms = v.clamp(100, 2000);
                }
            }
            "bold-is-bright" | "bold_is_bright" | "bold-text-in-bright" | "bold_text_in_bright" => {
                cfg.input.bold_is_bright = matches!(value, "true" | "yes" | "1");
            }
            "title" | "window-title" | "window_title" => {
                if !value.is_empty() {
                    cfg.window.title = value.to_string();
                }
            }
            "app-id" | "app_id" => {
                if !value.is_empty() {
                    cfg.window.app_id = value.to_string();
                }
            }
            "term" | "term-program" | "term_program" => {
                if !value.is_empty() {
                    cfg.terminal.term = Some(value.to_string());
                }
            }
            "login-shell" | "login_shell" => {
                cfg.terminal.login_shell = value == "yes" || value == "true";
            }
            "locked-title" | "locked_title" => {
                cfg.window.locked_title = matches!(value, "yes" | "true" | "1");
            }
            _ => return false,
        }
        true
    }

    /// Window behavior settings (size, mode, selection, notifications).
    fn apply_window(cfg: &mut Self, key: &str, value: &str) -> bool {
        match key {
            "selection-target" | "selection_target" => {
                cfg.input.selection_target = match value {
                    "none" => SelectionTarget::None,
                    "clipboard" => SelectionTarget::Clipboard,
                    "both" => SelectionTarget::Both,
                    _ => SelectionTarget::Primary,
                };
            }
            "word-delimiters" | "word_delimiters" => {
                if !value.is_empty() {
                    cfg.input.word_delimiters = Some(value.to_string());
                }
            }
            "notify" => {
                if !value.is_empty() {
                    cfg.terminal.notify_command = Some(value.to_string());
                }
            }
            "resize-delay-ms" | "resize_delay_ms" => {
                if let Ok(v) = value.parse::<u32>() {
                    cfg.window.resize_delay_ms = v.min(1000);
                }
            }
            "initial-window-size-pixels" => {
                if let Some(size) = parse_size_u32(value) {
                    cfg.window.initial_size_pixels = Some(size);
                }
            }
            "initial-window-size-chars" => {
                if let Some((w, h)) = parse_size_u32(value) {
                    let cols = u16::try_from(w).unwrap_or(u16::MAX);
                    let rows = u16::try_from(h).unwrap_or(u16::MAX);
                    cfg.window.initial_size_chars = Some((cols, rows));
                }
            }
            "initial-window-mode" => {
                cfg.window.initial_window_mode = match value {
                    "maximized" => WindowMode::Maximized,
                    "fullscreen" => WindowMode::Fullscreen,
                    _ => WindowMode::Windowed,
                };
            }
            "hide-when-typing" | "hide_when_typing" => {
                cfg.input.hide_when_typing = matches!(value, "yes" | "true" | "1");
            }
            "alternate-scroll-mode" | "alternate_scroll_mode" => {
                cfg.input.alternate_scroll_mode = matches!(value, "yes" | "true" | "1");
            }
            // foot keys: accepted but not configurable in horseshoe (yet)
            "dpi-aware"
            | "dpi_aware"
            | "workers"
            | "horizontal-letter-offset"
            | "vertical-letter-offset"
            | "underline-offset"
            | "underline_offset"
            | "box-drawings-uses-font-glyphs"
            | "urgent"
            | "command"
            | "command-focused"
            | "notify-focus-inhibit"
            | "indicator-position"
            | "indicator-format"
            | "launch"
            | "label-letters"
            | "osc8-underline"
            | "protocols"
            | "uri-characters"
            | "style"
            | "color"
            | "blink"
            | "beam-thickness"
            | "underline-thickness"
            | "preferred"
            | "size"
            | "hide-when-maximized"
            | "border-width"
            | "border-color"
            | "button-width"
            | "button-color"
            | "button-minimize-color"
            | "button-maximize-color"
            | "button-close-color"
            | "bind"
            | "keybind"
            | "keybinding" => {}
            _ => return false,
        }
        true
    }

    /// Font variant and scroll settings.
    fn apply_font_scroll(cfg: &mut Self, key: &str, value: &str) -> bool {
        match key {
            "line-height" | "line_height" => {
                if let Ok(v) = value.parse::<f32>() {
                    cfg.font.line_height = v.clamp(-10.0, 50.0);
                }
            }
            "letter-spacing" | "letter_spacing" => {
                if let Ok(v) = value.parse::<f32>() {
                    cfg.font.letter_spacing = v.clamp(-5.0, 20.0);
                }
            }
            "font-bold" => {
                if let Some(family) = parse_foot_font_family(value) {
                    cfg.font.bold = Some(family);
                }
            }
            "font-italic" => {
                if let Some(family) = parse_foot_font_family(value) {
                    cfg.font.italic = Some(family);
                }
            }
            "font-bold-italic" => {
                if let Some(family) = parse_foot_font_family(value) {
                    cfg.font.bold_italic = Some(family);
                }
            }
            "multiplier" => {
                if let Ok(v) = value.parse::<f32>() {
                    cfg.input.scroll_multiplier = v.clamp(0.1, 50.0);
                }
            }
            _ => return false,
        }
        true
    }

    /// Apply appearance settings (colors, padding, opacity).
    fn apply_appearance(cfg: &mut Self, key: &str, value: &str) {
        match key {
            "opacity" | "background-opacity" | "background_opacity" | "alpha" => {
                if let Ok(v) = value.parse::<f32>() {
                    cfg.colors.opacity = v.clamp(0.0, 1.0);
                }
            }
            "padding" | "pad" => {
                // Support foot's `pad=NxM [center]` syntax (use first value N).
                let numeric = value.split_whitespace().next().unwrap_or(value);
                let horizontal = numeric.split('x').next().unwrap_or(numeric);
                if let Ok(v) = horizontal.parse::<u32>() {
                    cfg.window.padding = v.min(100);
                }
            }
            "foreground" | "fg" => {
                if let Some(c) = parse_hex_color(value) {
                    cfg.colors.foreground = Some(c);
                }
            }
            "background" | "bg" => {
                if let Some(c) = parse_hex_color(value) {
                    cfg.colors.background = Some(c);
                }
            }
            "cursor-color" | "cursor_color" => {
                if let Some(c) = parse_hex_color(value) {
                    cfg.colors.cursor = Some(c);
                }
            }
            // foot: `cursor= TEXT_COLOR CURSOR_COLOR` (space-separated dual value)
            "cursor" => {
                if let Some(c) = parse_foot_cursor_color(value) {
                    cfg.colors.cursor = Some(c);
                }
            }
            "selection-foreground" | "selection_foreground" => {
                if let Some(c) = parse_hex_color(value) {
                    cfg.colors.selection_fg = Some(c);
                }
            }
            "selection-background" | "selection_background" => {
                if let Some(c) = parse_hex_color(value) {
                    cfg.colors.selection_bg = Some(c);
                }
            }
            // foot keys: accepted but not adjustable in horseshoe
            "jump-labels"
            | "scrollback-indicator"
            | "search-box-no-match"
            | "search-box-match"
            | "urls" => {}
            _ if key.starts_with("dim") && key.len() == 4 => {
                if let Some(idx_ch) = key.chars().nth(3)
                    && let Some(idx) = idx_ch.to_digit(10)
                    && (idx as usize) < 8
                    && let Some(slot) = cfg.colors.dim_palette.get_mut(idx as usize)
                    && let Some(c) = parse_hex_color(value)
                {
                    *slot = Some(c);
                }
            }
            _ => {
                // Check for palette colors: color0..color15, regular0..regular7, bright0..bright7
                if let Some(idx) = parse_palette_key(key)
                    && let Some(slot) = cfg.colors.palette.get_mut(idx)
                {
                    if let Some(c) = parse_hex_color(value) {
                        *slot = Some(c);
                    }
                } else {
                    eprintln!("Unknown config key: {key}");
                }
            }
        }
    }

    /// Generate OSC escape sequences to set configured colors on the terminal.
    pub fn color_osc_sequences(&self) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();

        // OSC 10 ; <color> ST — set default foreground
        if let Some(fg) = self.colors.foreground {
            let _ = write!(buf, "\x1b]10;#{:02x}{:02x}{:02x}\x1b\\", fg.r, fg.g, fg.b);
        }

        // OSC 11 ; <color> ST — set default background
        if let Some(bg) = self.colors.background {
            let _ = write!(buf, "\x1b]11;#{:02x}{:02x}{:02x}\x1b\\", bg.r, bg.g, bg.b);
        }

        // OSC 12 ; <color> ST — set cursor color
        if let Some(cc) = self.colors.cursor {
            let _ = write!(buf, "\x1b]12;#{:02x}{:02x}{:02x}\x1b\\", cc.r, cc.g, cc.b);
        }

        // OSC 4 ; <index> ; <color> ST — set palette color
        for (idx, slot) in self.colors.palette.iter().enumerate() {
            if let Some(c) = slot {
                let _ = write!(buf, "\x1b]4;{idx};#{:02x}{:02x}{:02x}\x1b\\", c.r, c.g, c.b);
            }
        }

        buf
    }
}

/// Parse foot's `font=Family:size=N` syntax, extracting the family name.
///
/// The family is everything before the first `:` modifier.
/// Returns `None` if the family is empty.
fn parse_foot_font_family(value: &str) -> Option<String> {
    let family = value.split(':').next()?.trim();
    if family.is_empty() {
        return None;
    }
    Some(family.to_string())
}

/// Parse foot's `font=Family:size=N` syntax, extracting the size.
fn parse_foot_font_size(value: &str) -> Option<f32> {
    // Look for `:size=N` or `size=N` anywhere in the value
    for part in value.split(':') {
        let trimmed = part.trim();
        if let Some(size_str) = trimmed.strip_prefix("size=") {
            return size_str.parse::<f32>().ok();
        }
    }
    None
}

/// Parse a `WIDTHxHEIGHT` size string into `(u32, u32)`.
fn parse_size_u32(value: &str) -> Option<(u32, u32)> {
    let (w_str, h_str) = value.split_once('x')?;
    let w = w_str.trim().parse::<u32>().ok()?;
    let h = h_str.trim().parse::<u32>().ok()?;
    if w > 0 && h > 0 { Some((w, h)) } else { None }
}

fn config_path() -> PathBuf {
    if let Some(config_dir) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(config_dir).join("foot").join("foot.ini")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
            .join(".config")
            .join("foot")
            .join("foot.ini")
    } else {
        PathBuf::from("/etc/foot/foot.ini")
    }
}

#[cfg(test)]
mod tests;
