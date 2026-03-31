//! VT conformance tests — verify escape sequence behavior matches foot's
//! documented terminal semantics from `foot-ctlseqs(7)`.
//!
//! Each test is a pure function: create a [`TerminalCb`], send bytes via
//! `vt_write`, then check cursor position, cell content, mode state, or
//! PTY responses. No PTY subprocess is needed — these run fast in CI.
//!
//! Test categories:
//! - CSI sequences (cursor movement, erase, SGR, scroll, tabs, modes)
//! - OSC sequences (title, clipboard)
//! - Device queries (DA1, DA2, DA3, XTVERSION, DSR)
//! - Mode behavior (alt screen, scroll region, origin, autowrap, insert)
//! - Character handling (UTF-8, wide chars, tabs, control characters)

#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::shadow_unrelated
)]

use horseshoe::terminal::render::RenderState;
use horseshoe::terminal::vt::{TerminalCb, TerminalOps};
use libghostty_vt::terminal::Mode;

/// Create a [`TerminalCb`] + [`RenderState`] pair with given dimensions.
fn make_term(cols: u16, rows: u16) -> (TerminalCb, RenderState) {
    let term = TerminalCb::new(cols, rows, 1000).expect("create terminal");
    let rs = RenderState::new().expect("create render state");
    (term, rs)
}

/// Extract grid text as a vector of strings (one per row), trimming
/// trailing whitespace.
fn grid_text(term: &TerminalCb, rs: &mut RenderState) -> Vec<String> {
    let _ = rs.update(term.inner());
    let (_cols, rows) = rs.dimensions();
    let mut lines: Vec<String> = vec![String::new(); usize::from(rows)];
    rs.for_each_cell(|row, _col, codepoints, _style, _wide| {
        if let Some(line) = lines.get_mut(row) {
            if let Some(&cp) = codepoints.first() {
                if cp == 0 {
                    line.push(' ');
                } else {
                    line.push(char::from_u32(cp).unwrap_or(' '));
                }
            } else {
                line.push(' ');
            }
        }
    });
    lines.iter().map(|l| l.trim_end().to_string()).collect()
}

/// Extract style attributes for a specific cell.
fn cell_style_at(
    term: &TerminalCb,
    rs: &mut RenderState,
    target_row: usize,
    target_col: usize,
) -> Option<horseshoe::terminal::render::CellStyle> {
    let _ = rs.update(term.inner());
    let mut result = None;
    rs.for_each_cell(|row, col, _codepoints, style, _wide| {
        if row == target_row && col == target_col {
            result = Some(style.clone());
        }
    });
    result
}

/// Check if a cell is marked as wide.
fn cell_is_wide(
    term: &TerminalCb,
    rs: &mut RenderState,
    target_row: usize,
    target_col: usize,
) -> bool {
    let _ = rs.update(term.inner());
    let mut wide = false;
    rs.for_each_cell(|row, col, _codepoints, _style, is_wide| {
        if row == target_row && col == target_col {
            wide = is_wide;
        }
    });
    wide
}

// ===========================================================================
// CSI: Cursor movement
// ===========================================================================

#[test]
fn conformance_cup_cursor_position() {
    let (mut term, _rs) = make_term(80, 24);
    // CUP: CSI row ; col H (1-based)
    term.vt_write(b"\x1b[5;10H");
    let (col, row) = term.cursor_position();
    assert_eq!(
        (col, row),
        (9, 4),
        "CUP 5;10 should place cursor at col=9, row=4 (0-based)"
    );
}

#[test]
fn conformance_cup_default_home() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"Hello");
    // CUP with no params = home (1,1)
    term.vt_write(b"\x1b[H");
    let (col, row) = term.cursor_position();
    assert_eq!(
        (col, row),
        (0, 0),
        "CUP with no params should go to home (0,0)"
    );
}

#[test]
fn conformance_cuu_cursor_up() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[10;5H"); // row 10, col 5
    term.vt_write(b"\x1b[3A"); // CUU: move up 3
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (4, 6), "CUU 3 from row 9 should go to row 6");
}

#[test]
fn conformance_cud_cursor_down() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[5;5H"); // row 5, col 5
    term.vt_write(b"\x1b[3B"); // CUD: move down 3
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (4, 7), "CUD 3 from row 4 should go to row 7");
}

#[test]
fn conformance_cuf_cursor_forward() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[1;1H"); // home
    term.vt_write(b"\x1b[10C"); // CUF: move right 10
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (10, 0), "CUF 10 from col 0 should go to col 10");
}

#[test]
fn conformance_cub_cursor_backward() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[1;20H"); // row 1, col 20
    term.vt_write(b"\x1b[5D"); // CUB: move left 5
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (14, 0), "CUB 5 from col 19 should go to col 14");
}

#[test]
fn conformance_cursor_clamp_at_boundaries() {
    let (mut term, _rs) = make_term(80, 24);
    // CUU from top row should clamp at row 0
    term.vt_write(b"\x1b[H"); // home
    term.vt_write(b"\x1b[999A"); // CUU way past top
    let (col, row) = term.cursor_position();
    assert_eq!(row, 0, "CUU should clamp at top row");
    assert_eq!(col, 0);

    // CUB from col 0 should clamp at col 0
    term.vt_write(b"\x1b[999D");
    let (col2, _) = term.cursor_position();
    assert_eq!(col2, 0, "CUB should clamp at left margin");
}

// ===========================================================================
// CSI: Erase sequences
// ===========================================================================

#[test]
fn conformance_ed_erase_display_below() {
    let (mut term, mut rs) = make_term(80, 24);
    // Fill first 3 rows
    term.vt_write(b"AAAAAAAAAA\r\nBBBBBBBBBB\r\nCCCCCCCCCC");
    // Move to row 2, col 0 and erase below (ED 0 = default)
    term.vt_write(b"\x1b[2;1H\x1b[J");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(
        lines[0].trim_end(),
        "AAAAAAAAAA",
        "Row 0 should be preserved"
    );
    assert!(
        lines[1].trim().is_empty(),
        "Row 1 (cursor row) should be erased"
    );
}

#[test]
fn conformance_ed_erase_display_above() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"AAAAAAAAAA\r\nBBBBBBBBBB\r\nCCCCCCCCCC");
    // Move to row 2, col 5, erase above (ED 1)
    term.vt_write(b"\x1b[2;6H\x1b[1J");
    let lines = grid_text(&term, &mut rs);
    assert!(lines[0].trim().is_empty(), "Row 0 should be erased by ED 1");
    assert_eq!(
        lines[2].trim_end(),
        "CCCCCCCCCC",
        "Row 2 should be preserved"
    );
}

#[test]
fn conformance_ed_erase_display_all() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"AAAAAAAAAA\r\nBBBBBBBBBB\r\nCCCCCCCCCC");
    // ED 2 = erase entire display
    term.vt_write(b"\x1b[2J");
    let lines = grid_text(&term, &mut rs);
    for (i, line) in lines.iter().enumerate() {
        assert!(line.trim().is_empty(), "Row {i} should be empty after ED 2");
    }
}

#[test]
fn conformance_el_erase_line_right() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"ABCDEFGHIJ");
    // Move to col 5 and erase to right (EL 0 = default)
    term.vt_write(b"\x1b[1;6H\x1b[K");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(
        lines[0], "ABCDE",
        "EL 0 should erase from cursor to end of line"
    );
}

#[test]
fn conformance_el_erase_line_left() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"ABCDEFGHIJ");
    // Move to col 5 and erase to left (EL 1)
    term.vt_write(b"\x1b[1;6H\x1b[1K");
    let lines = grid_text(&term, &mut rs);
    // Cols 0-5 erased, cols 6-9 remain
    assert!(
        lines[0].starts_with("      "),
        "EL 1 should erase from start to cursor"
    );
    assert!(
        lines[0].contains("GHIJ"),
        "EL 1 should preserve chars after cursor"
    );
}

#[test]
fn conformance_el_erase_entire_line() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"ABCDEFGHIJ");
    term.vt_write(b"\x1b[1;5H\x1b[2K"); // EL 2 = entire line
    let lines = grid_text(&term, &mut rs);
    assert!(lines[0].trim().is_empty(), "EL 2 should erase entire line");
}

#[test]
fn conformance_ech_erase_character() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"ABCDEFGHIJ");
    // Move to col 3, erase 4 characters (ECH)
    term.vt_write(b"\x1b[1;4H\x1b[4X");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(&lines[0][..3], "ABC", "Chars before cursor preserved");
    assert_eq!(&lines[0][3..7], "    ", "4 chars at cursor erased");
    assert_eq!(
        &lines[0][7..10],
        "HIJ",
        "Chars after erased region preserved"
    );
}

