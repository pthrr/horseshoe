/// Font configuration.
pub struct FontConfig {
    /// Font family name. When set, the font manager tries to load this
    /// family from system fonts at runtime; the embedded font is
    /// used as a fallback when the system font cannot be found.
    pub family: Option<String>,
    /// Bold font family override.
    pub bold: Option<String>,
    /// Italic font family override.
    pub italic: Option<String>,
    /// Bold+italic font family override.
    pub bold_italic: Option<String>,
    /// Font size in pixels (converted from points when parsed from foot.ini).
    pub size: f32,
    /// Additional line height in pixels.
    pub line_height: f32,
    /// Additional letter spacing in pixels.
    pub letter_spacing: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: None,
            bold: None,
            italic: None,
            bold_italic: None,
            size: 16.0,
            line_height: 0.0,
            letter_spacing: 0.0,
        }
    }
}
