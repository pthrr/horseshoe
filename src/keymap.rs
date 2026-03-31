use libghostty_vt::key;
use xkeysym::key as xkb;

/// Map an XKB keysym to a `key::Key` enum variant.
pub fn xkb_to_ghostty_key(keysym: u32) -> key::Key {
    match keysym {
        // Lowercase letters a-z
        xkb::a..=xkb::z => {
            let offset = keysym - xkb::a;
            key::Key::try_from(key::Key::A as u32 + offset).unwrap_or(key::Key::Unidentified)
        }
        // Uppercase letters A-Z (map to same keys)
        xkb::A..=xkb::Z => {
            let offset = keysym - xkb::A;
            key::Key::try_from(key::Key::A as u32 + offset).unwrap_or(key::Key::Unidentified)
        }
        // Digits 0-9
        xkb::_0..=xkb::_9 => {
            let offset = keysym - xkb::_0;
            key::Key::try_from(key::Key::Digit0 as u32 + offset).unwrap_or(key::Key::Unidentified)
        }

        // Special keys
        xkb::space => key::Key::Space,
        xkb::Return => key::Key::Enter,
        xkb::Escape => key::Key::Escape,
        xkb::BackSpace => key::Key::Backspace,
        xkb::Tab => key::Key::Tab,

        // Navigation
        xkb::Left => key::Key::ArrowLeft,
        xkb::Right => key::Key::ArrowRight,
        xkb::Up => key::Key::ArrowUp,
        xkb::Down => key::Key::ArrowDown,
        xkb::Home => key::Key::Home,
        xkb::End => key::Key::End,
        xkb::Page_Up => key::Key::PageUp,
        xkb::Page_Down => key::Key::PageDown,
        xkb::Insert => key::Key::Insert,
        xkb::Delete => key::Key::Delete,

        // Function keys
        xkb::F1 => key::Key::F1,
        xkb::F2 => key::Key::F2,
        xkb::F3 => key::Key::F3,
        xkb::F4 => key::Key::F4,
        xkb::F5 => key::Key::F5,
        xkb::F6 => key::Key::F6,
        xkb::F7 => key::Key::F7,
        xkb::F8 => key::Key::F8,
        xkb::F9 => key::Key::F9,
        xkb::F10 => key::Key::F10,
        xkb::F11 => key::Key::F11,
        xkb::F12 => key::Key::F12,

        // Modifier keys
        xkb::Shift_L => key::Key::ShiftLeft,
        xkb::Shift_R => key::Key::ShiftRight,
        xkb::Control_L => key::Key::ControlLeft,
        xkb::Control_R => key::Key::ControlRight,
        xkb::Alt_L => key::Key::AltLeft,
        xkb::Alt_R => key::Key::AltRight,
        xkb::Super_L => key::Key::MetaLeft,
        xkb::Super_R => key::Key::MetaRight,
        xkb::Caps_Lock => key::Key::CapsLock,
        xkb::Num_Lock => key::Key::NumLock,

        // Symbols
        xkb::semicolon => key::Key::Semicolon,
        xkb::apostrophe => key::Key::Quote,
        xkb::grave => key::Key::Backquote,
        xkb::comma => key::Key::Comma,
        xkb::period => key::Key::Period,
        xkb::slash => key::Key::Slash,
        xkb::backslash => key::Key::Backslash,
        xkb::minus => key::Key::Minus,
        xkb::equal => key::Key::Equal,
        xkb::bracketleft => key::Key::BracketLeft,
        xkb::bracketright => key::Key::BracketRight,

        // Numpad digits
        xkb::KP_0..=xkb::KP_9 => {
            let offset = keysym - xkb::KP_0;
            key::Key::try_from(key::Key::Numpad0 as u32 + offset).unwrap_or(key::Key::Unidentified)
        }

        // Numpad operators
        xkb::KP_Enter => key::Key::NumpadEnter,
        xkb::KP_Add => key::Key::NumpadAdd,
        xkb::KP_Subtract => key::Key::NumpadSubtract,
        xkb::KP_Multiply => key::Key::NumpadMultiply,
        xkb::KP_Divide => key::Key::NumpadDivide,
        xkb::KP_Decimal => key::Key::NumpadDecimal,
        xkb::KP_Separator => key::Key::NumpadSeparator,
        xkb::KP_Equal => key::Key::NumpadEqual,

        // Numpad navigation
        xkb::KP_Home => key::Key::NumpadHome,
        xkb::KP_End => key::Key::NumpadEnd,
        xkb::KP_Up => key::Key::NumpadUp,
        xkb::KP_Down => key::Key::NumpadDown,
        xkb::KP_Left => key::Key::NumpadLeft,
        xkb::KP_Right => key::Key::NumpadRight,
        xkb::KP_Page_Up => key::Key::NumpadPageUp,
        xkb::KP_Page_Down => key::Key::NumpadPageDown,
        xkb::KP_Begin => key::Key::NumpadBegin,
        xkb::KP_Insert => key::Key::NumpadInsert,
        xkb::KP_Delete => key::Key::NumpadDelete,

        _ => key::Key::Unidentified,
    }
}

