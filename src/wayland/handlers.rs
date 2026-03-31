use smithay_client_toolkit::{
    activation::{ActivationHandler, RequestData},
    compositor::CompositorHandler,
    data_device_manager::{
        data_device::DataDeviceHandler, data_offer::DataOfferHandler,
        data_source::DataSourceHandler,
    },
    delegate_activation, delegate_compositor, delegate_data_device, delegate_keyboard,
    delegate_output, delegate_pointer, delegate_primary_selection, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    primary_selection::{
        device::PrimarySelectionDeviceHandler, selection::PrimarySelectionSourceHandler,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerData, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec},
    },
    shell::{
        WaylandSurface,
        xdg::window::{Window, WindowConfigure, WindowHandler},
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{
        wl_data_device::WlDataDevice, wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface,
    },
};

use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use smithay_client_toolkit::reexports::protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
};
use smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

use horseshoe::keymap;
use horseshoe::terminal::TerminalOps;
use horseshoe::terminal::input::encode_focus;
use libghostty_vt::{key, mouse};

use super::cursor::{Cursor, ShmCursor};
use super::{App, dbg_log};

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_data_device!(App);
delegate_primary_selection!(App);
delegate_registry!(App);

delegate_activation!(App);

impl ActivationHandler for App {
    type RequestData = RequestData;

    fn new_token(&mut self, token: String, _data: &RequestData) {
        if let Some(ref activation) = self.wl.activation_state {
            activation.activate::<App>(self.wl.window.wl_surface(), token);
        }
    }
}

// Fractional scaling: handle preferred_scale events.
impl wayland_client::Dispatch<WpFractionalScaleV1, ()> for App {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && scale != state.geometry.scale_120
        {
            state.apply_scale(scale);
        }
    }
}

impl wayland_client::Dispatch<WpFractionalScaleManagerV1, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &WpFractionalScaleManagerV1,
        _event: <WpFractionalScaleManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

// Viewporter interfaces send no events; empty dispatch satisfies the trait bound.
impl wayland_client::Dispatch<WpViewporter, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewporter,
        _event: <WpViewporter as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<WpViewport, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewport,
        _event: <WpViewport as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

