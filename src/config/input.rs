use super::types::SelectionTarget;

/// Input and interaction configuration.
pub struct InputConfig {
    /// Custom word delimiter characters for double-click word selection.
    pub word_delimiters: Option<String>,
    /// Mouse scroll speed multiplier.
    pub scroll_multiplier: f32,
    /// Selection target: "none", "primary", "clipboard", "both".
    pub selection_target: SelectionTarget,
    pub bold_is_bright: bool,
    pub hide_when_typing: bool,
    pub alternate_scroll_mode: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            word_delimiters: None,
            scroll_multiplier: 3.0,
            selection_target: SelectionTarget::default(),
            bold_is_bright: false,
            hide_when_typing: false,
            alternate_scroll_mode: true,
        }
    }
}