/// Keyboard modifier state packed as a bitmask.
///
/// Wraps `key::Mods` with a builder-style API for constructing modifier
/// state from individual boolean flags.
#[derive(Clone, Copy)]
pub struct ModifierState(key::Mods);

impl Default for ModifierState {
    fn default() -> Self {
        Self(key::Mods::empty())
    }
}

impl ModifierState {
    /// Start with an empty state and chain `with_*` calls.
    pub const fn empty() -> Self {
        Self(key::Mods::empty())
    }

    #[must_use]
    pub const fn with_shift(self, on: bool) -> Self {
        if on {
            Self(self.0.union(key::Mods::SHIFT))
        } else {
            self
        }
    }
    #[must_use]
    pub const fn with_ctrl(self, on: bool) -> Self {
        if on {
            Self(self.0.union(key::Mods::CTRL))
        } else {
            self
        }
    }
    #[must_use]
    pub const fn with_alt(self, on: bool) -> Self {
        if on {
            Self(self.0.union(key::Mods::ALT))
        } else {
            self
        }
    }
    #[must_use]
    pub const fn with_logo(self, on: bool) -> Self {
        if on {
            Self(self.0.union(key::Mods::SUPER))
        } else {
            self
        }
    }
    #[must_use]
    pub const fn with_caps_lock(self, on: bool) -> Self {
        if on {
            Self(self.0.union(key::Mods::CAPS_LOCK))
        } else {
            self
        }
    }
    #[must_use]
    pub const fn with_num_lock(self, on: bool) -> Self {
        if on {
            Self(self.0.union(key::Mods::NUM_LOCK))
        } else {
            self
        }
    }

    /// Return the `key::Mods` bitmask.
    pub const fn to_mods(self) -> key::Mods {
        self.0
    }
}

