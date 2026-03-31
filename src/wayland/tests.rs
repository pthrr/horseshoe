use super::*;
use cursor::{
    ARROW_BITMAP, ARROW_H, ARROW_W, IBEAM_BITMAP, IBEAM_H, IBEAM_W, render_cursor_bitmap,
};
use horseshoe::config;
use libghostty_vt::mouse;
use osc::decode_osc52_base64;
use std::path::PathBuf;

#[test]
fn test_render_cursor_bitmap_all_transparent() {
    let bitmap = [0u8; 4];
    let mut canvas = [0u8; 16];
    render_cursor_bitmap(&bitmap, &mut canvas);
    for chunk in canvas.chunks_exact(4) {
        assert_eq!(chunk, &[0, 0, 0, 0]);
    }
}

#[test]
fn test_render_cursor_bitmap_all_black() {
    let bitmap = [1u8; 4];
    let mut canvas = [0u8; 16];
    render_cursor_bitmap(&bitmap, &mut canvas);
    for chunk in canvas.chunks_exact(4) {
        assert_eq!(chunk, &[0, 0, 0, 255]);
    }
}

#[test]
fn test_render_cursor_bitmap_all_white() {
    let bitmap = [2u8; 4];
    let mut canvas = [0u8; 16];
    render_cursor_bitmap(&bitmap, &mut canvas);
    for chunk in canvas.chunks_exact(4) {
        assert_eq!(chunk, &[255, 255, 255, 255]);
    }
}

#[test]
fn test_render_cursor_bitmap_mixed() {
    let bitmap = [0, 1, 2, 0];
    let mut canvas = [0u8; 16];
    render_cursor_bitmap(&bitmap, &mut canvas);
    assert_eq!(&canvas[0..4], &[0, 0, 0, 0]); // transparent
    assert_eq!(&canvas[4..8], &[0, 0, 0, 255]); // black
    assert_eq!(&canvas[8..12], &[255, 255, 255, 255]); // white
    assert_eq!(&canvas[12..16], &[0, 0, 0, 0]); // transparent
}

#[test]
fn test_render_cursor_bitmap_unknown_values() {
    let bitmap = [3, 99, 255];
    let mut canvas = [0u8; 12];
    render_cursor_bitmap(&bitmap, &mut canvas);
    // All unknown values → transparent
    for chunk in canvas.chunks_exact(4) {
        assert_eq!(chunk, &[0, 0, 0, 0]);
    }
}

#[test]
fn test_wayland_button_left() {
    assert_eq!(wayland_button_to_ghostty(0x110), mouse::Button::Left);
}

#[test]
fn test_wayland_button_right() {
    assert_eq!(wayland_button_to_ghostty(0x111), mouse::Button::Right);
}

#[test]
fn test_wayland_button_middle() {
    assert_eq!(wayland_button_to_ghostty(0x112), mouse::Button::Middle);
}

#[test]
fn test_wayland_button_four() {
    assert_eq!(wayland_button_to_ghostty(0x113), mouse::Button::Four);
}

#[test]
fn test_wayland_button_five() {
    assert_eq!(wayland_button_to_ghostty(0x114), mouse::Button::Five);
}

#[test]
fn test_wayland_button_unknown() {
    assert_eq!(wayland_button_to_ghostty(0x999), mouse::Button::Unknown);
    assert_eq!(wayland_button_to_ghostty(0), mouse::Button::Unknown);
}

#[test]
fn test_wayland_button_all_known() {
    // Verify all 5 known button codes map to distinct non-UNKNOWN values
    let buttons = [0x110, 0x111, 0x112, 0x113, 0x114];
    for &btn in &buttons {
        assert_ne!(
            wayland_button_to_ghostty(btn),
            mouse::Button::Unknown,
            "button {btn:#x} should be known"
        );
    }
}

#[test]
fn test_wayland_button_boundary() {
    // One below BTN_LEFT
    assert_eq!(wayland_button_to_ghostty(0x10F), mouse::Button::Unknown,);
    // One above BTN_FIVE
    assert_eq!(wayland_button_to_ghostty(0x115), mouse::Button::Unknown,);
}

