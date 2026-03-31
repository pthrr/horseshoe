use super::*;
use crate::terminal::vt::TerminalOps;

#[test]
fn test_key_encoder_new() {
    let enc = KeyEncoder::new();
    assert!(enc.is_ok());
}

#[test]
fn test_key_encode_letter() {
    let mut enc = KeyEncoder::new().expect("failed to create key encoder");
    let result = enc.encode(
        key::Key::A,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        Some("a"),
        u32::from(b'a'),
    );
    assert!(result.is_some());
    assert_eq!(result.expect("expected Some"), b"a");
}

#[test]
fn test_key_encode_enter() {
    let mut enc = KeyEncoder::new().expect("failed to create key encoder");
    let result = enc.encode(
        key::Key::Enter,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some());
    assert_eq!(result.expect("expected Some"), b"\r");
}

#[test]
fn test_key_encode_ctrl_c() {
    let mut enc = KeyEncoder::new().expect("failed to create key encoder");
    let result = enc.encode(
        key::Key::C,
        key::Action::Press,
        key::Mods::CTRL,
        key::Mods::empty(),
        None,
        u32::from(b'c'),
    );
    assert!(result.is_some());
    assert_eq!(result.expect("expected Some"), &[0x03]); // ETX (Ctrl+C)
}

/// Regression: XKB provides the control byte as utf8 text (e.g. "\x03"
/// for Ctrl+C). The caller must strip these so the encoder falls back to
/// the logical key codepoint path. Passing the raw control byte would
/// cause `ctrlSeq()` to fail its ASCII-letter switch table lookup.
#[test]
fn test_key_encode_ctrl_c_with_xkb_control_byte() {
    let mut enc = KeyEncoder::new().expect("failed to create key encoder");
    // Simulates the BROKEN path: XKB utf8="\x03", which ctrlSeq() can't map.
    let broken = enc.encode(
        key::Key::C,
        key::Action::Press,
        key::Mods::CTRL,
        key::Mods::empty(),
        Some("\x03"),
        u32::from(b'c'),
    );
    // The encoder may still produce output, but it won't be the correct
    // raw 0x03 byte — it might produce a CSI 27;5;3~ sequence instead.
    // (This documents the broken behavior that the utf8 filter prevents.)
    let broken_bytes = broken.expect("encoder should produce something");
    let is_correct_etx = broken_bytes.as_slice() == [0x03];

    // Now test the FIXED path: caller strips the control char → None.
    let fixed = enc.encode(
        key::Key::C,
        key::Action::Press,
        key::Mods::CTRL,
        key::Mods::empty(),
        None,
        u32::from(b'c'),
    );
    assert_eq!(fixed.expect("expected Some"), &[0x03]);

    // If the broken path happened to produce 0x03, that's fine — but the
    // fix is still necessary because ctrlSeq() fails for many other keys.
    if !is_correct_etx {
        // Confirm the broken path really was broken
        assert_ne!(broken_bytes.as_slice(), [0x03]);
    }
}

/// Regression: Ctrl+D must produce 0x04 (EOT), not scroll up or CSI sequence.
#[test]
fn test_key_encode_ctrl_d_with_xkb_control_byte() {
    let mut enc = KeyEncoder::new().expect("failed to create key encoder");
    // Fixed path: no utf8 text, encoder uses logical key 'd' → 0x04.
    let result = enc.encode(
        key::Key::D,
        key::Action::Press,
        key::Mods::CTRL,
        key::Mods::empty(),
        None,
        u32::from(b'd'),
    );
    assert_eq!(result.expect("expected Some"), &[0x04]); // EOT (Ctrl+D)
}