/// Return the Unicode codepoint for the unshifted version of a keysym.
/// For printable keysyms in the Latin-1 range (0x0020..=0x007e), the keysym
/// value is the same as the Unicode codepoint. For letter keysyms, this
/// returns the lowercase codepoint regardless of whether the keysym is
/// uppercase or lowercase.
pub const fn unshifted_codepoint(keysym: u32) -> u32 {
    match keysym {
        // Uppercase -> lowercase
        xkb::A..=xkb::Z => keysym - xkb::A + xkb::a,
        // Lowercase letters are already their own codepoint
        xkb::a..=xkb::z | xkb::_0..=xkb::_9 => keysym,
        // Space and printable ASCII symbols
        xkb::space => 0x0020,
        xkb::minus => 0x002d,
        xkb::equal => 0x003d,
        xkb::bracketleft => 0x005b,
        xkb::bracketright => 0x005d,
        xkb::backslash => 0x005c,
        xkb::semicolon => 0x003b,
        xkb::apostrophe => 0x0027,
        xkb::grave => 0x0060,
        xkb::comma => 0x002c,
        xkb::period => 0x002e,
        xkb::slash => 0x002f,
        // Non-printable / special keys have no unshifted codepoint
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xkb_to_ghostty_key_mapping() {
        let cases: &[(u32, key::Key)] = &[
            // Letters
            (0x0061, key::Key::A), // lowercase a
            (0x0041, key::Key::A), // uppercase A
            (0x007a, key::Key::Z), // lowercase z
            // Digits
            (0x0030, key::Key::Digit0),
            (0x0039, key::Key::Digit9),
            // Special keys
            (0xff0d, key::Key::Enter),
            (0xff1b, key::Key::Escape),
            (0xff08, key::Key::Backspace),
            (0xff09, key::Key::Tab),
            (0x0020, key::Key::Space),
            // Arrow keys
            (0xff51, key::Key::ArrowLeft),
            (0xff52, key::Key::ArrowUp),
            (0xff53, key::Key::ArrowRight),
            (0xff54, key::Key::ArrowDown),
            // Navigation
            (0xff50, key::Key::Home),
            (0xff57, key::Key::End),
            (0xff55, key::Key::PageUp),
            (0xff56, key::Key::PageDown),
            (0xff63, key::Key::Insert),
            (0xffff, key::Key::Delete),
            // Function keys
            (0xffbe, key::Key::F1),
            (0xffc9, key::Key::F12),
            // Modifier keys
            (0xffe1, key::Key::ShiftLeft),
            (0xffe3, key::Key::ControlLeft),
            (0xffe9, key::Key::AltLeft),
            (0xffeb, key::Key::MetaLeft),
            // Symbols
            (0x003b, key::Key::Semicolon),
            (0x002f, key::Key::Slash),
            (0x005b, key::Key::BracketLeft),
            // Numpad digits
            (xkb::KP_0, key::Key::Numpad0),
            (xkb::KP_9, key::Key::Numpad9),
            (0xffb5, key::Key::Numpad5),
            // Numpad operators
            (xkb::KP_Enter, key::Key::NumpadEnter),
            (xkb::KP_Add, key::Key::NumpadAdd),
            (xkb::KP_Subtract, key::Key::NumpadSubtract),
            (xkb::KP_Multiply, key::Key::NumpadMultiply),
            (xkb::KP_Divide, key::Key::NumpadDivide),
            (xkb::KP_Decimal, key::Key::NumpadDecimal),
            (xkb::KP_Equal, key::Key::NumpadEqual),
            // Numpad navigation
            (xkb::KP_Home, key::Key::NumpadHome),
            (xkb::KP_End, key::Key::NumpadEnd),
            (xkb::KP_Up, key::Key::NumpadUp),
            (xkb::KP_Down, key::Key::NumpadDown),
            (xkb::KP_Left, key::Key::NumpadLeft),
            (xkb::KP_Right, key::Key::NumpadRight),
            (xkb::KP_Page_Up, key::Key::NumpadPageUp),
            (xkb::KP_Page_Down, key::Key::NumpadPageDown),
            (xkb::KP_Insert, key::Key::NumpadInsert),
            (xkb::KP_Delete, key::Key::NumpadDelete),
            // Unknown
            (0xdead, key::Key::Unidentified),
        ];
        for &(keysym, expected) in cases {
            assert_eq!(
                xkb_to_ghostty_key(keysym),
                expected,
                "keysym 0x{keysym:04x} should map correctly"
            );
        }
    }

    #[test]
    fn test_modifier_state() {
        // Default/empty is zero
        assert_eq!(ModifierState::default().to_mods(), key::Mods::empty());
        assert_eq!(ModifierState::empty().to_mods(), key::Mods::empty());

        // Shift+Ctrl
        let sc = ModifierState::empty()
            .with_shift(true)
            .with_ctrl(true)
            .to_mods();
        assert!(sc.contains(key::Mods::SHIFT));
        assert!(sc.contains(key::Mods::CTRL));
        assert!(!sc.contains(key::Mods::ALT));
        assert!(!sc.contains(key::Mods::SUPER));

        // Alt only
        let alt = ModifierState::empty().with_alt(true).to_mods();
        assert!(alt.contains(key::Mods::ALT));
        assert!(!alt.contains(key::Mods::SHIFT));

        // Super only
        let sup = ModifierState::empty().with_logo(true).to_mods();
        assert!(sup.contains(key::Mods::SUPER));
    }

    #[test]
    fn test_unshifted_codepoint_mapping() {
        // Letters
        assert_eq!(unshifted_codepoint(0x0061), 0x0061); // 'a'
        assert_eq!(unshifted_codepoint(0x0041), 0x0061); // 'A' -> 'a'
        // Digits
        assert_eq!(unshifted_codepoint(0x0035), 0x0035); // '5'
        // Space
        assert_eq!(unshifted_codepoint(0x0020), 0x0020);
        // Symbols
        assert_eq!(unshifted_codepoint(xkb::semicolon), 0x003b);
        assert_eq!(unshifted_codepoint(xkb::slash), 0x002f);
        assert_eq!(unshifted_codepoint(xkb::minus), 0x002d);
        assert_eq!(unshifted_codepoint(xkb::equal), 0x003d);
        assert_eq!(unshifted_codepoint(xkb::bracketleft), 0x005b);
        assert_eq!(unshifted_codepoint(xkb::bracketright), 0x005d);
        assert_eq!(unshifted_codepoint(xkb::backslash), 0x005c);
        assert_eq!(unshifted_codepoint(xkb::apostrophe), 0x0027);
        assert_eq!(unshifted_codepoint(xkb::grave), 0x0060);
        assert_eq!(unshifted_codepoint(xkb::comma), 0x002c);
        assert_eq!(unshifted_codepoint(xkb::period), 0x002e);
        // Non-printable keys return 0
        assert_eq!(unshifted_codepoint(xkb::Return), 0);
        assert_eq!(unshifted_codepoint(xkb::Escape), 0);
        assert_eq!(unshifted_codepoint(xkb::F1), 0);
        assert_eq!(unshifted_codepoint(xkb::Left), 0);
        assert_eq!(unshifted_codepoint(xkb::Shift_L), 0);
        // Shifted symbols return 0
        assert_eq!(unshifted_codepoint(0x0040), 0); // @
        assert_eq!(unshifted_codepoint(0x0021), 0); // !
        assert_eq!(unshifted_codepoint(0x007e), 0); // ~
    }

    /// Shifted ASCII symbols return UNIDENTIFIED because they don't have
    /// dedicated `Key` variants. The keyboard handler MUST still pass
    /// these to the encoder with the UTF-8 text so they reach the PTY.
    #[test]
    fn test_shifted_symbols_are_unidentified() {
        let shifted_keysyms: &[u32] = &[
            0x0040, // @
            0x0021, // !
            0x0023, // #
            0x0024, // $
            0x0025, // %
            0x005e, // ^
            0x0026, // &
            0x002a, // *
            0x0028, // (
            0x0029, // )
            0x005f, // _
            0x002b, // +
            0x007b, // {
            0x007d, // }
            0x007c, // |
            0x003a, // :
            0x003c, // <
            0x003e, // >
            0x003f, // ?
            0x007e, // ~
            0x0022, // "
        ];
        for &ks in shifted_keysyms {
            assert_eq!(
                xkb_to_ghostty_key(ks),
                key::Key::Unidentified,
                "keysym 0x{ks:04x} ('{}') should be UNIDENTIFIED",
                char::from_u32(ks).unwrap_or('?')
            );
        }
    }

    #[test]
    fn test_modifier_state_all_flags() {
        let all = ModifierState::empty()
            .with_shift(true)
            .with_ctrl(true)
            .with_alt(true)
            .with_logo(true)
            .with_caps_lock(true)
            .with_num_lock(true)
            .to_mods();
        assert!(all.contains(key::Mods::SHIFT));
        assert!(all.contains(key::Mods::CTRL));
        assert!(all.contains(key::Mods::ALT));
        assert!(all.contains(key::Mods::SUPER));
        assert!(all.contains(key::Mods::CAPS_LOCK));
        assert!(all.contains(key::Mods::NUM_LOCK));
    }

    #[test]
    fn test_modifier_state_builder_idempotent() {
        // Applying with_shift(true) twice should produce same result
        let once = ModifierState::empty().with_shift(true).to_mods();
        let twice = ModifierState::empty()
            .with_shift(true)
            .with_shift(true)
            .to_mods();
        assert_eq!(once, twice);
    }

    #[test]
    fn test_modifier_state_caps_lock_only() {
        let caps = ModifierState::empty().with_caps_lock(true).to_mods();
        assert!(caps.contains(key::Mods::CAPS_LOCK));
        assert!(!caps.contains(key::Mods::SHIFT));
        assert!(!caps.contains(key::Mods::CTRL));
        assert!(!caps.contains(key::Mods::ALT));
    }

    #[test]
    fn test_modifier_state_num_lock_only() {
        let num = ModifierState::empty().with_num_lock(true).to_mods();
        assert!(num.contains(key::Mods::NUM_LOCK));
        assert!(!num.contains(key::Mods::SHIFT));
        assert!(!num.contains(key::Mods::CAPS_LOCK));
    }

    #[test]
    fn test_f1_through_f12_exhaustive() {
        let f_keysyms: &[(u32, key::Key)] = &[
            (xkb::F1, key::Key::F1),
            (xkb::F2, key::Key::F2),
            (xkb::F3, key::Key::F3),
            (xkb::F4, key::Key::F4),
            (xkb::F5, key::Key::F5),
            (xkb::F6, key::Key::F6),
            (xkb::F7, key::Key::F7),
            (xkb::F8, key::Key::F8),
            (xkb::F9, key::Key::F9),
            (xkb::F10, key::Key::F10),
            (xkb::F11, key::Key::F11),
            (xkb::F12, key::Key::F12),
        ];
        for &(keysym, expected) in f_keysyms {
            assert_eq!(
                xkb_to_ghostty_key(keysym),
                expected,
                "F-key keysym {keysym:#x} should map correctly"
            );
        }
        // Verify all keys are distinct
        let keys: Vec<_> = f_keysyms.iter().map(|&(_, k)| k).collect();
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                let ki = keys.get(i).expect("index i in bounds");
                let kj = keys.get(j).expect("index j in bounds");
                assert_ne!(ki, kj, "F-keys must map to distinct values");
            }
        }
    }

    #[test]
    fn test_numpad_digits_exhaustive() {
        for digit in 0..=9u32 {
            let keysym = xkb::KP_0 + digit;
            let expected =
                key::Key::try_from(key::Key::Numpad0 as u32 + digit).expect("valid numpad key");
            assert_eq!(
                xkb_to_ghostty_key(keysym),
                expected,
                "numpad digit {digit} should map correctly"
            );
        }
    }

    #[test]
    fn test_numpad_operators_exhaustive() {
        let ops: &[(u32, key::Key)] = &[
            (xkb::KP_Enter, key::Key::NumpadEnter),
            (xkb::KP_Add, key::Key::NumpadAdd),
            (xkb::KP_Subtract, key::Key::NumpadSubtract),
            (xkb::KP_Multiply, key::Key::NumpadMultiply),
            (xkb::KP_Divide, key::Key::NumpadDivide),
            (xkb::KP_Decimal, key::Key::NumpadDecimal),
            (xkb::KP_Separator, key::Key::NumpadSeparator),
            (xkb::KP_Equal, key::Key::NumpadEqual),
        ];
        for &(keysym, expected) in ops {
            assert_eq!(
                xkb_to_ghostty_key(keysym),
                expected,
                "numpad operator {keysym:#x} should map correctly"
            );
        }
    }

    #[test]
    fn test_numpad_navigation_exhaustive() {
        let nav: &[(u32, key::Key)] = &[
            (xkb::KP_Home, key::Key::NumpadHome),
            (xkb::KP_End, key::Key::NumpadEnd),
            (xkb::KP_Up, key::Key::NumpadUp),
            (xkb::KP_Down, key::Key::NumpadDown),
            (xkb::KP_Left, key::Key::NumpadLeft),
            (xkb::KP_Right, key::Key::NumpadRight),
            (xkb::KP_Page_Up, key::Key::NumpadPageUp),
            (xkb::KP_Page_Down, key::Key::NumpadPageDown),
            (xkb::KP_Begin, key::Key::NumpadBegin),
            (xkb::KP_Insert, key::Key::NumpadInsert),
            (xkb::KP_Delete, key::Key::NumpadDelete),
        ];
        for &(keysym, expected) in nav {
            assert_eq!(
                xkb_to_ghostty_key(keysym),
                expected,
                "numpad nav {keysym:#x} should map correctly"
            );
        }
    }

    #[test]
    fn test_lowercase_letters_exhaustive() {
        for ch in b'a'..=b'z' {
            let keysym = u32::from(ch);
            let expected = key::Key::try_from(key::Key::A as u32 + u32::from(ch - b'a'))
                .expect("valid letter key");
            assert_eq!(
                xkb_to_ghostty_key(keysym),
                expected,
                "letter '{}' should map correctly",
                ch as char
            );
        }
    }

    #[test]
    fn test_uppercase_letters_map_same_as_lowercase() {
        for ch in b'A'..=b'Z' {
            let keysym = u32::from(ch);
            let lower_keysym = u32::from(ch - b'A' + b'a');
            assert_eq!(
                xkb_to_ghostty_key(keysym),
                xkb_to_ghostty_key(lower_keysym),
                "uppercase '{}' should map same as lowercase",
                ch as char
            );
        }
    }

    #[test]
    fn test_modifier_state_all_64_combinations() {
        // 6 boolean flags → 2^6 = 64 combinations
        for bits in 0u8..64 {
            let mods = ModifierState::empty()
                .with_shift(bits & 1 != 0)
                .with_ctrl(bits & 2 != 0)
                .with_alt(bits & 4 != 0)
                .with_logo(bits & 8 != 0)
                .with_caps_lock(bits & 16 != 0)
                .with_num_lock(bits & 32 != 0)
                .to_mods();
            // Each bit should be reflected correctly
            assert_eq!(
                mods.contains(key::Mods::SHIFT),
                bits & 1 != 0,
                "shift for bits={bits}"
            );
            assert_eq!(
                mods.contains(key::Mods::CTRL),
                bits & 2 != 0,
                "ctrl for bits={bits}"
            );
            assert_eq!(
                mods.contains(key::Mods::ALT),
                bits & 4 != 0,
                "alt for bits={bits}"
            );
            assert_eq!(
                mods.contains(key::Mods::SUPER),
                bits & 8 != 0,
                "logo for bits={bits}"
            );
            assert_eq!(
                mods.contains(key::Mods::CAPS_LOCK),
                bits & 16 != 0,
                "caps for bits={bits}"
            );
            assert_eq!(
                mods.contains(key::Mods::NUM_LOCK),
                bits & 32 != 0,
                "num for bits={bits}"
            );
        }
    }

    #[test]
    fn test_modifier_chain_alternating() {
        // with_shift(false) is a no-op (does not clear the flag), so
        // empty().with_shift(true).with_shift(false) still has SHIFT set.
        let mods = ModifierState::empty()
            .with_shift(true)
            .with_shift(false)
            .to_mods();
        assert!(
            mods.contains(key::Mods::SHIFT),
            "with_shift(false) is a no-op, shift should remain set"
        );
    }

    #[test]
    fn test_modifier_false_on_default() {
        // Applying with_ctrl(false) to empty should still be empty
        let mods = ModifierState::empty().with_ctrl(false).to_mods();
        assert_eq!(mods, key::Mods::empty());
    }

    #[test]
    fn test_unshifted_codepoint_del() {
        // DEL (0x7F) is a control character, not in any match arm
        assert_eq!(unshifted_codepoint(0x007f), 0);
    }

    #[test]
    fn test_unshifted_codepoint_tilde() {
        // '~' (0x7E) is a shifted symbol, not explicitly matched
        assert_eq!(unshifted_codepoint(0x007e), 0);
    }

    #[test]
    fn test_unshifted_codepoint_non_latin() {
        // 0x1234 is outside Latin-1 range, returns 0
        assert_eq!(unshifted_codepoint(0x1234), 0);
        // Also test a higher codepoint
        assert_eq!(unshifted_codepoint(0x10000), 0);
    }

    #[test]
    fn test_xkb_to_ghostty_key_digit_5() {
        // Digit 5 keysym is 0x0035
        assert_eq!(xkb_to_ghostty_key(0x0035), key::Key::Digit5);
    }

    #[test]
    fn test_modifier_all_flags_set_then_unset() {
        // Set all 6 flags true
        let all_set = ModifierState::empty()
            .with_shift(true)
            .with_ctrl(true)
            .with_alt(true)
            .with_logo(true)
            .with_caps_lock(true)
            .with_num_lock(true);
        // Now "unset" all -- with_*(false) is a no-op, so flags remain
        let after_unset = all_set
            .with_shift(false)
            .with_ctrl(false)
            .with_alt(false)
            .with_logo(false)
            .with_caps_lock(false)
            .with_num_lock(false)
            .to_mods();
        // All flags should still be set because with_*(false) does not clear
        assert!(after_unset.contains(key::Mods::SHIFT));
        assert!(after_unset.contains(key::Mods::CTRL));
        assert!(after_unset.contains(key::Mods::ALT));
        assert!(after_unset.contains(key::Mods::SUPER));
        assert!(after_unset.contains(key::Mods::CAPS_LOCK));
        assert!(after_unset.contains(key::Mods::NUM_LOCK));
    }

    #[test]
    fn test_xkb_shift_right() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::Shift_R),
            key::Key::ShiftRight,
            "ShiftRight keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_control_right() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::Control_R),
            key::Key::ControlRight,
            "ControlRight keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_alt_right() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::Alt_R),
            key::Key::AltRight,
            "AltRight keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_super_right() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::Super_R),
            key::Key::MetaRight,
            "SuperRight keysym should map to MetaRight"
        );
    }

    #[test]
    fn test_xkb_caps_lock() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::Caps_Lock),
            key::Key::CapsLock,
            "CapsLock keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_num_lock() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::Num_Lock),
            key::Key::NumLock,
            "NumLock keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_apostrophe_to_quote() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::apostrophe),
            key::Key::Quote,
            "Apostrophe keysym should map to Quote"
        );
    }

    #[test]
    fn test_xkb_grave_to_backquote() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::grave),
            key::Key::Backquote,
            "Grave keysym should map to Backquote"
        );
    }

    #[test]
    fn test_xkb_comma() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::comma),
            key::Key::Comma,
            "Comma keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_period() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::period),
            key::Key::Period,
            "Period keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_backslash() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::backslash),
            key::Key::Backslash,
            "Backslash keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_minus() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::minus),
            key::Key::Minus,
            "Minus keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_equal() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::equal),
            key::Key::Equal,
            "Equal keysym should map correctly"
        );
    }

    #[test]
    fn test_xkb_bracket_right() {
        assert_eq!(
            xkb_to_ghostty_key(xkb::bracketright),
            key::Key::BracketRight,
            "BracketRight keysym should map correctly"
        );
    }
}