// text-input-v3: manager sends no events.
impl wayland_client::Dispatch<ZwpTextInputManagerV3, ()> for App {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpTextInputManagerV3,
        _event: <ZwpTextInputManagerV3 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

// text-input-v3: handle preedit, commit, and done events.
impl wayland_client::Dispatch<ZwpTextInputV3, ()> for App {
    fn event(
        state: &mut Self,
        _proxy: &ZwpTextInputV3,
        event: <ZwpTextInputV3 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::PreeditString { text, .. } => {
                state.ime.pending_preedit = text;
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                state.ime.pending_commit = text;
            }
            zwp_text_input_v3::Event::Done { .. } => {
                // Apply committed text to PTY
                if let Some(text) = state.ime.pending_commit.take() {
                    let _ = state.pty.write_all(text.as_bytes());
                    state.terminal.scroll_viewport_bottom();
                }
                // Update preedit display
                state.ime.preedit_text = state.ime.pending_preedit.take();
                state.dirty = true;
                // Re-enable for next input and update cursor rect
                state.enable_ime();
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                state.ime.preedit_text = None;
                state.dirty = true;
            }
            _ => {}
        }
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.wl.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        // Only apply integer scale if fractional scaling is not active
        if self.wl.fractional_scale_obj.is_some() {
            return;
        }
        let new_120 = u32::try_from(new_factor.max(1)).unwrap_or(1) * 120;
        if new_120 != self.geometry.scale_120 {
            self.apply_scale(new_120);
        }
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if self.dirty {
            let _ = self.draw();
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.wl.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &Window) {
        dbg_log!("[window] close requested");
        self.running = false;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let (new_w, new_h) = configure.new_size;
        if let (Some(w), Some(h)) = (new_w, new_h)
            && (w.get() != self.geometry.width || h.get() != self.geometry.height)
        {
            self.geometry.width = w.get();
            self.geometry.height = h.get();
            self.recalculate_grid();
        }
        if self.last_render_time.is_none() {
            self.last_render_time = Some(std::time::Instant::now());
        }
        self.dirty = true;
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.wl.shm
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.wl.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            let loop_handle = self.wl.loop_handle.clone();
            let result = self.wl.seat_state.get_keyboard_with_repeat(
                qh,
                &seat,
                None,
                loop_handle,
                Box::new(|state: &mut App, _kbd, event| {
                    let count = state.repeat.accumulated_ticks();
                    let repeat_qh = state.wl.qh.clone();
                    for _ in 0..count {
                        state.handle_key(&event, key::Action::Repeat, &repeat_qh);
                    }
                }),
            );
            if result.is_err() {
                eprintln!("Failed to get keyboard with repeat");
            }
        }
        if capability == Capability::Pointer && self.input.cursor.is_none() {
            let cursor_surface = self.wl.compositor_state.create_surface(qh);
            match self.wl.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.wl.shm.wl_shm(),
                cursor_surface,
                ThemeSpec::System,
            ) {
                Ok(tp) => {
                    self.input.cursor = Some(Cursor::Themed(tp));
                }
                Err(err) => {
                    eprintln!("Themed pointer unavailable ({err:?}), using SHM cursor");
                    let pointer = seat.get_pointer(qh, PointerData::new(seat.clone()));
                    let fallback_surface = self.wl.compositor_state.create_surface(qh);
                    if let Some(sc) = ShmCursor::new(pointer, fallback_surface, &self.wl.shm) {
                        self.input.cursor = Some(Cursor::Shm(sc));
                    }
                }
            }
        }
        // Create data device for clipboard support
        if self.clipboard.data_device.is_none() {
            self.clipboard.data_device =
                Some(self.wl.data_device_manager.get_data_device(qh, &seat));
        }
        // Create primary selection device if supported
        if self.clipboard.primary_selection_device.is_none()
            && let Some(ref mgr) = self.wl.primary_selection_manager
        {
            self.clipboard.primary_selection_device = Some(mgr.get_selection_device(qh, &seat));
        }
        // Create text input for IME support
        if self.ime.text_input.is_none()
            && let Some(ref mgr) = self.ime.text_input_manager
        {
            self.ime.text_input = Some(mgr.get_text_input(&seat, qh, ()));
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        _capability: Capability,
    ) {
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {
    }
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
        if !self.input.focused {
            self.input.focused = true;
            if let Some(data) = encode_focus(true) {
                let _ = self.pty.write_all(&data);
            }
            // Enable IME text input
            self.enable_ime();
        }
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        if self.input.focused {
            self.input.focused = false;
            if let Some(data) = encode_focus(false) {
                let _ = self.pty.write_all(&data);
            }
            // Disable IME text input
            self.disable_ime();
        }
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.clipboard.last_serial = serial;
        // Reset repeat timing so the first repeat of a new key doesn't
        // calculate a stale elapsed time from the previous key.
        self.repeat.last_callback = None;
        self.handle_key(&event, key::Action::Press, qh);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        if super::profiling() {
            eprintln!("[profile] key_release: keysym=0x{:04x}", event.keysym.raw());
        }
        self.handle_key(&event, key::Action::Release, qh);
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
        // SCTK's calloop callback handles repeat.
        // Compositor Repeated events are ignored to avoid double writes.
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
        self.input.mods = keymap::ModifierState::empty()
            .with_shift(modifiers.shift)
            .with_ctrl(modifiers.ctrl)
            .with_alt(modifiers.alt)
            .with_logo(modifiers.logo)
            .with_caps_lock(modifiers.caps_lock)
            .with_num_lock(modifiers.num_lock);
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        info: smithay_client_toolkit::seat::keyboard::RepeatInfo,
    ) {
        if let smithay_client_toolkit::seat::keyboard::RepeatInfo::Repeat { rate, .. } = info {
            self.repeat.interval_ms = 1000 / rate.get();
        }
        if super::profiling() {
            eprintln!(
                "[profile] repeat_info: {info:?} → interval={}ms",
                self.repeat.interval_ms
            );
        }
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        // Sync mouse encoder from terminal
        // SAFETY: self.terminal owns the pointer and is alive for the duration of App.
        self.input
            .mouse_encoder
            .sync_from_terminal(self.terminal.inner());
        self.input.mouse_encoder.set_size(
            self.geometry.phys(self.geometry.width),
            self.geometry.phys(self.geometry.height),
            self.font.cell_width,
            self.font.cell_height,
            self.geometry.phys(self.display.padding),
        );
        self.input
            .mouse_encoder
            .set_any_button_pressed(self.input.mouse.buttons_pressed > 0);
        self.input.mouse_encoder.set_track_last_cell(true);

        let mods = self.input.ghostty_mods();

        for event in events {
            match event.kind {
                PointerEventKind::Enter { serial } => {
                    self.handle_pointer_enter(conn, serial, event.position);
                }
                PointerEventKind::Leave { .. } => {}
                PointerEventKind::Motion { .. } => {
                    self.handle_pointer_motion(conn, event.position, mods);
                }
                PointerEventKind::Press { button, serial, .. } => {
                    self.clipboard.last_serial = serial;
                    self.handle_pointer_press(button, mods, qh);
                }
                PointerEventKind::Release { button, serial, .. } => {
                    self.clipboard.last_serial = serial;
                    self.handle_pointer_release(button, mods, qh);
                }
                PointerEventKind::Axis {
                    horizontal: _,
                    vertical,
                    ..
                } => {
                    self.handle_pointer_axis(&vertical, mods);
                }
            }
        }
    }
}