#[test]
fn test_mouse_state_default() {
    let m = MouseState::default();
    assert!(m.x.abs() < f64::EPSILON);
    assert!(m.y.abs() < f64::EPSILON);
    assert_eq!(m.buttons_pressed, 0);
    assert!(!m.scrollbar_dragging);
}

#[test]
fn test_display_config_defaults() {
    let d = DisplayConfig::default();
    assert!(!d.flags.bold_is_bright());
    assert!(d.cursor_blink_visible);
    assert!(!d.fullscreen);
    assert_eq!(d.padding, 0);
    assert!((d.opacity - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_binding_ctrl_shift_c_copy() {
    let b = config::Bindings::default();
    assert_eq!(b.lookup(0x0063, true, true), Some(config::KeyAction::Copy));
}

#[test]
fn test_binding_ctrl_shift_v_paste() {
    let b = config::Bindings::default();
    assert_eq!(b.lookup(0x0076, true, true), Some(config::KeyAction::Paste));
}

#[test]
fn test_binding_f11_fullscreen() {
    let b = config::Bindings::default();
    assert_eq!(
        b.lookup(0xffc8, false, false),
        Some(config::KeyAction::ToggleFullscreen)
    );
}

#[test]
fn test_binding_ctrl_shift_f_search() {
    let b = config::Bindings::default();
    assert_eq!(
        b.lookup(0x0066, true, true),
        Some(config::KeyAction::Search)
    );
}

#[test]
fn test_binding_ctrl_equal_font_up() {
    let b = config::Bindings::default();
    assert_eq!(
        b.lookup(0x003d, true, false),
        Some(config::KeyAction::FontSizeUp)
    );
}

#[test]
fn test_binding_ctrl_minus_font_down() {
    let b = config::Bindings::default();
    assert_eq!(
        b.lookup(0x002d, true, false),
        Some(config::KeyAction::FontSizeDown)
    );
}

#[test]
fn test_binding_ctrl_zero_font_reset() {
    let b = config::Bindings::default();
    assert_eq!(
        b.lookup(0x0030, true, false),
        Some(config::KeyAction::FontSizeReset)
    );
}

#[test]
fn test_binding_no_match() {
    let b = config::Bindings::default();
    assert!(b.lookup(0x0061, false, false).is_none()); // plain 'a'
    assert!(b.lookup(0x0041, false, false).is_none()); // plain 'A'
    assert!(b.lookup(0xdead, true, true).is_none()); // unknown keysym
}

#[test]
fn test_binding_partial_modifier_ctrl_only() {
    let b = config::Bindings::default();
    assert!(b.lookup(0x0063, true, false).is_none());
}

#[test]
fn test_binding_partial_modifier_shift_only() {
    let b = config::Bindings::default();
    assert!(b.lookup(0x0063, false, true).is_none());
}

#[test]
fn test_binding_f11_with_modifiers_no_match() {
    let b = config::Bindings::default();
    assert!(b.lookup(0xffc8, true, false).is_none());
    assert!(b.lookup(0xffc8, false, true).is_none());
}

#[test]
fn test_binding_ctrl_shift_uppercase_v_paste() {
    let b = config::Bindings::default();
    assert_eq!(b.lookup(0x0056, true, true), Some(config::KeyAction::Paste));
}

#[test]
fn test_binding_ctrl_shift_uppercase_c_copy() {
    let b = config::Bindings::default();
    assert_eq!(b.lookup(0x0043, true, true), Some(config::KeyAction::Copy));
}

#[test]
fn test_binding_ctrl_shift_uppercase_f_search() {
    let b = config::Bindings::default();
    assert_eq!(
        b.lookup(0x0046, true, true),
        Some(config::KeyAction::Search)
    );
}

#[test]
fn test_ibeam_bitmap_dimensions() {
    assert_eq!(IBEAM_BITMAP.len(), (IBEAM_W * IBEAM_H) as usize);
}

#[test]
fn test_arrow_bitmap_dimensions() {
    assert_eq!(ARROW_BITMAP.len(), (ARROW_W * ARROW_H) as usize);
}

#[test]
fn test_ibeam_bitmap_values_valid() {
    for &pixel in &IBEAM_BITMAP {
        assert!(pixel <= 2, "IBEAM bitmap pixel must be 0, 1, or 2");
    }
}

#[test]
fn test_arrow_bitmap_values_valid() {
    for &pixel in &ARROW_BITMAP {
        assert!(pixel <= 2, "ARROW bitmap pixel must be 0, 1, or 2");
    }
}

#[test]
fn test_osc52_basic_bel() {
    // "hello" in base64 = "aGVsbG8="
    let data = b"\x1b]52;c;aGVsbG8=\x07";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    let result = scan_osc52(&mut state, &mut buf, data);
    assert_eq!(result.as_deref(), Some("hello"));
}

#[test]
fn test_osc52_basic_st() {
    // "hello" in base64 = "aGVsbG8="
    let data = b"\x1b]52;c;aGVsbG8=\x1b\\";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    let result = scan_osc52(&mut state, &mut buf, data);
    assert_eq!(result.as_deref(), Some("hello"));
}

#[test]
fn test_osc52_split_across_reads() {
    let part1 = b"\x1b]52;c;aGVs";
    let part2 = b"bG8=\x07";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    assert!(scan_osc52(&mut state, &mut buf, part1).is_none());
    let result = scan_osc52(&mut state, &mut buf, part2);
    assert_eq!(result.as_deref(), Some("hello"));
}

#[test]
fn test_osc52_no_match() {
    let data = b"regular terminal output";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    assert!(scan_osc52(&mut state, &mut buf, data).is_none());
}

#[test]
fn test_osc52_other_osc_ignored() {
    // OSC 0 (title) should not trigger OSC 52
    let data = b"\x1b]0;window title\x07";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    assert!(scan_osc52(&mut state, &mut buf, data).is_none());
}

#[test]
fn test_decode_osc52_base64_simple() {
    assert_eq!(decode_osc52_base64(b"aGVsbG8=").as_deref(), Some("hello"));
}

#[test]
fn test_decode_osc52_base64_no_padding() {
    assert_eq!(decode_osc52_base64(b"aGVsbG8").as_deref(), Some("hello"));
}

#[test]
fn test_decode_osc52_base64_empty() {
    assert_eq!(decode_osc52_base64(b"").as_deref(), Some(""));
}

#[test]
fn test_osc52_primary_selection() {
    // OSC 52 to primary selection (p target)
    let data = b"\x1b]52;p;dGVzdA==\x07";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    let result = scan_osc52(&mut state, &mut buf, data);
    assert_eq!(result.as_deref(), Some("test"));
}

// ---- OscAccum tests ----

fn assert_cwd_event(event: &OscEvent, expected: &str) {
    match event {
        OscEvent::Cwd(p) => assert_eq!(p, &PathBuf::from(expected)),
        OscEvent::PromptMark => panic!("expected Cwd event, got PromptMark"),
    }
}

#[test]
fn test_osc_accum_osc7_bel() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b]7;file:///home/user/project\x07");
    assert_eq!(events.len(), 1);
    assert_cwd_event(events.first().expect("one event"), "/home/user/project");
}

#[test]
fn test_osc_accum_osc7_st() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b]7;file:///tmp/dir\x1b\\");
    assert_eq!(events.len(), 1);
    assert_cwd_event(events.first().expect("one event"), "/tmp/dir");
}

