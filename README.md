# horseshoe

Wayland terminal emulator. Drop-in replacement for [foot](https://codeberg.org/dnkl/foot) (standalone mode).

## Getting started

```sh
nix develop        # enter dev shell (Zig, Rust, wayland deps, task)
task               # list available tasks
```

## Common tasks

```sh
task build              # debug build
task run                # build and run
task release            # static musl release binaries (x86_64 + aarch64)
task test:unit          # unit tests
task test:conformance   # foot VT conformance tests
task test:integration   # PTY integration tests
task test:all           # all of the above
task lint:fmt           # format code
task lint:clippy        # clippy lints
task coverage           # line coverage report
task bench              # criterion benchmarks
```

## Without Nix

Ensure Zig 0.15, Rust 1.93+, `pkg-config`, `wayland-scanner`, wayland and xkbcommon dev libraries are available.

## Key bindings

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+C` | Copy |
| `Ctrl+Shift+V` | Paste |
| `Shift+Insert` | Primary paste |
| `Ctrl+Shift+F` | Search |
| `Ctrl+Shift+N` | New terminal |
| `Ctrl+Shift+E` | Scrollback in `$EDITOR` |
| `Ctrl+=` | Font size up |
| `Ctrl+-` | Font size down |
| `Ctrl+0` | Font size reset |
| `Shift+PageUp` | Scroll page up |
| `Shift+PageDown` | Scroll page down |
| `F11` | Toggle fullscreen |

Rebind in `~/.config/foot/foot.ini` under `[key-bindings]`.

## Configuration

Config file: `~/.config/foot/foot.ini` (foot-compatible). See [foot.ini(5)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot.ini.5.scd) for the base format.

Horseshoe-specific options beyond foot:

| Key | Default | Description |
|---|---|---|
| `app-id` | `hs` | Wayland app-id |
| `selection-target` | `primary` | Where selection goes: `none`, `primary`, `clipboard`, `both` |
| `word-delimiters` | (whitespace) | Characters that break word selection |
| `notify` | (none) | Command to run on bell |
| `resize-delay-ms` | `100` | Debounce delay for resize events |
| `initial-window-mode` | `windowed` | Startup mode: `windowed`, `maximized`, `fullscreen` |
| `initial-window-size-pixels` | (none) | Initial size as `WIDTHxHEIGHT` pixels |
| `initial-window-size-chars` | (none) | Initial size as `COLSxROWS` characters |
| `hide-when-typing` | `false` | Hide cursor while typing |
| `alternate-scroll-mode` | `true` | Convert scroll to arrow keys in alternate screen |
| `multiplier` | `3.0` | Scroll speed multiplier |
| `selection-foreground` | (none) | Selection text color (`#RRGGBB`) |
| `selection-background` | (none) | Selection highlight color (`#RRGGBB`) |
| `dim0`..`dim7` | (none) | Dimmed color palette |

## Source layout

```
src/
  config/        Config parsing, key bindings, color handling
  wayland/       Wayland client: input, drawing, selection, handlers
  renderer/      Pixel rendering: cells, cursor, overlays, scrollbar
  ghostty/       libghostty-vt terminal emulation wrapper
  num.rs         Numeric conversion utilities
  font.rs        Font rasterization (fontconfig + FreeType)
  pty.rs         PTY creation and management
  boxdraw.rs     Procedural box-drawing / block-element glyphs
  keymap.rs      XKB keymap helpers
  paste.rs       Bracketed paste support
  selection.rs   Selection types
```

## License

[MIT](LICENSE)
