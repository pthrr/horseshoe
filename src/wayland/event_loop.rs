use horseshoe::terminal::TerminalOps;

use super::App;

const PTY_READ_BUF_SIZE: usize = 16384;
const MAX_CONSECUTIVE_PTY_READS: u32 = 8;
const IDLE_IMMEDIATE_RENDER_MS: u64 = 8;
const STARTUP_DRAIN_DEADLINE_MS: u64 = 1500;
const RENDER_DELAY_MIN_MS: u64 = 2;
const RENDER_DELAY_MAX_MS: u64 = 8;
pub(crate) const EVENT_LOOP_POLL_MS: u64 = 16;
const CHILD_EXIT_POLL_MS: u64 = 100;

pub(crate) fn register_event_sources(
    loop_handle: &calloop::LoopHandle<'static, App>,
    conn: wayland_client::Connection,
    event_queue: wayland_client::EventQueue<App>,
    pty_fd: i32,
    child_pid: nix::unistd::Pid,
    cursor_blink_interval_ms: u64,
) {
    // Wayland event source
    let _ = calloop_wayland_source::WaylandSource::new(conn, event_queue)
        .insert(loop_handle.clone())
        .expect("failed to insert wayland source");

    // PTY fd source
    let _ = loop_handle
        .insert_source(
            calloop::generic::Generic::new(
                unsafe { std::os::fd::BorrowedFd::borrow_raw(pty_fd) },
                calloop::Interest::READ,
                calloop::Mode::Level,
            ),
            |_event, _metadata, state: &mut App| Ok(handle_pty_readable(state)),
        )
        .expect("failed to insert PTY source");

    register_timers(loop_handle, child_pid, cursor_blink_interval_ms);
}

fn handle_pty_readable(state: &mut App) -> calloop::PostAction {
    let pty_start = std::time::Instant::now();
    let _ = state.pty.flush_pending();
    let mut buf = [0u8; PTY_READ_BUF_SIZE];
    let mut reads = 0u32;
    let mut total_bytes = 0usize;
    loop {
        match state.pty.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(slice) = buf.get(..n) {
                    process_pty_chunk(state, slice);
                }
                state.dirty = true;
                state.last_data_time = std::time::Instant::now();
                total_bytes += n;
                reads += 1;
                if reads >= MAX_CONSECUTIVE_PTY_READS {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => {
                // EIO is the normal signal that the child process exited
                // and the slave side of the PTY was closed.
                if e.raw_os_error() != Some(nix::errno::Errno::EIO as i32) {
                    eprintln!("[pty] read error: {e}");
                }
                state.running = false;
                break;
            }
        }
    }

    // Immediate render when idle: if >8ms since last render,
    // render now instead of waiting for the timer.
    if state.dirty
        && let Some(last_render) = state.last_render_time
    {
        let since_render = pty_start.duration_since(last_render);
        if since_render >= std::time::Duration::from_millis(IDLE_IMMEDIATE_RENDER_MS) {
            if super::profiling() {
                eprintln!(
                    "[profile] pty: {}B in {} reads, {:.2}ms parse, idle {:.2}ms → immediate render",
                    total_bytes,
                    reads,
                    pty_start.elapsed().as_secs_f64() * 1000.0,
                    since_render.as_secs_f64() * 1000.0,
                );
            }
            if state.draw() {
                state.last_render_time = Some(std::time::Instant::now());
            }
            return calloop::PostAction::Continue;
        }
    }

    if super::profiling() && reads > 0 {
        let since_render = match state.last_render_time {
            Some(t) => pty_start.duration_since(t).as_secs_f64() * 1000.0,
            None => 0.0,
        };
        eprintln!(
            "[profile] pty: {}B in {} reads, {:.2}ms parse, since_render: {:.2}ms (batching)",
            total_bytes,
            reads,
            pty_start.elapsed().as_secs_f64() * 1000.0,
            since_render,
        );
    }
    calloop::PostAction::Continue
}

