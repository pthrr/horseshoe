# 12. Glossary

| Term | Definition |
|------|-----------|
| **VT** | Virtual Terminal -- the set of escape sequences and control codes that terminal emulators implement (VT100/VT220 family) |
| **PTY** | Pseudo-terminal -- a kernel mechanism providing a bidirectional communication channel between a terminal emulator and a shell process |
| **SHM** | Shared memory -- Wayland's `wl_shm` protocol for sharing pixel buffers between client and compositor without GPU |
| **sctk** | smithay-client-toolkit -- Rust library providing typed Wayland client protocol bindings and helpers |
| **XKB** | X Keyboard Extension -- keyboard layout and keymap system used by both X11 and Wayland |
| **OSC** | Operating System Command -- escape sequences (ESC ] ...) for terminal features like clipboard, window title, and color queries |
| **CSI** | Control Sequence Introducer -- escape sequences (ESC [ ...) for cursor movement, text formatting, and screen manipulation |
| **DA** | Device Attributes -- escape sequences (DA1, DA2, DA3) for terminal identification and capability reporting |
| **XTVERSION** | Extended version report -- escape sequence for reporting terminal name and version (horseshoe reports `horseshoe(<version>)`) |
| **DEC mode** | DEC private mode -- numbered modes (DECSET/DECRST) controlling terminal behavior like cursor visibility, alternate screen, mouse reporting |
| **Sixel** | Bitmap graphics protocol for terminals -- sends raster images as character-encoded data (not yet supported in horseshoe) |
| **CSD** | Client-Side Decorations -- window decorations (title bar, borders) drawn by the application rather than the compositor |
| **SSD** | Server-Side Decorations -- window decorations drawn by the Wayland compositor (preferred by most tiling WMs) |
| **musl** | Lightweight C standard library designed for static linking, producing fully-static binaries |
| **calloop** | Callback-based event loop library used by smithay for multiplexing I/O sources |
| **fontdue** | Pure Rust font rasterization library providing glyph rendering without system dependencies |
| **wl_surface** | Wayland protocol object representing a rectangular area of pixels on screen |
| **xdg_shell** | Wayland protocol extension for desktop window management (toplevel windows, popups) |
| **xdg_activation** | Wayland protocol for requesting user attention (used for bell urgency) |
| **MSRV** | Minimum Supported Rust Version -- the oldest Rust compiler version the project is tested against (1.93.0) |
