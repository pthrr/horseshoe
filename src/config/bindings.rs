use xkeysym::key as xkb;

/// Actions that can be bound to key combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Copy,
    Paste,
    PrimaryPaste,
    ToggleFullscreen,
    Search,
    FontSizeUp,
    FontSizeDown,
    FontSizeReset,
    ScrollPageUp,
    ScrollPageDown,
    ScrollHalfPageUp,
    ScrollHalfPageDown,
    ScrollLineUp,
    ScrollLineDown,
    SpawnTerminal,
    ScrollbackEditor,
    Noop,
}

/// A key combination: XKB keysym + modifier flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingKey {
    pub keysym: u32,
    pub ctrl: bool,
    pub shift: bool,
}

/// Configurable key-to-action bindings (parsed from `[key-bindings]`).
#[derive(Debug, Clone)]
pub struct Bindings {
    entries: Vec<(BindingKey, KeyAction)>,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            entries: vec![
                (
                    BindingKey {
                        keysym: xkb::c,
                        ctrl: true,
                        shift: true,
                    },
                    KeyAction::Copy,
                ), // Ctrl+Shift+C
                (
                    BindingKey {
                        keysym: xkb::v,
                        ctrl: true,
                        shift: true,
                    },
                    KeyAction::Paste,
                ), // Ctrl+Shift+V
                (
                    BindingKey {
                        keysym: xkb::F11,
                        ctrl: false,
                        shift: false,
                    },
                    KeyAction::ToggleFullscreen,
                ), // F11
                (
                    BindingKey {
                        keysym: xkb::f,
                        ctrl: true,
                        shift: true,
                    },
                    KeyAction::Search,
                ), // Ctrl+Shift+F
                (
                    BindingKey {
                        keysym: xkb::equal,
                        ctrl: true,
                        shift: false,
                    },
                    KeyAction::FontSizeUp,
                ), // Ctrl+=
                (
                    BindingKey {
                        keysym: xkb::minus,
                        ctrl: true,
                        shift: false,
                    },
                    KeyAction::FontSizeDown,
                ), // Ctrl+-
                (
                    BindingKey {
                        keysym: xkb::_0,
                        ctrl: true,
                        shift: false,
                    },
                    KeyAction::FontSizeReset,
                ), // Ctrl+0
                (
                    BindingKey {
                        keysym: xkb::n,
                        ctrl: true,
                        shift: true,
                    },
                    KeyAction::SpawnTerminal,
                ), // Ctrl+Shift+N
                (
                    BindingKey {
                        keysym: xkb::e,
                        ctrl: true,
                        shift: true,
                    },
                    KeyAction::ScrollbackEditor,
                ), // Ctrl+Shift+E
                (
                    BindingKey {
                        keysym: xkb::Insert,
                        ctrl: false,
                        shift: true,
                    },
                    KeyAction::PrimaryPaste,
                ), // Shift+Insert
                (
                    BindingKey {
                        keysym: xkb::Page_Up,
                        ctrl: false,
                        shift: true,
                    },
                    KeyAction::ScrollPageUp,
                ), // Shift+Page_Up
                (
                    BindingKey {
                        keysym: xkb::Page_Down,
                        ctrl: false,
                        shift: true,
                    },
                    KeyAction::ScrollPageDown,
                ), // Shift+Page_Down
            ],
        }
    }
}

impl Bindings {
    /// Look up a keybinding. Normalizes ASCII uppercase keysyms to lowercase.
    pub fn lookup(&self, keysym: u32, ctrl: bool, shift: bool) -> Option<KeyAction> {
        // Normalize ASCII uppercase → lowercase (XKB sends uppercase when Shift held)
        let sym = if (xkb::A..=xkb::Z).contains(&keysym) {
            keysym + (xkb::a - xkb::A)
        } else {
            keysym
        };
        for &(ref bk, action) in &self.entries {
            if bk.keysym == sym && bk.ctrl == ctrl && bk.shift == shift {
                return if action == KeyAction::Noop {
                    None
                } else {
                    Some(action)
                };
            }
        }
        None
    }

    /// Set or replace a binding for the given action. Removes any prior
    /// binding for the same action AND any prior binding on the same key combo.
    pub(super) fn set(&mut self, action: KeyAction, bk: BindingKey) {
        self.entries.retain(|&(ref k, a)| a != action && *k != bk);
        self.entries.push((bk, action));
    }

    /// Unbind any binding that maps to the given action.
    pub(super) fn unbind_action(&mut self, action: KeyAction) {
        self.entries.retain(|&(_, a)| a != action);
    }
}

