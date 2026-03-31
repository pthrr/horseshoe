use super::*;

#[test]
fn test_terminal_new_and_drop() {
    let term = Terminal::new(80, 24, 1000);
    assert!(term.is_ok());
}

#[test]
fn test_terminal_vt_write() {
    let mut term = Terminal::new(80, 24, 1000).expect("terminal creation failed");
    term.vt_write(b"Hello, world!\r\n");
}

#[test]
fn test_terminal_resize() {
    let mut term = Terminal::new(80, 24, 1000).expect("terminal creation failed");
    assert!(term.resize(120, 40).is_ok());
}

#[test]
fn test_terminal_scroll() {
    let mut term = Terminal::new(80, 24, 1000).expect("terminal creation failed");
    for i in 0..50 {
        let line = format!("line {i}\r\n");
        term.vt_write(line.as_bytes());
    }
    term.scroll_viewport_delta(-5);
    term.scroll_viewport_bottom();
}

#[test]
fn test_terminal_mode_get() {
    let term = Terminal::new(80, 24, 1000).expect("terminal creation failed");
    let result = term.mode_get(Mode::CURSOR_VISIBLE);
    assert!(result.is_some());
}

#[test]
fn test_terminal_mode_set_bracketed_paste() {
    let mut term = Terminal::new(80, 24, 1000).expect("terminal creation failed");
    assert_eq!(term.mode_get(Mode::BRACKETED_PASTE), Some(false));
    assert!(term.mode_set(Mode::BRACKETED_PASTE, true).is_ok());
    assert_eq!(term.mode_get(Mode::BRACKETED_PASTE), Some(true));
    assert!(term.mode_set(Mode::BRACKETED_PASTE, false).is_ok());
    assert_eq!(term.mode_get(Mode::BRACKETED_PASTE), Some(false));
}

#[test]
fn test_vt_write_moves_cursor() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal creation");
    term.vt_write(b"AB");
    let (col, row) = term.cursor_position();
    assert_eq!(col, 2);
    assert_eq!(row, 0);
}

#[test]
fn test_cursor_position_initial() {
    let term = Terminal::new(80, 24, 100).expect("terminal creation");
    let (col, row) = term.cursor_position();
    assert_eq!(col, 0);
    assert_eq!(row, 0);
}

#[test]
fn test_resize_updates_dimensions() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal creation");
    assert!(term.resize(40, 10).is_ok());
    let (col, row) = term.cursor_position();
    assert!(col < 40);
    assert!(row < 10);
}

#[test]
fn test_scrollbar_fresh() {
    let term = Terminal::new(80, 24, 100).expect("terminal creation");
    let sb = term.scrollbar();
    assert!(sb.is_some());
}

#[test]
fn test_is_alternate_screen_false() {
    let term = Terminal::new(80, 24, 100).expect("terminal creation");
    assert!(!term.is_alternate_screen());
}

#[test]
fn test_scroll_viewport_delta_noop() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal creation");
    term.scroll_viewport_delta(-5);
    term.scroll_viewport_delta(5);
}

#[test]
fn test_scroll_viewport_bottom_idempotent() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal creation");
    term.scroll_viewport_bottom();
    term.scroll_viewport_bottom();
}

#[test]
fn test_reset_returns_cursor_to_origin() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"Hello, world!");
    let (before_col, _) = term.cursor_position();
    assert!(before_col > 0);
    term.reset();
    let (after_col, after_row) = term.cursor_position();
    assert_eq!(after_col, 0);
    assert_eq!(after_row, 0);
}

#[test]
fn test_cursor_wraparound_at_eol() {
    let mut term = Terminal::new(10, 5, 100).expect("terminal");
    term.vt_write(b"0123456789");
    let (col, row) = term.cursor_position();
    let valid = (col == 9 && row == 0) || (col == 0 && row == 1);
    assert!(valid, "got ({col},{row})");
}

#[test]
fn test_scrollback_overflow() {
    let mut term = Terminal::new(80, 24, 10).expect("terminal");
    for i in 0..50 {
        let line = format!("line {i}\r\n");
        term.vt_write(line.as_bytes());
    }
    term.scroll_viewport_delta(-100);
    term.scroll_viewport_bottom();
}

