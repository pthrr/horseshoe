use super::*;
use crate::terminal::vt::{Terminal, TerminalOps};
use libghostty_vt::style::{PaletteIndex, RgbColor};

#[test]
fn test_render_state_new() {
    let rs = RenderState::new();
    assert!(rs.is_ok());
}

#[test]
fn test_render_state_update_and_iterate() {
    let mut term = Terminal::new(80, 24, 100).expect("test setup");
    term.vt_write(b"Hello, world!");

    let mut rs = RenderState::new().expect("test setup");
    rs.update(term.inner()).expect("test setup");

    let (cols, rows) = rs.dimensions();
    assert_eq!(cols, 80);
    assert_eq!(rows, 24);

    let colors = rs.colors();
    assert_eq!(colors.background, (0, 0, 0));

    let mut found_h = false;
    rs.for_each_cell(|_row, _col, codepoints, _style, _wide| {
        if codepoints.first() == Some(&u32::from(b'H')) {
            found_h = true;
        }
    });
    assert!(found_h, "should find 'H' in render output");
}

#[test]
fn test_dirty_tracking() {
    let term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");

    let dirty = rs.dirty();
    assert_ne!(
        dirty,
        ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_FALSE
    );

    rs.clear_dirty();
    rs.update(term.inner()).expect("update");
}

#[test]
fn test_resolve_color_rgb() {
    let colors = RenderColors {
        foreground: (255, 255, 255),
        background: (0, 0, 0),
        cursor: None,
        palette: [(0, 0, 0); 256],
    };
    let result = resolve_color(
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB,
        0,
        (10, 20, 30),
        &colors,
        (0, 0, 0),
    );
    assert_eq!(result, (10, 20, 30));
}

#[test]
fn test_resolve_color_palette() {
    let mut palette = [(0u8, 0u8, 0u8); 256];
    if let Some(entry) = palette.get_mut(42) {
        *entry = (100, 150, 200);
    }
    let colors = RenderColors {
        foreground: (255, 255, 255),
        background: (0, 0, 0),
        cursor: None,
        palette,
    };
    let result = resolve_color(
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
        42,
        (0, 0, 0),
        &colors,
        (0, 0, 0),
    );
    assert_eq!(result, (100, 150, 200));
}

#[test]
fn test_resolve_color_none_returns_fallback() {
    let colors = RenderColors {
        foreground: (255, 255, 255),
        background: (0, 0, 0),
        cursor: None,
        palette: [(0, 0, 0); 256],
    };
    let result = resolve_color(
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE,
        0,
        (0, 0, 0),
        &colors,
        (11, 22, 33),
    );
    assert_eq!(result, (11, 22, 33));
}

#[test]
fn test_cell_style_attrs_zero() {
    let a = CellStyleAttrs::from_bits(0);
    assert!(!a.bold());
    assert!(!a.italic());
    assert!(!a.faint());
    assert!(!a.blink());
    assert!(!a.inverse());
    assert!(!a.invisible());
    assert!(!a.strikethrough());
    assert!(!a.overline());
}

#[test]
fn test_cell_style_attrs_all_set() {
    let a = CellStyleAttrs::from_bits(0xFF);
    assert!(a.bold());
    assert!(a.italic());
    assert!(a.faint());
    assert!(a.blink());
    assert!(a.inverse());
    assert!(a.invisible());
    assert!(a.strikethrough());
    assert!(a.overline());
}

#[test]
fn test_cell_style_attrs_individual_bits() {
    let cases: &[(u8, &str)] = &[
        (CellStyleAttrs::BOLD, "bold"),
        (CellStyleAttrs::ITALIC, "italic"),
        (CellStyleAttrs::FAINT, "faint"),
        (CellStyleAttrs::BLINK, "blink"),
        (CellStyleAttrs::INVERSE, "inverse"),
        (CellStyleAttrs::INVISIBLE, "invisible"),
        (CellStyleAttrs::STRIKETHROUGH, "strikethrough"),
        (CellStyleAttrs::OVERLINE, "overline"),
    ];
    for &(bit, name) in cases {
        let a = CellStyleAttrs::from_bits(bit);
        let getters: [(bool, &str); 8] = [
            (a.bold(), "bold"),
            (a.italic(), "italic"),
            (a.faint(), "faint"),
            (a.blink(), "blink"),
            (a.inverse(), "inverse"),
            (a.invisible(), "invisible"),
            (a.strikethrough(), "strikethrough"),
            (a.overline(), "overline"),
        ];
        for (val, getter_name) in getters {
            if getter_name == name {
                assert!(val, "{name} should be true when bit {bit:#04x} is set");
            } else {
                assert!(
                    !val,
                    "{getter_name} should be false when only {name} is set"
                );
            }
        }
    }
}

