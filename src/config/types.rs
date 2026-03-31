/// Initial window mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

/// Selection target for automatic selection-to-clipboard behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionTarget {
    None,
    #[default]
    Primary,
    Clipboard,
    Both,
}

/// Conversion factor from typographic points to pixels at 96 DPI.
/// foot.ini specifies font sizes in points; fontdue expects pixels.
/// Standard desktop DPI is 96, and 1pt = 1/72 inch, so px = pt * 96/72.
pub const PT_TO_PX: f32 = 96.0 / 72.0;