#[test]
fn test_osc_accum_osc7_with_hostname() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b]7;file://myhost/home/user\x07");
    assert_eq!(events.len(), 1);
    assert_cwd_event(events.first().expect("one event"), "/home/user");
}

#[test]
fn test_osc_accum_osc133_prompt_mark() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b]133;A\x07");
    assert_eq!(events.len(), 1);
    assert!(matches!(events.first(), Some(OscEvent::PromptMark)));
}

#[test]
fn test_osc_accum_osc133_st() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b]133;A\x1b\\");
    assert_eq!(events.len(), 1);
    assert!(matches!(events.first(), Some(OscEvent::PromptMark)));
}

#[test]
fn test_osc_accum_split_across_feeds() {
    let mut accum = OscAccum::default();
    let events1 = accum.feed(b"\x1b]7;file:///ho");
    assert!(events1.is_empty());
    let events2 = accum.feed(b"me/user\x07");
    assert_eq!(events2.len(), 1);
    assert_cwd_event(events2.first().expect("one event"), "/home/user");
}

#[test]
fn test_osc_accum_ignores_other_osc() {
    let mut accum = OscAccum::default();
    // OSC 0 (set title) — should not produce events
    let events = accum.feed(b"\x1b]0;my title\x07");
    assert!(events.is_empty());
}