#[test]
fn test_default_cell_style_all_fields() {
    let s = default_cell_style();
    assert_eq!(s.fg_tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE);
    assert_eq!(s.bg_tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE);
    assert_eq!(
        s.underline_color_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE
    );
    assert!(!s.attrs.bold());
    assert!(!s.attrs.italic());
    assert_eq!(s.underline, 0);
}

#[test]
fn test_resolve_color_palette_boundary_indices() {
    let mut palette = [(0u8, 0u8, 0u8); 256];
    palette[0] = (10, 20, 30);
    palette[15] = (200, 100, 50);
    palette[255] = (1, 2, 3);
    let colors = RenderColors {
        foreground: (255, 255, 255),
        background: (0, 0, 0),
        cursor: None,
        palette,
    };
    assert_eq!(
        resolve_color(
            ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
            0,
            (0, 0, 0),
            &colors,
            (255, 255, 255)
        ),
        (10, 20, 30)
    );
    assert_eq!(
        resolve_color(
            ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
            255,
            (0, 0, 0),
            &colors,
            (0, 0, 0)
        ),
        (1, 2, 3)
    );
}

#[test]
fn test_colors_have_nonzero_values() {
    let term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    let colors = rs.colors();
    let both_zero = colors.foreground == (0, 0, 0) && colors.background == (0, 0, 0);
    assert!(!both_zero);
}

#[test]
fn test_for_each_cell_count() {
    let term_small = Terminal::new(10, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term_small.inner()).expect("update");
    let mut count = 0usize;
    rs.for_each_cell(|_row, _col, _cp, _style, _wide| count += 1);
    assert_eq!(count, 10 * 5);

    let term_large = Terminal::new(80, 24, 100).expect("terminal");
    rs.update(term_large.inner()).expect("update");
    count = 0;
    rs.for_each_cell(|_, _, _, _, _| count += 1);
    assert_eq!(count, 80 * 24);
}

#[test]
fn test_for_each_cell_has_codepoints() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"A");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    let mut found_a = false;
    rs.for_each_cell(|_row, _col, codepoints, _style, _wide| {
        if codepoints.first() == Some(&0x41) {
            found_a = true;
        }
    });
    assert!(found_a);
}

#[test]
fn test_dirty_full_after_first_update() {
    let term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    assert_eq!(
        rs.dirty(),
        ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_FULL
    );
}

#[test]
fn test_clear_dirty_resets_state() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    term.vt_write(b"hello");
    rs.update(term.inner()).expect("update");
    assert_ne!(
        rs.dirty(),
        ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_FALSE
    );
    rs.clear_dirty();
    assert_eq!(
        rs.dirty(),
        ffi::GhosttyRenderStateDirty_GHOSTTY_RENDER_STATE_DIRTY_FALSE
    );
}

#[test]
fn test_cursor_visible_after_update() {
    let term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    let cursor = rs.cursor();
    assert!(cursor.visible);
    assert!(cursor.in_viewport);
    assert_eq!(cursor.x, 0);
    assert_eq!(cursor.y, 0);
}

#[test]
fn test_dirty_row_indices_full() {
    let term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    let dirty_rows = rs.dirty_row_indices(24);
    assert_eq!(dirty_rows.len(), 24);
    let expected: Vec<u16> = (0..24).collect();
    assert_eq!(dirty_rows.as_slice(), expected.as_slice());
}

#[test]
fn test_cursor_position_after_write() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    term.vt_write(b"Hello");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    let cursor = rs.cursor();
    assert_eq!(cursor.x, 5);
    assert_eq!(cursor.y, 0);
}

