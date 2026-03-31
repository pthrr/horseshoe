# 1. Introduction and Goals

## Requirements Overview

**horseshoe** (`hs`) is a Wayland terminal emulator — drop-in replacement for [foot](https://codeberg.org/dnkl/foot) (standalone mode).

Core requirements:

- Parse and render VT sequences using libghostty-vt (Ghostty's terminal state machine)
- Read foot's configuration file (`~/.config/foot/foot.ini`) directly
- Produce a single fully-static musl binary with zero runtime dependencies
- Support the same CLI flags and keybindings as foot

## Quality Goals

| Priority | Goal | Description |
|----------|------|-------------|
| 1 | Correctness | Accurate VT rendering -- delegate to libghostty-vt for escape sequence parsing |
| 2 | Performance | CPU/SHM rendering with damage tracking, fast div-255 blending, no unnecessary allocations |
| 3 | Zero dependencies | Static musl binary, embedded fonts, no shared libraries at runtime |
| 4 | Foot compatibility | Same config format, keybindings, CLI flags, and shell integration |

## Stakeholders

| Role | Expectation |
|------|-------------|
| End user | A fast, lightweight terminal that works with existing foot configuration |
| Developer | Clean codebase with high test coverage, strict linting, architecture docs |