#[test]
fn test_resize_preserves_content() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"Hello");
    term.resize(40, 12).expect("resize");
    let (col, row) = term.cursor_position();
    assert!(col < 40);
    assert!(row < 12);
}

// -- TerminalCb tests --

#[test]
fn test_terminal_cb_create_and_drop() {
    let term = TerminalCb::new(80, 24, 1000);
    assert!(term.is_ok());
}

#[test]
fn test_terminal_cb_da1_response() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b[c");
    let responses = term.take_pty_responses();
    assert!(!responses.is_empty(), "DA1 should produce a response");
    assert!(
        responses.starts_with(b"\x1b[?"),
        "DA1 response should start with ESC[?: {:?}",
        String::from_utf8_lossy(&responses)
    );
}

#[test]
fn test_terminal_cb_da2_response() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b[>c");
    let responses = term.take_pty_responses();
    assert!(!responses.is_empty(), "DA2 should produce a response");
    assert!(
        responses.starts_with(b"\x1b[>"),
        "DA2 response format: {:?}",
        String::from_utf8_lossy(&responses)
    );
}

#[test]
fn test_terminal_cb_da3_response() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b[=c");
    let responses = term.take_pty_responses();
    assert!(!responses.is_empty(), "DA3 should produce a response");
    let resp_str = String::from_utf8_lossy(&responses);
    assert!(
        resp_str.contains("485253"),
        "DA3 should contain HRS hex: {resp_str}"
    );
}

#[test]
fn test_terminal_cb_xtversion_response() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b[>0q");
    let responses = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&responses);
    let expected = concat!("horseshoe(", env!("CARGO_PKG_VERSION"), ")");
    assert!(
        resp_str.contains(expected),
        "XTVERSION should report {expected}: {resp_str}"
    );
}

#[test]
fn test_terminal_cb_dsr_status() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b[5n");
    let responses = term.take_pty_responses();
    assert_eq!(responses, b"\x1b[0n");
}

#[test]
fn test_terminal_cb_dsr_cursor_position() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"AB");
    let _ = term.take_pty_responses();
    term.vt_write(b"\x1b[6n");
    let responses = term.take_pty_responses();
    assert_eq!(
        responses,
        b"\x1b[1;3R",
        "DSR 6n cursor report: {:?}",
        String::from_utf8_lossy(&responses)
    );
}

#[test]
fn test_terminal_cb_bell() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    assert!(!term.take_bell());
    term.vt_write(b"\x07");
    assert!(term.take_bell());
    assert!(!term.take_bell());
}

#[test]
fn test_terminal_cb_title_change() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    assert!(term.take_title().is_none());
    term.vt_write(b"\x1b]0;my test title\x07");
    let title = term.take_title();
    assert_eq!(title.as_deref(), Some("my test title"));
    assert!(term.take_title().is_none());
}

#[test]
fn test_terminal_cb_title_osc2() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b]2;another title\x1b\\");
    let title = term.take_title();
    assert_eq!(title.as_deref(), Some("another title"));
}

#[test]
fn test_terminal_cb_resize() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    assert!(term.resize(120, 40).is_ok());
}

#[test]
fn test_terminal_cb_mode_get_set() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    assert_eq!(term.mode_get(Mode::BRACKETED_PASTE), Some(false));
    assert!(term.mode_set(Mode::BRACKETED_PASTE, true).is_ok());
    assert_eq!(term.mode_get(Mode::BRACKETED_PASTE), Some(true));
}

#[test]
fn test_terminal_cb_scroll() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    for i in 0..50 {
        let line = format!("line {i}\r\n");
        term.vt_write(line.as_bytes());
    }
    term.scroll_viewport_delta(-5);
    term.scroll_viewport_bottom();
}

#[test]
fn test_terminal_cb_cursor_position() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    let (col, row) = term.cursor_position();
    assert_eq!(col, 0);
    assert_eq!(row, 0);
    term.vt_write(b"ABC");
    let (col2, row2) = term.cursor_position();
    assert_eq!(col2, 3);
    assert_eq!(row2, 0);
}

