mod wayland;

use std::path::PathBuf;

use clap::Parser;

use horseshoe::config;
use horseshoe::font;
use horseshoe::pty;

#[derive(Parser)]
#[command(name = "hs", version = env!("CARGO_PKG_VERSION"), about = "Wayland terminal emulator", trailing_var_arg = true)]
struct Cli {
    /// Config file path [default: ~/.config/foot/foot.ini]
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Override config: [section.]key=value
    #[arg(short = 'o', long = "override", value_name = "KEY=VALUE")]
    overrides: Vec<String>,

    /// Font (fontconfig format, e.g. `JetBrains Mono:size=12`)
    #[arg(short, long)]
    font: Option<String>,

    /// Initial window size in pixels (`WIDTHxHEIGHT`)
    #[arg(short = 'w', long)]
    window_size_pixels: Option<String>,

    /// Initial window size in characters (`COLSxROWS`)
    #[arg(short = 'W', long)]
    window_size_chars: Option<String>,

    /// TERM environment variable
    #[arg(short, long)]
    term: Option<String>,

    /// Window title
    #[arg(short = 'T', long)]
    title: Option<String>,

    /// Wayland app-id
    #[arg(short, long)]
    app_id: Option<String>,

    /// Working directory
    #[arg(short = 'D', long)]
    working_directory: Option<PathBuf>,

    /// Log level (error, warning, info, debug)
    #[arg(short = 'd', long)]
    log_level: Option<String>,

    /// Command to execute (must be last; optionally preceded by -e for foot compat)
    #[arg(allow_hyphen_values = true, num_args = 0..)]
    command: Vec<String>,

    #[command(flatten)]
    launch: CliLaunchMode,

    #[command(flatten)]
    shell: CliShellOptions,
}

#[derive(clap::Args)]
struct CliLaunchMode {
    /// Validate config and exit (0=ok, 1=error)
    #[arg(short = 'C', long)]
    check_config: bool,

    /// Start maximized
    #[arg(short, long)]
    maximized: bool,

    /// Start fullscreen
    #[arg(short = 'F', long)]
    fullscreen: bool,
}

#[derive(clap::Args)]
struct CliShellOptions {
    /// Start as login shell
    #[arg(short = 'L', long)]
    login_shell: bool,

    /// Keep window open after child exits
    #[arg(short = 'H', long)]
    hold: bool,
}

/// Apply CLI overrides to the config.
fn apply_cli_to_config(cli: &Cli, cfg: &mut config::Config) {
    if let Some(ref font) = cli.font {
        cfg.apply_override(&format!("font={font}"));
    }
    if let Some(ref title) = cli.title {
        cfg.window.title.clone_from(title);
    }
    if let Some(ref app_id) = cli.app_id {
        cfg.window.app_id.clone_from(app_id);
    }
    if let Some(ref term) = cli.term {
        cfg.terminal.term = Some(term.clone());
    }
    if cli.shell.login_shell {
        cfg.terminal.login_shell = true;
    }
    if cli.shell.hold {
        cfg.window.hold = true;
    }
    if cli.launch.maximized {
        cfg.window.initial_window_mode = config::WindowMode::Maximized;
    }
    if cli.launch.fullscreen {
        cfg.window.initial_window_mode = config::WindowMode::Fullscreen;
    }
    if let Some(ref dir) = cli.working_directory {
        cfg.terminal.working_directory = Some(dir.clone());
    }
    if let Some(ref size) = cli.window_size_pixels
        && let Some((w, h)) = parse_cli_size(size)
    {
        cfg.window.initial_size_pixels = Some((w, h));
    }
    if let Some(ref size) = cli.window_size_chars
        && let Some((w, h)) = parse_cli_size(size)
    {
        let cols = u16::try_from(w).unwrap_or(u16::MAX);
        let rows = u16::try_from(h).unwrap_or(u16::MAX);
        cfg.window.initial_size_chars = Some((cols, rows));
    }
    // Apply -o overrides last (highest priority)
    for kv in &cli.overrides {
        cfg.apply_override(kv);
    }
}

fn parse_cli_size(s: &str) -> Option<(u32, u32)> {
    let (w_str, h_str) = s.split_once('x')?;
    let width = w_str.trim().parse::<u32>().ok()?;
    let height = h_str.trim().parse::<u32>().ok()?;
    if width > 0 && height > 0 {
        Some((width, height))
    } else {
        None
    }
}