#[test]
fn test_osc_accum_multiple_events() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b]133;A\x07text\x1b]7;file:///tmp\x07");
    assert_eq!(events.len(), 2);
    let mut iter = events.iter();
    assert!(matches!(iter.next(), Some(OscEvent::PromptMark)));
    assert_cwd_event(iter.next().expect("second event"), "/tmp");
}

#[test]
fn test_osc_accum_interrupted_esc() {
    // ESC followed by non-] resets to Normal
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b[H\x1b]133;A\x07");
    assert_eq!(events.len(), 1);
    assert!(matches!(events.first(), Some(OscEvent::PromptMark)));
}

// ---- MouseState scroll accumulator tests (Bug 3: scroll down) ----

#[test]
fn test_mouse_state_default_scroll_accum() {
    let m = MouseState::default();
    assert!(
        m.scroll_accum.abs() < f64::EPSILON,
        "scroll_accum should default to 0.0"
    );
}

#[test]
fn test_mouse_state_scroll_accum_field_exists() {
    // Verify the field is writable and readable via struct literal
    let m_pos = MouseState {
        scroll_accum: 7.5,
        ..MouseState::default()
    };
    assert!((m_pos.scroll_accum - 7.5).abs() < f64::EPSILON);

    let m_neg = MouseState {
        scroll_accum: -3.2,
        ..MouseState::default()
    };
    assert!((m_neg.scroll_accum + 3.2).abs() < f64::EPSILON);
}

// ---- WindowGeometry tests ----

#[test]
fn test_geometry_phys_at_1x() {
    let g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 120, // 1.0x
        base_font_size: 16.0,
        initial_font_size: 16.0,
    };
    assert_eq!(g.phys(100), 100, "1.0x scale should not change value");
}

#[test]
fn test_geometry_phys_at_2x() {
    let g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 240, // 2.0x
        base_font_size: 16.0,
        initial_font_size: 16.0,
    };
    assert_eq!(g.phys(100), 200, "2.0x scale should double value");
}

#[test]
fn test_geometry_scale_f64() {
    let g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 180, // 1.5x
        base_font_size: 16.0,
        initial_font_size: 16.0,
    };
    assert!((g.scale_f64() - 1.5).abs() < f64::EPSILON);
}

#[test]
fn test_geometry_adjust_font_size() {
    let mut g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 120,
        base_font_size: 16.0,
        initial_font_size: 16.0,
    };
    assert!(g.adjust_font_size(1.0));
    assert!((g.base_font_size - 17.0).abs() < f32::EPSILON);
}

#[test]
fn test_geometry_adjust_font_size_clamp_min() {
    let mut g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 120,
        base_font_size: 6.0,
        initial_font_size: 16.0,
    };
    // Already at minimum — should return false (no change)
    assert!(!g.adjust_font_size(-1.0));
}

#[test]
fn test_geometry_adjust_font_size_clamp_max() {
    let mut g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 120,
        base_font_size: 72.0,
        initial_font_size: 16.0,
    };
    // Already at maximum — should return false (no change)
    assert!(!g.adjust_font_size(1.0));
}

#[test]
fn test_geometry_reset_font_size() {
    let mut g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 120,
        base_font_size: 24.0,
        initial_font_size: 16.0,
    };
    assert!(g.reset_font_size());
    assert!((g.base_font_size - 16.0).abs() < f32::EPSILON);
}

#[test]
fn test_geometry_reset_font_size_noop() {
    let mut g = WindowGeometry {
        width: 800,
        height: 600,
        term_cols: 80,
        term_rows: 24,
        scale_120: 120,
        base_font_size: 16.0,
        initial_font_size: 16.0,
    };
    // Already at initial — should return false (no change)
    assert!(!g.reset_font_size());
}

// ---- Scroll accumulator arithmetic tests ----
//
// These test the accumulator pattern used in handle_pointer_axis
// to verify symmetry between scroll-up and scroll-down.

