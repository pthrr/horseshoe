# 9. Architecture Decisions

## ADR-1: Use libghostty-vt for VT Parsing

**Status:** Accepted

**Context:** A terminal emulator needs a complete VT state machine handling hundreds of escape sequences, DEC modes, and edge cases. Implementing this from scratch is error-prone and maintenance-heavy.

**Decision:** Use libghostty-vt, which wraps Ghostty's battle-tested VT implementation as a Rust crate.

**Consequences:** Depends on upstream for VT correctness. Some features (OSC 10/11/12 color queries) are blocked until upstream adds callbacks. Build requires Zig 0.15 for the sys crate.

## ADR-2: fontdue for Font Rendering

**Status:** Accepted

**Context:** Font rendering options include FreeType (C library, complex linking), rusttype (pure Rust but unmaintained), and fontdue (pure Rust, actively maintained).

**Decision:** Use fontdue for glyph rasterization.

**Consequences:** Pure Rust -- no system dependency. Embedded fonts via `include_bytes!()` eliminate fontconfig. No subpixel rendering (fontdue doesn't support it), but sufficient for a terminal.

## ADR-3: Static musl Binary

**Status:** Accepted

**Context:** Users want a single binary they can copy to any Linux system without installing dependencies.

**Decision:** Build with `--target x86_64-unknown-linux-musl` and `--target aarch64-unknown-linux-musl` for fully-static ELF binaries. Use Nix to provide static libxkbcommon.

**Consequences:** Zero runtime dependencies. Binary size is larger due to static libc. Requires Nix (or manual static lib setup) for the build environment.

## ADR-4: foot.ini Configuration Format

**Status:** Accepted

**Context:** horseshoe targets foot users who want to switch without reconfiguring.

**Decision:** Read `~/.config/foot/foot.ini` directly using the configparser crate. Support the same section names and key names.

**Consequences:** Direct compatibility -- users switch by changing their default terminal. Not all ~200 foot keys are supported (currently ~55), but the most common ones are covered.

## ADR-5: CPU/SHM Rendering (No GPU)

**Status:** Accepted

**Context:** GPU rendering (OpenGL, Vulkan) provides better performance for complex scenes but adds dependencies and complexity.

**Decision:** Render entirely on CPU using fontdue rasterization into a wl_shm buffer.

**Consequences:** Works on any Wayland compositor without GPU driver requirements. Performance is sufficient for terminal workloads with damage tracking. No GPU-accelerated effects (transparency, blur).

## ADR-6: Box<Terminal> for VTable Safety

**Status:** Accepted

**Context:** libghostty-vt's C layer stores raw VTable pointers during callback registration. If the Rust `Terminal` object moves in memory after registration, the pointers become dangling.

**Decision:** Wrap `Terminal` in `Box` to ensure heap allocation with a stable address.

**Consequences:** Prevents SIGSEGV from VTable invalidation. One extra heap allocation (negligible cost). All access goes through the Box indirection.
