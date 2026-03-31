use std::path::PathBuf;

/// Terminal behavior configuration.
pub struct TerminalConfig {
    pub scrollback: usize,
    pub shell: Option<String>,
    /// TERM environment variable to set in the child shell.
    /// If None, inherits from parent or falls back to xterm-256color.
    pub term: Option<String>,
    /// Working directory for the child shell.
    pub working_directory: Option<PathBuf>,
    pub cursor_blink_interval_ms: u64,
    /// Desktop notification command (run on BEL).
    pub notify_command: Option<String>,
    pub cursor_blink: bool,
    pub login_shell: bool,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback: 10000,
            shell: None,
            term: None,
            working_directory: None,
            cursor_blink_interval_ms: 500,
            notify_command: None,
            cursor_blink: true,
            login_shell: false,
        }
    }
}