/// Simulate the scroll accumulator logic to verify symmetry.
/// Uses the given line height (matching the real code which uses `cell_height`).
/// Returns the number of discrete steps for a given sequence of pixel deltas.
fn simulate_scroll_accum_with_height(deltas: &[f64], line_height: f64) -> Vec<i32> {
    let mut accum = 0.0f64;
    let mut steps_out = Vec::new();
    for &delta in deltas {
        accum += delta;
        let steps_f64 = (accum / line_height).trunc();
        let steps =
            i32::try_from(horseshoe::num::float_to_i64(steps_f64)).expect("scroll steps fits i32");
        accum -= f64::from(steps) * line_height;
        steps_out.push(steps);
    }
    steps_out
}

/// Simulate with default cell height (19px for 16pt `JetBrainsMono`).
fn simulate_scroll_accum(deltas: &[f64]) -> Vec<i32> {
    simulate_scroll_accum_with_height(deltas, 19.0)
}

#[test]
fn test_scroll_accum_small_positive_accumulates() {
    // Small positive scroll events (scroll down) should accumulate
    // and eventually produce a step when they exceed line_height (19px).
    let steps = simulate_scroll_accum(&[7.0, 7.0, 7.0]);
    // 7+7+7=21px > 19px → 1 step on third event
    assert_eq!(
        steps,
        vec![0, 0, 1],
        "three 7px scrolls should produce one step on third"
    );
}

#[test]
fn test_scroll_accum_small_negative_accumulates() {
    // Small negative scroll events (scroll up) should accumulate
    // and eventually produce a step.
    let steps = simulate_scroll_accum(&[-7.0, -7.0, -7.0]);
    assert_eq!(
        steps,
        vec![0, 0, -1],
        "three -7px scrolls should produce one step on third"
    );
}

#[test]
fn test_scroll_accum_symmetry() {
    // The key bug: positive and negative small values must behave
    // symmetrically. Previously, `div_euclid(line_height).floor()` was asymmetric.
    let pos = simulate_scroll_accum(&[10.0, 10.0, 10.0]);
    let neg = simulate_scroll_accum(&[-10.0, -10.0, -10.0]);

    // The absolute step counts should be equal
    let pos_total: i32 = pos.iter().sum();
    let neg_total: i32 = neg.iter().sum();
    assert_eq!(
        pos_total.abs(),
        neg_total.abs(),
        "positive and negative scrolls must produce symmetric step counts: pos={pos_total}, neg={neg_total}"
    );
}

#[test]
fn test_scroll_accum_large_single_event() {
    // A single large scroll should produce the right number of steps.
    // With line_height=19: 57px / 19 = 3 steps exactly.
    let steps = simulate_scroll_accum(&[57.0]);
    assert_eq!(steps, vec![3], "57px / 19px = 3 steps");

    let steps_neg = simulate_scroll_accum(&[-57.0]);
    assert_eq!(steps_neg, vec![-3], "-57px / 19px = -3 steps");
}

#[test]
fn test_scroll_accum_residual_preserved() {
    // After a partial step, the residual should carry over.
    // 10+10=20px, with line_height=19 → 1 step + 1px residual.
    let steps = simulate_scroll_accum(&[10.0, 10.0]);
    assert_eq!(steps, vec![0, 1], "10+10=20px > 19px → 1 step");

    // 10+10+10=30px / 19 → 1 step (at 20px), then residual 1px + 10 = 11px < 19, no step.
    let steps2 = simulate_scroll_accum(&[10.0, 10.0, 10.0]);
    assert_eq!(steps2, vec![0, 1, 0], "10+10+10=30px → 1 step total");
}

#[test]
fn test_scroll_accum_direction_change() {
    // Scrolling in one direction then the other should work correctly
    let steps = simulate_scroll_accum(&[20.0, -20.0]);
    let total: i32 = steps.iter().sum();
    assert_eq!(total, 0, "equal scroll up and down should cancel out");
}

#[test]
fn test_scroll_accum_zero_event() {
    // Zero-valued events should not change the accumulator
    let steps = simulate_scroll_accum(&[0.0, 0.0, 19.0]);
    assert_eq!(steps, vec![0, 0, 1], "zero events should not produce steps");
}

