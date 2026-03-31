# 10. Quality Requirements

## Quality Tree

```
Quality
├── Correctness
│   ├── VT compliance (delegated to libghostty-vt)
│   ├── Comprehensive test suite (`task test:all`)
│   └── Coverage threshold enforced (`task coverage:check`, 80%)
├── Performance
│   ├── Damage tracking (dirty row rendering)
│   ├── Fast div-255 blending
│   ├── Font path caching on zoom
│   └── Criterion benchmarks
├── Maintainability
│   ├── Strict clippy (project-local clippy.toml)
│   ├── MSRV 1.93.0
│   ├── arc42 architecture documentation
│   └── Clean module boundaries (10 lib modules)
└── Deployability
    ├── Static musl binaries (x86_64 + aarch64)
    ├── Embedded fonts
    └── Zero runtime dependencies
```

## Quality Scenarios

| ID | Quality | Scenario | Measure |
|----|---------|----------|---------|
| QS-1 | Correctness | VT escape sequences render identically to Ghostty | libghostty-vt handles parsing; integration tests verify output |
| QS-2 | Correctness | All keybindings work with Ctrl+Shift modifiers | keymap.rs tests; normalized ASCII matching |
| QS-3 | Performance | Font zoom completes without visible lag | `rebuild_at_size()` skips filesystem scan; benchmarked |
| QS-4 | Performance | Only changed rows are re-rendered | Damage tracking in renderer/; test coverage enforced |
| QS-5 | Maintainability | New developer understands architecture quickly | arc42 docs, C4 diagrams, high test coverage |
| QS-6 | Deployability | Binary runs on any Linux with Wayland | Static musl, no shared libs, embedded fonts |

## Test Coverage

`task test:all` and `task coverage:check` (80% threshold).