impl DataDeviceHandler for App {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
        _wl_surface: &wl_surface::WlSurface,
    ) {
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _data_device: &WlDataDevice) {}

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
    ) {
    }

    fn selection(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
        // Selection updated; the new offer is accessible via data_device.data().selection_offer()
    }

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
    }
}

impl DataSourceHandler for App {
    fn accept_mime(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
        _mime: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        source: &wayland_client::protocol::wl_data_source::WlDataSource,
        mime: String,
        mut fd: smithay_client_toolkit::data_device_manager::WritePipe,
    ) {
        // Check if this is our copy-paste source
        if let Some(ref cps) = self.clipboard.copy_paste_source
            && cps.inner() == source
        {
            dbg_log!(
                "[sel] DataSource send_request: writing {} bytes, mime={mime}",
                self.clipboard.clipboard_content.len()
            );
            let result =
                std::io::Write::write_all(&mut fd, self.clipboard.clipboard_content.as_bytes());
            dbg_log!("[sel] DataSource send_request: write result={result:?}");
        } else {
            dbg_log!(
                "[sel] DataSource send_request: source mismatch (own={}, mime={mime})",
                self.clipboard.copy_paste_source.is_some()
            );
        }
    }

    fn cancelled(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        source: &wayland_client::protocol::wl_data_source::WlDataSource,
    ) {
        // If our source was cancelled, drop it
        if let Some(ref cps) = self.clipboard.copy_paste_source
            && cps.inner() == source
        {
            dbg_log!(
                "[sel] DataSource cancelled! clipboard_content was {} bytes",
                self.clipboard.clipboard_content.len()
            );
            self.clipboard.copy_paste_source = None;
        }
    }

    fn dnd_dropped(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
    ) {
    }

    fn dnd_finished(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
    ) {
    }

    fn action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &wayland_client::protocol::wl_data_source::WlDataSource,
        _action: wayland_client::protocol::wl_data_device_manager::DndAction,
    ) {
    }
}

impl DataOfferHandler for App {
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut smithay_client_toolkit::data_device_manager::data_offer::DragOffer,
        _actions: wayland_client::protocol::wl_data_device_manager::DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut smithay_client_toolkit::data_device_manager::data_offer::DragOffer,
        _actions: wayland_client::protocol::wl_data_device_manager::DndAction,
    ) {
    }
}

impl PrimarySelectionDeviceHandler for App {
    fn selection(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _device: &ZwpPrimarySelectionDeviceV1,
    ) {
        // Primary selection updated; the new offer is accessible via device.data().selection_offer()
    }
}

impl PrimarySelectionSourceHandler for App {
    fn send_request(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &ZwpPrimarySelectionSourceV1,
        mime: String,
        mut fd: smithay_client_toolkit::data_device_manager::WritePipe,
    ) {
        dbg_log!(
            "[sel] send_request: mime={mime}, {} bytes",
            self.clipboard.primary_selection_content.len()
        );
        let _ =
            std::io::Write::write_all(&mut fd, self.clipboard.primary_selection_content.as_bytes());
    }

    fn cancelled(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &ZwpPrimarySelectionSourceV1,
    ) {
        dbg_log!("[sel] primary selection source cancelled");
        self.clipboard.primary_selection_source = None;
    }
}

/// Convert Wayland button code to `mouse::Button`.
pub(super) const fn wayland_button_to_ghostty(button: u32) -> mouse::Button {
    // Linux button codes (BTN_LEFT = 0x110, etc.)
    match button {
        0x110 => mouse::Button::Left,
        0x111 => mouse::Button::Right,
        0x112 => mouse::Button::Middle,
        0x113 => mouse::Button::Four,
        0x114 => mouse::Button::Five,
        _ => mouse::Button::Unknown,
    }
}
