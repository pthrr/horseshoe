use horseshoe::config;
use horseshoe::font;
use horseshoe::keymap;
use horseshoe::pty;
use horseshoe::terminal;
use horseshoe::terminal::TerminalOps;

use smithay_client_toolkit::{
    activation::ActivationState,
    compositor::CompositorState,
    data_device_manager::DataDeviceManagerState,
    output::OutputState,
    primary_selection::PrimarySelectionManagerState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        xdg::{window::WindowDecorations, XdgShell},
    },
    shm::{slot::SlotPool, Shm},
};
use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use wayland_client::{globals::registry_queue_init, Connection};

use super::App;

const DEFAULT_WINDOW_WIDTH: u32 = 800;
const DEFAULT_WINDOW_HEIGHT: u32 = 600;
const MIN_WINDOW_WIDTH: u32 = 80;
const MIN_WINDOW_HEIGHT: u32 = 50;

fn init_terminal(cfg: &config::Config, cols: u16, rows: u16) -> terminal::vt::TerminalCb {
    let mut terminal = terminal::vt::TerminalCb::new(cols, rows, cfg.terminal.scrollback)
        .expect("failed to create terminal");
    let color_seqs = cfg.color_osc_sequences();
    if !color_seqs.is_empty() {
        terminal.vt_write(&color_seqs);
    }
    terminal
}

struct WlSetup {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm: Shm,
    xdg_shell: XdgShell,
    data_device_manager: DataDeviceManagerState,
    primary_selection_manager: Option<PrimarySelectionManagerState>,
    viewporter: Option<WpViewporter>,
    fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    text_input_manager: Option<ZwpTextInputManagerV3>,
    activation_state: Option<ActivationState>,
    window: smithay_client_toolkit::shell::xdg::window::Window,
    pool: SlotPool,
}

fn init_wayland(
    initial_width: u32,
    initial_height: u32,
    title: &str,
    app_id: &str,
) -> (WlSetup, Connection, wayland_client::EventQueue<App>) {
    let conn = Connection::connect_to_env()
        .expect("failed to connect to Wayland compositor — is WAYLAND_DISPLAY set?");
    let (globals, event_queue) = registry_queue_init(&conn).expect("failed to init registry");
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).expect("compositor not available");
    let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg_shell not available");
    let shm = Shm::bind(&globals, &qh).expect("shm not available");
    let data_device_manager =
        DataDeviceManagerState::bind(&globals, &qh).expect("data device manager not available");
    let primary_selection_manager = PrimarySelectionManagerState::bind(&globals, &qh).ok();

    // Bind viewporter + fractional-scale (optional, for HiDPI fractional scaling)
    let viewporter: Option<WpViewporter> = globals.bind::<WpViewporter, _, _>(&qh, 1..=1, ()).ok();
    let fractional_scale_manager: Option<WpFractionalScaleManagerV1> = globals
        .bind::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ())
        .ok();

    // Bind text-input-v3 (optional, for IME support)
    let text_input_manager: Option<ZwpTextInputManagerV3> = globals
        .bind::<ZwpTextInputManagerV3, _, _>(&qh, 1..=1, ())
        .ok();

    // Bind xdg-activation (optional, for urgency hints)
    let activation_state = ActivationState::bind(&globals, &qh).ok();

    let surface = compositor_state.create_surface(&qh);
    let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title(title);
    window.set_app_id(app_id);
    window.set_min_size(Some((MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)));
    window.commit();

    // Pre-allocate for triple-buffering (3 frames) so normal rendering
    // never needs to resize the pool — resize only happens on window resize.
    let pool_size = initial_width as usize * initial_height as usize * 4 * 3;
    let pool = SlotPool::new(pool_size, &shm).expect("failed to create SHM pool");

    let state = WlSetup {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor_state,
        shm,
        xdg_shell,
        data_device_manager,
        primary_selection_manager,
        viewporter,
        fractional_scale_manager,
        text_input_manager,
        activation_state,
        window,
        pool,
    };

    (state, conn, event_queue)
}