#[test]
fn test_terminal_cb_reset() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"Hello");
    term.reset();
    let (col, row) = term.cursor_position();
    assert_eq!(col, 0);
    assert_eq!(row, 0);
}

#[test]
fn test_terminal_cb_multiple_responses() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b[c\x1b[5n");
    let responses = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&responses);
    assert!(
        resp_str.contains("[?"),
        "should contain DA1 response: {resp_str}"
    );
    assert!(
        resp_str.contains("[0n"),
        "should contain DSR response: {resp_str}"
    );
}

#[test]
fn test_terminal_cb_decrqm_response() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b[?2004h");
    let _ = term.take_pty_responses();
    term.vt_write(b"\x1b[?2004$p");
    let responses = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&responses);
    assert!(
        resp_str.contains("2004;1$y"),
        "DECRQM should report mode 2004 as set: {resp_str}"
    );
}

#[test]
fn test_terminal_cb_osc_color_query_not_supported() {
    // The upstream libghostty_vt Terminal does not generate OSC 10/11/12
    // color query responses through the callback API. This is a known
    // limitation — color queries were handled by our custom Zig handler.
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    term.vt_write(b"\x1b]11;#1e1e2e\x1b\\");
    let _ = term.take_pty_responses();
    term.vt_write(b"\x1b]11;?\x07");
    let responses = term.take_pty_responses();
    assert!(
        responses.is_empty(),
        "OSC color queries produce no response"
    );
}

#[test]
fn test_alternate_screen_enter_exit() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    assert!(
        !term.is_alternate_screen(),
        "should start on primary screen"
    );
    // CSI ? 1049 h — enter alternate screen
    term.vt_write(b"\x1b[?1049h");
    assert!(
        term.is_alternate_screen(),
        "should be on alternate screen after CSI?1049h"
    );
    // CSI ? 1049 l — exit alternate screen
    term.vt_write(b"\x1b[?1049l");
    assert!(
        !term.is_alternate_screen(),
        "should be back on primary screen after CSI?1049l"
    );
}

#[test]
fn test_terminal_cb_alternate_screen() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    assert!(!term.is_alternate_screen());
    term.vt_write(b"\x1b[?1049h");
    assert!(term.is_alternate_screen());
    term.vt_write(b"\x1b[?1049l");
    assert!(!term.is_alternate_screen());
}

#[test]
fn test_scroll_viewport_with_content() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Generate enough lines to create scrollback
    for i in 0..50 {
        term.vt_write(format!("line {i}\r\n").as_bytes());
    }
    let sb_before = term.scrollbar().expect("scrollbar");
    term.scroll_viewport_delta(-10);
    let sb_after = term.scrollbar().expect("scrollbar after scroll");
    // After scrolling up, the offset should change
    assert_ne!(
        sb_before.offset, sb_after.offset,
        "scrollbar offset should change after scroll_viewport_delta"
    );
}

#[test]
fn test_scroll_viewport_bottom_resets() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    for i in 0..50 {
        term.vt_write(format!("line {i}\r\n").as_bytes());
    }
    term.scroll_viewport_delta(-10);
    term.scroll_viewport_bottom();
    let sb = term.scrollbar().expect("scrollbar");
    // After scroll_viewport_bottom, offset + len should equal total
    assert_eq!(
        sb.offset + sb.len,
        sb.total,
        "scroll_viewport_bottom should return to end: offset={} len={} total={}",
        sb.offset,
        sb.len,
        sb.total
    );
}

#[test]
fn test_mode_set_cursor_visible() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    assert_eq!(term.mode_get(Mode::CURSOR_VISIBLE), Some(true));
    assert!(term.mode_set(Mode::CURSOR_VISIBLE, false).is_ok());
    assert_eq!(term.mode_get(Mode::CURSOR_VISIBLE), Some(false));
    assert!(term.mode_set(Mode::CURSOR_VISIBLE, true).is_ok());
    assert_eq!(term.mode_get(Mode::CURSOR_VISIBLE), Some(true));
}