// ===========================================================================
// CSI: Insert / Delete line and character
// ===========================================================================

#[test]
fn conformance_il_insert_lines() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3");
    // Move to row 2, insert 1 line (IL)
    term.vt_write(b"\x1b[2;1H\x1b[L");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "LINE1", "Row 0 unaffected");
    assert!(lines[1].trim().is_empty(), "Inserted blank line at row 1");
    assert_eq!(lines[2], "LINE2", "LINE2 shifted down");
    assert_eq!(lines[3], "LINE3", "LINE3 shifted down");
}

#[test]
fn conformance_dl_delete_lines() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3\r\nLINE4");
    // Move to row 2, delete 1 line (DL)
    term.vt_write(b"\x1b[2;1H\x1b[M");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "LINE1", "Row 0 unaffected");
    assert_eq!(lines[1], "LINE3", "LINE3 moved up to row 1");
    assert_eq!(lines[2], "LINE4", "LINE4 moved up to row 2");
}

#[test]
fn conformance_ich_insert_characters() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"ABCDEF");
    // Move to col 3, insert 2 blank characters (ICH)
    term.vt_write(b"\x1b[1;4H\x1b[2@");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(&lines[0][..3], "ABC", "Chars before cursor preserved");
    assert_eq!(&lines[0][3..5], "  ", "2 blank chars inserted");
    assert_eq!(&lines[0][5..8], "DEF", "Existing chars shifted right");
}

#[test]
fn conformance_dch_delete_characters() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"ABCDEF");
    // Move to col 3, delete 2 characters (DCH)
    // Cursor at col 3 (0-based) = 'D', deleting 2 removes D and E
    term.vt_write(b"\x1b[1;4H\x1b[2P");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(
        lines[0], "ABCF",
        "DCH removes 2 chars at cursor (D,E), shifts F left"
    );
}

// ===========================================================================
// CSI: SGR (Select Graphic Rendition)
// ===========================================================================

#[test]
fn conformance_sgr_bold() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[1mB\x1b[0mN");
    let bold_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(bold_style.attrs.bold(), "Cell with SGR 1 should be bold");
    assert!(
        !normal_style.attrs.bold(),
        "Cell after SGR 0 should not be bold"
    );
}

#[test]
fn conformance_sgr_dim() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[2mD\x1b[0mN");
    let dim_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(
        dim_style.attrs.faint(),
        "Cell with SGR 2 should be faint/dim"
    );
    assert!(
        !normal_style.attrs.faint(),
        "Cell after SGR 0 should not be faint"
    );
}

#[test]
fn conformance_sgr_italic() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[3mI\x1b[0mN");
    let italic_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(
        italic_style.attrs.italic(),
        "Cell with SGR 3 should be italic"
    );
    assert!(
        !normal_style.attrs.italic(),
        "Cell after SGR 0 should not be italic"
    );
}

#[test]
fn conformance_sgr_underline() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[4mU\x1b[0mN");
    let ul_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    // underline field > 0 means some form of underline is active
    assert!(
        ul_style.underline > 0,
        "Cell with SGR 4 should have underline"
    );
    assert_eq!(
        normal_style.underline, 0,
        "Cell after SGR 0 should have no underline"
    );
}

#[test]
fn conformance_sgr_reverse() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[7mR\x1b[0mN");
    let rev_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(
        rev_style.attrs.inverse(),
        "Cell with SGR 7 should be inverse"
    );
    assert!(
        !normal_style.attrs.inverse(),
        "Cell after SGR 0 should not be inverse"
    );
}

#[test]
fn conformance_sgr_strikethrough() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[9mS\x1b[0mN");
    let strike_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(
        strike_style.attrs.strikethrough(),
        "Cell with SGR 9 should have strikethrough"
    );
    assert!(
        !normal_style.attrs.strikethrough(),
        "Cell after SGR 0 should not have strikethrough"
    );
}

// ===========================================================================
// CSI: Scroll
// ===========================================================================

#[test]
fn conformance_su_scroll_up() {
    let (mut term, mut rs) = make_term(80, 5);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3\r\nLINE4\r\nLINE5");
    // SU: scroll up 1 line (content moves up, blank line at bottom)
    term.vt_write(b"\x1b[S");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "LINE2", "After SU, row 0 should show LINE2");
    assert_eq!(lines[3], "LINE5", "After SU, row 3 should show LINE5");
    assert!(
        lines[4].trim().is_empty(),
        "After SU, last row should be blank"
    );
}

#[test]
fn conformance_sd_scroll_down() {
    let (mut term, mut rs) = make_term(80, 5);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3\r\nLINE4\r\nLINE5");
    // SD: scroll down 1 line (content moves down, blank line at top)
    term.vt_write(b"\x1b[T");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].trim().is_empty(),
        "After SD, first row should be blank"
    );
    assert_eq!(lines[1], "LINE1", "After SD, row 1 should show LINE1");
    assert_eq!(lines[4], "LINE4", "After SD, row 4 should show LINE4");
}

// ===========================================================================
// CSI: Tab stops
// ===========================================================================

#[test]
fn conformance_default_tab_stops() {
    let (mut term, _rs) = make_term(80, 24);
    // Default tab stops are at every 8 columns
    term.vt_write(b"\t");
    let (col, _) = term.cursor_position();
    assert_eq!(col, 8, "First tab should move to col 8");
}

#[test]
fn conformance_hts_set_tab_stop() {
    let (mut term, _rs) = make_term(80, 24);
    // Clear all tab stops, then set one at col 5
    term.vt_write(b"\x1b[3g"); // TBC: clear all tab stops
    term.vt_write(b"\x1b[1;6H"); // Move to col 6 (0-based: 5)
    term.vt_write(b"\x1bH"); // HTS: set tab stop at current position
    term.vt_write(b"\x1b[1;1H"); // Go home
    term.vt_write(b"\t"); // Tab should go to col 5
    let (col, _) = term.cursor_position();
    assert_eq!(col, 5, "Tab should stop at the HTS-set position (col 5)");
}

#[test]
fn conformance_tbc_clear_tab_stop() {
    let (mut term, _rs) = make_term(80, 24);
    // Move to col 8 (where default tab stop is) and clear it
    term.vt_write(b"\x1b[1;9H"); // Move to col 9 (0-based: 8)
    term.vt_write(b"\x1b[0g"); // TBC 0: clear tab stop at cursor
    term.vt_write(b"\x1b[1;1H"); // Go home
    term.vt_write(b"\t"); // Should skip col 8 and go to col 16
    let (col, _) = term.cursor_position();
    assert_eq!(
        col, 16,
        "Tab should skip cleared stop and go to next (col 16)"
    );
}

// ===========================================================================
// CSI: Mode set/reset (SM/RM)
// ===========================================================================

#[test]
fn conformance_dectcem_cursor_visibility() {
    let (mut term, _rs) = make_term(80, 24);
    // DECTCEM: CSI ?25l (hide), CSI ?25h (show)
    assert_eq!(
        term.mode_get(Mode::CURSOR_VISIBLE),
        Some(true),
        "Cursor visible by default"
    );
    term.vt_write(b"\x1b[?25l"); // hide
    assert_eq!(
        term.mode_get(Mode::CURSOR_VISIBLE),
        Some(false),
        "Cursor hidden after ?25l"
    );
    term.vt_write(b"\x1b[?25h"); // show
    assert_eq!(
        term.mode_get(Mode::CURSOR_VISIBLE),
        Some(true),
        "Cursor visible after ?25h"
    );
}

#[test]
fn conformance_bracketed_paste_mode() {
    let (mut term, _rs) = make_term(80, 24);
    assert_eq!(
        term.mode_get(Mode::BRACKETED_PASTE),
        Some(false),
        "Bracketed paste off by default"
    );
    term.vt_write(b"\x1b[?2004h"); // enable
    assert_eq!(
        term.mode_get(Mode::BRACKETED_PASTE),
        Some(true),
        "Bracketed paste on after ?2004h"
    );
    term.vt_write(b"\x1b[?2004l"); // disable
    assert_eq!(
        term.mode_get(Mode::BRACKETED_PASTE),
        Some(false),
        "Bracketed paste off after ?2004l"
    );
}

#[test]
fn conformance_alt_screen_mode() {
    let (mut term, _rs) = make_term(80, 24);
    assert!(!term.is_alternate_screen(), "Primary screen by default");
    term.vt_write(b"\x1b[?1049h"); // enable alt screen
    assert!(term.is_alternate_screen(), "Alt screen after ?1049h");
    term.vt_write(b"\x1b[?1049l"); // disable alt screen
    assert!(!term.is_alternate_screen(), "Primary screen after ?1049l");
}

#[test]
fn conformance_mouse_modes() {
    let (mut term, _rs) = make_term(80, 24);
    // Normal mouse tracking (1000)
    assert_eq!(term.mode_get(Mode::NORMAL_MOUSE), Some(false));
    term.vt_write(b"\x1b[?1000h");
    assert_eq!(term.mode_get(Mode::NORMAL_MOUSE), Some(true));
    term.vt_write(b"\x1b[?1000l");
    assert_eq!(term.mode_get(Mode::NORMAL_MOUSE), Some(false));

    // SGR mouse (1006)
    assert_eq!(term.mode_get(Mode::SGR_MOUSE), Some(false));
    term.vt_write(b"\x1b[?1006h");
    assert_eq!(term.mode_get(Mode::SGR_MOUSE), Some(true));
    term.vt_write(b"\x1b[?1006l");
    assert_eq!(term.mode_get(Mode::SGR_MOUSE), Some(false));
}

// ===========================================================================
// OSC: Title
// ===========================================================================

#[test]
fn conformance_osc_0_set_title() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b]0;My Terminal Title\x07");
    let title = term.take_title();
    assert_eq!(
        title.as_deref(),
        Some("My Terminal Title"),
        "OSC 0 should set title"
    );
}

#[test]
fn conformance_osc_2_set_title() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b]2;Window Title\x07");
    let title = term.take_title();
    assert_eq!(
        title.as_deref(),
        Some("Window Title"),
        "OSC 2 should set title"
    );
}