/// Regression test: the old `div_euclid(15.0).floor()` implementation
/// was asymmetric. Verify the new accumulator doesn't have this bug.
#[test]
fn test_scroll_accum_old_bug_regression() {
    // The old code: `(value).div_euclid(step).floor()`
    // For value=7.0:  `7.0.div_euclid(19.0).floor()` = `0.0.floor()` = 0 (no step)
    // For value=-7.0: `(-7.0).div_euclid(19.0).floor()` = `(-1.0).floor()` = -1 (step!)
    // This meant scroll-up worked but scroll-down was dropped for small events.

    // The new accumulator must treat both directions equally:
    let pos_steps = simulate_scroll_accum(&[7.0]);
    let neg_steps = simulate_scroll_accum(&[-7.0]);
    let pos_val = pos_steps.first().copied().expect("pos step");
    let neg_val = neg_steps.first().copied().expect("neg step");
    assert_eq!(
        pos_val.abs(),
        neg_val.abs(),
        "single 7px event must be symmetric: pos={pos_val}, neg={neg_val}"
    );
}

/// Verify many small scroll events eventually produce the correct
/// number of total steps.
#[test]
fn test_scroll_accum_many_small_events() {
    // 38 events of 1px each = 38px total = 2 full steps (38/19=2)
    let events: Vec<f64> = vec![1.0; 38];
    let steps = simulate_scroll_accum(&events);
    let total: i32 = steps.iter().sum();
    assert_eq!(total, 2, "38 * 1px = 38px should produce 2 steps (38/19)");

    // Same for negative direction
    let events_neg: Vec<f64> = vec![-1.0; 38];
    let steps_neg = simulate_scroll_accum(&events_neg);
    let total_neg: i32 = steps_neg.iter().sum();
    assert_eq!(
        total_neg, -2,
        "38 * -1px = -38px should produce -2 steps (38/19)"
    );
}

/// Verify the accumulator works correctly with different line heights
/// (foot uses actual `cell_height`, not a hardcoded value).
#[test]
fn test_scroll_accum_various_line_heights() {
    // Small font (12px cell height)
    let steps_small = simulate_scroll_accum_with_height(&[12.0], 12.0);
    assert_eq!(steps_small, vec![1], "12px with 12px line height = 1 step");

    // Large font (24px cell height)
    let steps_large = simulate_scroll_accum_with_height(&[24.0], 24.0);
    assert_eq!(steps_large, vec![1], "24px with 24px line height = 1 step");

    // Fractional: 10px scroll with 12px line height
    let steps_frac = simulate_scroll_accum_with_height(&[10.0, 10.0], 12.0);
    // 10px: 0 steps (10 < 12), 10+10=20px: 1 step (20/12=1 remainder 8)
    assert_eq!(
        steps_frac,
        vec![0, 1],
        "10+10=20px with 12px height = 1 step"
    );
}

/// Verify symmetry is maintained across different line heights.
#[test]
fn test_scroll_accum_symmetry_various_heights() {
    for height in [12.0, 16.0, 19.0, 24.0, 32.0] {
        let pos = simulate_scroll_accum_with_height(&[5.0, 5.0, 5.0, 5.0, 5.0], height);
        let neg = simulate_scroll_accum_with_height(&[-5.0, -5.0, -5.0, -5.0, -5.0], height);
        let pos_total: i32 = pos.iter().sum();
        let neg_total: i32 = neg.iter().sum();
        assert_eq!(
            pos_total.abs(),
            neg_total.abs(),
            "symmetry broken at line_height={height}: pos={pos_total}, neg={neg_total}"
        );
    }
}

// ---- prev_char_boundary tests ----

use osc::{
    next_char_boundary, prev_char_boundary, search_word_boundary_left, search_word_boundary_right,
};

#[test]
fn test_prev_char_boundary_start_of_string() {
    assert_eq!(prev_char_boundary("hello", 0), 0);
}

#[test]
fn test_prev_char_boundary_ascii_mid() {
    assert_eq!(prev_char_boundary("hello", 3), 2);
}

#[test]
fn test_prev_char_boundary_multibyte() {
    // "aé" is [0x61, 0xC3, 0xA9] — 'a' at 0, 'é' at 1..3
    assert_eq!(prev_char_boundary("aé", 3), 1);
}

