# 5. Building Block View

## Container View (C4 Level 2)

horseshoe is a single binary with an embedded library crate.

```mermaid
C4Container
    title Container View - horseshoe

    Person(user, "User")
    System_Ext(compositor, "Wayland Compositor")
    System_Ext(shell, "Shell")

    Container_Boundary(hs, "horseshoe binary") {
        Component(main, "main.rs", "Binary", "calloop event loop, timers")
        Component(wayland, "wayland/", "Binary", "sctk App, Wayland handlers")
        Component(lib, "lib.rs", "Library", "10 modules")
    }

    Rel(user, wayland, "Input events")
    Rel(wayland, compositor, "wl_shm, xdg_shell")
    Rel(main, shell, "PTY fork/exec")
    Rel(main, lib, "Uses")
    Rel(wayland, lib, "Uses")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

## Component View (C4 Level 3)

The library crate exposes 10 modules:

```mermaid
C4Component
    title Component View - horseshoe library

    Container_Boundary(lib, "horseshoe library (src/lib.rs)") {
        Component(terminal, "ghostty/terminal.rs", "Terminal + TerminalCb", "VT state, callbacks, CallbackState")
        Component(render_state, "ghostty/render.rs", "RenderState", "Cell grid access, CellStyle, color resolution")
        Component(input, "ghostty/input.rs", "KeyEncoder + MouseEncoder", "Key/mouse event encoding for VT")
        Component(keymap, "keymap.rs", "Keymap", "XKB keysym to key::Key mapping")
        Component(renderer, "renderer/", "Renderer", "SHM pixel rendering, damage tracking, blending")
        Component(config, "config/", "Config", "foot.ini parsing, ~55 keys")
        Component(font, "font.rs", "FontCache", "fontdue rasterization, glyph cache, zoom")
        Component(pty, "pty.rs", "Pty", "fork/exec, PTY fd management")
        Component(selection, "selection.rs", "Selection", "Text selection, extract_selected_text")
        Component(paste, "paste.rs", "PasteBuf", "Bracketed paste mode")
        Component(boxdraw, "boxdraw.rs", "Boxdraw", "Box-drawing character rendering")
        Component(num, "num.rs", "Num", "Numeric conversion helpers, clamped casts")
    }

    Rel(terminal, render_state, "Provides cell grid")
    Rel(render_state, renderer, "CellStyle -> pixels")
    Rel(renderer, font, "Glyph rasterization")
    Rel(renderer, boxdraw, "Box-drawing glyphs")
    Rel(input, keymap, "keysym -> Key")
    Rel(selection, render_state, "Cell coordinates")

    UpdateLayoutConfig($c4ShapeInRow="4", $c4BoundaryInRow="1")
```

## Module Responsibilities

| Module | File | Responsibility |
|--------|------|---------------|
| `ghostty::terminal` | `src/ghostty/terminal.rs` | `Terminal` (no callbacks) and `TerminalCb` (with callbacks) wrapping `libghostty_vt::Terminal`. `CallbackState` accumulates PTY responses, bell, title, grid size. |
| `ghostty::render` | `src/ghostty/render.rs` | `RenderState` wraps terminal for cell grid access. `CellStyle` + `CellStyleAttrs` bitfield for per-cell styling. Color resolution (256-color, RGB, default). |
| `ghostty::input` | `src/ghostty/input.rs` | `KeyEncoder` translates key events into VT escape sequences. `MouseEncoder` handles mouse reporting. `encode_focus()` for focus in/out. |
| `keymap` | `src/keymap.rs` | Maps XKB keysyms to `libghostty_vt::key::Key` enum. `ModifierState` newtype over `key::Mods`. |
| `renderer` | `src/renderer/` | SHM buffer pixel rendering. `Surface`, `Rect`, `GridMetrics`. Damage tracking, alpha blending with fast div-255. |
| `config` | `src/config/` | Parses `~/.config/foot/foot.ini`. ~55 config keys across `[main]`, `[cursor]`, `[colors]`, `[key-bindings]`. `Bindings` struct + `KeyAction` enum. |
| `font` | `src/font.rs` | `FontCache` with fontdue rasterization. Glyph caching, `rebuild_at_size()` for zoom (skips filesystem rescan). |
| `pty` | `src/pty.rs` | `Pty::spawn` via fork/exec. `SpawnOptions` struct. PTY fd read/write. |
| `selection` | `src/selection.rs` | Grid text selection. `extract_selected_text` with single-String accumulator. |
| `paste` | `src/paste.rs` | `PasteBuf` for bracketed paste mode handling. |
| `num` | `src/num.rs` | Numeric conversion helpers (`f32_metric_to_u32`, `float_to_i64`, clamped casts). |
| `boxdraw` | `src/boxdraw.rs` | Pixel-perfect box-drawing and block character rendering. |
