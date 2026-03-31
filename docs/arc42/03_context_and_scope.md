# 3. Context and Scope

## Business Context

horseshoe sits between the user's shell and the Wayland compositor, translating terminal I/O into rendered pixels.

## System Context (C4 Level 1)

```mermaid
C4Context
    title System Context - horseshoe

    Person(user, "User", "Types commands, reads output")

    System(hs, "horseshoe (hs)", "Wayland terminal emulator")

    System_Ext(compositor, "Wayland Compositor", "sway, Hyprland, etc.")
    System_Ext(shell, "Shell / PTY", "bash, zsh, fish")
    System_Ext(config, "foot.ini", "~/.config/foot/foot.ini")
    System_Ext(clipboard, "Wayland Clipboard", "wl_data_device")

    Rel(user, hs, "Keyboard/mouse input")
    Rel(hs, compositor, "wl_surface, wl_shm, xdg_shell")
    Rel(hs, shell, "PTY read/write (fork/exec)")
    Rel(hs, config, "Reads at startup")
    Rel(hs, clipboard, "OSC 52 copy/paste")

    UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")
```

## Technical Context

| Interface | Protocol / Mechanism | Direction |
|-----------|---------------------|-----------|
| Display output | Wayland wl_shm + wl_surface | hs -> compositor |
| Window management | xdg_shell (toplevel) | hs <-> compositor |
| Keyboard input | wl_keyboard + XKB | compositor -> hs |
| Mouse input | wl_pointer | compositor -> hs |
| Shell I/O | PTY (fork/exec) | hs <-> shell |
| Clipboard | wl_data_device (OSC 52) | hs <-> compositor |
| Bell urgency | xdg_activation | hs -> compositor |
| Configuration | INI file read | config -> hs |