fn process_pty_chunk(state: &mut App, slice: &[u8]) {
    // Scan for OSC 52 clipboard sequences
    if let Some(clip_text) =
        super::scan_osc52(&mut state.osc.osc52_state, &mut state.osc.osc52_buf, slice)
    {
        let qh = state.wl.qh.clone();
        state.set_clipboard_osc52(clip_text, &qh);
    }
    // Scan for OSC 7 (cwd) and OSC 133 (prompt marks)
    let osc_events = state.osc.osc_accum.feed(slice);
    for ev in osc_events {
        match ev {
            super::OscEvent::Cwd(path) => {
                state.osc.cwd = Some(path);
            }
            super::OscEvent::PromptMark => {
                let (_, cursor_row) = state.terminal.cursor_position();
                state.osc.prompt_marks.push(cursor_row);
            }
        }
    }
    state.terminal.vt_write(slice);
    state.terminal_changed = true;
    let responses = state.terminal.take_pty_responses();
    if !responses.is_empty() {
        let _ = state.pty.write_all(&responses);
    }
    if let Some(t) = state.terminal.take_title()
        && !state.display.flags.locked_title()
    {
        state.wl.window.set_title(&t);
    }
    if state.terminal.take_bell() {
        state.request_activation();
    }
}

fn register_timers(
    loop_handle: &calloop::LoopHandle<'static, App>,
    child_pid: nix::unistd::Pid,
    cursor_blink_interval_ms: u64,
) {
    // Two-tier render timer (matches foot's approach):
    // - Lower bound (2ms): delay rendering briefly to batch rapid data bursts.
    // - Upper bound (8ms): force render to prevent stalls during continuous output.
    // - Idle: slow poll at 100ms when no data is arriving.
    let timer =
        calloop::timer::Timer::from_duration(std::time::Duration::from_millis(RENDER_DELAY_MIN_MS));
    let _ = loop_handle
        .insert_source(timer, |_event, _metadata, state: &mut App| {
            // Flush any pending PTY writes (e.g. VT query responses that got
            // EAGAIN). Without this, the shell can deadlock waiting for a
            // response that is stuck in our write buffer.
            let _ = state.pty.flush_pending();

            // Don't render until the window has received its first configure event.
            let Some(last_render) = state.last_render_time else {
                return calloop::timer::TimeoutAction::ToDuration(
                    std::time::Duration::from_millis(EVENT_LOOP_POLL_MS),
                );
            };
            if state.dirty {
                let now = std::time::Instant::now();
                let since_data = now.duration_since(state.last_data_time);
                let since_render = now.duration_since(last_render);
                if since_data >= std::time::Duration::from_millis(RENDER_DELAY_MIN_MS)
                    || since_render >= std::time::Duration::from_millis(RENDER_DELAY_MAX_MS)
                {
                    if super::profiling() {
                        eprintln!(
                            "[profile] timer: since_data={:.2}ms since_render={:.2}ms → render",
                            since_data.as_secs_f64() * 1000.0,
                            since_render.as_secs_f64() * 1000.0,
                        );
                    }
                    if state.draw() {
                        state.last_render_time = Some(now);
                    }
                }
                calloop::timer::TimeoutAction::ToDuration(std::time::Duration::from_millis(
                    RENDER_DELAY_MIN_MS,
                ))
            } else {
                // Idle: poll at 16ms (one vsync frame) instead of 100ms
                // to keep worst-case latency low for edge cases not caught
                // by the PTY immediate-render path.
                calloop::timer::TimeoutAction::ToDuration(std::time::Duration::from_millis(
                    EVENT_LOOP_POLL_MS,
                ))
            }
        })
        .expect("failed to insert timer");

    // Cursor blink timer
    let blink_timer = calloop::timer::Timer::from_duration(std::time::Duration::from_millis(
        cursor_blink_interval_ms,
    ));
    let _ = loop_handle
        .insert_source(blink_timer, move |_event, _metadata, state: &mut App| {
            let cursor = state.render_state.cursor();
            state.display.cursor_blink_visible = !state.display.cursor_blink_visible;
            if cursor.visible && cursor.blinking && cursor.in_viewport {
                state.dirty = true;
            }
            calloop::timer::TimeoutAction::ToDuration(std::time::Duration::from_millis(
                cursor_blink_interval_ms,
            ))
        })
        .expect("failed to insert cursor blink timer");

    // Child process exit polling timer
    let child_poll_timer =
        calloop::timer::Timer::from_duration(std::time::Duration::from_millis(CHILD_EXIT_POLL_MS));
    let _ = loop_handle
        .insert_source(
            child_poll_timer,
            move |_event, _metadata, state: &mut App| {
                if let Ok(
                    status @ (nix::sys::wait::WaitStatus::Exited(_, _)
                    | nix::sys::wait::WaitStatus::Signaled(_, _, _)),
                ) =
                    nix::sys::wait::waitpid(child_pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG))
                {
                    if std::env::var_os("HAND_DEBUG").is_some() {
                        eprintln!("[child] exited: {status:?}");
                    }
                    if !state.display.flags.hold() {
                        state.running = false;
                    }
                }
                calloop::timer::TimeoutAction::ToDuration(std::time::Duration::from_millis(
                    CHILD_EXIT_POLL_MS,
                ))
            },
        )
        .expect("failed to insert child poll timer");
}