pub(crate) struct AppParams<'a> {
    pub cfg: &'a config::Config,
    pub initial_width: u32,
    pub initial_height: u32,
    pub cols: u16,
    pub rows: u16,
    pub font: font::FontManager,
    pub loop_handle: calloop::LoopHandle<'static, App>,
    pub command: Option<&'a [String]>,
    pub working_directory: Option<&'a std::path::Path>,
}

pub(crate) fn create_app(p: AppParams<'_>) -> (App, Connection, wayland_client::EventQueue<App>) {
    let render_state = terminal::render::RenderState::new().expect("failed to create render state");
    let key_encoder = terminal::input::KeyEncoder::new().expect("failed to create key encoder");
    let mouse_encoder =
        terminal::input::MouseEncoder::new().expect("failed to create mouse encoder");
    let terminal = init_terminal(p.cfg, p.cols, p.rows);
    let px_w = u16::try_from(p.initial_width.min(u32::from(u16::MAX))).unwrap_or(u16::MAX);
    let px_h = u16::try_from(p.initial_height.min(u32::from(u16::MAX))).unwrap_or(u16::MAX);
    let pty_handle = pty::Pty::spawn(&pty::SpawnOptions {
        cols: p.cols,
        rows: p.rows,
        shell: p.cfg.terminal.shell.as_deref(),
        term: p.cfg.terminal.term.as_deref(),
        pixel_width: px_w,
        pixel_height: px_h,
        login: p.cfg.terminal.login_shell,
        command: p.command,
        working_directory: p.working_directory,
    })
    .expect("failed to spawn PTY");

    let (wl, conn, event_queue) = init_wayland(
        p.initial_width,
        p.initial_height,
        &p.cfg.window.title,
        &p.cfg.window.app_id,
    );
    let qh = event_queue.handle();

    // Create per-surface fractional scale + viewport objects if both protocols are available
    let (viewport, fractional_scale_obj) = match (&wl.viewporter, &wl.fractional_scale_manager) {
        (Some(vp), Some(fsm)) => {
            let vp_obj = vp.get_viewport(wl.window.wl_surface(), &qh, ());
            let frac = fsm.get_fractional_scale(wl.window.wl_surface(), &qh, ());
            (Some(vp_obj), Some(frac))
        }
        _ => (None, None),
    };

    let app = assemble_app(
        wl,
        p,
        TermSetup {
            terminal,
            render_state,
            pty: pty_handle,
            key_encoder,
            mouse_encoder,
        },
        WlExtras {
            viewport,
            fractional_scale_obj,
            conn: conn.clone(),
            qh: qh.clone(),
        },
    );
    (app, conn, event_queue)
}

struct TermSetup {
    terminal: terminal::vt::TerminalCb,
    render_state: terminal::render::RenderState,
    pty: pty::Pty,
    key_encoder: terminal::input::KeyEncoder,
    mouse_encoder: terminal::input::MouseEncoder,
}

struct WlExtras {
    viewport: Option<smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport>,
    fractional_scale_obj: Option<smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1>,
    conn: Connection,
    qh: wayland_client::QueueHandle<App>,
}