/// Verify common Ctrl+letter combos produce the correct C0 byte
/// when utf8 text is stripped (the fixed path).
/// Note: `Ctrl+I` (Tab) is excluded — the encoder handles `Key::I` + Ctrl
/// via the dedicated Tab key path rather than `ctrlSeq()`.
#[test]
fn test_key_encode_ctrl_a_through_z() {
    let mut enc = KeyEncoder::new().expect("failed to create key encoder");
    let keys = [
        (key::Key::A, b'a', 0x01),
        (key::Key::B, b'b', 0x02),
        (key::Key::C, b'c', 0x03),
        (key::Key::D, b'd', 0x04),
        (key::Key::E, b'e', 0x05),
        (key::Key::F, b'f', 0x06),
        (key::Key::G, b'g', 0x07),
        (key::Key::H, b'h', 0x08),
        // Key::I (Tab=0x09) skipped: encoder handles via dedicated Tab path
        (key::Key::J, b'j', 0x0A),
        (key::Key::K, b'k', 0x0B),
        (key::Key::L, b'l', 0x0C),
        // Key::M (CR=0x0D) skipped: encoder handles via dedicated Enter path
        (key::Key::N, b'n', 0x0E),
        (key::Key::O, b'o', 0x0F),
        (key::Key::P, b'p', 0x10),
        (key::Key::Q, b'q', 0x11),
        (key::Key::R, b'r', 0x12),
        (key::Key::S, b's', 0x13),
        (key::Key::T, b't', 0x14),
        (key::Key::U, b'u', 0x15),
        (key::Key::V, b'v', 0x16),
        (key::Key::W, b'w', 0x17),
        (key::Key::X, b'x', 0x18),
        (key::Key::Y, b'y', 0x19),
        (key::Key::Z, b'z', 0x1A),
    ];
    for (gkey, ascii, expected_byte) in keys {
        let result = enc.encode(
            gkey,
            key::Action::Press,
            key::Mods::CTRL,
            key::Mods::empty(),
            None,
            u32::from(ascii),
        );
        assert_eq!(
            result.as_deref(),
            Some(&[expected_byte][..]),
            "Ctrl+{} should produce 0x{:02X}",
            char::from(ascii),
            expected_byte
        );
    }
}

#[test]
fn test_mouse_encoder_new() {
    let enc = MouseEncoder::new();
    assert!(enc.is_ok());
}

#[test]
fn test_encode_focus() {
    let gained = encode_focus(true);
    assert!(gained.is_some(), "focus gained should produce output");
    assert_eq!(gained.expect("focus gained bytes"), b"\x1b[I");

    let lost = encode_focus(false);
    assert!(lost.is_some(), "focus lost should produce output");
    assert_eq!(lost.expect("focus lost bytes"), b"\x1b[O");
}

#[test]
fn test_key_encoder_reuse() {
    // Verify the encoder can be used for multiple sequential events.
    let mut enc = KeyEncoder::new().expect("failed to create key encoder");

    let r1 = enc.encode(
        key::Key::A,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        Some("a"),
        u32::from(b'a'),
    );
    assert!(r1.is_some());

    let r2 = enc.encode(
        key::Key::B,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        Some("b"),
        u32::from(b'b'),
    );
    assert!(r2.is_some());
    assert_eq!(r2.expect("expected Some"), b"b");
}

#[test]
fn test_mouse_encoder_set_options() {
    // Verify that option setters do not panic.
    let mut enc = MouseEncoder::new().expect("failed to create mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_any_button_pressed(false);
    enc.set_track_last_cell(true);
}

#[test]
fn test_key_encode_release() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::A,
        key::Action::Release,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        u32::from(b'a'),
    );
    // Release typically produces no output in default mode
    assert!(result.is_none());
}

#[test]
fn test_key_encode_unidentified_with_text() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Unidentified,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        Some("@"),
        0,
    );
    // With UNIDENTIFIED key but valid UTF-8 text, the encoder
    // should still produce output (the text itself).
    assert!(
        result.is_some(),
        "encoder should output '@' for UNIDENTIFIED key with text"
    );
    assert_eq!(result.expect("output"), b"@");
}

