# 7. Deployment View

## Build Pipeline

```mermaid
flowchart LR
    subgraph Nix["Nix devShell"]
        Zig["Zig 0.15"]
        Rust["Rust (edition 2024)"]
        WP["wayland-protocols"]
        XKB["static libxkbcommon"]
    end

    subgraph Build["cargo build --release (per target)"]
        SysCrate["libghostty-vt-sys"]
        LibVT["libghostty-vt.a"]
        Crate["horseshoe crate"]
    end

    Zig --> SysCrate
    SysCrate -->|"-Demit-lib-vt -Dsimd=false"| LibVT
    LibVT --> Crate
    Rust --> Crate
    WP --> Crate
    XKB --> Crate

    Crate -->|"LTO + strip"| Binary_x86["hs (x86_64 static ELF)"]
    Crate -->|"LTO + strip"| Binary_arm["hs (aarch64 static ELF)"]
```

## Deployment Artifact

| Property | Value |
|----------|-------|
| Binary name | `hs` |
| Targets | `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` |
| Linking | Fully static (musl libc) |
| LTO | Fat LTO, single codegen unit |
| Strip | Enabled |
| Panic | Abort |
| Runtime dependencies | None |

## Configuration Path

| File | Purpose |
|------|---------|
| `~/.config/foot/foot.ini` | User configuration (shared with foot) |

No other files are read from the filesystem at runtime. Fonts are embedded in the binary.

## Runtime Environment

The binary requires only:

- A running Wayland compositor (sway, Hyprland, GNOME, KDE, etc.)
- A Wayland socket (`$WAYLAND_DISPLAY`)
- A working PTY subsystem (`/dev/ptmx`)