// Separated from `create_app` purely to stay under the too-many-lines lint.
// All arguments come from `create_app` — no independent logic here.
fn assemble_app(wl: WlSetup, p: AppParams<'_>, ts: TermSetup, wx: WlExtras) -> App {
    let wl_state = super::WaylandState {
        registry_state: wl.registry_state,
        seat_state: wl.seat_state,
        output_state: wl.output_state,
        compositor_state: wl.compositor_state,
        shm: wl.shm,
        _xdg_shell: wl.xdg_shell,
        data_device_manager: wl.data_device_manager,
        primary_selection_manager: wl.primary_selection_manager,
        loop_handle: p.loop_handle,
        window: wl.window,
        pool: wl.pool,
        buffer: None,
        viewport: wx.viewport,
        fractional_scale_obj: wx.fractional_scale_obj,
        conn: wx.conn,
        activation_state: wl.activation_state,
        qh: wx.qh,
    };
    let input = super::InputState {
        key_encoder: ts.key_encoder,
        mouse_encoder: ts.mouse_encoder,
        mods: keymap::ModifierState::default(),
        mouse: super::MouseState::default(),
        cursor: None,
        bindings: p.cfg.bindings.clone(),
        focused: true,
    };
    let display = super::DisplayConfig {
        cursor_blink_visible: true,
        fullscreen: matches!(
            p.cfg.window.initial_window_mode,
            config::WindowMode::Fullscreen
        ),
        padding: p.cfg.window.padding,
        opacity: p.cfg.colors.opacity,
        selection_fg: p.cfg.colors.selection_fg.map(|c| (c.r, c.g, c.b)),
        selection_bg: p.cfg.colors.selection_bg.map(|c| (c.r, c.g, c.b)),
        scroll_multiplier: p.cfg.input.scroll_multiplier,
        flags: super::DisplayFlags::new()
            .with_bold_is_bright(p.cfg.input.bold_is_bright)
            .with_locked_title(p.cfg.window.locked_title)
            .with_hold(p.cfg.window.hold)
            .with_hide_when_typing(p.cfg.input.hide_when_typing)
            .with_alternate_scroll_mode(p.cfg.input.alternate_scroll_mode),
    };
    App {
        wl: wl_state,
        retained_buf: Vec::new(),
        terminal: ts.terminal,
        render_state: ts.render_state,
        cached_colors: None,
        pty: ts.pty,
        font: p.font,
        input,
        geometry: super::WindowGeometry {
            width: p.initial_width,
            height: p.initial_height,
            term_cols: p.cols,
            term_rows: p.rows,
            scale_120: 120,
            base_font_size: p.cfg.font.size,
            initial_font_size: p.cfg.font.size,
        },
        dirty: true,
        terminal_changed: true,
        running: true,
        display,
        selection: super::SelectionState {
            start: None,
            end: None,
            active: false,
            last_click_time: None,
            last_click_pos: (0, 0),
            click_count: 0,
        },
        clipboard: super::ClipboardState {
            data_device: None,
            primary_selection_device: None,
            copy_paste_source: None,
            clipboard_content: String::new(),
            primary_selection_source: None,
            primary_selection_content: String::new(),
            has_wl_copy: check_wl_copy(),
            last_serial: 0,
        },
        repeat: super::RepeatState::default(),
        search: super::SearchState::default(),
        osc: super::OscTracking::default(),
        ime: super::ImeState {
            text_input_manager: wl.text_input_manager,
            text_input: None,
            preedit_text: None,
            pending_preedit: None,
            pending_commit: None,
        },
        last_data_time: std::time::Instant::now(),
        last_render_time: None,
    }
}

/// Check whether `wl-copy` is available by attempting to run it.
fn check_wl_copy() -> bool {
    std::process::Command::new("wl-copy")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Compute initial window size from config, respecting priority:
/// pixel size > char size > cols/rows > default 800x600.
pub(crate) fn compute_initial_size(
    cfg: &config::Config,
    cell_w: u32,
    cell_h: u32,
    pad: u32,
) -> (u32, u32) {
    if let Some((w, h)) = cfg.window.initial_size_pixels {
        return (w, h);
    }
    if let Some((cols, rows)) = cfg.window.initial_size_chars {
        let w = u32::from(cols) * cell_w + pad * 2;
        let h = u32::from(rows) * cell_h + pad * 2;
        return (w, h);
    }
    let w = if cfg.window.initial_cols > 0 {
        u32::from(cfg.window.initial_cols) * cell_w + pad * 2
    } else {
        DEFAULT_WINDOW_WIDTH
    };
    let h = if cfg.window.initial_rows > 0 {
        u32::from(cfg.window.initial_rows) * cell_h + pad * 2
    } else {
        DEFAULT_WINDOW_HEIGHT
    };
    (w, h)
}