/// Drain startup PTY data before entering the event loop.
///
/// Bash/readline sends queries (DA1, DECRQM) during init and expects
/// immediate responses.  Without this drain, the calloop Wayland handler
/// delays PTY processing, causing readline to time out and display
/// response bytes as garbage on the prompt.
///
/// Two-phase approach:
/// - Phase 1: wait for bash to produce *any* output (the prompt).
///   Heavy .bashrc files (sourcing z.sh, git-prompt.sh, etc.) can take
///   hundreds of milliseconds before the first byte appears.
/// - Phase 2: once output arrives, wait for silence (bash is idle at its
///   prompt, all startup queries answered).
pub(crate) fn drain_startup_pty(app: &mut App) {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(STARTUP_DRAIN_DEADLINE_MS);
    let mut idle = 0u32;
    let mut got_data = false;
    let mut buf = [0u8; PTY_READ_BUF_SIZE];
    while std::time::Instant::now() < deadline {
        let _ = app.pty.flush_pending();
        match app.pty.read(&mut buf) {
            Ok(0) | Err(_) => {
                idle += 1;
                // Before any data: wait longer — bash is still sourcing scripts.
                // After data: shorter window — bash showed its prompt.
                let idle_threshold = if got_data { 25 } else { 80 };
                if idle > idle_threshold {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(n) => {
                idle = 0;
                got_data = true;
                if let Some(slice) = buf.get(..n) {
                    app.terminal.vt_write(slice);
                    app.terminal_changed = true;
                    let responses = app.terminal.take_pty_responses();
                    if !responses.is_empty() {
                        let _ = app.pty.write_all(&responses);
                    }
                }
                app.dirty = true;
            }
        }
    }
    // Final flush: ensure all pending VT responses reach bash before the
    // event loop starts dispatching Wayland events.
    let _ = app.pty.flush_pending();
}

pub(crate) fn debug_dump_grid(app: &mut App) {
    let _ = app.render_state.update(app.terminal.inner());
    let max_rows = usize::from(app.geometry.term_rows).min(5);
    let max_cols = usize::from(app.geometry.term_cols);
    let mut grid: Vec<Vec<char>> = vec![vec![' '; max_cols]; max_rows];
    app.render_state
        .for_each_cell(|row, col, codepoints, _style, _wide| {
            if row < max_rows
                && col < max_cols
                && let Some(&cp) = codepoints.first()
                && cp != 0
                && let Some(ch) = char::from_u32(cp)
                && let Some(cell) = grid.get_mut(row).and_then(|r| r.get_mut(col))
            {
                *cell = ch;
            }
        });
    eprintln!(
        "hs: drain done, grid ({} cols x {} rows):",
        app.geometry.term_cols, app.geometry.term_rows
    );
    for (row, line) in grid.iter().enumerate() {
        let s: String = line.iter().collect();
        eprintln!("  row {row}: |{}|", s.trim_end());
    }
}