#[test]
fn conformance_osc_title_st_terminator() {
    let (mut term, _rs) = make_term(80, 24);
    // ST terminator: ESC \  (0x1b 0x5c)
    term.vt_write(b"\x1b]0;ST Title\x1b\\");
    let title = term.take_title();
    assert_eq!(
        title.as_deref(),
        Some("ST Title"),
        "OSC with ST terminator should work"
    );
}

// ===========================================================================
// OSC 52: Clipboard
// ===========================================================================

#[test]
fn conformance_osc_52_clipboard_write() {
    let (mut term, _rs) = make_term(80, 24);
    // OSC 52 ; c ; <base64-data> BEL
    // base64("Hello") = "SGVsbG8="
    term.vt_write(b"\x1b]52;c;SGVsbG8=\x07");
    // The terminal should process it without error
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (0, 0), "OSC 52 should not move cursor");
}

// ===========================================================================
// Device queries
// ===========================================================================

#[test]
fn conformance_da1_primary_device_attributes() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[c");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.starts_with("\x1b[?"),
        "DA1 response should start with CSI ?"
    );
    assert!(resp_str.ends_with('c'), "DA1 response should end with 'c'");
}

#[test]
fn conformance_da2_secondary_device_attributes() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[>c");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.starts_with("\x1b[>"),
        "DA2 response should start with CSI >"
    );
    assert!(resp_str.ends_with('c'), "DA2 response should end with 'c'");
}

#[test]
fn conformance_da3_tertiary_device_attributes() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[=c");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    // DA3 returns DCS ! | <unit-id> ST
    assert!(
        resp_str.starts_with("\x1bP!|"),
        "DA3 response should start with DCS ! |"
    );
    assert!(
        resp_str.ends_with("\x1b\\"),
        "DA3 response should end with ST"
    );
}

#[test]
fn conformance_xtversion() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[>q");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    // XTVERSION returns DCS >| <version> ST
    assert!(
        resp_str.contains(">|"),
        "XTVERSION response should contain >|"
    );
    assert!(
        resp_str.contains("horseshoe"),
        "XTVERSION should identify as horseshoe"
    );
}

#[test]
fn conformance_dsr_status_report() {
    let (mut term, _rs) = make_term(80, 24);
    // DSR 5n — device status report (should respond CSI 0 n = "OK")
    term.vt_write(b"\x1b[5n");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert_eq!(
        resp_str, "\x1b[0n",
        "DSR 5n should respond with CSI 0 n (OK)"
    );
}

#[test]
fn conformance_dsr_cursor_position_report() {
    let (mut term, _rs) = make_term(80, 24);
    // Move cursor to known position then query
    term.vt_write(b"\x1b[10;20H");
    term.vt_write(b"\x1b[6n"); // DSR 6n — cursor position report
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    // Should respond CSI row ; col R (1-based)
    assert_eq!(
        resp_str, "\x1b[10;20R",
        "DSR 6n should report cursor at row 10, col 20 (1-based)"
    );
}

#[test]
fn conformance_decid_identify() {
    let (mut term, _rs) = make_term(80, 24);
    // DECID: ESC Z — same as DA1
    term.vt_write(b"\x1bZ");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.starts_with("\x1b[?"),
        "DECID should respond like DA1"
    );
    assert!(
        resp_str.ends_with('c'),
        "DECID response should end with 'c'"
    );
}

// ===========================================================================
// Mode behavior: Alternate screen
// ===========================================================================

#[test]
fn conformance_alt_screen_preserves_primary() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"PRIMARY_CONTENT");
    // Enter alt screen
    term.vt_write(b"\x1b[?1049h");
    term.vt_write(b"ALT_CONTENT");
    let alt_lines = grid_text(&term, &mut rs);
    assert!(
        alt_lines[0].contains("ALT_CONTENT"),
        "Alt screen should show alt content"
    );

    // Exit alt screen — primary content should be restored
    term.vt_write(b"\x1b[?1049l");
    let primary_lines = grid_text(&term, &mut rs);
    assert!(
        primary_lines[0].contains("PRIMARY_CONTENT"),
        "Primary content should be restored after leaving alt screen"
    );
}

// ===========================================================================
// Mode behavior: Scroll region (DECSTBM)
// ===========================================================================

#[test]
fn conformance_decstbm_scroll_region() {
    let (mut term, mut rs) = make_term(80, 10);
    // Fill screen using CUP to avoid scrolling from trailing \r\n
    for i in 1..=10 {
        let line = format!("\x1b[{i};1HLINE{i:02}");
        term.vt_write(line.as_bytes());
    }
    let pre_lines = grid_text(&term, &mut rs);
    assert_eq!(pre_lines[0], "LINE01", "Pre-check: row 0 should be LINE01");

    // Set scroll region to rows 3-7 (1-based)
    term.vt_write(b"\x1b[3;7r");
    // Move to last row of scroll region and write a newline to trigger scroll
    term.vt_write(b"\x1b[7;1H");
    term.vt_write(b"\n");
    let lines = grid_text(&term, &mut rs);
    // Rows outside scroll region should be unaffected
    assert_eq!(
        lines[0], "LINE01",
        "Row above scroll region should be preserved"
    );
    assert_eq!(
        lines[1], "LINE02",
        "Row above scroll region should be preserved"
    );
    // LINE08 and beyond should also be unaffected
    assert_eq!(
        lines[7], "LINE08",
        "Row below scroll region should be preserved"
    );
}

// ===========================================================================
// Mode behavior: Auto-wrap (DECAWM)
// ===========================================================================

#[test]
fn conformance_decawm_wraps_at_margin() {
    let (mut term, mut rs) = make_term(10, 5);
    // DECAWM is on by default; write more chars than columns
    term.vt_write(b"1234567890WRAP");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "1234567890", "First row should have 10 chars");
    assert_eq!(lines[1], "WRAP", "Overflow should wrap to next row");
}

#[test]
fn conformance_decawm_off_no_wrap() {
    let (mut term, mut rs) = make_term(10, 5);
    // Disable auto-wrap
    term.vt_write(b"\x1b[?7l");
    term.vt_write(b"1234567890EXTRA");
    let lines = grid_text(&term, &mut rs);
    // With DECAWM off, writing past the margin overwrites the last cell
    assert!(
        lines[1].trim().is_empty(),
        "No wrap should occur with DECAWM off"
    );
    let (col, row) = term.cursor_position();
    assert_eq!(row, 0, "Cursor should stay on row 0");
    assert_eq!(col, 9, "Cursor should be at last column");
}

// ===========================================================================
// Mode behavior: Insert mode (IRM)
// ===========================================================================

#[test]
fn conformance_irm_insert_mode() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"ABCDEF");
    // Move to col 3, enable insert mode (SM 4), type characters
    term.vt_write(b"\x1b[1;4H");
    term.vt_write(b"\x1b[4h"); // SM 4: enable insert mode
    term.vt_write(b"XY");
    let lines = grid_text(&term, &mut rs);
    assert_eq!(
        lines[0], "ABCXYDEF",
        "IRM should insert chars, shifting existing text right"
    );
}