/// Parse foot's `[key-bindings]` action name into a `KeyAction`.
pub(super) fn parse_binding_action(name: &str) -> Option<KeyAction> {
    match name {
        "clipboard-copy" => Some(KeyAction::Copy),
        "clipboard-paste" => Some(KeyAction::Paste),
        "primary-paste" => Some(KeyAction::PrimaryPaste),
        "fullscreen" => Some(KeyAction::ToggleFullscreen),
        "search-start" => Some(KeyAction::Search),
        "font-increase" => Some(KeyAction::FontSizeUp),
        "font-decrease" => Some(KeyAction::FontSizeDown),
        "font-reset" => Some(KeyAction::FontSizeReset),
        "scrollback-up-page" => Some(KeyAction::ScrollPageUp),
        "scrollback-down-page" => Some(KeyAction::ScrollPageDown),
        "scrollback-up-half-page" => Some(KeyAction::ScrollHalfPageUp),
        "scrollback-down-half-page" => Some(KeyAction::ScrollHalfPageDown),
        "scrollback-up-line" => Some(KeyAction::ScrollLineUp),
        "scrollback-down-line" => Some(KeyAction::ScrollLineDown),
        "spawn-terminal" => Some(KeyAction::SpawnTerminal),
        "scrollback-editor" => Some(KeyAction::ScrollbackEditor),
        "noop" => Some(KeyAction::Noop),
        _ => None,
    }
}

/// Parse a foot-style key combo like `Control+Shift+c` into a `BindingKey`.
///
/// Supported modifier names: `Control`, `Shift` (case-insensitive).
/// The last `+`-separated token is the key name, mapped to an XKB keysym.
pub(super) fn parse_binding_combo(combo: &str) -> Option<BindingKey> {
    let mut ctrl = false;
    let mut shift = false;
    let parts: Vec<&str> = combo.split('+').collect();
    if parts.is_empty() {
        return None;
    }
    // All tokens except the last are modifiers; the last is the key name
    let (&key_token, mod_tokens) = parts.split_last()?;
    for &part in mod_tokens {
        match part.trim().to_lowercase().as_str() {
            "control" | "ctrl" => ctrl = true,
            "shift" => shift = true,
            _ => {} // Ignore unknown modifiers (Super, Alt, etc.)
        }
    }
    let key_name = key_token.trim();
    let keysym = binding_key_to_keysym(key_name)?;
    Some(BindingKey {
        keysym,
        ctrl,
        shift,
    })
}

