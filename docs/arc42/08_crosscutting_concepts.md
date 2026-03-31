# 8. Crosscutting Concepts

## Damage Tracking

Only rows that have changed since the last frame are re-rendered. The renderer tracks dirty rows and submits `wl_surface.damage_buffer` for the corresponding pixel regions. This avoids full-screen redraws on every PTY output.

## Box<Terminal> Safety

`TerminalCb.inner` is `Box<libghostty_vt::Terminal>` because the upstream C layer stores raw VTable pointers during callback registration. If the `Terminal` were moved after callbacks are registered, the VTable pointer would be invalidated, causing SIGSEGV. `Box` ensures the terminal's heap address is stable regardless of how `TerminalCb` itself is moved.

## Fast Alpha Blending

The `blend_channel` function uses the identity `(val + 1) * 257 >> 16` for exact division by 255. This avoids floating-point arithmetic and is provably exact for all `u8` inputs. Loop-invariant hoisting precomputes `fg * alpha` per channel outside the pixel loop.

## Embedded Fonts

Fonts are compiled into the binary via `include_bytes!()` from `data/fonts/`. This eliminates fontconfig as a runtime dependency and ensures the binary works on systems without JetBrains Mono installed. The `FontCache` uses fontdue for rasterization and caches glyphs by (codepoint, size) pairs.

## Font Zoom Path Caching

When the user zooms (Ctrl+scroll), `rebuild_at_size()` re-rasterizes at the new size but skips filesystem font path scanning. The font paths are cached from the initial load, so zoom operations are fast.

## Debug Logging

The `dbg_log!` macro gates `eprintln!` behind the `HAND_DEBUG=1` environment variable. This avoids any overhead in release builds while providing detailed trace output during development.

## Callback-Driven State

Terminal events flow through registered callbacks into `Rc<RefCell<CallbackState>>`:

- `on_pty_write` -- accumulates VT responses to write back to PTY
- `on_bell` -- sets `bell_pending` flag
- `on_title_changed` -- updates window title string
- `on_xtversion` / `on_device_attributes` / `on_enquiry` -- identity responses
- `on_size` -- reports grid dimensions

The event loop drains `CallbackState` each frame, dispatching accumulated events.

## OSC 52 Clipboard

A stateful byte scanner processes OSC 52 sequences across PTY read boundaries. The scanner accumulates base64-encoded data and decodes it on sequence completion. This handles the common case where a single OSC 52 sequence spans multiple PTY reads.