// ===========================================================================
// Character handling: UTF-8
// ===========================================================================

#[test]
fn conformance_utf8_multibyte_cursor() {
    let (mut term, _rs) = make_term(80, 24);
    // "e\u{0301}" is 2 bytes in UTF-8 but 1 column wide
    term.vt_write("é".as_bytes());
    let (col, _) = term.cursor_position();
    assert_eq!(col, 1, "Single-width UTF-8 char should advance cursor by 1");
}

#[test]
fn conformance_utf8_cjk_wide_char() {
    let (mut term, mut rs) = make_term(80, 24);
    // CJK character (U+4E16) is 2 cells wide
    term.vt_write("世".as_bytes());
    let (col, _) = term.cursor_position();
    assert_eq!(col, 2, "Wide CJK char should advance cursor by 2");
    assert!(
        cell_is_wide(&term, &mut rs, 0, 0),
        "CJK char cell should be marked wide"
    );
}

#[test]
fn conformance_utf8_emoji() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write("A".as_bytes());
    term.vt_write("❤".as_bytes());
    let (col, _) = term.cursor_position();
    // The cursor should have advanced past 'A' (1) + the emoji width
    assert!(col >= 2, "Emoji should advance cursor by at least 1 cell");
}

// ===========================================================================
// Character handling: Control characters
// ===========================================================================

#[test]
fn conformance_bel_triggers_bell() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x07"); // BEL
    assert!(term.take_bell(), "BEL (0x07) should trigger bell callback");
}

#[test]
fn conformance_bs_backspace() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"ABC");
    let (col_before, _) = term.cursor_position();
    assert_eq!(col_before, 3);
    term.vt_write(b"\x08"); // BS
    let (col_after, _) = term.cursor_position();
    assert_eq!(col_after, 2, "BS should move cursor back by 1");
}

#[test]
fn conformance_ht_horizontal_tab() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"AB\t");
    let (col, _) = term.cursor_position();
    assert_eq!(
        col, 8,
        "HT from col 2 should advance to next tab stop at col 8"
    );
}

#[test]
fn conformance_lf_line_feed() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"A\n");
    let (col, row) = term.cursor_position();
    assert_eq!(row, 1, "LF should move cursor down one row");
    // LF alone does not reset column in raw mode
    assert_eq!(col, 1, "LF should not change column");
}

#[test]
fn conformance_cr_carriage_return() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"ABCDEF\r");
    let (col, row) = term.cursor_position();
    assert_eq!(col, 0, "CR should return cursor to column 0");
    assert_eq!(row, 0, "CR should not change row");
}

#[test]
fn conformance_cr_lf_combination() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"Hello\r\n");
    let (col, row) = term.cursor_position();
    assert_eq!(col, 0, "CR+LF should return to column 0");
    assert_eq!(row, 1, "CR+LF should advance to next row");
}

// ===========================================================================
// Terminal reset
// ===========================================================================

#[test]
fn conformance_hard_reset() {
    let (mut term, _rs) = make_term(80, 24);
    // Change some state
    term.vt_write(b"\x1b[10;20H");
    term.vt_write(b"\x1b[?25l");
    term.vt_write(b"\x1b[?2004h");

    // Hard reset (RIS: ESC c)
    term.vt_write(b"\x1bc");
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (0, 0), "RIS should reset cursor to home");
    assert_eq!(
        term.mode_get(Mode::CURSOR_VISIBLE),
        Some(true),
        "RIS should show cursor"
    );
    assert_eq!(
        term.mode_get(Mode::BRACKETED_PASTE),
        Some(false),
        "RIS should disable bracketed paste"
    );
}

// ---- DECCKM (application cursor keys) conformance ----

/// DECCKM enable: CSI ? 1 h
#[test]
fn conformance_decckm_enable() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[?1h");
    assert_eq!(
        term.mode_get(Mode::DECCKM),
        Some(true),
        "DECCKM should be enabled after CSI ? 1 h"
    );
}

/// DECCKM disable: CSI ? 1 l
#[test]
fn conformance_decckm_disable() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[?1h"); // enable first
    term.vt_write(b"\x1b[?1l"); // then disable
    assert_eq!(
        term.mode_get(Mode::DECCKM),
        Some(false),
        "DECCKM should be disabled after CSI ? 1 l"
    );
}

/// DECCKM should be off by default.
#[test]
fn conformance_decckm_default_off() {
    let (term, _rs) = make_term(80, 24);
    assert_eq!(
        term.mode_get(Mode::DECCKM),
        Some(false),
        "DECCKM should be off by default"
    );
}

/// DECCKM should survive alt screen switch (tmux pattern).
#[test]
fn conformance_decckm_with_alt_screen() {
    let (mut term, _rs) = make_term(80, 24);
    // tmux sequence: alt screen on, then enable DECCKM
    term.vt_write(b"\x1b[?1049h"); // alt screen
    term.vt_write(b"\x1b[?1h"); // DECCKM

    assert!(term.is_alternate_screen(), "should be on alternate screen");
    assert_eq!(
        term.mode_get(Mode::DECCKM),
        Some(true),
        "DECCKM should be enabled on alt screen"
    );
}

/// Hard reset should disable DECCKM.
#[test]
fn conformance_decckm_reset_by_ris() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[?1h"); // enable
    assert_eq!(term.mode_get(Mode::DECCKM), Some(true));

    term.vt_write(b"\x1bc"); // RIS
    assert_eq!(
        term.mode_get(Mode::DECCKM),
        Some(false),
        "RIS should disable DECCKM"
    );
}

// ---- Alternate screen mode conformance ----

#[test]
fn conformance_alt_screen_default_off() {
    let (term, _rs) = make_term(80, 24);
    assert!(
        !term.is_alternate_screen(),
        "should not be on alternate screen by default"
    );
}

#[test]
fn conformance_alt_screen_enable() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[?1049h");
    assert!(
        term.is_alternate_screen(),
        "CSI ? 1049 h should enable alt screen"
    );
}

#[test]
fn conformance_alt_screen_disable() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[?1049h");
    term.vt_write(b"\x1b[?1049l");
    assert!(
        !term.is_alternate_screen(),
        "CSI ? 1049 l should disable alt screen"
    );
}

// ---- Scrollbar state conformance ----

#[test]
fn conformance_scrollbar_initial() {
    let (term, _rs) = make_term(80, 24);
    let scrollbar = term.scrollbar().expect("scrollbar should be available");
    // Initial terminal has no scrollback yet
    assert_eq!(scrollbar.offset, 0, "initial scrollbar offset should be 0");
}

#[test]
fn conformance_scrollbar_after_content() {
    let (mut term, _rs) = make_term(80, 24);
    // Write enough lines to push content into scrollback
    for i in 0..50 {
        let line = format!("line {i}\r\n");
        term.vt_write(line.as_bytes());
    }
    let scrollbar = term
        .scrollbar()
        .expect("scrollbar should be available after content");
    assert!(
        scrollbar.total > scrollbar.len,
        "total ({}) should exceed visible len ({}) after writing 50 lines",
        scrollbar.total,
        scrollbar.len
    );
}

#[test]
fn conformance_scroll_viewport_delta_symmetry() {
    let (mut term, _rs) = make_term(80, 24);
    // Write enough lines to create scrollback
    for i in 0..100 {
        let line = format!("line {i}\r\n");
        term.vt_write(line.as_bytes());
    }

    // Record the initial (bottom) offset
    let sb_initial = term.scrollbar().expect("scrollbar");
    let initial_offset = sb_initial.offset;

    // Scroll up 10 lines
    term.scroll_viewport_delta(-10);
    let sb_after_up = term.scrollbar().expect("scrollbar after up");

    // The offset should have changed after scrolling up
    assert_ne!(
        sb_after_up.offset, initial_offset,
        "scrolling up should change the offset"
    );

    // Scroll down 10 lines (this was the broken direction)
    term.scroll_viewport_delta(10);
    let sb_after_down = term.scrollbar().expect("scrollbar after down");

    // After scrolling up 10 and down 10, we should be back at the original position
    assert_eq!(
        sb_after_down.offset, initial_offset,
        "scroll up 10 + down 10 should return to original offset {initial_offset}, got {}",
        sb_after_down.offset
    );
}

// ===========================================================================
// Ported from Ghostty stream_terminal.zig: Basic print
// ===========================================================================

#[test]
fn conformance_basic_print() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"Hello");
    let (col, row) = term.cursor_position();
    assert_eq!(
        (col, row),
        (5, 0),
        "Cursor should be at col 5 after printing 'Hello'"
    );
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "Hello", "First row should contain 'Hello'");
}