fn main() -> std::process::ExitCode {
    // Early check: horseshoe requires a Wayland compositor.
    // Without WAYLAND_DISPLAY/WAYLAND_SOCKET the connection will fail with a
    // confusing error (or the dynamic linker may fail on missing libxkbcommon).
    if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("WAYLAND_SOCKET").is_none()
    {
        eprintln!("hs: error: no Wayland compositor detected (WAYLAND_DISPLAY is not set)");
        eprintln!(
            "hs: horseshoe is a Wayland-native terminal and cannot run without a Wayland session"
        );
        return std::process::ExitCode::FAILURE;
    }

    let cli = Cli::parse();

    // Load config: from --config path or default
    let mut cfg = if let Some(ref path) = cli.config {
        config::Config::load_from(path)
    } else {
        config::Config::load()
    };

    // --check-config: validate and exit
    if cli.launch.check_config {
        let path = cli.config.unwrap_or_else(config::Config::default_path);
        match config::Config::check(&path) {
            Ok(()) => {
                eprintln!("Config OK: {}", path.display());
                return std::process::ExitCode::SUCCESS;
            }
            Err(errors) => {
                for e in &errors {
                    eprintln!("Config error: {e}");
                }
                return std::process::ExitCode::from(230);
            }
        }
    }

    // Apply CLI flags → config (file < CLI < -o overrides)
    apply_cli_to_config(&cli, &mut cfg);

    // Strip leading "-e" for foot compatibility (foot accepts both `foot cmd` and `foot -e cmd`)
    let command: Option<Vec<String>> = {
        let mut args = cli.command;
        if args.first().is_some_and(|a| a == "-e") {
            let _ = args.remove(0);
        }
        if args.is_empty() { None } else { Some(args) }
    };

    let debug = std::env::var_os("HAND_DEBUG").is_some();

    if debug {
        eprintln!(
            "hs: shell={:?} term={:?} login={} $SHELL={:?} default_shell={}",
            cfg.terminal.shell,
            cfg.terminal.term,
            cfg.terminal.login_shell,
            std::env::var("SHELL"),
            pty::default_shell(),
        );
    }

    let font = font::FontManager::new_with_family(cfg.font.size, cfg.font.family.as_deref());
    let cell_w = font.cell_width;
    let cell_h = font.cell_height;
    let pad = cfg.window.padding;

    // Determine initial window size: CLI pixel size > CLI char size > config chars > config cols/rows > default
    let (initial_width, initial_height) =
        wayland::init::compute_initial_size(&cfg, cell_w, cell_h, pad);

    let usable_w = initial_width.saturating_sub(pad * 2);
    let usable_h = initial_height.saturating_sub(pad * 2);
    let cols = u16::try_from((usable_w / cell_w).max(1)).expect("column count overflows u16");
    let rows = u16::try_from((usable_h / cell_h).max(1)).expect("row count overflows u16");

    let mut event_loop =
        calloop::EventLoop::<wayland::App>::try_new().expect("failed to create event loop");
    let loop_handle = event_loop.handle();

    let (mut app, conn, event_queue) = wayland::init::create_app(wayland::init::AppParams {
        cfg: &cfg,
        initial_width,
        initial_height,
        cols,
        rows,
        font,
        loop_handle: loop_handle.clone(),
        command: command.as_deref(),
        working_directory: cfg.terminal.working_directory.as_deref(),
    });

    // Apply initial window mode
    match cfg.window.initial_window_mode {
        config::WindowMode::Maximized => app.wl.window.set_maximized(),
        config::WindowMode::Fullscreen => app.wl.window.set_fullscreen(None),
        config::WindowMode::Windowed => {}
    }

    let pty_fd = app.pty.master_fd();
    let child_pid = app.pty.child_pid();
    wayland::event_loop::register_event_sources(
        &loop_handle,
        conn,
        event_queue,
        pty_fd,
        child_pid,
        cfg.terminal.cursor_blink_interval_ms,
    );

    wayland::event_loop::drain_startup_pty(&mut app);

    if debug {
        wayland::event_loop::debug_dump_grid(&mut app);
    }

    while app.running {
        event_loop
            .dispatch(
                std::time::Duration::from_millis(wayland::event_loop::EVENT_LOOP_POLL_MS),
                &mut app,
            )
            .expect("event loop dispatch failed");
    }

    // Persist clipboard content so it survives after exit
    app.persist_clipboard();

    // app.pty is dropped here, sending SIGHUP and reaping the child
    std::process::ExitCode::SUCCESS
}