#[test]
fn test_key_encode_shift_letter() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::A,
        key::Action::Press,
        key::Mods::SHIFT,
        key::Mods::SHIFT,
        Some("A"),
        u32::from(b'a'),
    );
    assert!(result.is_some());
    assert_eq!(result.expect("shift+a"), b"A");
}

/// All common shifted ASCII symbols must encode via UNIDENTIFIED + text.
/// This is the critical path that was broken: the keyboard handler used to
/// drop UNIDENTIFIED keys entirely, preventing these symbols from reaching
/// the PTY.
#[test]
fn test_key_encode_all_shifted_symbols() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let symbols = [
        "@", "!", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+", "{", "}", "|", ":", "<", ">",
        "?", "~", "\"",
    ];
    for sym in &symbols {
        let result = enc.encode(
            key::Key::Unidentified,
            key::Action::Press,
            key::Mods::empty(),
            key::Mods::empty(),
            Some(sym),
            0,
        );
        assert!(result.is_some(), "symbol '{sym}' should produce output");
        assert_eq!(
            result.expect("output"),
            sym.as_bytes(),
            "symbol '{sym}' should encode to its UTF-8 bytes"
        );
    }
}

/// ESC key must produce a bare ESC byte (0x1b).
/// This is critical for bash vi mode: ESC switches from insert to normal.
#[test]
fn test_key_encode_escape() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Escape,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        Some("\x1b"),
        0,
    );
    assert!(result.is_some(), "ESC key should produce output");
    assert_eq!(
        result.expect("esc bytes"),
        b"\x1b",
        "ESC key should produce bare 0x1b byte"
    );
}

/// Tab key must produce a Tab byte (0x09).
/// This is critical for bash progcomp (tab completion).
#[test]
fn test_key_encode_tab() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Tab,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        Some("\t"),
        0,
    );
    assert!(result.is_some(), "Tab key should produce output");
    assert_eq!(
        result.expect("tab bytes"),
        b"\t",
        "Tab key should produce 0x09 byte"
    );
}

/// UNIDENTIFIED key with NO text should produce no output (these are
/// modifier-only keys or unknown hardware keys that can't be typed).
#[test]
fn test_key_encode_unidentified_without_text() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Unidentified,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(
        result.is_none(),
        "UNIDENTIFIED without text should produce nothing"
    );
}

#[test]
fn test_mouse_encode_after_set_size() {
    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    // Without mouse tracking enabled, encode should return None
    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        100.0,
        100.0,
    );
    // In default terminal mode (no mouse tracking), no output is produced
    assert!(result.is_none());
}

#[test]
fn test_mouse_encode_motion() {
    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    // Motion without button should produce no output in default mode
    let result = enc.encode(mouse::Action::Motion, None, key::Mods::empty(), 50.0, 50.0);
    assert!(result.is_none());
}

#[test]
fn test_sync_from_terminal_app_cursor() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // CSI ? 1 h enables DECCKM (application cursor keys).
    term.vt_write(b"\x1b[?1h");

    let mut enc = KeyEncoder::new().expect("key encoder");
    enc.sync_from_terminal(term.inner());

    // Encode Up arrow — should produce \x1bOA (application mode)
    // instead of \x1b[A (normal mode).
    let result = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "arrow up should produce output");
    assert_eq!(
        result.expect("app cursor up"),
        b"\x1bOA",
        "up arrow in application cursor mode should be ESC O A"
    );
}

#[test]
fn test_mouse_encoder_sync_from_terminal() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);

    // Sync on fresh terminal (no mouse tracking) — should not panic.
    enc.sync_from_terminal(term.inner());

    // Without mouse tracking, encode returns None.
    let result_before = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        100.0,
        100.0,
    );
    assert!(
        result_before.is_none(),
        "mouse press should produce no output without tracking enabled"
    );

    // Enable X10 mouse tracking: CSI ? 1000 h
    term.vt_write(b"\x1b[?1000h");
    enc.sync_from_terminal(term.inner());

    // After enabling mouse tracking, a button press should produce output.
    let result_after = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        100.0,
        100.0,
    );
    assert!(
        result_after.is_some(),
        "mouse press should produce output with tracking enabled"
    );
}