// ===========================================================================
// Ported from Ghostty: Cursor save and restore (DECSC/DECRC)
// ===========================================================================

#[test]
fn conformance_decsc_decrc_save_restore() {
    let (mut term, _rs) = make_term(80, 24);
    // Move to row 10, col 15 (1-based), then save cursor
    term.vt_write(b"\x1b[10;15H");
    term.vt_write(b"\x1b7"); // DECSC: save cursor
    // Move somewhere else
    term.vt_write(b"\x1b[1;1H");
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (0, 0), "Cursor should be at home after move");
    // Restore cursor
    term.vt_write(b"\x1b8"); // DECRC: restore cursor
    let (col, row) = term.cursor_position();
    assert_eq!(
        (col, row),
        (14, 9),
        "Cursor should be restored to col 14, row 9 (0-based)"
    );
}

#[test]
fn conformance_decsc_csi_save_restore() {
    let (mut term, _rs) = make_term(80, 24);
    // CSI s / CSI u variant of save/restore
    term.vt_write(b"\x1b[5;20H");
    term.vt_write(b"\x1b[s"); // CSI s: save cursor
    term.vt_write(b"\x1b[1;1H");
    term.vt_write(b"\x1b[u"); // CSI u: restore cursor
    let (col, row) = term.cursor_position();
    assert_eq!(
        (col, row),
        (19, 4),
        "CSI s/u should save and restore cursor position"
    );
}

// ===========================================================================
// Ported from Ghostty: DECALN screen alignment
// ===========================================================================

#[test]
fn conformance_decaln_screen_alignment() {
    let (mut term, mut rs) = make_term(10, 5);
    // ESC # 8 fills screen with 'E'
    term.vt_write(b"\x1b#8");
    let lines = grid_text(&term, &mut rs);
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line, "EEEEEEEEEE",
            "Row {i} should be filled with 'E' after DECALN"
        );
    }
    // DECALN resets cursor to home
    let (col, row) = term.cursor_position();
    assert_eq!((col, row), (0, 0), "DECALN should reset cursor to home");
}

// ===========================================================================
// Ported from Ghostty: DEC Special Graphics charset
// ===========================================================================

#[test]
fn conformance_charset_dec_special_graphics() {
    let (mut term, mut rs) = make_term(80, 24);
    // ESC ( 0 activates DEC Special Graphics charset
    // In this charset, '`' (0x60) maps to diamond (◆)
    term.vt_write(b"\x1b(0");
    term.vt_write(b"`"); // diamond
    term.vt_write(b"\x1b(B"); // back to ASCII
    let lines = grid_text(&term, &mut rs);
    // The exact Unicode mapping: ` = U+25C6 (◆)
    assert!(
        lines[0].starts_with('\u{25C6}'),
        "DEC Special Graphics '`' should map to diamond (◆), got: {:?}",
        &lines[0]
    );
}

#[test]
fn conformance_charset_dec_line_drawing() {
    let (mut term, mut rs) = make_term(80, 24);
    // 'j' in DEC Special Graphics = ┘ (U+2518), 'k' = ┐ (U+2510), 'l' = ┌ (U+250C), 'm' = └ (U+2514)
    term.vt_write(b"\x1b(0");
    term.vt_write(b"jklm");
    term.vt_write(b"\x1b(B");
    let lines = grid_text(&term, &mut rs);
    let chars: Vec<char> = lines[0].chars().collect();
    assert_eq!(chars[0], '\u{2518}', "DEC 'j' should map to ┘");
    assert_eq!(chars[1], '\u{2510}', "DEC 'k' should map to ┐");
    assert_eq!(chars[2], '\u{250C}', "DEC 'l' should map to ┌");
    assert_eq!(chars[3], '\u{2514}', "DEC 'm' should map to └");
}

// ===========================================================================
// Ported from Ghostty: DECRQM mode query
// ===========================================================================

#[test]
fn conformance_decrqm_wraparound_enabled() {
    let (mut term, _rs) = make_term(80, 24);
    // DECRQM: CSI ? 7 $ p queries wraparound mode (DECAWM)
    // Default: enabled → response CSI ? 7 ; 1 $ y
    term.vt_write(b"\x1b[?7$p");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.contains("?7;1$y") || resp_str.contains("?7;3$y"),
        "DECRQM for DECAWM should report set (1) or permanently set (3), got: {resp_str:?}"
    );
}

#[test]
fn conformance_decrqm_wraparound_disabled() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[?7l"); // disable DECAWM
    term.vt_write(b"\x1b[?7$p");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.contains("?7;2$y") || resp_str.contains("?7;4$y"),
        "DECRQM for disabled DECAWM should report reset (2) or permanently reset (4), got: {resp_str:?}"
    );
}

// ===========================================================================
// Ported from Ghostty: DSR cursor position with origin mode
// ===========================================================================

#[test]
fn conformance_dsr_cursor_position_origin_mode() {
    let (mut term, _rs) = make_term(80, 24);
    // Set scroll region rows 5-20, enable origin mode, move cursor
    term.vt_write(b"\x1b[5;20r"); // DECSTBM: scroll region rows 5-20
    term.vt_write(b"\x1b[?6h"); // DECOM: enable origin mode
    term.vt_write(b"\x1b[3;5H"); // Move to row 3, col 5 within region
    term.vt_write(b"\x1b[6n"); // DSR: request cursor position
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    // With origin mode, position is relative to scroll region
    assert_eq!(
        resp_str, "\x1b[3;5R",
        "DSR with origin mode should report position relative to scroll region"
    );
    // Clean up origin mode
    term.vt_write(b"\x1b[?6l");
}

// ===========================================================================
// Ported from Ghostty: Malformed CSI handling
// ===========================================================================

#[test]
fn conformance_malformed_csi_no_crash() {
    let (mut term, mut rs) = make_term(80, 24);
    // CSI ? W with intermediate but no params — should not crash
    term.vt_write(b"\x1b[?W");
    // Verify terminal still functional after malformed sequence
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].contains("OK"),
        "Terminal should remain functional after malformed CSI"
    );
}

#[test]
fn conformance_malformed_csi_excessive_params() {
    let (mut term, mut rs) = make_term(80, 24);
    // Send a CSI with way too many parameters
    term.vt_write(b"\x1b[1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19;20m");
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].contains("OK"),
        "Terminal should handle excessive CSI params gracefully"
    );
}

// ===========================================================================
// Ported from Ghostty: Empty window title
// ===========================================================================

#[test]
fn conformance_osc_2_empty_title() {
    let (mut term, _rs) = make_term(80, 24);
    // First set a title, then clear it
    term.vt_write(b"\x1b]2;My Title\x1b\\");
    let title = term.take_title();
    assert_eq!(title.as_deref(), Some("My Title"));
    // Set empty title
    term.vt_write(b"\x1b]2;\x1b\\");
    let title2 = term.take_title();
    // Empty title should be reported (either None or Some(""))
    assert!(
        title2.as_deref() == Some("") || title2.is_none(),
        "Empty OSC 2 should clear or empty the title, got: {title2:?}"
    );
}

// ===========================================================================
// Ported from Ghostty: Kitty keyboard protocol query
// ===========================================================================

#[test]
fn conformance_kitty_keyboard_query() {
    let (mut term, _rs) = make_term(80, 24);
    // CSI ? u queries current kitty keyboard flags
    term.vt_write(b"\x1b[?u");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    // Default should be flags=0
    assert_eq!(
        resp_str, "\x1b[?0u",
        "Kitty keyboard query should report flags=0 by default"
    );
}

#[test]
fn conformance_kitty_keyboard_push_query() {
    let (mut term, _rs) = make_term(80, 24);
    // Push flags=1 (disambiguate), then query
    term.vt_write(b"\x1b[>1u");
    term.vt_write(b"\x1b[?u");
    let resp = term.take_pty_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert_eq!(
        resp_str, "\x1b[?1u",
        "After pushing flags=1, query should report 1"
    );
}

// ===========================================================================
// SGR: Additional attributes (ported from Ghostty sgr.zig)
// ===========================================================================

