use super::*;

#[test]
fn test_default_config() {
    let cfg = Config::default();
    assert!((cfg.font.size - 16.0).abs() < f32::EPSILON);
    assert_eq!(cfg.terminal.scrollback, 10000);
    assert!(cfg.terminal.cursor_blink);
    assert!(!cfg.input.bold_is_bright);
}

#[test]
fn test_parse_empty() {
    let cfg = Config::parse("");
    assert!((cfg.font.size - 16.0).abs() < f32::EPSILON);
}

#[test]
fn test_parse_values() {
    let input = r"
# comment
font-size = 14.0
scrollback = 5000
shell = /bin/zsh
cursor-blink = false
bold-is-bright = false
cols = 120
rows = 40
";
    let cfg = Config::parse(input);
    assert!((cfg.font.size - 14.0 * PT_TO_PX).abs() < 0.01);
    assert_eq!(cfg.terminal.scrollback, 5000);
    assert_eq!(cfg.terminal.shell.as_deref(), Some("/bin/zsh"));
    assert!(!cfg.terminal.cursor_blink);
    assert!(!cfg.input.bold_is_bright);
    assert_eq!(cfg.window.initial_cols, 120);
    assert_eq!(cfg.window.initial_rows, 40);
}

#[test]
fn test_parse_clamping() {
    let cfg_high = Config::parse("font-size = 200.0");
    assert!((cfg_high.font.size - 96.0).abs() < f32::EPSILON);

    let cfg_low = Config::parse("font-size = 1.0");
    assert!((cfg_low.font.size - 8.0).abs() < f32::EPSILON);
}

#[test]
fn test_parse_unknown_keys_ignored() {
    let cfg = Config::parse("unknown_key = value\nfont-size = 20.0");
    assert!((cfg.font.size - 20.0 * PT_TO_PX).abs() < 0.01);
}

#[test]
fn test_parse_colors() {
    let input = r"
foreground = #d4be98
background = #282828
cursor-color = #d4be98
color0 = #282828
color1 = #ea6962
color15 = #d4be98
";
    let cfg = Config::parse(input);
    let fg = cfg.colors.foreground.expect("foreground");
    assert_eq!(fg.r, 0xd4);
    assert_eq!(fg.g, 0xbe);
    assert_eq!(fg.b, 0x98);

    let bg = cfg.colors.background.expect("background");
    assert_eq!(bg.r, 0x28);

    let c0 = cfg.colors.palette[0].expect("color0");
    assert_eq!(c0.r, 0x28);

    let c1 = cfg.colors.palette[1].expect("color1");
    assert_eq!(c1.r, 0xea);

    let c15 = cfg.colors.palette[15].expect("color15");
    assert_eq!(c15.r, 0xd4);

    assert!(cfg.colors.palette[2].is_none());
}

#[test]
fn test_parse_opacity() {
    let cfg_normal = Config::parse("opacity = 0.8");
    assert!((cfg_normal.colors.opacity - 0.8).abs() < f32::EPSILON);

    // Clamping
    let cfg_high = Config::parse("opacity = 2.0");
    assert!((cfg_high.colors.opacity - 1.0).abs() < f32::EPSILON);

    let cfg_neg = Config::parse("opacity = -0.5");
    assert!(cfg_neg.colors.opacity.abs() < f32::EPSILON);

    // Alternative key name
    let cfg_alt = Config::parse("background-opacity = 0.5");
    assert!((cfg_alt.colors.opacity - 0.5).abs() < f32::EPSILON);
}

#[test]
fn test_color_osc_sequences() {
    let mut cfg = Config {
        colors: ColorConfig {
            foreground: Some(Color {
                r: 0xff,
                g: 0x00,
                b: 0x00,
            }),
            ..ColorConfig::default()
        },
        ..Config::default()
    };
    if let Some(slot) = cfg.colors.palette.get_mut(0) {
        *slot = Some(Color {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        });
    }

    let seqs = cfg.color_osc_sequences();
    let text = String::from_utf8(seqs).expect("valid utf8");
    assert!(text.contains("\x1b]10;#ff0000\x1b\\"));
    assert!(text.contains("\x1b]4;0;#112233\x1b\\"));
    assert!(!text.contains("\x1b]11;")); // no background set
}