/// Map a foot-style key name to an XKB keysym value.
pub(super) fn binding_key_to_keysym(name: &str) -> Option<u32> {
    // Single ASCII character
    if let &[ch] = name.as_bytes() {
        // Uppercase → lowercase keysym
        if ch.is_ascii_uppercase() {
            return Some(u32::from(ch.to_ascii_lowercase()));
        }
        // Lowercase letter, digit, or symbol
        if ch.is_ascii() {
            return Some(u32::from(ch));
        }
    }
    // Named keys
    match name.to_lowercase().as_str() {
        "f1" => Some(xkb::F1),
        "f2" => Some(xkb::F2),
        "f3" => Some(xkb::F3),
        "f4" => Some(xkb::F4),
        "f5" => Some(xkb::F5),
        "f6" => Some(xkb::F6),
        "f7" => Some(xkb::F7),
        "f8" => Some(xkb::F8),
        "f9" => Some(xkb::F9),
        "f10" => Some(xkb::F10),
        "f11" => Some(xkb::F11),
        "f12" => Some(xkb::F12),
        "insert" => Some(xkb::Insert),
        "delete" => Some(xkb::Delete),
        "home" => Some(xkb::Home),
        "end" => Some(xkb::End),
        "page_up" | "prior" => Some(xkb::Page_Up),
        "page_down" | "next" => Some(xkb::Page_Down),
        "up" => Some(xkb::Up),
        "down" => Some(xkb::Down),
        "left" => Some(xkb::Left),
        "right" => Some(xkb::Right),
        "return" | "enter" => Some(xkb::Return),
        "escape" => Some(xkb::Escape),
        "tab" => Some(xkb::Tab),
        "backspace" => Some(xkb::BackSpace),
        "space" => Some(xkb::space),
        "minus" => Some(xkb::minus),
        "plus" | "equal" => Some(xkb::equal),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_binding_combo_ctrl_shift_c() {
        let bk = parse_binding_combo("Control+Shift+c").expect("valid combo");
        assert_eq!(bk.keysym, 0x0063);
        assert!(bk.ctrl);
        assert!(bk.shift);
    }

    #[test]
    fn test_parse_binding_combo_f11() {
        let bk = parse_binding_combo("F11").expect("valid combo");
        assert_eq!(bk.keysym, 0xffc8);
        assert!(!bk.ctrl);
        assert!(!bk.shift);
    }

    #[test]
    fn test_parse_binding_combo_shift_insert() {
        let bk = parse_binding_combo("Shift+Insert").expect("valid combo");
        assert_eq!(bk.keysym, 0xff63);
        assert!(!bk.ctrl);
        assert!(bk.shift);
    }

    #[test]
    fn test_parse_binding_combo_ctrl_minus() {
        let bk = parse_binding_combo("Control+-").expect("valid combo");
        assert_eq!(bk.keysym, 0x002d);
        assert!(bk.ctrl);
        assert!(!bk.shift);
    }

    #[test]
    fn test_parse_binding_combo_unknown_key() {
        assert!(parse_binding_combo("Control+Shift+NONEXISTENT").is_none());
    }

    #[test]
    fn test_parse_binding_action_names() {
        assert_eq!(
            parse_binding_action("clipboard-copy"),
            Some(KeyAction::Copy)
        );
        assert_eq!(
            parse_binding_action("clipboard-paste"),
            Some(KeyAction::Paste)
        );
        assert_eq!(
            parse_binding_action("primary-paste"),
            Some(KeyAction::PrimaryPaste)
        );
        assert_eq!(
            parse_binding_action("fullscreen"),
            Some(KeyAction::ToggleFullscreen)
        );
        assert_eq!(
            parse_binding_action("search-start"),
            Some(KeyAction::Search)
        );
        assert_eq!(
            parse_binding_action("font-increase"),
            Some(KeyAction::FontSizeUp)
        );
        assert_eq!(
            parse_binding_action("font-decrease"),
            Some(KeyAction::FontSizeDown)
        );
        assert_eq!(
            parse_binding_action("font-reset"),
            Some(KeyAction::FontSizeReset)
        );
        assert_eq!(
            parse_binding_action("spawn-terminal"),
            Some(KeyAction::SpawnTerminal)
        );
        assert_eq!(parse_binding_action("noop"), Some(KeyAction::Noop));
        assert!(parse_binding_action("unknown-action").is_none());
    }

    #[test]
    fn test_bindings_noop_returns_none() {
        let mut b = Bindings::default();
        b.set(
            KeyAction::Noop,
            BindingKey {
                keysym: 0x0063,
                ctrl: true,
                shift: true,
            },
        );
        assert!(b.lookup(0x0063, true, true).is_none());
    }

    #[test]
    fn test_bindings_set_replaces_existing() {
        let mut b = Bindings::default();
        assert_eq!(b.lookup(0x0063, true, true), Some(KeyAction::Copy));
        b.set(
            KeyAction::Copy,
            BindingKey {
                keysym: 0x0079,
                ctrl: true,
                shift: true,
            },
        );
        assert!(b.lookup(0x0063, true, true).is_none());
        assert_eq!(b.lookup(0x0079, true, true), Some(KeyAction::Copy));
    }

    #[test]
    fn test_bindings_unbind_action() {
        let mut b = Bindings::default();
        assert_eq!(b.lookup(0x0063, true, true), Some(KeyAction::Copy));
        b.unbind_action(KeyAction::Copy);
        assert!(b.lookup(0x0063, true, true).is_none());
    }

    #[test]
    fn test_binding_key_to_keysym_single_chars() {
        assert_eq!(binding_key_to_keysym("a"), Some(0x0061));
        assert_eq!(binding_key_to_keysym("z"), Some(0x007a));
        assert_eq!(binding_key_to_keysym("A"), Some(0x0061));
        assert_eq!(binding_key_to_keysym("0"), Some(0x0030));
        assert_eq!(binding_key_to_keysym("-"), Some(0x002d));
        assert_eq!(binding_key_to_keysym("="), Some(0x003d));
    }

    #[test]
    fn test_binding_key_to_keysym_named_keys() {
        assert_eq!(binding_key_to_keysym("F1"), Some(0xffbe));
        assert_eq!(binding_key_to_keysym("F12"), Some(0xffc9));
        assert_eq!(binding_key_to_keysym("Insert"), Some(0xff63));
        assert_eq!(binding_key_to_keysym("Delete"), Some(0xffff));
        assert_eq!(binding_key_to_keysym("Home"), Some(0xff50));
        assert_eq!(binding_key_to_keysym("End"), Some(0xff57));
        assert_eq!(binding_key_to_keysym("Page_Up"), Some(0xff55));
        assert_eq!(binding_key_to_keysym("Page_Down"), Some(0xff56));
        assert_eq!(binding_key_to_keysym("Return"), Some(0xff0d));
        assert_eq!(binding_key_to_keysym("Escape"), Some(0xff1b));
        assert_eq!(binding_key_to_keysym("Space"), Some(0x0020));
    }

    #[test]
    fn test_bindings_lookup_uppercase_normalization() {
        let b = Bindings::default();
        assert_eq!(b.lookup(0x0043, true, true), Some(KeyAction::Copy));
        assert_eq!(b.lookup(0x0056, true, true), Some(KeyAction::Paste));
        assert_eq!(b.lookup(0x0046, true, true), Some(KeyAction::Search));
        assert_eq!(b.lookup(0x004E, true, true), Some(KeyAction::SpawnTerminal));
        assert!(b.lookup(0x0041, true, true).is_none());
        assert!(b.lookup(0x005A, true, true).is_none());
    }

    #[test]
    fn test_parse_binding_combo_ctrl_shorthand() {
        let bk = parse_binding_combo("Ctrl+Shift+c").expect("ctrl shorthand");
        assert!(bk.ctrl);
        assert!(bk.shift);
        assert_eq!(bk.keysym, 0x0063);
    }

    #[test]
    fn test_parse_binding_combo_unknown_modifier_ignored() {
        let bk = parse_binding_combo("Super+Alt+Control+c").expect("unknown mod ignored");
        assert!(bk.ctrl);
        assert!(!bk.shift);
        assert_eq!(bk.keysym, 0x0063);
    }
}