#[test]
fn conformance_sgr_blink() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[5mB\x1b[0mN");
    let blink_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(blink_style.attrs.blink(), "SGR 5 should set blink");
    assert!(!normal_style.attrs.blink(), "SGR 0 should clear blink");
}

#[test]
fn conformance_sgr_invisible() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[8mI\x1b[0mN");
    let inv_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(inv_style.attrs.invisible(), "SGR 8 should set invisible");
    assert!(
        !normal_style.attrs.invisible(),
        "SGR 0 should clear invisible"
    );
}

#[test]
fn conformance_sgr_overline() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[53mO\x1b[0mN");
    let ol_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    let normal_style = cell_style_at(&term, &mut rs, 0, 1).unwrap();
    assert!(ol_style.attrs.overline(), "SGR 53 should set overline");
    assert!(
        !normal_style.attrs.overline(),
        "SGR 0 should clear overline"
    );
}

// ===========================================================================
// SGR: Attribute-specific resets (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_sgr_reset_bold_22() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[1m\x1b[22mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(!style.attrs.bold(), "SGR 22 should reset bold");
    assert!(!style.attrs.faint(), "SGR 22 should also reset faint");
}

#[test]
fn conformance_sgr_reset_italic_23() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[3m\x1b[23mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(!style.attrs.italic(), "SGR 23 should reset italic");
}

#[test]
fn conformance_sgr_reset_underline_24() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[4m\x1b[24mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.underline, 0, "SGR 24 should reset underline");
}

#[test]
fn conformance_sgr_reset_blink_25() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[5m\x1b[25mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(!style.attrs.blink(), "SGR 25 should reset blink");
}

#[test]
fn conformance_sgr_reset_inverse_27() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[7m\x1b[27mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(!style.attrs.inverse(), "SGR 27 should reset inverse");
}

#[test]
fn conformance_sgr_reset_invisible_28() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[8m\x1b[28mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(!style.attrs.invisible(), "SGR 28 should reset invisible");
}

#[test]
fn conformance_sgr_reset_strikethrough_29() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[9m\x1b[29mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(
        !style.attrs.strikethrough(),
        "SGR 29 should reset strikethrough"
    );
}

#[test]
fn conformance_sgr_reset_overline_55() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[53m\x1b[55mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(!style.attrs.overline(), "SGR 55 should reset overline");
}

#[test]
fn conformance_sgr_reset_all_0() {
    let (mut term, mut rs) = make_term(80, 24);
    // Set many attributes, then reset all with SGR 0
    term.vt_write(b"\x1b[1;3;4;7;9m\x1b[0mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(!style.attrs.bold(), "SGR 0 should clear bold");
    assert!(!style.attrs.italic(), "SGR 0 should clear italic");
    assert_eq!(style.underline, 0, "SGR 0 should clear underline");
    assert!(!style.attrs.inverse(), "SGR 0 should clear inverse");
    assert!(
        !style.attrs.strikethrough(),
        "SGR 0 should clear strikethrough"
    );
}

// ===========================================================================
// SGR: Underline styles (ported from Ghostty sgr.zig)
// ===========================================================================

#[test]
fn conformance_sgr_underline_double() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 21 = double underline
    term.vt_write(b"\x1b[21mA\x1b[0mB");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.underline, 2, "SGR 21 should set double underline (2)");
}

#[test]
fn conformance_sgr_underline_curly() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 4:3 = curly underline (colon sub-parameter)
    term.vt_write(b"\x1b[4:3mA\x1b[0mB");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.underline, 3, "SGR 4:3 should set curly underline (3)");
}

#[test]
fn conformance_sgr_underline_dotted() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[4:4mA\x1b[0mB");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.underline, 4,
        "SGR 4:4 should set dotted underline (4)"
    );
}

#[test]
fn conformance_sgr_underline_dashed() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[4:5mA\x1b[0mB");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.underline, 5,
        "SGR 4:5 should set dashed underline (5)"
    );
}

#[test]
fn conformance_sgr_underline_none_subparam() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 4:0 = no underline via sub-parameter
    term.vt_write(b"\x1b[4m"); // enable single underline
    term.vt_write(b"\x1b[4:0mA\x1b[0mB"); // disable via sub-param
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.underline, 0, "SGR 4:0 should disable underline");
}

#[test]
fn conformance_sgr_underline_single_subparam() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[4:1mA\x1b[0mB");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.underline, 1,
        "SGR 4:1 should set single underline (1)"
    );
}

#[test]
fn conformance_sgr_underline_double_subparam() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[4:2mA\x1b[0mB");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.underline, 2,
        "SGR 4:2 should set double underline (2)"
    );
}

// ===========================================================================
// SGR: 8-color foreground/background (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_sgr_fg_8color() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 31 = red foreground (palette index 1)
    term.vt_write(b"\x1b[31mR\x1b[0mN");
    let red_style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(red_style.fg_tag, 1, "SGR 31 should set fg to palette mode");
    assert_eq!(
        red_style.fg_palette, 1,
        "SGR 31 should set fg palette index 1 (red)"
    );
}

#[test]
fn conformance_sgr_bg_8color() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 43 = yellow background (palette index 3)
    term.vt_write(b"\x1b[43mY\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.bg_tag, 1, "SGR 43 should set bg to palette mode");
    assert_eq!(
        style.bg_palette, 3,
        "SGR 43 should set bg palette index 3 (yellow)"
    );
}

#[test]
fn conformance_sgr_bright_fg() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 90 = bright black foreground (palette index 8)
    term.vt_write(b"\x1b[90mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.fg_tag, 1, "SGR 90 should set fg to palette mode");
    assert_eq!(
        style.fg_palette, 8,
        "SGR 90 should set fg palette index 8 (bright black)"
    );
}

#[test]
fn conformance_sgr_bright_bg() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 103 = bright yellow background (palette index 11)
    term.vt_write(b"\x1b[103mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.bg_tag, 1, "SGR 103 should set bg to palette mode");
    assert_eq!(
        style.bg_palette, 11,
        "SGR 103 should set bg palette index 11 (bright yellow)"
    );
}

// ===========================================================================
// SGR: 256-color (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_sgr_256_fg() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 38;5;161 = 256-color fg index 161
    term.vt_write(b"\x1b[38;5;161mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.fg_tag, 1, "SGR 38;5;N should set fg to palette mode");
    assert_eq!(
        style.fg_palette, 161,
        "SGR 38;5;161 should set fg palette index 161"
    );
}

#[test]
fn conformance_sgr_256_bg() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 48;5;236 = 256-color bg index 236
    term.vt_write(b"\x1b[48;5;236mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.bg_tag, 1, "SGR 48;5;N should set bg to palette mode");
    assert_eq!(
        style.bg_palette, 236,
        "SGR 48;5;236 should set bg palette index 236"
    );
}

// ===========================================================================
// SGR: 24-bit RGB color (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_sgr_rgb_fg() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 38;2;40;44;52 = RGB foreground
    term.vt_write(b"\x1b[38;2;40;44;52mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.fg_tag, 2, "SGR 38;2;R;G;B should set fg to RGB mode");
    assert_eq!(
        style.fg_rgb,
        (40, 44, 52),
        "SGR 38;2;40;44;52 should set fg RGB"
    );
}

#[test]
fn conformance_sgr_rgb_bg() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 48;2;100;200;50 = RGB background
    term.vt_write(b"\x1b[48;2;100;200;50mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.bg_tag, 2, "SGR 48;2;R;G;B should set bg to RGB mode");
    assert_eq!(
        style.bg_rgb,
        (100, 200, 50),
        "SGR 48;2;100;200;50 should set bg RGB"
    );
}

#[test]
fn conformance_sgr_rgb_bg_colon() {
    let (mut term, mut rs) = make_term(80, 24);
    // Colon-separated variant: SGR 48:2:1:2:3 (no colorspace param in 5-param form)
    term.vt_write(b"\x1b[48:2:1:2:3mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.bg_tag, 2,
        "SGR 48:2:R:G:B (colon) should set bg to RGB mode"
    );
    assert_eq!(style.bg_rgb, (1, 2, 3), "SGR 48:2:1:2:3 should set bg RGB");
}

// ===========================================================================
// SGR: Default color reset (39, 49)
// ===========================================================================

#[test]
fn conformance_sgr_default_fg_39() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[31m\x1b[39mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.fg_tag, 0, "SGR 39 should reset fg to default (NONE)");
}

#[test]
fn conformance_sgr_default_bg_49() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[41m\x1b[49mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.bg_tag, 0, "SGR 49 should reset bg to default (NONE)");
}

// ===========================================================================
// SGR: Underline color (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_sgr_underline_color_rgb() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 58:2:R:G:B = underline color (colon form, no colorspace)
    term.vt_write(b"\x1b[58:2:255:128:0mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.underline_color_tag, 2,
        "SGR 58:2:R:G:B should set underline color to RGB"
    );
    assert_eq!(
        style.underline_color_rgb,
        (255, 128, 0),
        "Underline color RGB should match"
    );
}

#[test]
fn conformance_sgr_underline_color_256() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 58;5;9 = underline color palette index 9
    term.vt_write(b"\x1b[58;5;9mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.underline_color_tag, 1,
        "SGR 58;5;N should set underline color to palette"
    );
    assert_eq!(
        style.underline_color_palette, 9,
        "SGR 58;5;9 should set underline color palette index 9"
    );
}