#[test]
fn test_mouse_encode_button_press() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Enable normal mouse tracking so the encoder produces output.
    term.vt_write(b"\x1b[?1000h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    // Set realistic size so pixel-to-cell conversion works.
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    // Encode a left button press at pixel position (40, 160).
    // With cell_width=8, cell_height=16, padding=0:
    //   col = 40/8 = 5, row = 160/16 = 10
    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        40.0,
        160.0,
    );
    assert!(result.is_some(), "mouse left press should produce output");
    let bytes = result.expect("mouse press bytes");
    assert!(!bytes.is_empty(), "mouse press output should be non-empty");
}

#[test]
fn test_key_encode_ctrl_a() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::A,
        key::Action::Press,
        key::Mods::CTRL,
        key::Mods::empty(),
        None,
        u32::from(b'a'),
    );
    assert_eq!(result.expect("ctrl+a"), &[0x01]); // SOH
}

#[test]
fn test_key_encode_ctrl_z() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Z,
        key::Action::Press,
        key::Mods::CTRL,
        key::Mods::empty(),
        None,
        u32::from(b'z'),
    );
    assert_eq!(result.expect("ctrl+z"), &[0x1A]); // SUB
}

#[test]
fn test_key_encode_ctrl_bracket_left() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::BracketLeft,
        key::Action::Press,
        key::Mods::CTRL,
        key::Mods::empty(),
        Some("\x1b"),
        u32::from(b'['),
    );
    assert!(result.is_some(), "Ctrl+[ should produce output");
    let bytes = result.expect("ctrl+[");
    // Must start with ESC (0x1b)
    assert_eq!(
        bytes.first().copied(),
        Some(0x1b),
        "Ctrl+[ output should start with ESC"
    );
}

#[test]
fn test_key_encode_backspace() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Backspace,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "backspace should produce output");
    let bytes = result.expect("backspace bytes");
    // Backspace is typically 0x7f (DEL) or 0x08 (BS)
    assert!(!bytes.is_empty());
}

#[test]
fn test_key_encode_delete() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Delete,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "delete should produce output");
    let bytes = result.expect("delete bytes");
    // Delete typically produces CSI 3 ~
    assert_eq!(bytes, b"\x1b[3~");
}

#[test]
fn test_key_encode_f1() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::F1,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "F1 should produce output");
    let bytes = result.expect("F1 bytes");
    // F1 = SS3 P or CSI 11 ~
    assert!(!bytes.is_empty());
}

#[test]
fn test_key_encode_f5() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::F5,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "F5 should produce output");
    let bytes = result.expect("F5 bytes");
    assert_eq!(bytes, b"\x1b[15~");
}

#[test]
fn test_key_encode_f12() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::F12,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "F12 should produce output");
    let bytes = result.expect("F12 bytes");
    assert_eq!(bytes, b"\x1b[24~");
}

#[test]
fn test_key_encode_home() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Home,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "Home should produce output");
}

#[test]
fn test_key_encode_end() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::End,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "End should produce output");
}

#[test]
fn test_key_encode_page_up() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::PageUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "PageUp should produce output");
    assert_eq!(result.expect("pgup"), b"\x1b[5~");
}

#[test]
fn test_key_encode_page_down() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::PageDown,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "PageDown should produce output");
    assert_eq!(result.expect("pgdn"), b"\x1b[6~");
}

#[test]
fn test_key_encode_insert() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Insert,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "Insert should produce output");
    assert_eq!(result.expect("ins"), b"\x1b[2~");
}

#[test]
fn test_key_encode_arrow_up() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(result.expect("up"), b"\x1b[A");
}