#[test]
fn test_parse_title() {
    let cfg_title = Config::parse("title = My Terminal");
    assert_eq!(cfg_title.window.title, "My Terminal");

    let cfg_window = Config::parse("window-title = Custom");
    assert_eq!(cfg_window.window.title, "Custom");

    // Default
    let cfg_default = Config::default();
    assert_eq!(cfg_default.window.title, "hs");
}

#[test]
fn test_parse_app_id() {
    let cfg_dash = Config::parse("app-id = my-term");
    assert_eq!(cfg_dash.window.app_id, "my-term");

    let cfg_under = Config::parse("app_id = other");
    assert_eq!(cfg_under.window.app_id, "other");

    // Default
    let cfg_default = Config::default();
    assert_eq!(cfg_default.window.app_id, "hs");
}

#[test]
fn test_parse_term() {
    let cfg_foot = Config::parse("term = foot");
    assert_eq!(cfg_foot.terminal.term.as_deref(), Some("foot"));

    let cfg_xterm = Config::parse("term-program = xterm-256color");
    assert_eq!(cfg_xterm.terminal.term.as_deref(), Some("xterm-256color"));

    // Default — None (inherit from parent)
    let cfg_default = Config::default();
    assert!(cfg_default.terminal.term.is_none());
}

#[test]
fn test_padding_clamped_to_100() {
    let cfg = Config::parse("padding = 200");
    assert_eq!(cfg.window.padding, 100);
}

#[test]
fn test_padding_normal() {
    let cfg = Config::parse("padding = 10");
    assert_eq!(cfg.window.padding, 10);
}

#[test]
fn test_padding_zero() {
    let cfg = Config::parse("padding = 0");
    assert_eq!(cfg.window.padding, 0);
}

#[test]
fn test_blink_interval_too_low() {
    let cfg = Config::parse("cursor-blink-interval = 50");
    assert_eq!(cfg.terminal.cursor_blink_interval_ms, 100);
}

#[test]
fn test_blink_interval_too_high() {
    let cfg = Config::parse("cursor-blink-interval = 5000");
    assert_eq!(cfg.terminal.cursor_blink_interval_ms, 2000);
}

#[test]
fn test_blink_interval_normal() {
    let cfg = Config::parse("cursor-blink-interval = 600");
    assert_eq!(cfg.terminal.cursor_blink_interval_ms, 600);
}

#[test]
fn test_empty_title_keeps_default() {
    let cfg = Config::parse("title = ");
    assert_eq!(cfg.window.title, "hs");
}

#[test]
fn test_empty_app_id_keeps_default() {
    let cfg = Config::parse("app-id = ");
    assert_eq!(cfg.window.app_id, "hs");
}

#[test]
fn test_color_osc_cursor_color() {
    let cfg = Config {
        colors: ColorConfig {
            cursor: Some(Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            }),
            ..ColorConfig::default()
        },
        ..Config::default()
    };
    let seqs = cfg.color_osc_sequences();
    let text = String::from_utf8(seqs).expect("valid utf8");
    assert!(text.contains("\x1b]12;#aabbcc\x1b\\"));
}

#[test]
fn test_color_osc_all_palette() {
    let mut cfg = Config::default();
    for (i, slot) in cfg.colors.palette.iter_mut().enumerate() {
        *slot = Some(Color {
            r: u8::try_from(i).expect("fits"),
            g: u8::try_from(i * 2).expect("fits"),
            b: u8::try_from(i * 3).expect("fits"),
        });
    }
    let seqs = cfg.color_osc_sequences();
    let text = String::from_utf8(seqs).expect("valid utf8");
    for i in 0..16 {
        let expected_seq = format!("\x1b]4;{i};");
        assert!(text.contains(&expected_seq), "missing palette {i}");
    }
}

#[test]
fn test_parse_multiple_sections() {
    let input = r"
font-size = 18.0
scrollback = 2000
padding = 5
opacity = 0.9
foreground = #ffffff
background = #000000
cursor-blink = false
bold-is-bright = false
title = My Term
app-id = myterm
";
    let cfg = Config::parse(input);
    assert!((cfg.font.size - 18.0 * PT_TO_PX).abs() < 0.01);
    assert_eq!(cfg.terminal.scrollback, 2000);
    assert_eq!(cfg.window.padding, 5);
    assert!((cfg.colors.opacity - 0.9).abs() < f32::EPSILON);
    assert!(cfg.colors.foreground.is_some());
    assert!(cfg.colors.background.is_some());
    assert!(!cfg.terminal.cursor_blink);
    assert!(!cfg.input.bold_is_bright);
    assert_eq!(cfg.window.title, "My Term");
    assert_eq!(cfg.window.app_id, "myterm");
}