#[test]
fn conformance_sgr_reset_underline_color_59() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b[58;5;9m\x1b[59mA");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(
        style.underline_color_tag, 0,
        "SGR 59 should reset underline color to NONE"
    );
}

// ===========================================================================
// SGR: Combined multi-attribute sequences (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_sgr_combined_bold_italic_underline() {
    let (mut term, mut rs) = make_term(80, 24);
    // Multiple SGR params in one sequence
    term.vt_write(b"\x1b[1;3;4mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert!(style.attrs.bold(), "Combined SGR should set bold");
    assert!(style.attrs.italic(), "Combined SGR should set italic");
    assert!(style.underline > 0, "Combined SGR should set underline");
}

#[test]
fn conformance_sgr_kakoune_style() {
    let (mut term, mut rs) = make_term(80, 24);
    // Real-world Kakoune editor sequence: curly underline + fg RGB + underline color
    // 4:3 = curly underline, 38;2;175;175;215 = fg RGB, 58:2:0:190:80:70 = underline color
    term.vt_write(b"\x1b[4:3;38;2;175;175;215mA\x1b[0mN");
    let style = cell_style_at(&term, &mut rs, 0, 0).unwrap();
    assert_eq!(style.underline, 3, "Kakoune: should have curly underline");
    assert_eq!(style.fg_tag, 2, "Kakoune: fg should be RGB");
    assert_eq!(
        style.fg_rgb,
        (175, 175, 215),
        "Kakoune: fg RGB should match"
    );
}

// ===========================================================================
// SGR: Robustness / edge cases (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_sgr_missing_256_color_index() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 38;5 with no index — should not crash
    term.vt_write(b"\x1b[38;5mA");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].contains('A'),
        "Terminal should handle missing 256-color index gracefully"
    );
}

#[test]
fn conformance_sgr_missing_rgb_values() {
    let (mut term, mut rs) = make_term(80, 24);
    // SGR 38;2;44;52 with only 2 of 3 RGB values
    term.vt_write(b"\x1b[38;2;44;52mA");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].contains('A'),
        "Terminal should handle incomplete RGB params gracefully"
    );
}

// ===========================================================================
// Ported from Ghostty: OSC 4 palette colors
// ===========================================================================

#[test]
fn conformance_osc4_set_palette_color() {
    let (mut term, mut rs) = make_term(80, 24);
    // OSC 4;0;rgb:ff/00/00 ST — set palette index 0 to red
    term.vt_write(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
    let _ = rs.update(term.inner());
    let colors = rs.colors();
    assert_eq!(
        colors.palette[0],
        (255, 0, 0),
        "OSC 4 should set palette[0] to red"
    );
}

#[test]
fn conformance_osc4_reset_palette_color() {
    let (mut term, mut rs) = make_term(80, 24);
    let _ = rs.update(term.inner());
    let original = rs.colors().palette[0];
    // Set, then reset
    term.vt_write(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
    term.vt_write(b"\x1b]104;0\x1b\\");
    let _ = rs.update(term.inner());
    let colors = rs.colors();
    assert_eq!(
        colors.palette[0], original,
        "OSC 104 should reset palette[0] to original"
    );
}

#[test]
fn conformance_osc104_reset_all_palette() {
    let (mut term, mut rs) = make_term(80, 24);
    let _ = rs.update(term.inner());
    let original_5 = rs.colors().palette[5];
    // Modify two entries
    term.vt_write(b"\x1b]4;0;rgb:ff/00/00\x1b\\");
    term.vt_write(b"\x1b]4;5;rgb:00/ff/00\x1b\\");
    // Reset all
    term.vt_write(b"\x1b]104\x1b\\");
    let _ = rs.update(term.inner());
    let colors = rs.colors();
    assert_eq!(
        colors.palette[5], original_5,
        "OSC 104 (no param) should reset all palette entries"
    );
}

// ===========================================================================
// Ported from Ghostty: OSC 10/11/12 dynamic colors
// ===========================================================================

#[test]
fn conformance_osc10_set_foreground() {
    let (mut term, mut rs) = make_term(80, 24);
    // OSC 10 sets dynamic foreground; verify terminal stays functional
    // (color queries require effect callbacks not available in test harness)
    term.vt_write(b"\x1b]10;rgb:ff/00/00\x1b\\");
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(lines[0].contains("OK"), "Terminal should work after OSC 10");
}

#[test]
fn conformance_osc11_set_background() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b]11;rgb:00/ff/00\x1b\\");
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(lines[0].contains("OK"), "Terminal should work after OSC 11");
}

#[test]
fn conformance_osc12_set_cursor_color() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b]12;rgb:00/00/ff\x1b\\");
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(lines[0].contains("OK"), "Terminal should work after OSC 12");
}

#[test]
fn conformance_osc110_reset_foreground() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b]10;rgb:ff/00/00\x1b\\");
    term.vt_write(b"\x1b]110\x1b\\");
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].contains("OK"),
        "Terminal should work after OSC 110 reset"
    );
}

#[test]
fn conformance_osc111_reset_background() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b]11;rgb:00/ff/00\x1b\\");
    term.vt_write(b"\x1b]111\x1b\\");
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].contains("OK"),
        "Terminal should work after OSC 111 reset"
    );
}

#[test]
fn conformance_osc112_reset_cursor_color() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"\x1b]12;rgb:00/00/ff\x1b\\");
    term.vt_write(b"\x1b]112\x1b\\");
    term.vt_write(b"OK");
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].contains("OK"),
        "Terminal should work after OSC 112 reset"
    );
}

// ===========================================================================
// Mode tests: additional modes (ported from Ghostty)
// ===========================================================================

#[test]
fn conformance_origin_mode() {
    let (mut term, _rs) = make_term(80, 24);
    // Set scroll region, enable origin mode
    term.vt_write(b"\x1b[5;20r"); // rows 5-20
    term.vt_write(b"\x1b[?6h"); // enable origin mode
    // CUP 1;1 should go to top of scroll region (row 4 absolute, row 0 relative)
    term.vt_write(b"\x1b[1;1H");
    let (col, row) = term.cursor_position();
    assert_eq!(
        (col, row),
        (0, 4),
        "With origin mode, CUP 1;1 should go to top of scroll region"
    );
    // Clean up
    term.vt_write(b"\x1b[?6l");
    term.vt_write(b"\x1b[r"); // reset scroll region
}

#[test]
fn conformance_reverse_wrap_mode() {
    let (mut term, _rs) = make_term(10, 5);
    // Enable reverse wrap (DEC mode 45)
    term.vt_write(b"\x1b[?45h");
    assert_eq!(
        term.mode_get(Mode::REVERSE_WRAP),
        Some(true),
        "Reverse wrap should be enabled"
    );
    term.vt_write(b"\x1b[?45l");
    assert_eq!(
        term.mode_get(Mode::REVERSE_WRAP),
        Some(false),
        "Reverse wrap should be disabled"
    );
}

#[test]
fn conformance_focus_event_mode() {
    let (mut term, _rs) = make_term(80, 24);
    assert_eq!(term.mode_get(Mode::FOCUS_EVENT), Some(false));
    term.vt_write(b"\x1b[?1004h");
    assert_eq!(
        term.mode_get(Mode::FOCUS_EVENT),
        Some(true),
        "Focus event mode should be enabled"
    );
    term.vt_write(b"\x1b[?1004l");
    assert_eq!(
        term.mode_get(Mode::FOCUS_EVENT),
        Some(false),
        "Focus event mode should be disabled"
    );
}

#[test]
fn conformance_sync_output_mode() {
    let (mut term, _rs) = make_term(80, 24);
    assert_eq!(term.mode_get(Mode::SYNC_OUTPUT), Some(false));
    term.vt_write(b"\x1b[?2026h");
    assert_eq!(
        term.mode_get(Mode::SYNC_OUTPUT),
        Some(true),
        "Sync output mode should be enabled"
    );
    term.vt_write(b"\x1b[?2026l");
    assert_eq!(
        term.mode_get(Mode::SYNC_OUTPUT),
        Some(false),
        "Sync output mode should be disabled"
    );
}

// ===========================================================================
// Ported from Ghostty: Full reset clears all state
// ===========================================================================