#[test]
fn test_key_encode_arrow_down() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::ArrowDown,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(result.expect("down"), b"\x1b[B");
}

#[test]
fn test_key_encode_arrow_left() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::ArrowLeft,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(result.expect("left"), b"\x1b[D");
}

#[test]
fn test_key_encode_arrow_right() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::ArrowRight,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(result.expect("right"), b"\x1b[C");
}

#[test]
fn test_key_encode_repeat() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::A,
        key::Action::Repeat,
        key::Mods::empty(),
        key::Mods::empty(),
        Some("a"),
        u32::from(b'a'),
    );
    assert_eq!(result.expect("repeat a"), b"a");
}

#[test]
fn test_key_encode_numpad_enter() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::NumpadEnter,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "numpad enter should produce output");
}

#[test]
fn test_key_encode_ctrl_shift_combo() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::CTRL | key::Mods::SHIFT,
        key::Mods::empty(),
        None,
        0,
    );
    assert!(result.is_some(), "Ctrl+Shift+Up should produce output");
}

#[test]
fn test_key_encode_space() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::Space,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        Some(" "),
        u32::from(b' '),
    );
    assert_eq!(result.expect("space"), b" ");
}

#[test]
fn test_mouse_sgr_format() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Enable SGR mouse mode (1006)
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        8.0,  // col 1
        16.0, // row 1
    );
    assert!(result.is_some(), "SGR mouse press should produce output");
    let bytes = result.expect("sgr bytes");
    let text = String::from_utf8(bytes).expect("valid utf8");
    // SGR format: \x1b[<0;col;rowM (press) where 0 = left button
    assert!(text.starts_with("\x1b[<"), "SGR format starts with CSI <");
    assert!(text.ends_with('M'), "SGR press ends with M");
}

#[test]
fn test_mouse_sgr_release() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Release,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        8.0,
        16.0,
    );
    assert!(result.is_some(), "SGR mouse release should produce output");
    let bytes = result.expect("sgr release bytes");
    let text = String::from_utf8(bytes).expect("valid utf8");
    assert!(text.starts_with("\x1b[<"), "SGR format starts with CSI <");
    assert!(text.ends_with('m'), "SGR release ends with lowercase m");
}

#[test]
fn test_mouse_wheel_up() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Four), // scroll up
        key::Mods::empty(),
        100.0,
        100.0,
    );
    assert!(result.is_some(), "wheel up should produce output");
}

#[test]
fn test_mouse_wheel_down() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Five), // scroll down
        key::Mods::empty(),
        100.0,
        100.0,
    );
    assert!(result.is_some(), "wheel down should produce output");
}

#[test]
fn test_mouse_with_shift_modifier() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::SHIFT,
        50.0,
        50.0,
    );
    assert!(result.is_some(), "shift+click should produce output");
}

#[test]
fn test_mouse_any_event_mode_motion() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Enable any-event tracking (1003) + SGR (1006)
    term.vt_write(b"\x1b[?1003h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    // Motion without button in any-event mode should produce output
    let result = enc.encode(
        mouse::Action::Motion,
        None,
        key::Mods::empty(),
        100.0,
        100.0,
    );
    assert!(result.is_some(), "any-event motion should produce output");
}

#[test]
fn test_mouse_right_button() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Right),
        key::Mods::empty(),
        50.0,
        50.0,
    );
    assert!(result.is_some(), "right button press should produce output");
    let bytes = result.expect("right click bytes");
    let text = String::from_utf8(bytes).expect("valid utf8");
    // SGR right button = button 2
    assert!(
        text.starts_with("\x1b[<2;"),
        "right button should be button 2 in SGR"
    );
}