#[test]
fn test_foot_ini_sections_skipped() {
    let input = "[colors]\nforeground=839496\n[scrollback]\nlines=50000\n";
    let cfg = Config::parse(input);
    assert!(cfg.colors.foreground.is_some());
    assert_eq!(cfg.colors.foreground.expect("fg").r, 0x83);
    assert_eq!(cfg.terminal.scrollback, 50000);
}

#[test]
fn test_foot_font_syntax() {
    let cfg = Config::parse("font=JetBrains Mono:size=11");
    // 11pt * 96/72 = 14.666...px
    assert!((cfg.font.size - 11.0 * 96.0 / 72.0).abs() < 0.01);
}

#[test]
fn test_foot_font_size_only() {
    // No :size= part — font-size unchanged
    let cfg = Config::parse("font=JetBrains Mono");
    assert!((cfg.font.size - 16.0).abs() < f32::EPSILON);
}

#[test]
fn test_foot_regular_colors() {
    let input = "regular0=073642\nregular7=eee8d5\n";
    let cfg = Config::parse(input);
    let c0 = cfg.colors.palette[0].expect("regular0");
    assert_eq!(c0.r, 0x07);
    assert_eq!(c0.g, 0x36);
    assert_eq!(c0.b, 0x42);
    let c7 = cfg.colors.palette[7].expect("regular7");
    assert_eq!(c7.r, 0xee);
}

#[test]
fn test_foot_bright_colors() {
    let input = "bright0=002b36\nbright7=fdf6e3\n";
    let cfg = Config::parse(input);
    let c8 = cfg.colors.palette[8].expect("bright0 → color8");
    assert_eq!(c8.r, 0x00);
    assert_eq!(c8.g, 0x2b);
    let c15 = cfg.colors.palette[15].expect("bright7 → color15");
    assert_eq!(c15.r, 0xfd);
}

#[test]
fn test_foot_cursor_dual_value() {
    let cfg = Config::parse("cursor= 002b36 93a1a1");
    let cc = cfg.colors.cursor.expect("cursor color");
    assert_eq!(cc.r, 0x93);
    assert_eq!(cc.g, 0xa1);
    assert_eq!(cc.b, 0xa1);
}

#[test]
fn test_foot_cursor_single_value() {
    let cfg = Config::parse("cursor=93a1a1");
    let cc = cfg.colors.cursor.expect("cursor color");
    assert_eq!(cc.r, 0x93);
}

#[test]
fn test_foot_alpha() {
    let cfg = Config::parse("alpha=0.8");
    assert!((cfg.colors.opacity - 0.8).abs() < f32::EPSILON);
}

#[test]
fn test_foot_lines_as_scrollback() {
    let cfg = Config::parse("lines=50000");
    assert_eq!(cfg.terminal.scrollback, 50000);
}

#[test]
fn test_foot_bold_text_in_bright() {
    let cfg_yes = Config::parse("bold-text-in-bright=yes");
    assert!(cfg_yes.input.bold_is_bright);

    let cfg_no = Config::parse("bold-text-in-bright=no");
    assert!(!cfg_no.input.bold_is_bright);
}