#[test]
fn test_prev_char_boundary_at_boundary() {
    // "aé" at pos=1 (start of 'é') → prev is 'a' at 0
    assert_eq!(prev_char_boundary("aé", 1), 0);
}

#[test]
fn test_prev_char_boundary_pos_one() {
    assert_eq!(prev_char_boundary("ab", 1), 0);
}

// ---- next_char_boundary tests ----

#[test]
fn test_next_char_boundary_end_of_string() {
    assert_eq!(next_char_boundary("hello", 5), 5);
}

#[test]
fn test_next_char_boundary_ascii_mid() {
    assert_eq!(next_char_boundary("hello", 2), 3);
}

#[test]
fn test_next_char_boundary_multibyte() {
    // "aé" — at pos=1 (start of 'é', 2 bytes) → next is 3 (end)
    assert_eq!(next_char_boundary("aé", 1), 3);
}

#[test]
fn test_next_char_boundary_at_last_char() {
    // "ab" at pos=1 → next is 2 (end)
    assert_eq!(next_char_boundary("ab", 1), 2);
}

// ---- search_word_boundary_left tests ----

#[test]
fn test_word_boundary_left_mid_word() {
    // "hello world" at pos=8 (middle of "world") → should go to 6 (start of "world")
    assert_eq!(search_word_boundary_left("hello world", 8), 6);
}

#[test]
fn test_word_boundary_left_at_word_start() {
    // "hello world" at pos=6 (start of "world") → should go to 0 (start of "hello")
    assert_eq!(search_word_boundary_left("hello world", 6), 0);
}

#[test]
fn test_word_boundary_left_with_leading_whitespace() {
    // "  hello" at pos=7 → should go to 2 (start of "hello")
    assert_eq!(search_word_boundary_left("  hello", 7), 2);
}

#[test]
fn test_word_boundary_left_at_start() {
    assert_eq!(search_word_boundary_left("hello", 0), 0);
}

#[test]
fn test_word_boundary_left_all_whitespace() {
    assert_eq!(search_word_boundary_left("   ", 3), 0);
}

#[test]
fn test_word_boundary_left_multiple_words() {
    // "one two three" at pos=13 → start of "three" at 8
    assert_eq!(search_word_boundary_left("one two three", 13), 8);
}

// ---- search_word_boundary_right tests ----

#[test]
fn test_word_boundary_right_mid_word() {
    // "hello world" at pos=2 → end of "hello" + skip space → 6
    assert_eq!(search_word_boundary_right("hello world", 2), 6);
}

#[test]
fn test_word_boundary_right_at_word_end() {
    // "hello world" at pos=5 (space) → skip word (none) → skip space → 6 (start of "world")
    assert_eq!(search_word_boundary_right("hello world", 5), 6);
}

#[test]
fn test_word_boundary_right_at_end() {
    assert_eq!(search_word_boundary_right("hello", 5), 5);
}

#[test]
fn test_word_boundary_right_with_trailing_whitespace() {
    // "hello  " at pos=0 → skip "hello" → skip spaces → 7 (end)
    assert_eq!(search_word_boundary_right("hello  ", 0), 7);
}

#[test]
fn test_word_boundary_right_multiple_words() {
    // "one two three" at pos=0 → end of "one" + skip space → 4
    assert_eq!(search_word_boundary_right("one two three", 0), 4);
}

// ---- OSC accumulator edge case tests ----

#[test]
fn test_osc_accum_osc7_empty_path() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"\x1b]7;file:///\x07");
    assert_eq!(events.len(), 1);
    assert_cwd_event(events.first().expect("one event"), "/");
}

#[test]
fn test_osc52_invalid_base64() {
    // Invalid base64 should not produce a result
    let data = b"\x1b]52;c;!!!invalid!!!\x07";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    assert!(scan_osc52(&mut state, &mut buf, data).is_none());
}

#[test]
fn test_osc52_query_no_data() {
    // OSC 52 query: ESC ] 52 ; c BEL — no semicolon after target, so no data
    let data = b"\x1b]52;c\x07";
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    assert!(scan_osc52(&mut state, &mut buf, data).is_none());
}