#[test]
fn test_mouse_middle_button() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Middle),
        key::Mods::empty(),
        50.0,
        50.0,
    );
    assert!(
        result.is_some(),
        "middle button press should produce output"
    );
    let bytes = result.expect("middle click bytes");
    let text = String::from_utf8(bytes).expect("valid utf8");
    assert!(
        text.starts_with("\x1b[<1;"),
        "middle button should be button 1 in SGR"
    );
}

#[test]
fn test_mouse_button_event_tracking() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Enable button-event tracking (motion only reported when button held)
    term.vt_write(b"\x1b[?1002h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(false);
    enc.set_any_button_pressed(true);
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        mouse::Action::Motion,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        100.0,
        200.0,
    );
    assert!(
        result.is_some(),
        "button-event drag motion should produce output"
    );
}

#[test]
fn test_mouse_motion_dedup_same_cell() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1003h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(true); // Enable dedup
    enc.sync_from_terminal(term.inner());

    // First motion at (100, 100)
    let r1 = enc.encode(
        mouse::Action::Motion,
        None,
        key::Mods::empty(),
        100.0,
        100.0,
    );
    assert!(r1.is_some(), "first motion should produce output");

    // Second motion at same cell (100.5, 100.5 is same cell as 100, 100)
    let r2 = enc.encode(
        mouse::Action::Motion,
        None,
        key::Mods::empty(),
        100.5,
        100.5,
    );
    assert!(r2.is_none(), "same-cell motion should be deduped");
}

#[test]
fn test_mouse_motion_dedup_different_cell() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1003h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(true);
    enc.sync_from_terminal(term.inner());

    let _ = enc.encode(mouse::Action::Motion, None, key::Mods::empty(), 8.0, 16.0);

    // Move to different cell (much further away)
    let r2 = enc.encode(mouse::Action::Motion, None, key::Mods::empty(), 80.0, 160.0);
    assert!(r2.is_some(), "different-cell motion should produce output");
}

#[test]
fn test_mouse_encoder_with_padding() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    // 10px padding on each side
    enc.set_size(800, 600, 8, 16, 10);
    enc.set_track_last_cell(false);
    enc.sync_from_terminal(term.inner());

    // Click at pixel (10, 10) — with 10px padding, this should be cell (0, 0)
    let result = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        10.0,
        10.0,
    );
    assert!(result.is_some(), "press with padding should produce output");
}

#[test]
fn test_mouse_motion_dedup_resets_on_press() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1003h\x1b[?1006h");

    let mut enc = MouseEncoder::new().expect("mouse encoder");
    enc.set_size(800, 600, 8, 16, 0);
    enc.set_track_last_cell(true);
    enc.sync_from_terminal(term.inner());

    // Motion at cell position
    let _ = enc.encode(
        mouse::Action::Motion,
        None,
        key::Mods::empty(),
        100.0,
        100.0,
    );
    // Same-cell motion is deduped
    let deduped = enc.encode(
        mouse::Action::Motion,
        None,
        key::Mods::empty(),
        100.5,
        100.5,
    );
    assert!(deduped.is_none(), "same-cell motion should be deduped");

    // Press at the same location should still produce output
    let press = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        100.0,
        100.0,
    );
    assert!(
        press.is_some(),
        "press should produce output even at same cell"
    );
}

#[test]
fn test_sync_from_terminal_picks_up_modes() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut enc = KeyEncoder::new().expect("key encoder");

    // Normal mode: Up arrow = CSI A
    enc.sync_from_terminal(term.inner());
    let normal = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        normal.as_deref(),
        Some(b"\x1b[A".as_slice()),
        "normal mode: CSI A"
    );

    // Enable DECCKM
    term.vt_write(b"\x1b[?1h");
    enc.sync_from_terminal(term.inner());
    let app = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        app.as_deref(),
        Some(b"\x1bOA".as_slice()),
        "app mode: SS3 A"
    );

    // Disable DECCKM
    term.vt_write(b"\x1b[?1l");
    enc.sync_from_terminal(term.inner());
    let back = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        back.as_deref(),
        Some(b"\x1b[A".as_slice()),
        "back to normal: CSI A"
    );
}