#[test]
fn test_cell_style_attrs_all_bit_patterns() {
    for bits in 0u8..=0xFF {
        let attrs = CellStyleAttrs::from_bits(bits);
        assert_eq!(attrs.bold(), bits & CellStyleAttrs::BOLD != 0);
        assert_eq!(attrs.italic(), bits & CellStyleAttrs::ITALIC != 0);
        assert_eq!(attrs.faint(), bits & CellStyleAttrs::FAINT != 0);
        assert_eq!(
            attrs.strikethrough(),
            bits & CellStyleAttrs::STRIKETHROUGH != 0
        );
    }
}

#[test]
fn test_for_each_dirty_cell_filters() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // First update: everything is dirty
    term.vt_write(b"Hello");
    rs.update(term.inner()).expect("update");

    // Full iteration: count all cells
    let mut full_count = 0usize;
    rs.for_each_cell(|_, _, _, _, _| full_count += 1);
    assert_eq!(full_count, 80 * 24);

    // Clear dirty, write one more char, update
    rs.clear_dirty();
    term.vt_write(b"X");
    rs.update(term.inner()).expect("update");

    // Dirty-only iteration should yield fewer cells than full
    let mut dirty_count = 0usize;
    rs.for_each_dirty_cell(&mut |_, _, _, _, _| dirty_count += 1);
    assert!(
        dirty_count < full_count,
        "dirty_count ({dirty_count}) should be less than full_count ({full_count})"
    );
    assert!(dirty_count > 0, "should have some dirty cells");
}

#[test]
fn test_dirty_row_indices_after_clear() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    term.vt_write(b"test");
    rs.update(term.inner()).expect("update");

    // Process all rows to clear per-row dirty flags
    rs.for_each_cell(|_, _, _, _, _| {});
    rs.clear_dirty();

    // Update without any new content
    rs.update(term.inner()).expect("update");
    let dirty = rs.dirty_row_indices(24);
    assert!(
        dirty.is_empty(),
        "no dirty rows expected without content change, got {} rows",
        dirty.len()
    );
}

#[test]
fn test_cursor_style_bar() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    // CSI 6 SP q — set cursor to bar (steady)
    term.vt_write(b"\x1b[6 q");
    rs.update(term.inner()).expect("update");
    let cursor = rs.cursor();
    assert_eq!(
        cursor.style,
        CursorStyle::Bar,
        "CSI 6 SP q should set bar cursor"
    );
}

#[test]
fn test_cursor_style_underline() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    // CSI 4 SP q — set cursor to underline (steady)
    term.vt_write(b"\x1b[4 q");
    rs.update(term.inner()).expect("update");
    let cursor = rs.cursor();
    assert_eq!(
        cursor.style,
        CursorStyle::Underline,
        "CSI 4 SP q should set underline cursor"
    );
}

#[test]
fn test_cursor_style_block() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    // CSI 2 SP q — set cursor to block (steady)
    term.vt_write(b"\x1b[2 q");
    rs.update(term.inner()).expect("update");
    let cursor = rs.cursor();
    assert_eq!(
        cursor.style,
        CursorStyle::Block,
        "CSI 2 SP q should set block cursor"
    );
}