#[test]
fn test_foot_ignored_keys_no_panic() {
    let input = r"
locked-title=no
dpi-aware=no
selection-target=both
selection-foreground=93a1a1
selection-background=073642
login-shell=no
word-delimiters=,│`|
hide-when-typing=no
alternate-scroll-mode=yes
preferred=server
";
    let cfg = Config::parse(input);
    // Should parse without panic and not affect defaults
    assert_eq!(cfg.window.title, "hs");
}

#[test]
fn test_foot_full_config() {
    let input = r"
title=foot
locked-title=no
font=JetBrains Mono:size=11
dpi-aware=no
selection-target=both
[scrollback]
lines=50000
[colors]
cursor= 002b36 93a1a1
background= 002b36
foreground= 839496
regular0=   073642
regular1=   dc322f
regular2=   859900
regular3=   b58900
regular4=   268bd2
regular5=   d33682
regular6=   2aa198
regular7=   eee8d5
bright0=    002b36
bright1=    cb4b16
bright2=    586e75
bright3=    657b83
bright4=    839496
bright5=    6c71c4
bright6=    93a1a1
bright7=    fdf6e3
selection-foreground=93a1a1
selection-background=073642
";
    let cfg = Config::parse(input);
    assert_eq!(cfg.window.title, "foot");
    assert!((cfg.font.size - 11.0 * PT_TO_PX).abs() < 0.01);
    assert_eq!(cfg.terminal.scrollback, 50000);

    let fg = cfg.colors.foreground.expect("foreground");
    assert_eq!(fg.r, 0x83);

    let bg = cfg.colors.background.expect("background");
    assert_eq!(bg.r, 0x00);
    assert_eq!(bg.g, 0x2b);

    let cc = cfg.colors.cursor.expect("cursor color");
    assert_eq!(cc.r, 0x93);

    // All 16 palette colors should be set
    for (i, slot) in cfg.colors.palette.iter().enumerate() {
        assert!(slot.is_some(), "palette[{i}] should be set");
    }

    // Spot-check: regular0 -> color0
    let c0 = cfg.colors.palette.first().expect("slot 0").expect("color0");
    assert_eq!(c0.r, 0x07);

    // Spot-check: bright7 -> color15
    let c15 = cfg
        .colors
        .palette
        .get(15)
        .expect("slot 15")
        .expect("color15");
    assert_eq!(c15.r, 0xfd);
}

#[test]
fn test_parse_foot_font_size() {
    assert_eq!(parse_foot_font_size("JetBrains Mono:size=11"), Some(11.0));
    assert_eq!(parse_foot_font_size("Mono:size=14.5"), Some(14.5));
    assert_eq!(parse_foot_font_size("JetBrains Mono"), None);
    assert_eq!(parse_foot_font_size("size=12"), Some(12.0));
}

#[test]
fn test_default_shell_is_none() {
    let cfg = Config::default();
    assert!(cfg.terminal.shell.is_none(), "default shell should be None");
}

#[test]
fn test_default_term_is_none() {
    let cfg = Config::default();
    assert!(cfg.terminal.term.is_none(), "default term should be None");
}

#[test]
fn test_color_osc_sequences_with_colors() {
    let cfg = Config {
        colors: ColorConfig {
            foreground: Some(Color { r: 255, g: 0, b: 0 }),
            background: Some(Color { r: 0, g: 255, b: 0 }),
            cursor: Some(Color { r: 0, g: 0, b: 255 }),
            ..ColorConfig::default()
        },
        ..Config::default()
    };
    let seqs = cfg.color_osc_sequences();
    let text = String::from_utf8(seqs).expect("valid utf8");
    // OSC 10 for foreground
    assert!(
        text.contains("\x1b]10;#ff0000\x1b\\"),
        "should contain OSC 10 for foreground"
    );
    // OSC 11 for background
    assert!(
        text.contains("\x1b]11;#00ff00\x1b\\"),
        "should contain OSC 11 for background"
    );
    // OSC 12 for cursor color
    assert!(
        text.contains("\x1b]12;#0000ff\x1b\\"),
        "should contain OSC 12 for cursor color"
    );
}

#[test]
fn test_color_osc_sequences_empty() {
    let cfg = Config::default();
    let seqs = cfg.color_osc_sequences();
    assert!(
        seqs.is_empty(),
        "default config should produce no OSC sequences"
    );
}

#[test]
fn test_initial_cols_rows_default() {
    let cfg = Config::default();
    assert_eq!(cfg.window.initial_cols, 0);
    assert_eq!(cfg.window.initial_rows, 0);
}

#[test]
fn test_opacity_default() {
    let cfg = Config::default();
    assert!(
        (cfg.colors.opacity - 1.0).abs() < f32::EPSILON,
        "default opacity should be 1.0 (fully opaque)"
    );
}

#[test]
fn test_font_size_from_ini_line() {
    let mut cfg = Config::default();
    Config::apply_setting(&mut cfg, "font", "monospace:size=8");
    assert!((cfg.font.size - 8.0 * PT_TO_PX).abs() < 0.01);

    Config::apply_setting(&mut cfg, "font", "monospace:size=72");
    assert!((cfg.font.size - 72.0 * PT_TO_PX).abs() < 0.01);
}

#[test]
fn test_apply_setting_garbage_no_crash() {
    let mut cfg = Config::default();
    Config::apply_setting(&mut cfg, "", "");
    Config::apply_setting(&mut cfg, "unknown-key", "unknown-value");
    Config::apply_setting(&mut cfg, "font", "");
    Config::apply_setting(&mut cfg, "pad", "not-a-number");
    assert_eq!(cfg.window.padding, Config::default().window.padding);
    // pad=10x10x10 parses horizontal value (10) from foot's NxM syntax
    Config::apply_setting(&mut cfg, "pad", "10x10x10");
    assert_eq!(cfg.window.padding, 10);
}

#[test]
fn test_parse_selection_colors() {
    let input = "[colors]\nselection-foreground=93a1a1\nselection-background=073642\n";
    let cfg = Config::parse(input);
    let foreground = cfg.colors.selection_fg.expect("selection_fg should be set");
    assert_eq!(foreground.r, 0x93);
    assert_eq!(foreground.g, 0xa1);
    assert_eq!(foreground.b, 0xa1);
    let background = cfg.colors.selection_bg.expect("selection_bg should be set");
    assert_eq!(background.r, 0x07);
    assert_eq!(background.g, 0x36);
    assert_eq!(background.b, 0x42);
}

#[test]
fn test_config_malformed_ini_no_panic() {
    let gibberish = "===\n\x00\x01\x02\n[[[[\n}}}\nkey\nkey=\n=value\n🎉=🎉\n";
    let cfg = Config::parse(gibberish);
    // Should fall back to defaults without panicking
    assert!((cfg.font.size - 16.0).abs() < f32::EPSILON);
}

#[test]
fn test_config_extreme_font_sizes() {
    // Negative should clamp to minimum (8.0px)
    let cfg_neg = Config::parse("font-size = -1.0");
    assert!(
        (cfg_neg.font.size - 8.0).abs() < f32::EPSILON,
        "negative clamped to 8.0"
    );

    // Very large should clamp to maximum (96.0px)
    let cfg_huge = Config::parse("font-size = 99999.0");
    assert!(
        (cfg_huge.font.size - 96.0).abs() < f32::EPSILON,
        "huge clamped to 96.0"
    );

    // Non-numeric should keep default
    let cfg_nan = Config::parse("font-size = not_a_number");
    assert!(
        (cfg_nan.font.size - 16.0).abs() < f32::EPSILON,
        "non-numeric keeps default"
    );

    // Zero should clamp to minimum
    let cfg_zero = Config::parse("font-size = 0.0");
    assert!(
        (cfg_zero.font.size - 8.0).abs() < f32::EPSILON,
        "zero clamped to 8.0"
    );
}

#[test]
fn test_color_osc_sequences_valid_bytes() {
    let cfg = Config {
        colors: ColorConfig {
            foreground: Some(Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            }),
            background: Some(Color {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            }),
            cursor: Some(Color {
                r: 0x44,
                g: 0x55,
                b: 0x66,
            }),
            ..ColorConfig::default()
        },
        ..Config::default()
    };
    let seqs = cfg.color_osc_sequences();
    // Each OSC sequence should start with ESC ] and end with ESC backslash
    let text = String::from_utf8(seqs).expect("valid utf8");
    let osc_count = text.matches("\x1b]").count();
    let st_count = text.matches("\x1b\\").count();
    assert_eq!(osc_count, 3, "should have 3 OSC sequences (fg, bg, cursor)");
    assert_eq!(st_count, 3, "should have 3 ST terminators");
}

#[test]
fn test_config_duplicate_last_wins() {
    let input = "font-size = 12.0\nfont-size = 20.0\n";
    let cfg = Config::parse(input);
    assert!(
        (cfg.font.size - 20.0 * PT_TO_PX).abs() < 0.01,
        "last value should win"
    );
}

#[test]
fn test_config_utf8_bom() {
    let input = "\u{FEFF}font-size = 14.0\n";
    let cfg = Config::parse(input);
    // BOM might not break parsing (line starts with BOM + key)
    // Just ensure no panic
    let _ = cfg.font.size;
}

#[test]
fn test_config_font_spaces() {
    let input = "font = JetBrains Mono:size=12\n";
    let cfg = Config::parse(input);
    assert_eq!(cfg.font.family.as_deref(), Some("JetBrains Mono"));
    assert!((cfg.font.size - 12.0 * PT_TO_PX).abs() < 0.01);
}

#[test]
fn test_config_empty_value() {
    let input = "shell=\nfont-size=18\n";
    let cfg = Config::parse(input);
    // Empty shell should not set the field
    assert!(
        cfg.terminal.shell.is_none(),
        "empty value should not set shell"
    );
    assert!((cfg.font.size - 18.0 * PT_TO_PX).abs() < 0.01);
}

#[test]
fn test_config_key_bindings_section() {
    let input = "\
font-size=14
[key-bindings]
clipboard-copy=Control+Shift+y
fullscreen=none
[colors]
foreground=ffffff
";
    let cfg = Config::parse(input);
    // Copy should now be on Ctrl+Shift+Y
    assert_eq!(
        cfg.bindings.lookup(0x0079, true, true),
        Some(KeyAction::Copy)
    );
    // Old Ctrl+Shift+C should no longer be Copy
    assert_ne!(
        cfg.bindings.lookup(0x0063, true, true),
        Some(KeyAction::Copy)
    );
    // Fullscreen was unbound
    assert!(cfg.bindings.lookup(0xffc8, false, false).is_none());
    // Config values outside key-bindings still parsed
    assert!((cfg.font.size - 14.0 * PT_TO_PX).abs() < 0.01);
    assert!(cfg.colors.foreground.is_some());
}

#[test]
fn test_load_from_nonexistent_returns_default() {
    let path = std::path::Path::new("/tmp/hand_test_nonexistent_config_file.ini");
    // Ensure it doesn't exist
    let _ = std::fs::remove_file(path);
    let cfg = Config::load_from(path);
    assert!((cfg.font.size - 16.0).abs() < f32::EPSILON);
    assert_eq!(cfg.terminal.scrollback, 10000);
}

#[test]
fn test_load_from_valid_file() {
    let path = std::env::temp_dir().join("hand_test_load_from_valid.ini");
    std::fs::write(&path, "font-size = 22.0\nscrollback = 3000\n").expect("write temp");
    let cfg = Config::load_from(&path);
    assert!((cfg.font.size - 22.0 * PT_TO_PX).abs() < 0.01);
    assert_eq!(cfg.terminal.scrollback, 3000);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_check_valid_file() {
    let path = std::env::temp_dir().join("hand_test_check_valid.ini");
    std::fs::write(&path, "font-size = 14.0\nscrollback = 5000\n").expect("write temp");
    let result = Config::check(&path);
    assert!(result.is_ok(), "valid config should pass check");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_check_invalid_file_missing_equals() {
    let path = std::env::temp_dir().join("hand_test_check_invalid.ini");
    std::fs::write(&path, "this line has no separator\nfont-size = 14.0\n").expect("write temp");
    let result = Config::check(&path);
    let Err(errors) = result else {
        let _ = std::fs::remove_file(&path);
        panic!("invalid config should fail check");
    };
    assert!(!errors.is_empty());
    assert!(errors.first().expect("has error").contains("missing '='"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_check_nonexistent_file() {
    let path = std::path::Path::new("/tmp/hand_test_check_nonexistent.ini");
    let _ = std::fs::remove_file(path);
    let result = Config::check(path);
    let Err(errors) = result else {
        panic!("nonexistent config should fail check");
    };
    assert!(errors.first().expect("has error").contains("not found"));
}

#[test]
fn test_apply_override_valid_key_value() {
    let mut cfg = Config::default();
    cfg.apply_override("font-size=20.0");
    assert!((cfg.font.size - 20.0 * PT_TO_PX).abs() < 0.01);
}

#[test]
fn test_apply_override_section_prefix() {
    let mut cfg = Config::default();
    cfg.apply_override("colors.foreground=#ff0000");
    let fg = cfg.colors.foreground.expect("foreground should be set");
    assert_eq!(fg.r, 0xff);
    assert_eq!(fg.g, 0x00);
    assert_eq!(fg.b, 0x00);
}

#[test]
fn test_apply_override_missing_equals() {
    let mut cfg = Config::default();
    let original_size = cfg.font.size;
    cfg.apply_override("font-size20.0");
    // Should not change anything
    assert!((cfg.font.size - original_size).abs() < f32::EPSILON);
}

#[test]
fn test_apply_key_binding_unknown_action() {
    let mut cfg = Config::default();
    // Unknown action name should be silently ignored
    Config::apply_key_binding(&mut cfg, "totally-unknown-action", "Control+Shift+x");
    // Bindings should still have default entries intact
    assert_eq!(
        cfg.bindings.lookup(0x0063, true, true),
        Some(KeyAction::Copy)
    );
}

#[test]
fn test_selection_target_default_fallback() {
    // An unrecognized value should fall back to Primary
    let cfg = Config::parse("selection-target = something-else");
    assert_eq!(cfg.input.selection_target, SelectionTarget::Primary);
}

#[test]
fn test_selection_target_known_values() {
    let cfg_none = Config::parse("selection-target = none");
    assert_eq!(cfg_none.input.selection_target, SelectionTarget::None);

    let cfg_clip = Config::parse("selection-target = clipboard");
    assert_eq!(cfg_clip.input.selection_target, SelectionTarget::Clipboard);

    let cfg_both = Config::parse("selection-target = both");
    assert_eq!(cfg_both.input.selection_target, SelectionTarget::Both);
}

#[test]
fn test_notify_command() {
    let cfg = Config::parse("notify = notify-send bell");
    assert_eq!(
        cfg.terminal.notify_command.as_deref(),
        Some("notify-send bell")
    );
}

#[test]
fn test_notify_empty_ignored() {
    let cfg = Config::parse("notify = ");
    assert!(cfg.terminal.notify_command.is_none());
}

#[test]
fn test_resize_delay_ms() {
    let cfg = Config::parse("resize-delay-ms = 50");
    assert_eq!(cfg.window.resize_delay_ms, 50);
}

#[test]
fn test_resize_delay_ms_clamped() {
    let cfg = Config::parse("resize-delay-ms = 5000");
    assert_eq!(cfg.window.resize_delay_ms, 1000);
}

#[test]
fn test_initial_window_size_pixels() {
    let cfg = Config::parse("initial-window-size-pixels = 800x600");
    assert_eq!(cfg.window.initial_size_pixels, Some((800, 600)));
}

#[test]
fn test_initial_window_size_chars() {
    let cfg = Config::parse("initial-window-size-chars = 120x40");
    assert_eq!(cfg.window.initial_size_chars, Some((120, 40)));
}

#[test]
fn test_initial_window_size_chars_large_clamped_to_u16() {
    // Values larger than u16::MAX should clamp
    let cfg = Config::parse("initial-window-size-chars = 100000x100000");
    let (cols, rows) = cfg.window.initial_size_chars.expect("should parse");
    assert_eq!(cols, u16::MAX);
    assert_eq!(rows, u16::MAX);
}

#[test]
fn test_initial_window_mode_maximized() {
    let cfg = Config::parse("initial-window-mode = maximized");
    assert_eq!(cfg.window.initial_window_mode, WindowMode::Maximized);
}

#[test]
fn test_initial_window_mode_fullscreen() {
    let cfg = Config::parse("initial-window-mode = fullscreen");
    assert_eq!(cfg.window.initial_window_mode, WindowMode::Fullscreen);
}

#[test]
fn test_initial_window_mode_unknown_defaults_to_windowed() {
    let cfg = Config::parse("initial-window-mode = something");
    assert_eq!(cfg.window.initial_window_mode, WindowMode::Windowed);
}

#[test]
fn test_line_height() {
    let cfg = Config::parse("line-height = 2.5");
    assert!((cfg.font.line_height - 2.5).abs() < f32::EPSILON);
}

#[test]
fn test_line_height_clamped() {
    let cfg_high = Config::parse("line-height = 100.0");
    assert!((cfg_high.font.line_height - 50.0).abs() < f32::EPSILON);

    let cfg_low = Config::parse("line-height = -20.0");
    assert!((cfg_low.font.line_height - (-10.0)).abs() < f32::EPSILON);
}

#[test]
fn test_letter_spacing() {
    let cfg = Config::parse("letter-spacing = 1.5");
    assert!((cfg.font.letter_spacing - 1.5).abs() < f32::EPSILON);
}

#[test]
fn test_letter_spacing_clamped() {
    let cfg_high = Config::parse("letter-spacing = 50.0");
    assert!((cfg_high.font.letter_spacing - 20.0).abs() < f32::EPSILON);

    let cfg_low = Config::parse("letter-spacing = -10.0");
    assert!((cfg_low.font.letter_spacing - (-5.0)).abs() < f32::EPSILON);
}

#[test]
fn test_font_bold() {
    let cfg = Config::parse("font-bold = FiraCode Nerd Font:size=12");
    assert_eq!(cfg.font.bold.as_deref(), Some("FiraCode Nerd Font"));
}

#[test]
fn test_font_italic() {
    let cfg = Config::parse("font-italic = JetBrains Mono Italic:size=12");
    assert_eq!(cfg.font.italic.as_deref(), Some("JetBrains Mono Italic"));
}

#[test]
fn test_font_bold_italic() {
    let cfg = Config::parse("font-bold-italic = Mono Bold Italic:size=14");
    assert_eq!(cfg.font.bold_italic.as_deref(), Some("Mono Bold Italic"));
}

#[test]
fn test_scroll_multiplier() {
    let cfg = Config::parse("multiplier = 5.0");
    assert!((cfg.input.scroll_multiplier - 5.0).abs() < f32::EPSILON);
}

#[test]
fn test_scroll_multiplier_clamped() {
    let cfg_high = Config::parse("multiplier = 100.0");
    assert!((cfg_high.input.scroll_multiplier - 50.0).abs() < f32::EPSILON);

    let cfg_low = Config::parse("multiplier = 0.01");
    assert!((cfg_low.input.scroll_multiplier - 0.1).abs() < f32::EPSILON);
}

#[test]
fn test_dim_palette_colors() {
    use std::fmt::Write;
    let mut lines = String::new();
    for i in 0..8 {
        let _ = writeln!(lines, "dim{i} = #{i:02x}{i:02x}{i:02x}");
    }
    let cfg = Config::parse(&lines);
    for i in 0..8 {
        let c = cfg
            .colors
            .dim_palette
            .get(i)
            .expect("slot")
            .expect("color set");
        assert_eq!(c.r, u8::try_from(i).expect("fits"), "dim{i} red channel");
    }
}

#[test]
fn test_dim_palette_out_of_range_ignored() {
    // dim8 and dim9 are out of the 0..8 range, should be ignored
    let cfg = Config::parse("dim8 = #ffffff\ndim9 = #ffffff\n");
    // dim_palette should still be all None
    for (i, slot) in cfg.colors.dim_palette.iter().enumerate() {
        assert!(slot.is_none(), "dim_palette[{i}] should be None");
    }
}

#[test]
fn test_parse_size_u32_valid() {
    assert_eq!(parse_size_u32("100x200"), Some((100, 200)));
    assert_eq!(parse_size_u32("1920x1080"), Some((1920, 1080)));
}

#[test]
fn test_parse_size_u32_invalid() {
    assert!(parse_size_u32("abc").is_none());
    assert!(parse_size_u32("100").is_none());
    assert!(parse_size_u32("x200").is_none());
    assert!(parse_size_u32("100x").is_none());
    assert!(parse_size_u32("").is_none());
}

#[test]
fn test_parse_size_u32_zero_rejected() {
    assert!(parse_size_u32("0x200").is_none());
    assert!(parse_size_u32("100x0").is_none());
    assert!(parse_size_u32("0x0").is_none());
}

#[test]
fn test_config_path_returns_foot_ini() {
    let path = config_path();
    // Regardless of env vars, config_path always ends with foot/foot.ini
    assert!(
        path.ends_with("foot/foot.ini"),
        "config_path should end with foot/foot.ini, got: {}",
        path.display(),
    );
}

#[test]
fn test_default_path_matches_config_path() {
    assert_eq!(Config::default_path(), config_path());
}