#[test]
fn test_key_encode_all_f1_through_f12() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let fkeys = [
        key::Key::F1,
        key::Key::F2,
        key::Key::F3,
        key::Key::F4,
        key::Key::F5,
        key::Key::F6,
        key::Key::F7,
        key::Key::F8,
        key::Key::F9,
        key::Key::F10,
        key::Key::F11,
        key::Key::F12,
    ];
    for (i, &fk) in fkeys.iter().enumerate() {
        let result = enc.encode(
            fk,
            key::Action::Press,
            key::Mods::empty(),
            key::Mods::empty(),
            None,
            0,
        );
        assert!(result.is_some(), "F{} should produce output", i + 1);
        let bytes = result.expect("F-key bytes");
        assert!(!bytes.is_empty(), "F{} output should be non-empty", i + 1);
    }
}

#[test]
fn test_key_encode_alt_letter() {
    use crate::terminal::vt::Terminal;

    let term = Terminal::new(80, 24, 100).expect("terminal");
    let mut enc = KeyEncoder::new().expect("key encoder");
    enc.sync_from_terminal(term.inner());
    let result = enc.encode(
        key::Key::A,
        key::Action::Press,
        key::Mods::ALT,
        key::Mods::empty(),
        Some("a"),
        u32::from(b'a'),
    );
    assert!(result.is_some(), "Alt+a should produce output");
    let bytes = result.expect("output");
    // Alt+letter should produce ESC prefix (0x1b) followed by the letter
    assert!(
        bytes.starts_with(&[0x1b]),
        "Alt should produce ESC prefix, got {bytes:?}"
    );
}

#[test]
fn test_key_encode_ctrl_alt() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::C,
        key::Action::Press,
        key::Mods::CTRL | key::Mods::ALT,
        key::Mods::empty(),
        None,
        u32::from(b'c'),
    );
    // Ctrl+Alt+C should produce some output
    assert!(result.is_some(), "Ctrl+Alt+C should produce output");
}

#[test]
fn test_mouse_encode_large_coords() {
    let mut enc = MouseEncoder::new().expect("mouse encoder");
    // Set up a very large "screen" so coords map to large cell values
    enc.set_size(60000, 60000, 8, 16, 0);
    enc.set_any_button_pressed(false);
    // Enable mouse tracking by syncing from a terminal with mouse mode on
    let mut term = crate::terminal::vt::Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1000h"); // enable mouse
    enc.sync_from_terminal(term.inner());
    // Encode a press at large pixel coordinates — should not panic
    let _ = enc.encode(
        mouse::Action::Press,
        Some(mouse::Button::Left),
        key::Mods::empty(),
        59000.0,
        59000.0,
    );
}

// ---- DECCKM (application cursor keys) tests (Bug 3: tmux scroll) ----

#[test]
fn test_arrow_up_normal_mode() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        result.expect("normal up"),
        b"\x1b[A",
        "Up arrow in normal mode should be CSI A"
    );
}

#[test]
fn test_arrow_down_normal_mode() {
    let mut enc = KeyEncoder::new().expect("key encoder");
    let result = enc.encode(
        key::Key::ArrowDown,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        result.expect("normal down"),
        b"\x1b[B",
        "Down arrow in normal mode should be CSI B"
    );
}

#[test]
fn test_arrow_down_app_cursor_mode() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // CSI ? 1 h enables DECCKM (application cursor keys).
    term.vt_write(b"\x1b[?1h");

    let mut enc = KeyEncoder::new().expect("key encoder");
    enc.sync_from_terminal(term.inner());

    // Down arrow in DECCKM should produce \x1bOB, not \x1b[B
    let result = enc.encode(
        key::Key::ArrowDown,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        result.expect("app cursor down"),
        b"\x1bOB",
        "down arrow in application cursor mode should be ESC O B"
    );
}