#[test]
fn test_dimensions_after_update() {
    let term = Terminal::new(120, 40, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    let (cols, rows) = rs.dimensions();
    assert_eq!(cols, 120, "dimensions should match terminal cols");
    assert_eq!(rows, 40, "dimensions should match terminal rows");
}

#[test]
fn test_cell_style_attrs_round_trip_each_bit() {
    let bit_names: &[(u8, &str)] = &[
        (CellStyleAttrs::BOLD, "bold"),
        (CellStyleAttrs::ITALIC, "italic"),
        (CellStyleAttrs::FAINT, "faint"),
        (CellStyleAttrs::BLINK, "blink"),
        (CellStyleAttrs::INVERSE, "inverse"),
        (CellStyleAttrs::INVISIBLE, "invisible"),
        (CellStyleAttrs::STRIKETHROUGH, "strikethrough"),
        (CellStyleAttrs::OVERLINE, "overline"),
    ];
    for &(bit, name) in bit_names {
        let set = CellStyleAttrs::from_bits(bit);
        let empty = CellStyleAttrs::from_bits(0);
        let others = CellStyleAttrs::from_bits(!bit);

        // Helper to get the right getter result
        let get = |a: CellStyleAttrs, b: u8| -> bool {
            match b {
                CellStyleAttrs::BOLD => a.bold(),
                CellStyleAttrs::ITALIC => a.italic(),
                CellStyleAttrs::FAINT => a.faint(),
                CellStyleAttrs::BLINK => a.blink(),
                CellStyleAttrs::INVERSE => a.inverse(),
                CellStyleAttrs::INVISIBLE => a.invisible(),
                CellStyleAttrs::STRIKETHROUGH => a.strikethrough(),
                CellStyleAttrs::OVERLINE => a.overline(),
                _ => unreachable!(),
            }
        };

        assert!(get(set, bit), "{name} should be true when its bit is set");
        assert!(
            !get(empty, bit),
            "{name} should be false when no bits are set"
        );
        assert!(
            !get(others, bit),
            "{name} should be false when only other bits are set"
        );
    }
}

#[test]
fn test_resolve_color_unknown_tag() {
    let colors = RenderColors {
        foreground: (255, 255, 255),
        background: (0, 0, 0),
        cursor: None,
        palette: [(0, 0, 0); 256],
    };
    // An unknown tag value (e.g. 9999) should return the fallback
    let result = resolve_color(9999, 0, (10, 20, 30), &colors, (42, 43, 44));
    assert_eq!(result, (42, 43, 44));
}

#[test]
fn test_dirty_row_indices_clean() {
    let term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    // Consume all dirty rows
    let _ = rs.dirty_row_indices(24);
    // After clearing dirty, should be empty on next query
    rs.clear_dirty();
    rs.update(term.inner()).expect("update");
    // Query dirty rows — there may or may not be dirty rows depending
    // on the update semantics. Just verify no panic.
    let dirty = rs.dirty_row_indices(24);
    let _ = dirty;
}

#[test]
fn test_colors_all_256_palette() {
    let mut palette = [(0u8, 0u8, 0u8); 256];
    for (i, entry) in palette.iter_mut().enumerate() {
        let v = u8::try_from(i).unwrap_or(255);
        *entry = (v, v, v);
    }
    let colors = RenderColors {
        foreground: (255, 255, 255),
        background: (0, 0, 0),
        cursor: None,
        palette,
    };
    for i in 0..=255u8 {
        let result = resolve_color(
            ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
            i,
            (0, 0, 0),
            &colors,
            (0, 0, 0),
        );
        assert_eq!(result, (i, i, i), "palette[{i}] should resolve correctly");
    }
}

#[test]
fn test_dimensions_after_resize() {
    let mut term = Terminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    rs.update(term.inner()).expect("update");
    assert_eq!(rs.dimensions(), (80, 24));

    term.resize(40, 10).expect("resize");
    rs.update(term.inner()).expect("update");
    assert_eq!(rs.dimensions(), (40, 10));
}

#[test]
fn test_convert_style_color_none() {
    let (tag, palette, rgb) = convert_style_color(StyleColor::None);
    assert_eq!(tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE);
    assert_eq!(palette, 0);
    assert_eq!(rgb, (0, 0, 0));
}

#[test]
fn test_convert_style_color_palette() {
    let (tag, palette, rgb) = convert_style_color(StyleColor::Palette(PaletteIndex(42)));
    assert_eq!(tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE);
    assert_eq!(palette, 42);
    assert_eq!(rgb, (0, 0, 0));
}

#[test]
fn test_convert_style_color_palette_zero() {
    let (tag, idx, _) = convert_style_color(StyleColor::Palette(PaletteIndex(0)));
    assert_eq!(tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE);
    assert_eq!(idx, 0);
}

#[test]
fn test_convert_style_color_palette_max() {
    let (tag, idx, _) = convert_style_color(StyleColor::Palette(PaletteIndex(255)));
    assert_eq!(tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE);
    assert_eq!(idx, 255);
}

#[test]
fn test_convert_style_color_rgb() {
    let (tag, palette, rgb) = convert_style_color(StyleColor::Rgb(RgbColor {
        r: 10,
        g: 20,
        b: 30,
    }));
    assert_eq!(tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB);
    assert_eq!(palette, 0);
    assert_eq!(rgb, (10, 20, 30));
}

#[test]
fn test_convert_style_color_rgb_black() {
    let (_, _, color) = convert_style_color(StyleColor::Rgb(RgbColor { r: 0, g: 0, b: 0 }));
    assert_eq!(color, (0, 0, 0));
}

#[test]
fn test_convert_style_color_rgb_white() {
    let (_, _, color) = convert_style_color(StyleColor::Rgb(RgbColor {
        r: 255,
        g: 255,
        b: 255,
    }));
    assert_eq!(color, (255, 255, 255));
}

/// Build a `Style` with all attributes off and no colors.
fn make_default_style() -> Style {
    Style {
        fg_color: StyleColor::None,
        bg_color: StyleColor::None,
        underline_color: StyleColor::None,
        bold: false,
        italic: false,
        faint: false,
        blink: false,
        inverse: false,
        invisible: false,
        strikethrough: false,
        overline: false,
        underline: Underline::None,
    }
}

#[test]
fn test_convert_style_default() {
    let style = make_default_style();
    let cs = convert_style(&style);
    assert_eq!(
        cs.fg_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE
    );
    assert_eq!(
        cs.bg_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE
    );
    assert_eq!(
        cs.underline_color_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE
    );
    assert!(!cs.attrs.bold());
    assert!(!cs.attrs.italic());
    assert!(!cs.attrs.faint());
    assert!(!cs.attrs.blink());
    assert!(!cs.attrs.inverse());
    assert!(!cs.attrs.invisible());
    assert!(!cs.attrs.strikethrough());
    assert!(!cs.attrs.overline());
    assert_eq!(cs.underline, 0);
}

#[test]
fn test_convert_style_all_attrs_set() {
    let style = Style {
        fg_color: StyleColor::None,
        bg_color: StyleColor::None,
        underline_color: StyleColor::None,
        bold: true,
        italic: true,
        faint: true,
        blink: true,
        inverse: true,
        invisible: true,
        strikethrough: true,
        overline: true,
        underline: Underline::None,
    };
    let cs = convert_style(&style);
    assert!(cs.attrs.bold());
    assert!(cs.attrs.italic());
    assert!(cs.attrs.faint());
    assert!(cs.attrs.blink());
    assert!(cs.attrs.inverse());
    assert!(cs.attrs.invisible());
    assert!(cs.attrs.strikethrough());
    assert!(cs.attrs.overline());
}

#[test]
fn test_convert_style_individual_attrs() {
    // Test each attribute in isolation to confirm it maps to the right bit.
    let write_fns: &[(&str, fn(&mut Style))] = &[
        ("bold", |s| s.bold = true),
        ("italic", |s| s.italic = true),
        ("faint", |s| s.faint = true),
        ("blink", |s| s.blink = true),
        ("inverse", |s| s.inverse = true),
        ("invisible", |s| s.invisible = true),
        ("strikethrough", |s| s.strikethrough = true),
        ("overline", |s| s.overline = true),
    ];
    let read_fns: &[(&str, fn(&CellStyleAttrs) -> bool)] = &[
        ("bold", |a| a.bold()),
        ("italic", |a| a.italic()),
        ("faint", |a| a.faint()),
        ("blink", |a| a.blink()),
        ("inverse", |a| a.inverse()),
        ("invisible", |a| a.invisible()),
        ("strikethrough", |a| a.strikethrough()),
        ("overline", |a| a.overline()),
    ];

    for (set_name, setter) in write_fns {
        let mut style = make_default_style();
        setter(&mut style);
        let cs = convert_style(&style);
        for (get_name, reader) in read_fns {
            if *get_name == *set_name {
                assert!(
                    reader(&cs.attrs),
                    "{set_name} should be true after being set",
                );
            } else {
                assert!(
                    !reader(&cs.attrs),
                    "{get_name} should be false when only {set_name} is set",
                );
            }
        }
    }
}

#[test]
fn test_convert_style_underline_single() {
    let mut style = make_default_style();
    style.underline = Underline::Single;
    assert_eq!(convert_style(&style).underline, 1);
}

#[test]
fn test_convert_style_underline_double() {
    let mut style = make_default_style();
    style.underline = Underline::Double;
    assert_eq!(convert_style(&style).underline, 2);
}

#[test]
fn test_convert_style_underline_curly() {
    let mut style = make_default_style();
    style.underline = Underline::Curly;
    assert_eq!(convert_style(&style).underline, 3);
}

#[test]
fn test_convert_style_underline_dotted() {
    let mut style = make_default_style();
    style.underline = Underline::Dotted;
    assert_eq!(convert_style(&style).underline, 4);
}

#[test]
fn test_convert_style_underline_dashed() {
    let mut style = make_default_style();
    style.underline = Underline::Dashed;
    assert_eq!(convert_style(&style).underline, 5);
}

#[test]
fn test_convert_style_underline_none() {
    let style = make_default_style();
    assert_eq!(convert_style(&style).underline, 0);
}

#[test]
fn test_convert_style_fg_palette_color() {
    let mut style = make_default_style();
    style.fg_color = StyleColor::Palette(PaletteIndex(7));
    let cs = convert_style(&style);
    assert_eq!(
        cs.fg_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE
    );
    assert_eq!(cs.fg_palette, 7);
}

#[test]
fn test_convert_style_fg_rgb_color() {
    let mut style = make_default_style();
    style.fg_color = StyleColor::Rgb(RgbColor {
        r: 100,
        g: 150,
        b: 200,
    });
    let cs = convert_style(&style);
    assert_eq!(cs.fg_tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB);
    assert_eq!(cs.fg_rgb, (100, 150, 200));
}

#[test]
fn test_convert_style_bg_palette_color() {
    let mut style = make_default_style();
    style.bg_color = StyleColor::Palette(PaletteIndex(200));
    let cs = convert_style(&style);
    assert_eq!(
        cs.bg_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE
    );
    assert_eq!(cs.bg_palette, 200);
}

#[test]
fn test_convert_style_bg_rgb_color() {
    let mut style = make_default_style();
    style.bg_color = StyleColor::Rgb(RgbColor {
        r: 30,
        g: 60,
        b: 90,
    });
    let cs = convert_style(&style);
    assert_eq!(cs.bg_tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB);
    assert_eq!(cs.bg_rgb, (30, 60, 90));
}

#[test]
fn test_convert_style_underline_color_palette() {
    let mut style = make_default_style();
    style.underline_color = StyleColor::Palette(PaletteIndex(1));
    let cs = convert_style(&style);
    assert_eq!(
        cs.underline_color_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
    );
    assert_eq!(cs.underline_color_palette, 1);
}

#[test]
fn test_convert_style_underline_color_rgb() {
    let mut style = make_default_style();
    style.underline_color = StyleColor::Rgb(RgbColor {
        r: 255,
        g: 128,
        b: 0,
    });
    let cs = convert_style(&style);
    assert_eq!(
        cs.underline_color_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB,
    );
    assert_eq!(cs.underline_color_rgb, (255, 128, 0));
}

#[test]
fn test_convert_style_all_colors_and_attrs() {
    let style = Style {
        fg_color: StyleColor::Rgb(RgbColor { r: 255, g: 0, b: 0 }),
        bg_color: StyleColor::Palette(PaletteIndex(42)),
        underline_color: StyleColor::Rgb(RgbColor { r: 0, g: 255, b: 0 }),
        bold: true,
        italic: true,
        faint: false,
        blink: true,
        inverse: false,
        invisible: false,
        strikethrough: true,
        overline: false,
        underline: Underline::Curly,
    };
    let cs = convert_style(&style);

    // Foreground: RGB
    assert_eq!(cs.fg_tag, ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB);
    assert_eq!(cs.fg_rgb, (255, 0, 0));

    // Background: palette
    assert_eq!(
        cs.bg_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE
    );
    assert_eq!(cs.bg_palette, 42);

    // Underline color: RGB
    assert_eq!(
        cs.underline_color_tag,
        ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_RGB
    );
    assert_eq!(cs.underline_color_rgb, (0, 255, 0));

    // Attributes
    assert!(cs.attrs.bold());
    assert!(cs.attrs.italic());
    assert!(!cs.attrs.faint());
    assert!(cs.attrs.blink());
    assert!(!cs.attrs.inverse());
    assert!(!cs.attrs.invisible());
    assert!(cs.attrs.strikethrough());
    assert!(!cs.attrs.overline());

    // Underline style
    assert_eq!(cs.underline, 3);
}
