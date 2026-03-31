# 4. Solution Strategy

## Technology Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| VT engine | libghostty-vt | Battle-tested Ghostty state machine; avoids reimplementing VT parsing |
| Font rendering | fontdue | Pure Rust, no system dependencies, fast rasterization |
| Wayland toolkit | smithay-client-toolkit 0.20 | Mature Rust Wayland client bindings |
| Configuration | foot.ini (configparser) | Direct compatibility with foot users |
| Static linking | musl + Zig | Single binary, no shared library dependencies |
| Fonts | `include_bytes!()` | Embedded JetBrains Mono, no fontconfig needed |

## Key Architecture Approaches

1. **Delegate VT complexity** -- libghostty-vt handles all escape sequence parsing, cursor management, and screen buffer state. horseshoe only reads the resulting cell grid for rendering.

2. **Callback-driven architecture** -- Terminal events (bell, title change, PTY write) flow through registered callbacks into `CallbackState`, which the event loop drains each frame.

3. **CPU/SHM rendering** -- All rendering happens on the CPU into a wl_shm buffer. Damage tracking ensures only changed rows are re-rendered.

4. **Box<Terminal> for safety** -- The terminal is heap-allocated via `Box` because libghostty-vt stores raw VTable pointers during callback registration. Moving the terminal would invalidate these pointers.

5. **calloop event loop** -- PTY fd, Wayland fd, and timers are multiplexed in a single calloop event loop (same approach as smithay).