#[test]
fn conformance_ris_clears_scroll_region() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[5;20r"); // set scroll region
    term.vt_write(b"\x1b[?6h"); // enable origin mode
    term.vt_write(b"\x1b[?7l"); // disable autowrap
    term.vt_write(b"\x1bc"); // RIS
    // Origin mode should be off, so CUP goes to absolute position
    term.vt_write(b"\x1b[1;1H");
    let (col, row) = term.cursor_position();
    assert_eq!(
        (col, row),
        (0, 0),
        "After RIS, CUP 1;1 should go to absolute (0,0)"
    );
    // Autowrap should be re-enabled
    assert_eq!(
        term.mode_get(Mode::WRAPAROUND),
        Some(true),
        "RIS should re-enable autowrap"
    );
}

// ===========================================================================
// Ported from Ghostty: Resize
// ===========================================================================

#[test]
fn conformance_resize_preserves_content() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"Hello World");
    let _ = term.resize(40, 12);
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].starts_with("Hello World"),
        "Content should survive resize"
    );
}

#[test]
fn conformance_resize_cursor_clamp() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[24;80H"); // Move to bottom-right
    let _ = term.resize(40, 12);
    let (col, row) = term.cursor_position();
    assert!(col < 40, "Cursor column should be within new width");
    assert!(row < 12, "Cursor row should be within new height");
}

// ===========================================================================
// CSI: Cursor line-relative movement (CNL, CPL)
// ===========================================================================

#[test]
fn conformance_cnl_cursor_next_line() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[5;20H"); // row 5, col 20
    term.vt_write(b"\x1b[3E"); // CNL: cursor next line, 3 down
    let (col, row) = term.cursor_position();
    assert_eq!(row, 7, "CNL 3 from row 4 should go to row 7");
    assert_eq!(col, 0, "CNL should reset column to 0");
}

#[test]
fn conformance_cpl_cursor_preceding_line() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[10;20H"); // row 10, col 20
    term.vt_write(b"\x1b[3F"); // CPL: cursor preceding line, 3 up
    let (col, row) = term.cursor_position();
    assert_eq!(row, 6, "CPL 3 from row 9 should go to row 6");
    assert_eq!(col, 0, "CPL should reset column to 0");
}

// ===========================================================================
// CSI: CHA (Cursor Character Absolute) and VPA (Line Position Absolute)
// ===========================================================================

#[test]
fn conformance_cha_cursor_character_absolute() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[5;20H"); // Move somewhere
    term.vt_write(b"\x1b[10G"); // CHA: move to column 10 (1-based)
    let (col, row) = term.cursor_position();
    assert_eq!(col, 9, "CHA 10 should move to column 9 (0-based)");
    assert_eq!(row, 4, "CHA should not change row");
}

#[test]
fn conformance_vpa_line_position_absolute() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[5;20H"); // Move somewhere
    term.vt_write(b"\x1b[10d"); // VPA: move to row 10 (1-based)
    let (col, row) = term.cursor_position();
    assert_eq!(row, 9, "VPA 10 should move to row 9 (0-based)");
    assert_eq!(col, 19, "VPA should not change column");
}

// ===========================================================================
// CSI: REP (Repeat Character)
// ===========================================================================

#[test]
fn conformance_rep_repeat_character() {
    let (mut term, mut rs) = make_term(80, 24);
    term.vt_write(b"A\x1b[4b"); // Print 'A', then REP 4 (repeat last char 4 times)
    let lines = grid_text(&term, &mut rs);
    assert_eq!(
        &lines[0][..5],
        "AAAAA",
        "REP 4 after 'A' should produce 'AAAAA'"
    );
}

// ===========================================================================
// Reverse index (RI)
// ===========================================================================

#[test]
fn conformance_ri_reverse_index() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[3;1H"); // row 3
    term.vt_write(b"\x1bM"); // RI: reverse index (move up)
    let (col, row) = term.cursor_position();
    assert_eq!(row, 1, "RI should move cursor up one row");
    assert_eq!(col, 0, "RI should not change column");
}

#[test]
fn conformance_ri_at_top_scrolls_down() {
    let (mut term, mut rs) = make_term(80, 5);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3");
    term.vt_write(b"\x1b[1;1H"); // go to top
    term.vt_write(b"\x1bM"); // RI at top should scroll content down
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].trim().is_empty(),
        "RI at top should insert blank line at top"
    );
    assert_eq!(lines[1], "LINE1", "Previous top line should shift down");
}

// ===========================================================================
// NEL (Next Line)
// ===========================================================================

#[test]
fn conformance_nel_next_line() {
    let (mut term, _rs) = make_term(80, 24);
    term.vt_write(b"\x1b[1;20H"); // col 20
    term.vt_write(b"\x1bE"); // NEL: next line
    let (col, row) = term.cursor_position();
    assert_eq!(row, 1, "NEL should advance to next row");
    assert_eq!(col, 0, "NEL should reset column to 0");
}

// ===========================================================================
// Linefeed mode (LNM)
// ===========================================================================

#[test]
fn conformance_linefeed_mode() {
    let (mut term, _rs) = make_term(80, 24);
    assert_eq!(
        term.mode_get(Mode::LINEFEED),
        Some(false),
        "Linefeed mode should be off by default"
    );
    term.vt_write(b"\x1b[20h"); // Enable LNM (ANSI mode 20)
    assert_eq!(
        term.mode_get(Mode::LINEFEED),
        Some(true),
        "LNM should be enabled after SM 20"
    );
    term.vt_write(b"\x1b[20l"); // Disable
    assert_eq!(
        term.mode_get(Mode::LINEFEED),
        Some(false),
        "LNM should be disabled after RM 20"
    );
}

// ===========================================================================
// Scroll multiple lines (SU/SD with count)
// ===========================================================================

#[test]
fn conformance_su_scroll_up_multiple() {
    let (mut term, mut rs) = make_term(80, 5);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3\r\nLINE4\r\nLINE5");
    term.vt_write(b"\x1b[3S"); // SU 3: scroll up 3 lines
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "LINE4", "After SU 3, row 0 should be LINE4");
    assert_eq!(lines[1], "LINE5", "After SU 3, row 1 should be LINE5");
    assert!(
        lines[2].trim().is_empty(),
        "After SU 3, row 2 should be blank"
    );
}

#[test]
fn conformance_sd_scroll_down_multiple() {
    let (mut term, mut rs) = make_term(80, 5);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3\r\nLINE4\r\nLINE5");
    term.vt_write(b"\x1b[2T"); // SD 2: scroll down 2 lines
    let lines = grid_text(&term, &mut rs);
    assert!(
        lines[0].trim().is_empty(),
        "After SD 2, row 0 should be blank"
    );
    assert!(
        lines[1].trim().is_empty(),
        "After SD 2, row 1 should be blank"
    );
    assert_eq!(lines[2], "LINE1", "After SD 2, row 2 should be LINE1");
}

// ===========================================================================
// IL/DL with count
// ===========================================================================

#[test]
fn conformance_il_insert_multiple_lines() {
    let (mut term, mut rs) = make_term(80, 10);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3\r\nLINE4\r\nLINE5");
    term.vt_write(b"\x1b[2;1H\x1b[3L"); // Insert 3 lines at row 2
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "LINE1");
    assert!(lines[1].trim().is_empty());
    assert!(lines[2].trim().is_empty());
    assert!(lines[3].trim().is_empty());
    assert_eq!(lines[4], "LINE2");
}

#[test]
fn conformance_dl_delete_multiple_lines() {
    let (mut term, mut rs) = make_term(80, 10);
    term.vt_write(b"LINE1\r\nLINE2\r\nLINE3\r\nLINE4\r\nLINE5");
    term.vt_write(b"\x1b[2;1H\x1b[2M"); // Delete 2 lines at row 2
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "LINE1");
    assert_eq!(lines[1], "LINE4");
    assert_eq!(lines[2], "LINE5");
}

// ===========================================================================
// Scroll region + scroll within region
// ===========================================================================

#[test]
fn conformance_scroll_region_su() {
    let (mut term, mut rs) = make_term(80, 10);
    for i in 1..=10 {
        let line = format!("\x1b[{i};1HLINE{i:02}");
        term.vt_write(line.as_bytes());
    }
    // Set scroll region to rows 3-7, scroll up 1 within it
    term.vt_write(b"\x1b[3;7r");
    term.vt_write(b"\x1b[S"); // SU 1
    let lines = grid_text(&term, &mut rs);
    assert_eq!(lines[0], "LINE01", "Above region should be unchanged");
    assert_eq!(lines[1], "LINE02", "Above region should be unchanged");
    // Within region: LINE03 scrolled out, LINE04-LINE07 shift up, blank at bottom of region
    assert_eq!(lines[2], "LINE04", "Region row 0 should now be LINE04");
    assert!(
        lines[6].trim().is_empty(),
        "Bottom of region should be blank"
    );
    assert_eq!(lines[7], "LINE08", "Below region should be unchanged");
}
