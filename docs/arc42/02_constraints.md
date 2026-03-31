# 2. Constraints

## Technical Constraints

| Constraint | Rationale |
|-----------|-----------|
| Wayland-only | Modern Linux display protocol; no X11 support needed |
| Linux-only | Wayland is Linux-specific; musl target is Linux |
| CPU/SHM rendering | No GPU dependency; works everywhere Wayland runs |
| Static musl binary | Zero runtime deps; single-file deployment |
| No client/server | Standalone mode only (foot's default mode) |
| Embedded fonts | JetBrains Mono via `include_bytes!()` -- no fontconfig |
| Zig 0.15 build dependency | libghostty-vt-sys compiles libghostty-vt.a from Zig source |

## Organizational Constraints

| Constraint | Rationale |
|-----------|-----------|
| foot.ini format | Config compatibility -- users switch without changing config |
| Strict clippy | Project-local `clippy.toml` with disallowed methods; all warnings are errors |
| MSRV 1.93.0 | Edition 2024 features |

## Convention Constraints

| Constraint | Rationale |
|-----------|-----------|
| XTVERSION `horseshoe(<version>)` | Terminal identification for scripts, version from Cargo.toml |
| DA3 unit ID `0x485253` ("HRS") | Device attributes identification |
| App ID `hs` | Wayland app-id for window managers |
| Binary name `hs` | Short, memorable, matches app-id |
