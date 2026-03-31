/// Parsed RGB color.
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Color configuration.
pub struct ColorConfig {
    /// Custom foreground color.
    pub foreground: Option<Color>,
    /// Custom background color.
    pub background: Option<Color>,
    /// Custom cursor color.
    pub cursor: Option<Color>,
    /// Custom selection foreground color.
    pub selection_fg: Option<Color>,
    /// Custom selection background color.
    pub selection_bg: Option<Color>,
    /// Custom ANSI palette colors (indices 0-15).
    pub palette: [Option<Color>; 16],
    /// Dimmed color palette (indices 0-7).
    pub dim_palette: [Option<Color>; 8],
    /// Background opacity (0.0 = fully transparent, 1.0 = fully opaque).
    pub opacity: f32,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            foreground: None,
            background: None,
            cursor: None,
            selection_fg: None,
            selection_bg: None,
            palette: [None; 16],
            dim_palette: [None; 8],
            opacity: 1.0,
        }
    }
}

/// Parse a hex color string (#RRGGBB or RRGGBB).
pub(super) fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
    let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
    let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
    Some(Color { r, g, b })
}

/// Parse a palette key name to a palette index (0..=15).
///
/// Accepts `colorN` (horseshoe), `regularN` (foot, 0-7), `brightN` (foot, 8-15),
/// and bare numeric indices `0`..`255` (foot extended palette).
pub(super) fn parse_palette_key(key: &str) -> Option<usize> {
    if let Some(idx_str) = key.strip_prefix("color") {
        return idx_str.parse::<usize>().ok().filter(|&i| i < 16);
    }
    if let Some(idx_str) = key.strip_prefix("regular") {
        return idx_str.parse::<usize>().ok().filter(|&i| i < 8);
    }
    if let Some(idx_str) = key.strip_prefix("bright") {
        return idx_str
            .parse::<usize>()
            .ok()
            .filter(|&i| i < 8)
            .map(|i| i + 8);
    }
    // foot also allows bare indices (16..255) but we only store 0-15
    key.parse::<usize>().ok().filter(|&i| i < 16)
}

/// Parse foot's dual cursor color: `cursor= TEXT_COLOR CURSOR_COLOR`.
///
/// Takes the last hex color (the actual cursor color). Also handles
/// a single color value.
pub(super) fn parse_foot_cursor_color(value: &str) -> Option<Color> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    // foot: `TEXT_COLOR CURSOR_COLOR` — take the last one
    // single value: just the cursor color
    let color_str = parts.last()?;
    parse_hex_color(color_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        let c_hash = parse_hex_color("#ff8040").expect("valid color");
        assert_eq!(c_hash.r, 0xff);
        assert_eq!(c_hash.g, 0x80);
        assert_eq!(c_hash.b, 0x40);

        let c_bare = parse_hex_color("00ff00").expect("valid color without #");
        assert_eq!(c_bare.r, 0);
        assert_eq!(c_bare.g, 255);
        assert_eq!(c_bare.b, 0);

        assert!(parse_hex_color("xyz").is_none());
        assert!(parse_hex_color("#fff").is_none());
    }

    #[test]
    fn test_parse_hex_color_lowercase() {
        let color = parse_hex_color("#aabbcc").expect("lowercase hex should parse");
        assert_eq!(color.r, 0xAA);
        assert_eq!(color.g, 0xBB);
        assert_eq!(color.b, 0xCC);
    }

    #[test]
    fn test_parse_hex_color_invalid_inputs() {
        assert!(parse_hex_color("").is_none(), "empty string");
        assert!(parse_hex_color("#").is_none(), "bare hash");
        assert!(
            parse_hex_color("#fff").is_none(),
            "3-digit shorthand not supported"
        );
        assert!(parse_hex_color("#GGHHII").is_none(), "non-hex characters");
        assert!(parse_hex_color("#1234567").is_none(), "7 digits after hash");
        assert!(parse_hex_color("zzzzzz").is_none(), "non-hex without hash");
    }

    #[test]
    fn test_parse_palette_key_variants() {
        assert_eq!(parse_palette_key("color0"), Some(0));
        assert_eq!(parse_palette_key("color15"), Some(15));
        assert_eq!(parse_palette_key("color16"), None);
        assert_eq!(parse_palette_key("regular0"), Some(0));
        assert_eq!(parse_palette_key("regular7"), Some(7));
        assert_eq!(parse_palette_key("regular8"), None);
        assert_eq!(parse_palette_key("bright0"), Some(8));
        assert_eq!(parse_palette_key("bright7"), Some(15));
        assert_eq!(parse_palette_key("bright8"), None);
        assert_eq!(parse_palette_key("0"), Some(0));
        assert_eq!(parse_palette_key("15"), Some(15));
        assert_eq!(parse_palette_key("unknown"), None);
    }

    #[test]
    fn test_parse_foot_cursor_color() {
        let c_dual = parse_foot_cursor_color("002b36 93a1a1").expect("dual");
        assert_eq!(c_dual.r, 0x93);
        let c_single = parse_foot_cursor_color("93a1a1").expect("single");
        assert_eq!(c_single.r, 0x93);
        assert!(parse_foot_cursor_color("").is_none());
    }
}