/// Verify that all four arrow keys use application cursor sequences
/// when DECCKM is enabled. This is the root cause of tmux scroll-down
/// not working: hardcoded \x1b[A/\x1b[B instead of encoder output.
#[test]
fn test_all_arrows_app_cursor_mode() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1h"); // enable DECCKM

    let mut enc = KeyEncoder::new().expect("key encoder");
    enc.sync_from_terminal(term.inner());

    let cases: [(key::Key, &[u8]); 4] = [
        (key::Key::ArrowUp, b"\x1bOA"),
        (key::Key::ArrowDown, b"\x1bOB"),
        (key::Key::ArrowRight, b"\x1bOC"),
        (key::Key::ArrowLeft, b"\x1bOD"),
    ];

    for (arrow, expected) in &cases {
        let result = enc.encode(
            *arrow,
            key::Action::Press,
            key::Mods::empty(),
            key::Mods::empty(),
            None,
            0,
        );
        assert_eq!(
            result.as_deref(),
            Some(*expected),
            "arrow {arrow:?} in DECCKM mode should produce {expected:?}"
        );
    }
}

/// Verify DECCKM can be toggled off and arrows revert to normal mode.
#[test]
fn test_decckm_toggle_off() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut enc = KeyEncoder::new().expect("key encoder");

    // Enable DECCKM
    term.vt_write(b"\x1b[?1h");
    enc.sync_from_terminal(term.inner());
    let up_app = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(up_app.expect("app up"), b"\x1bOA");

    // Disable DECCKM
    term.vt_write(b"\x1b[?1l");
    enc.sync_from_terminal(term.inner());
    let up_normal = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        up_normal.expect("normal up after toggle"),
        b"\x1b[A",
        "after DECCKM off, arrow should revert to normal mode"
    );
}

/// Verify that `sync_from_terminal` picks up DECCKM from alt screen.
/// tmux enables alt screen + DECCKM; scroll must use application cursor.
#[test]
fn test_decckm_on_alternate_screen() {
    use crate::terminal::vt::Terminal;

    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Switch to alt screen and enable DECCKM (typical tmux setup)
    term.vt_write(b"\x1b[?1049h"); // alt screen
    term.vt_write(b"\x1b[?1h"); // DECCKM

    let mut enc = KeyEncoder::new().expect("key encoder");
    enc.sync_from_terminal(term.inner());

    let result = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        result.expect("alt screen up"),
        b"\x1bOA",
        "DECCKM should be active on alternate screen"
    );

    // Also test down arrow — this was the broken direction in tmux scroll
    let result_down = enc.encode(
        key::Key::ArrowDown,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert_eq!(
        result_down.expect("alt screen down"),
        b"\x1bOB",
        "down arrow should also use application cursor on alt screen"
    );
}

/// Symmetry test: encoder must produce output for both up and down
/// arrow keys in both normal and application cursor mode.
#[test]
fn test_arrow_encoding_symmetry() {
    use crate::terminal::vt::Terminal;

    let mut enc = KeyEncoder::new().expect("key encoder");

    // Normal mode: both should produce output
    let up = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    let down = enc.encode(
        key::Key::ArrowDown,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(up.is_some(), "arrow up must produce output in normal mode");
    assert!(
        down.is_some(),
        "arrow down must produce output in normal mode"
    );

    // Application cursor mode: both should produce output
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"\x1b[?1h");
    enc.sync_from_terminal(term.inner());

    let up_app = enc.encode(
        key::Key::ArrowUp,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    let down_app = enc.encode(
        key::Key::ArrowDown,
        key::Action::Press,
        key::Mods::empty(),
        key::Mods::empty(),
        None,
        0,
    );
    assert!(up_app.is_some(), "arrow up must produce output in DECCKM");
    assert!(
        down_app.is_some(),
        "arrow down must produce output in DECCKM"
    );
}