#[test]
fn test_osc_accum_empty_feed() {
    let mut accum = OscAccum::default();
    let events = accum.feed(b"");
    assert!(events.is_empty());
}

#[test]
fn test_osc52_large_payload() {
    // Large base64 payload (1000 'A's = "QUFB..." repeated)
    let payload = "QUFB".repeat(250); // decodes to "AAA..." (750 bytes)
    let mut data = Vec::new();
    data.extend_from_slice(b"\x1b]52;c;");
    data.extend_from_slice(payload.as_bytes());
    data.push(0x07);
    let mut state = Osc52State::default();
    let mut buf = Vec::new();
    let result = scan_osc52(&mut state, &mut buf, &data);
    assert!(result.is_some());
    let text = result.expect("decoded text");
    assert_eq!(text.len(), 750);
    assert!(text.chars().all(|c| c == 'A'));
}

// ---- consumed_mods logic tests ----

#[test]
fn test_consumed_mods_unshifted_zero() {
    // unshifted=0 always returns empty regardless of shift
    let mods = consumed_mods_helper(0, true);
    assert_eq!(mods, libghostty_vt::key::Mods::empty());
}

#[test]
fn test_consumed_mods_unshifted_with_shift() {
    // unshifted!=0 and shift held → SHIFT consumed
    let mods = consumed_mods_helper(0x41, true);
    assert_eq!(mods, libghostty_vt::key::Mods::SHIFT);
}

#[test]
fn test_consumed_mods_unshifted_without_shift() {
    // unshifted!=0 but shift not held → empty
    let mods = consumed_mods_helper(0x41, false);
    assert_eq!(mods, libghostty_vt::key::Mods::empty());
}

/// Helper that mirrors the logic from `App::consumed_mods_for_key`.
fn consumed_mods_helper(unshifted: u32, shift_held: bool) -> libghostty_vt::key::Mods {
    if unshifted != 0 && shift_held {
        libghostty_vt::key::Mods::SHIFT
    } else {
        libghostty_vt::key::Mods::empty()
    }
}

// ---- DisplayConfig field defaults tests ----

#[test]
fn test_display_config_alternate_scroll_mode_default() {
    let d = DisplayConfig::default();
    assert!(
        d.flags.alternate_scroll_mode(),
        "alternate_scroll_mode should default to true"
    );
}

#[test]
fn test_display_config_scroll_multiplier_default() {
    let d = DisplayConfig::default();
    assert!(
        (d.scroll_multiplier - 3.0).abs() < f32::EPSILON,
        "scroll_multiplier should default to 3.0"
    );
}

#[test]
fn test_display_config_hide_when_typing_default() {
    let d = DisplayConfig::default();
    assert!(
        !d.flags.hide_when_typing(),
        "hide_when_typing should default to false"
    );
}

// ---- Scroll accumulator with varying line heights ----

#[test]
fn test_scroll_accum_line_height_1() {
    // Minimum line height: every pixel produces a step
    let steps = simulate_scroll_accum_with_height(&[3.0], 1.0);
    assert_eq!(steps, vec![3], "3px with 1px line height = 3 steps");
}

#[test]
fn test_scroll_accum_line_height_40() {
    // Large font: need more pixels per step
    let steps = simulate_scroll_accum_with_height(&[20.0, 20.0, 20.0], 40.0);
    // 20: 0 steps, 40: 1 step, 60: 0 steps (residual 20)
    assert_eq!(
        steps,
        vec![0, 1, 0],
        "40px line height needs more accumulation"
    );
}

#[test]
fn test_scroll_accum_speed_proportional_to_height() {
    // Same pixel delta should produce fewer steps with larger line height
    let deltas = vec![100.0];
    let steps_small = simulate_scroll_accum_with_height(&deltas, 10.0);
    let steps_large = simulate_scroll_accum_with_height(&deltas, 40.0);
    let total_small: i32 = steps_small.iter().sum();
    let total_large: i32 = steps_large.iter().sum();
    assert_eq!(total_small, 10, "100px / 10px = 10 steps");
    assert_eq!(total_large, 2, "100px / 40px = 2 steps");
    assert!(
        total_small > total_large,
        "smaller line height should produce more steps"
    );
}
