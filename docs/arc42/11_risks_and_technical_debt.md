# 11. Risks and Technical Debt

## Risks

| ID | Risk | Probability | Impact | Mitigation |
|----|------|-------------|--------|------------|
| R-1 | Sixel image support blocked on upstream | High | Medium | Deferred until libghostty-vt adds Sixel callbacks |
| R-2 | OSC 10/11/12 color queries unsupported | High | Low | Upstream has no callback for these; apps fall back gracefully |
| R-3 | libghostty-vt API breaking changes | Medium | High | Pinned version in Cargo.toml; local sys crate override |
| R-4 | Zig 0.15 build compatibility | Medium | Medium | Nix flake pins Zig version; sys crate tested in CI |

## Technical Debt

| ID | Debt | Location | Description |
|----|------|----------|-------------|
| TD-1 | `inner_raw()` pointer cast | `src/ghostty/render.rs` | Uses transmute-style pointer cast to access the Object's inner raw pointer. Fragile if upstream layout changes. |
| TD-2 | pty.rs low coverage | `src/pty.rs` | fork/exec paths are inherently untestable in unit tests (75% coverage). Integration tests cover the happy path. |
| TD-3 | `-Dsimd=false` unconditional | `crates/libghostty-vt-sys/` | SIMD disabled for all builds because it pulls in C++ runtime. Performance impact is minimal for VT parsing but should be revisited. |
| TD-4 | CSD not implemented | `src/wayland/` | Client-side decorations are low priority; most tiling WM users use SSD. |
| TD-5 | Limited foot.ini coverage | `src/config/` | ~55 of foot's ~200 config keys supported. Remaining keys are niche but may be needed for full compatibility. |