#[test]
fn test_application_cursor_keys_via_vt() {
    // DECCKM is set via CSI ? 1 h, not via mode_set API, so test via VT sequence
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    assert!(!term.is_alternate_screen(), "precondition: primary screen");
    // CSI ? 1 h enables application cursor keys (DECCKM)
    term.vt_write(b"\x1b[?1h");
    // We can't query DECCKM directly, but we verified this works via
    // key encoder tests (test_sync_from_terminal_app_cursor). Just ensure no panic.
    term.vt_write(b"\x1b[?1l");
}

#[test]
fn test_reset_clears_modes_and_cursor() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Set some modes
    assert!(term.mode_set(Mode::BRACKETED_PASTE, true).is_ok());
    term.vt_write(b"Some text here");
    let (pre_col, _) = term.cursor_position();
    assert!(pre_col > 0, "cursor should have moved");

    term.reset();

    // After reset, modes should return to defaults and cursor to origin
    assert_eq!(term.mode_get(Mode::BRACKETED_PASTE), Some(false));
    let (post_col, post_row) = term.cursor_position();
    assert_eq!(post_col, 0, "cursor col should be 0 after reset");
    assert_eq!(post_row, 0, "cursor row should be 0 after reset");
}

#[test]
fn test_terminal_cb_size_report() {
    let mut term = TerminalCb::new(80, 24, 100).expect("cb terminal");
    // CSI 18 t — request terminal size in characters
    term.vt_write(b"\x1b[18t");
    let responses = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&responses);
    // Response format: CSI 8 ; rows ; cols t
    assert!(
        resp_str.contains("8;24;80"),
        "size report should contain 8;24;80: {resp_str}"
    );
}

#[test]
fn test_terminal_vt_write_binary_data() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    // Feed raw bytes 0x80-0xFF which are not valid UTF-8 on their own
    let data: Vec<u8> = (0x80..=0xFF).collect();
    term.vt_write(&data);
    // Just verify no panic — the terminal should handle arbitrary byte streams
}

#[test]
fn test_terminal_rapid_resize() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"Some content here\r\n");
    for i in 0u16..100 {
        let cols = 20 + (i % 200);
        let rows = 5 + (i % 50);
        assert!(
            term.resize(cols, rows).is_ok(),
            "resize to {cols}x{rows} should succeed"
        );
    }
    // After all resizes, basic operations should still work
    term.vt_write(b"after resize");
    let (col, row) = term.cursor_position();
    assert!(col < 300);
    assert!(row < 100);
}

#[test]
fn test_terminal_cb_box_stability() {
    // Moving TerminalCb should not invalidate callbacks
    let cb = TerminalCb::new(80, 24, 100).expect("terminal creation");
    // Move it into a new binding
    let mut moved_cb = cb;
    // Write data — callbacks should still fire correctly
    moved_cb.vt_write(b"\x07"); // BEL
    assert!(moved_cb.take_bell(), "bell should fire after move");
}

#[test]
fn test_terminal_cb_title_utf8() {
    let mut cb = TerminalCb::new(80, 24, 100).expect("terminal creation");
    // OSC 2 ; title ST (multi-byte UTF-8)
    cb.vt_write(b"\x1b]2;Heiz\xc3\xb6lr\xc3\xbcckf\xc3\xbchrung\x1b\\");
    let title = cb.take_title();
    assert_eq!(title.as_deref(), Some("Heizölrückführung"));
}

#[test]
fn test_resize_to_1x1() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal creation");
    assert!(term.resize(1, 1).is_ok());
    let (col, row) = term.cursor_position();
    assert_eq!(col, 0);
    assert_eq!(row, 0);
}

#[test]
fn test_resize_in_alternate_screen() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal creation");
    // Enable alternate screen via DECSET 1049
    term.vt_write(b"\x1b[?1049h");
    assert!(term.is_alternate_screen());
    assert!(term.resize(40, 10).is_ok());
    // Still in alternate screen
    assert!(term.is_alternate_screen());
}

#[test]
fn test_scroll_viewport_beyond_history() {
    let mut term = Terminal::new(80, 5, 100).expect("terminal creation");
    // Fill just a few lines
    for i in 0..3 {
        let line = format!("line {i}\r\n");
        term.vt_write(line.as_bytes());
    }
    // Scroll way past history — should not panic, just clamp
    term.scroll_viewport_delta(-1000);
    term.scroll_viewport_bottom();
}
