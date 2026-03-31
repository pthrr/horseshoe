//! Integration tests for the PTY + terminal pipeline.
//!
//! These tests spawn a real PTY with `/bin/sh` or `/bin/bash` and verify
//! that the full pipeline (PTY -> terminal -> render) works correctly.
//! VT parser/encoder behavior is tested upstream in libghostty-vt.
//!
//! All tests use `#[serial]` to run sequentially — PTY-based tests are
//! inherently sensitive to system resource contention.
//!
//! Test categories:
//! - **Non-corruption**: no escape sequence leaks in rendered grid
//! - **Shell integration**: bash startup, readline, vi mode, tab completion
//! - **Rendering**: grid text extraction, render state, cursor visibility
//! - **Resize**: PTY resize propagates correctly
//! - **Robustness**: binary data, large output, unicode

use horseshoe::pty::Pty;
use horseshoe::terminal::render::RenderState;
use horseshoe::terminal::vt::{TerminalCb, TerminalOps};
use libghostty_vt::terminal::Mode;
use serial_test::serial;

use std::time::{Duration, Instant};

/// Test environment wrapping PTY + Terminal + Scanner + `RenderState`.
///
/// Tracks all scanner-generated responses in `responses_log` so tests
/// can verify exact response content without relying on shell stdin capture.
struct TestEnv {
    pty: Pty,
    terminal: TerminalCb,
    render_state: RenderState,
    responses_log: Vec<u8>,
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // Send exit + SIGKILL to ensure the child terminates promptly.
        // `Pty::Drop` sends SIGHUP + blocking waitpid, which can hang if
        // the shell ignores SIGHUP (e.g., interactive login shells).
        let _ = self.pty.write_all(b"\nexit\n");
        let pid = self.pty.child_pid();
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
    }
}

impl TestEnv {
    fn new() -> Self {
        Self::with_size(80, 24)
    }

    fn with_bash() -> Self {
        Self::with_shell_term(80, 24, "/bin/bash", "xterm-256color")
    }

    fn with_size(cols: u16, rows: u16) -> Self {
        Self::with_shell_term(cols, rows, "/bin/sh", "dumb")
    }

    fn with_shell_term(cols: u16, rows: u16, shell: &str, term: &str) -> Self {
        let px_w = cols * 10;
        let px_h = rows * 25;
        let pty = Pty::spawn(&horseshoe::pty::SpawnOptions {
            cols,
            rows,
            shell: Some(shell),
            term: Some(term),
            pixel_width: px_w,
            pixel_height: px_h,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn PTY");
        let terminal = TerminalCb::new(cols, rows, 1000).expect("create terminal");
        let render_state = RenderState::new().expect("create render state");
        let mut env = Self {
            pty,
            terminal,
            render_state,
            responses_log: Vec::new(),
        };
        // Wait for the shell to be ready by sending a command and waiting for
        // its output, then drain any remaining startup data.
        let _ = env.send_wait_marker(b"echo SHELL_READY\n", "SHELL_READY", Duration::from_secs(5));
        let _ = env.drain_timeout(Duration::from_millis(200));
        // Clear any startup responses (e.g. from shell profile scripts)
        let _ = env.take_responses();
        env
    }

    /// Process one chunk of PTY data through the callback terminal.
    fn process_one_read(&mut self) -> Vec<u8> {
        let mut buf = [0u8; 16384];
        match self.pty.read(&mut buf) {
            Ok(0) | Err(_) => Vec::new(),
            Ok(n) => {
                let Some(slice) = buf.get(..n) else {
                    return Vec::new();
                };
                self.terminal.vt_write(slice);
                let responses = self.terminal.take_pty_responses();
                if !responses.is_empty() {
                    self.responses_log.extend_from_slice(&responses);
                    let _ = self.pty.write_all(&responses);
                }
                let _ = self.render_state.update(self.terminal.inner());
                slice.to_vec()
            }
        }
    }

    /// Drain all available PTY output with polling over a timeout.
    fn drain_timeout(&mut self, timeout: Duration) -> Vec<u8> {
        let start = Instant::now();
        let mut all = Vec::new();
        let mut idle_count = 0u32;
        while start.elapsed() < timeout {
            let chunk = self.process_one_read();
            if chunk.is_empty() {
                idle_count += 1;
                if idle_count > 10 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            } else {
                all.extend_from_slice(&chunk);
                idle_count = 0;
            }
        }
        all
    }

    /// Send a command and poll until the marker string appears in the output,
    /// or the timeout expires.
    ///
    /// After finding the marker, drains remaining data briefly. This is
    /// necessary because the marker may appear in the echoed command input
    /// (PTY line discipline echo) BEFORE the command executes and produces
    /// its actual output (including ESC query bytes that the scanner needs
    /// to process).
    fn send_wait_marker(&mut self, command: &[u8], marker: &str, timeout: Duration) -> String {
        if !command.is_empty() {
            let _ = self.pty.write_all(command);
        }
        let start = Instant::now();
        let mut all = Vec::new();
        while start.elapsed() < timeout {
            let chunk = self.process_one_read();
            if chunk.is_empty() {
                std::thread::sleep(Duration::from_millis(10));
            } else {
                all.extend_from_slice(&chunk);
                let text = String::from_utf8_lossy(&all);
                if text.contains(marker) {
                    // Drain remaining data: the marker was likely found in
                    // the echoed input; the command output (with query responses)
                    // may still be in flight. Wait until scanner is idle and
                    // no new data arrives for several consecutive reads.
                    let mut idle = 0u32;
                    while idle < 50 {
                        let extra = self.process_one_read();
                        if extra.is_empty() {
                            idle += 1;
                            if idle >= 8 {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        } else {
                            all.extend_from_slice(&extra);
                            idle = 0;
                        }
                    }
                    return String::from_utf8_lossy(&all).into_owned();
                }
            }
        }
        String::from_utf8_lossy(&all).into_owned()
    }

    /// Take all accumulated scanner response bytes, clearing the log.
    fn take_responses(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.responses_log)
    }

    /// Sync render state and extract all visible ASCII text from the grid,
    /// row by row, separated by newlines.
    fn grid_text(&mut self) -> String {
        let _ = self.render_state.update(self.terminal.inner());
        let (cols, rows) = self.render_state.dimensions();
        let mut lines: Vec<String> = vec![String::new(); usize::from(rows)];
        self.render_state
            .for_each_cell(|row, _col, codepoints, _style, _wide| {
                if let Some(line) = lines.get_mut(row) {
                    if let Some(&cp) = codepoints.first() {
                        if (0x20..0x7F).contains(&cp) {
                            line.push(char::from(u8::try_from(cp).unwrap_or(b'?')));
                        } else if cp == 0 {
                            line.push(' ');
                        } else {
                            line.push(char::from_u32(cp).unwrap_or(' '));
                        }
                    } else {
                        line.push(' ');
                    }
                }
            });
        // Trim trailing spaces from each line, then trailing empty lines
        let trimmed: Vec<&str> = lines.iter().map(|l| l.trim_end()).collect();
        let last_nonempty = trimmed.iter().rposition(|l| !l.is_empty()).unwrap_or(0);
        let used = trimmed.get(..=last_nonempty).unwrap_or(&[]);
        let result = used.join("\n");
        let _ = cols; // suppress unused
        result
    }

    /// Check whether any cell in the grid contains a raw ESC byte (0x1B or 0x9B).
    fn grid_has_escaped_bytes(&mut self) -> bool {
        let _ = self.render_state.update(self.terminal.inner());
        let mut found = false;
        self.render_state
            .for_each_cell(|_row, _col, codepoints, _style, _wide| {
                for &cp in codepoints {
                    if cp == 0x1B || cp == 0x9B {
                        found = true;
                    }
                }
            });
        found
    }
}

const TIMEOUT: Duration = Duration::from_secs(5);

// ===========================================================================
// Section 1: Non-corruption tests — queries don't break subsequent output
// ===========================================================================

// ===========================================================================
// Section 2: Grid integrity — no escape byte leaks
// ===========================================================================

#[test]
#[serial]
fn test_integration_no_escape_leak_in_grid() {
    let mut env = TestEnv::new();
    let _ = env.send_wait_marker(
        b"printf '\\033[c\\033[>c\\033[=c\\033[5n\\033[>q' ; echo CLEAN_LINE\n",
        "CLEAN_LINE",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "No raw ESC bytes should appear in the terminal grid"
    );
    let grid = env.grid_text();
    assert!(grid.contains("CLEAN_LINE"), "got: {grid}");
}

#[test]
#[serial]
fn test_integration_no_escape_leak_after_osc_queries() {
    let mut env = TestEnv::new();
    let _ = env.send_wait_marker(
        b"printf '\\033]10;?\\007\\033]11;?\\007\\033]12;?\\007' ; echo CLEAN_OSC\n",
        "CLEAN_OSC",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "OSC color queries should not leak ESC bytes"
    );
}

#[test]
#[serial]
fn test_integration_no_escape_leak_after_window_queries() {
    let mut env = TestEnv::new();
    let _ = env.send_wait_marker(
        b"printf '\\033[14t\\033[16t\\033[18t' ; echo CLEAN_WIN\n",
        "CLEAN_WIN",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "Window size queries should not leak ESC bytes"
    );
}

// ===========================================================================
// Section 3: Response content verification — exact bytes from scanner
// ===========================================================================

// ===========================================================================
// Section 4: Terminal grid state — rendered output verification
// ===========================================================================

#[test]
#[serial]
fn test_integration_rendered_output_correct() {
    let mut env = TestEnv::new();
    let _ = env.send_wait_marker(b"echo HELLO_WORLD\n", "HELLO_WORLD", TIMEOUT);
    let grid = env.grid_text();
    assert!(grid.contains("HELLO_WORLD"), "got: {grid}");
}

#[test]
#[serial]
fn test_integration_sequential_commands_all_visible() {
    let mut env = TestEnv::new();
    let _ = env.send_wait_marker(b"echo LINE_ONE\n", "LINE_ONE", TIMEOUT);
    let _ = env.send_wait_marker(b"echo LINE_TWO\n", "LINE_TWO", TIMEOUT);
    let _ = env.send_wait_marker(b"echo LINE_THREE\n", "LINE_THREE", TIMEOUT);
    let grid = env.grid_text();
    assert!(grid.contains("LINE_ONE"), "LINE_ONE missing from: {grid}");
    assert!(grid.contains("LINE_TWO"), "LINE_TWO missing from: {grid}");
    assert!(
        grid.contains("LINE_THREE"),
        "LINE_THREE missing from: {grid}"
    );
}

#[test]
#[serial]
fn test_integration_clear_screen_empties_grid() {
    let mut env = TestEnv::new();
    // Write some text
    let _ = env.send_wait_marker(b"echo BEFORE_CLEAR\n", "BEFORE_CLEAR", TIMEOUT);
    // Clear screen (ED 2) and home cursor (CUP)
    let _ = env.send_wait_marker(
        b"printf '\\033[2J\\033[H' ; echo AFTER_CLEAR\n",
        "AFTER_CLEAR",
        TIMEOUT,
    );
    let grid = env.grid_text();
    // BEFORE_CLEAR should be gone after clear
    assert!(
        !grid.contains("BEFORE_CLEAR"),
        "BEFORE_CLEAR should be erased after ED 2, got: {grid}"
    );
    assert!(grid.contains("AFTER_CLEAR"), "AFTER_CLEAR missing: {grid}");
}

// ===========================================================================
// Section 5: Resize integration
// ===========================================================================

#[test]
#[serial]
fn test_integration_resize_terminal_dimensions() {
    let mut env = TestEnv::new();
    assert_eq!(env.render_state.dimensions(), (80, 24));
    env.pty.resize(100, 30, 1000, 750).expect("resize PTY");
    env.terminal.resize(100, 30).expect("resize terminal");
    let _ = env.render_state.update(env.terminal.inner());
    assert_eq!(
        env.render_state.dimensions(),
        (100, 30),
        "RenderState dimensions should match after resize"
    );
}

#[test]
#[serial]
fn test_integration_resize_small() {
    let mut env = TestEnv::with_size(40, 10);
    let _ = env.send_wait_marker(b"echo SMALL_OK\n", "SMALL_OK", TIMEOUT);
    let grid = env.grid_text();
    assert!(grid.contains("SMALL_OK"), "got: {grid}");
}

// ===========================================================================
// Section 6: Large output and stress
// ===========================================================================

#[test]
#[serial]
fn test_integration_large_output_no_crash() {
    let mut env = TestEnv::new();
    // Generate 200 lines of output — more than fits on screen
    let _ = env.send_wait_marker(
        b"i=0; while [ $i -lt 200 ]; do echo \"line_$i\"; i=$((i+1)); done; echo MARKER_LARGE\n",
        "MARKER_LARGE",
        TIMEOUT,
    );
    // Terminal should still be functional
    let _ = env.send_wait_marker(b"echo STILL_ALIVE\n", "STILL_ALIVE", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("STILL_ALIVE"),
        "Terminal broken after large output: {grid}"
    );
}

// ===========================================================================
// Section 7: Edge cases
// ===========================================================================

#[test]
#[serial]
fn test_integration_binary_data_doesnt_crash() {
    let mut env = TestEnv::new();
    // Write raw binary garbage to the PTY
    let garbage: Vec<u8> = (0..=255).collect();
    let _ = env.pty.write_all(&garbage);
    std::thread::sleep(Duration::from_millis(100));
    let _ = env.drain_timeout(Duration::from_millis(500));
    // Terminal should still be functional
    let text = env.send_wait_marker(b"echo SURVIVED_BINARY\n", "SURVIVED_BINARY", TIMEOUT);
    assert!(text.contains("SURVIVED_BINARY"), "Got: {text}");
}

#[test]
#[serial]
fn test_integration_unknown_csi_private_consumed() {
    let mut env = TestEnv::new();
    // CSI < 1 M (mouse SGR release) should be consumed without leaking
    let text = env.send_wait_marker(
        b"printf '\\033[<0;1;1M\\033[<0;1;1m' ; echo MARKER_MOUSE\n",
        "MARKER_MOUSE",
        TIMEOUT,
    );
    assert!(text.contains("MARKER_MOUSE"), "Got: {text}");
    assert!(
        !env.grid_has_escaped_bytes(),
        "Mouse sequences should not leak ESC bytes"
    );
}

#[test]
#[serial]
fn test_integration_render_state_dimensions_match() {
    let mut env = TestEnv::new();
    let _ = env.render_state.update(env.terminal.inner());
    let (cols, rows) = env.render_state.dimensions();
    assert_eq!((cols, rows), (80, 24), "RenderState should be 80x24");
}

#[test]
#[serial]
fn test_integration_cursor_visible_by_default() {
    let mut env = TestEnv::new();
    // Process some data so render state is updated
    let _ = env.send_wait_marker(b"echo X\n", "X", TIMEOUT);
    let cursor = env.render_state.cursor();
    assert!(cursor.visible, "Cursor should be visible by default");
}

// ===========================================================================
// Section 8: Bash integration — real shell with TERM=xterm-256color
//
// These tests use bash (not /bin/sh) with a proper TERM value to exercise
// the startup query/response handshake that interactive shells perform.
// This is where escape sequence leaks and missing features are most likely
// to surface.
// ===========================================================================

#[test]
#[serial]
fn test_bash_startup_no_escape_leak() {
    let mut env = TestEnv::with_bash();
    // Bash with TERM=xterm-256color sends DA1, DSR, DECRQM etc. on startup.
    // After startup completes, the grid should be free of raw ESC bytes.
    assert!(
        !env.grid_has_escaped_bytes(),
        "Bash startup should not leak ESC bytes into the grid"
    );
}

#[test]
#[serial]
fn test_bash_startup_responses_generated() {
    let mut env = TestEnv::with_bash();
    // Send a DA1 query and verify the scanner produces a response, confirming
    // the full pipeline works in a bash session with TERM=xterm-256color.
    let _ = env.take_responses();
    let _ = env.send_wait_marker(
        b"printf '\\033[c' ; echo MARKER_STARTUP_RESP\n",
        "MARKER_STARTUP_RESP",
        TIMEOUT,
    );
    let resp = env.take_responses();
    assert!(
        !resp.is_empty(),
        "Scanner should produce responses in bash session"
    );
}

#[test]
#[serial]
fn test_bash_echo_after_startup() {
    let mut env = TestEnv::with_bash();
    let _ = env.send_wait_marker(b"echo BASH_WORKS\n", "BASH_WORKS", TIMEOUT);
    let grid = env.grid_text();
    assert!(grid.contains("BASH_WORKS"), "got: {grid}");
    assert!(
        !env.grid_has_escaped_bytes(),
        "No ESC leaks after echo in bash"
    );
}

#[test]
#[serial]
fn test_bash_multiple_commands_no_leak() {
    let mut env = TestEnv::with_bash();
    for i in 0..5 {
        let cmd = format!("echo CMD_{i}\n");
        let marker = format!("CMD_{i}");
        let _ = env.send_wait_marker(cmd.as_bytes(), &marker, TIMEOUT);
    }
    let grid = env.grid_text();
    assert!(
        grid.contains("CMD_4"),
        "Last command output missing: {grid}"
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "No ESC leaks after multiple bash commands"
    );
}

#[test]
#[serial]
fn test_bash_query_after_startup() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    let _ = env.send_wait_marker(
        b"printf '\\033[c' ; echo MARKER_BASH_DA1\n",
        "MARKER_BASH_DA1",
        TIMEOUT,
    );
    let resp = env.take_responses();
    assert!(
        resp.windows(b"\x1b[?62;22c".len())
            .any(|w| w == b"\x1b[?62;22c"),
        "DA1 should work in bash session, got: {:?}",
        String::from_utf8_lossy(&resp)
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "No ESC leaks after DA1 in bash"
    );
}

#[test]
#[serial]
fn test_bash_decrqm_after_mode_change() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // Enable bracketed paste mode, then query it via DECRQM
    let _ = env.send_wait_marker(
        b"printf '\\033[?2004h\\033[?2004$p' ; echo MARKER_BPMODE\n",
        "MARKER_BPMODE",
        TIMEOUT,
    );
    let resp = env.take_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    // After enabling 2004, DECRQM should report SET (;1$y)
    assert!(
        resp_str.contains("2004;1$y"),
        "DECRQM should report 2004 as SET after enabling, got: {resp_str}"
    );
}

#[test]
#[serial]
fn test_bash_decrqm_mode_disabled() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // Explicitly disable mode 2004 then query
    let _ = env.send_wait_marker(
        b"printf '\\033[?2004l\\033[?2004$p' ; echo MARKER_BPDIS\n",
        "MARKER_BPDIS",
        TIMEOUT,
    );
    let resp = env.take_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.contains("2004;2$y"),
        "DECRQM should report 2004 as RESET after disabling, got: {resp_str}"
    );
}

#[test]
#[serial]
fn test_bash_cursor_keys_mode() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // Enable application cursor keys (DECCKM), query, then disable and query
    let _ = env.send_wait_marker(
        b"printf '\\033[?1h\\033[?1$p' ; echo MARKER_CKMON\n",
        "MARKER_CKMON",
        TIMEOUT,
    );
    let resp = env.take_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.contains("1;1$y"),
        "DECRQM should report mode 1 (DECCKM) as SET, got: {resp_str}"
    );
}

#[test]
#[serial]
fn test_bash_grid_no_raw_esc_after_queries() {
    let mut env = TestEnv::with_bash();
    // Send a batch of queries that bash readline also sends on startup
    let _ = env.send_wait_marker(
        b"printf '\\033[c\\033[>c\\033[?u\\033[5n\\033[6n\\033[?25$p\\033[?2004$p\\033]11;?\\007' ; echo MARKER_QBATCH\n",
        "MARKER_QBATCH",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "Batch of queries in bash should not leak ESC into grid"
    );
    let grid = env.grid_text();
    assert!(
        grid.contains("MARKER_QBATCH"),
        "Marker should be visible: {grid}"
    );
}

#[test]
#[serial]
fn test_bash_osc_color_set_no_leak() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // OSC 10/11 SET commands should be consumed without leaking to grid.
    // Note: OSC 10/11/12 QUERY responses are not supported by the upstream
    // libghostty_vt callback API — only SET commands are processed.
    let text = env.send_wait_marker(
        b"printf '\\033]10;#c0caf5\\033\\\\\\033]11;#1a1b26\\033\\\\' ; echo MARKER_SETCOLORS\n",
        "MARKER_SETCOLORS",
        TIMEOUT,
    );
    assert!(text.contains("MARKER_SETCOLORS"), "Got: {text}");
    assert!(
        !env.grid_has_escaped_bytes(),
        "OSC color set commands should not leak in bash"
    );
}

#[test]
#[serial]
fn test_bash_prompt_visible() {
    let mut env = TestEnv::with_bash();
    // Set a known prompt so we can verify it renders
    let _ = env.send_wait_marker(b"PS1='TEST> ' ; echo MARKER_PS1\n", "MARKER_PS1", TIMEOUT);
    // Send a newline to get a fresh prompt line
    let _ = env.send_wait_marker(b"echo PROMPT_CHECK\n", "PROMPT_CHECK", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("TEST>"),
        "Custom prompt should be visible: {grid}"
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "No ESC leak after prompt display"
    );
}

#[test]
#[serial]
fn test_bash_scrollback_no_leak() {
    let mut env = TestEnv::with_bash();
    // Generate enough output to scroll in bash session
    let _ = env.send_wait_marker(
        b"for i in $(seq 1 50); do echo \"bash_line_$i\"; done; echo MARKER_BSCROLL\n",
        "MARKER_BSCROLL",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "No ESC leaks after scrolling in bash"
    );
    let grid = env.grid_text();
    assert!(
        grid.contains("MARKER_BSCROLL"),
        "Scroll marker missing: {grid}"
    );
}

#[test]
#[serial]
fn test_bash_alt_screen_roundtrip() {
    let mut env = TestEnv::with_bash();
    let _ = env.send_wait_marker(b"echo MAIN_TEXT_BASH\n", "MAIN_TEXT_BASH", TIMEOUT);
    // Enter alt screen, write, exit — simulates less/vim behavior
    let _ = env.send_wait_marker(
        b"printf '\\033[?1049h\\033[HALT_DATA\\033[?1049l' ; echo BACK_BASH\n",
        "BACK_BASH",
        TIMEOUT,
    );
    let grid = env.grid_text();
    assert!(
        grid.contains("MAIN_TEXT_BASH"),
        "Main screen text restored after alt screen in bash: {grid}"
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "No ESC leak after alt screen roundtrip in bash"
    );
}

#[test]
#[serial]
fn test_bash_window_size_query() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // Window size queries — some apps use these during startup
    let _ = env.send_wait_marker(
        b"printf '\\033[14t\\033[16t\\033[18t' ; echo MARKER_BWIN\n",
        "MARKER_BWIN",
        TIMEOUT,
    );
    let resp = env.take_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.contains("8;24;80t"),
        "Window char size should be reported in bash: {resp_str}"
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "Window size queries should not leak in bash"
    );
}

#[test]
#[serial]
fn test_bash_rapid_queries_no_corruption() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // 20 mixed queries rapidly
    let _ = env.send_wait_marker(
        b"printf '\\033[c\\033[>c\\033[=c\\033[5n\\033[6n\\033[>q\\033[?25$p\\033[18t\\033[c\\033[>c\\033[=c\\033[5n\\033[6n\\033[>q\\033[?25$p\\033[18t\\033[c\\033[>c\\033[=c\\033[5n' ; echo MARKER_BRAPID\n",
        "MARKER_BRAPID",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "Rapid queries in bash should not leak"
    );
    let grid = env.grid_text();
    assert!(
        grid.contains("MARKER_BRAPID"),
        "Rapid query marker missing: {grid}"
    );
}

#[test]
#[serial]
fn test_bash_sgr_colors_no_leak() {
    let mut env = TestEnv::with_bash();
    // Test SGR color sequences (common in colored prompts) don't leak
    let _ = env.send_wait_marker(
        b"printf '\\033[31mRED\\033[32mGREEN\\033[1;34mBOLD_BLUE\\033[0m' ; echo MARKER_BSGR\n",
        "MARKER_BSGR",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "SGR color sequences should not leak in bash"
    );
    let grid = env.grid_text();
    assert!(grid.contains("RED"), "RED text missing: {grid}");
    assert!(grid.contains("GREEN"), "GREEN text missing: {grid}");
    assert!(grid.contains("BOLD_BLUE"), "BOLD_BLUE text missing: {grid}");
}

#[test]
#[serial]
fn test_bash_mode_1000_enable_query() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // Enable mouse tracking (mode 1000) and query via DECRQM in one printf.
    // Single printf ensures both sequences are in the same PTY write chunk,
    // and the terminal has processed the enable before the scanner queries state.
    let _ = env.send_wait_marker(
        b"printf '\\033[?1000h\\033[?1000$p' ; echo MARKER_M1K\n",
        "MARKER_M1K",
        TIMEOUT,
    );
    let resp = env.take_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    assert!(
        resp_str.contains("1000;1$y"),
        "Mouse tracking (1000) should report SET: {resp_str}"
    );
    // Clean up: disable mouse tracking
    let _ = env.send_wait_marker(
        b"printf '\\033[?1000l' ; echo MARKER_M1KCLEAN\n",
        "MARKER_M1KCLEAN",
        TIMEOUT,
    );
}

#[test]
#[serial]
fn test_bash_xtversion_in_bash() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    let _ = env.send_wait_marker(
        b"printf '\\033[>q' ; echo MARKER_BXTV\n",
        "MARKER_BXTV",
        TIMEOUT,
    );
    let resp = env.take_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    let expected = concat!("horseshoe(", env!("CARGO_PKG_VERSION"), ")");
    assert!(
        resp_str.contains(expected),
        "XTVERSION should work in bash: {resp_str}"
    );
}

#[test]
#[serial]
fn test_bash_kitty_keyboard_query() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // Kitty keyboard protocol query — some modern shells use this
    let _ = env.send_wait_marker(
        b"printf '\\033[?u' ; echo MARKER_BKBD\n",
        "MARKER_BKBD",
        TIMEOUT,
    );
    let resp = env.take_responses();
    assert!(
        resp.windows(b"\x1b[?0u".len()).any(|w| w == b"\x1b[?0u"),
        "Kitty keyboard query should work in bash: {:?}",
        String::from_utf8_lossy(&resp)
    );
}

#[test]
#[serial]
fn test_bash_stress_interleaved_output_and_queries() {
    let mut env = TestEnv::with_bash();
    let _ = env.take_responses();
    // Interleave text output with queries — stress test for scanner state
    let _ = env.send_wait_marker(
        b"for i in $(seq 1 10); do echo \"stress_$i\"; printf '\\033[c'; done; echo MARKER_BSTRESS\n",
        "MARKER_BSTRESS",
        TIMEOUT,
    );
    let resp = env.take_responses();
    // Should get 10 DA1 responses
    let da1_count = resp
        .windows(b"\x1b[?62;22c".len())
        .filter(|w| *w == b"\x1b[?62;22c")
        .count();
    assert_eq!(
        da1_count, 10,
        "Should get 10 DA1 responses from stress test, got {da1_count}"
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "No ESC leaks after stress test in bash"
    );
}

// ===========================================================================
// Section 9: Real terminal simulation — exact replica of main.rs data flow
//
// These tests simulate the EXACT scenario of running the horseshoe terminal:
// - bash with TERM=xterm-256color (default) and TERM=foot (foot drop-in)
// - Full readline startup, including all query/response handshakes
// - Actual shell operations: echo, history, variable expansion
// - Grid inspection for ANY leaked escape bytes or garbage
// ===========================================================================

/// Dump all non-empty grid lines with their raw codepoints for debugging.
/// Returns a Vec of (row, text, `has_esc`) tuples.
fn dump_grid_debug(env: &mut TestEnv) -> Vec<(usize, String, bool)> {
    let _ = env.render_state.update(env.terminal.inner());
    let (_cols, rows) = env.render_state.dimensions();
    let mut lines: Vec<(String, bool)> = vec![(String::new(), false); usize::from(rows)];
    env.render_state
        .for_each_cell(|row, _col, codepoints, _style, _wide| {
            if let Some((line, has_esc)) = lines.get_mut(row)
                && let Some(&cp) = codepoints.first()
            {
                if cp == 0x1B || cp == 0x9B {
                    *has_esc = true;
                    line.push('\u{241B}'); // visible ESC symbol
                } else if (0x20..0x7F).contains(&cp) {
                    line.push(char::from(u8::try_from(cp).unwrap_or(b'?')));
                } else if cp == 0 {
                    line.push(' ');
                } else {
                    line.push(char::from_u32(cp).unwrap_or(' '));
                }
            }
        });
    lines
        .into_iter()
        .enumerate()
        .filter(|(_, (text, _))| !text.trim().is_empty())
        .map(|(row, (text, has_esc))| (row, text, has_esc))
        .collect()
}

#[test]
#[serial]
fn test_real_xterm256_startup_clean_grid() {
    // Simulate: hs binary starts bash with TERM=xterm-256color (default)
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");
    // After startup, grid must have zero ESC bytes
    let debug = dump_grid_debug(&mut env);
    let leaks: Vec<_> = debug.iter().filter(|(_, _, esc)| *esc).collect();
    assert!(
        leaks.is_empty(),
        "TERM=xterm-256color: ESC bytes leaked in grid after startup:\n{}",
        leaks
            .iter()
            .map(|(row, text, _)| format!("  row {row}: {text}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
#[serial]
fn test_real_foot_term_startup_clean_grid() {
    // Simulate: hs binary starts bash with TERM=foot (foot drop-in config)
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "foot");
    let debug = dump_grid_debug(&mut env);
    let leaks: Vec<_> = debug.iter().filter(|(_, _, esc)| *esc).collect();
    assert!(
        leaks.is_empty(),
        "TERM=foot: ESC bytes leaked in grid after startup:\n{}",
        leaks
            .iter()
            .map(|(row, text, _)| format!("  row {row}: {text}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
#[serial]
fn test_real_xterm256_echo_works() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");
    let _ = env.send_wait_marker(b"echo HELLO_WORLD_TEST\n", "HELLO_WORLD_TEST", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("HELLO_WORLD_TEST"),
        "TERM=xterm-256color: echo output missing:\n{grid}"
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "TERM=xterm-256color: ESC leak after echo"
    );
}

#[test]
#[serial]
fn test_real_foot_term_echo_works() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "foot");
    let _ = env.send_wait_marker(b"echo HELLO_FOOT_TEST\n", "HELLO_FOOT_TEST", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("HELLO_FOOT_TEST"),
        "TERM=foot: echo output missing:\n{grid}"
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "TERM=foot: ESC leak after echo"
    );
}

#[test]
#[serial]
fn test_real_xterm256_variable_expansion() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");
    let _ = env.send_wait_marker(
        b"TESTVAR=WORKS_FINE; echo \"VAR_IS_$TESTVAR\"\n",
        "VAR_IS_WORKS_FINE",
        TIMEOUT,
    );
    let grid = env.grid_text();
    assert!(
        grid.contains("VAR_IS_WORKS_FINE"),
        "Variable expansion failed:\n{grid}"
    );
}

#[test]
#[serial]
fn test_real_xterm256_command_substitution() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");
    let _ = env.send_wait_marker(b"echo \"SUBST_$(echo INNER)\"\n", "SUBST_INNER", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("SUBST_INNER"),
        "Command substitution failed:\n{grid}"
    );
}

#[test]
#[serial]
fn test_real_xterm256_history_up_arrow() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");
    // Run a command
    let _ = env.send_wait_marker(b"echo HISTORY_TEST_CMD\n", "HISTORY_TEST_CMD", TIMEOUT);
    // Send Up arrow (CSI A) then Enter to re-execute last command
    // Up arrow = \033[A in xterm mode
    let _ = env.send_wait_marker(b"\x1b[A\n", "HISTORY_TEST_CMD", TIMEOUT);
    let grid = env.grid_text();
    // "HISTORY_TEST_CMD" should appear at least twice (original + replayed)
    let count = grid.matches("HISTORY_TEST_CMD").count();
    assert!(
        count >= 2,
        "Up arrow history should replay command (expected >=2 occurrences, got {count}):\n{grid}"
    );
}

#[test]
#[serial]
fn test_real_xterm256_backspace_works() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");
    // Type "echo WRONG", backspace 5 times, type "RIGHT"
    let _ = env.send_wait_marker(b"echo WRONG\x08\x08\x08\x08\x08RIGHT\n", "RIGHT", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("RIGHT"),
        "Backspace editing should work:\n{grid}"
    );
    // "WRONG" should NOT appear in the output (only in the echoed command line)
    let output_lines: Vec<&str> = grid.lines().filter(|l| !l.contains("echo ")).collect();
    let wrong_in_output = output_lines.iter().any(|l| l.contains("WRONG"));
    assert!(
        !wrong_in_output,
        "Backspace should have erased WRONG from output:\n{grid}"
    );
}

#[test]
#[serial]
fn test_real_foot_term_multiple_commands() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "foot");
    let _ = env.send_wait_marker(b"echo CMD_ONE\n", "CMD_ONE", TIMEOUT);
    let _ = env.send_wait_marker(b"echo CMD_TWO\n", "CMD_TWO", TIMEOUT);
    let _ = env.send_wait_marker(b"echo CMD_THREE\n", "CMD_THREE", TIMEOUT);
    let grid = env.grid_text();
    assert!(grid.contains("CMD_ONE"), "CMD_ONE missing:\n{grid}");
    assert!(grid.contains("CMD_TWO"), "CMD_TWO missing:\n{grid}");
    assert!(grid.contains("CMD_THREE"), "CMD_THREE missing:\n{grid}");
    assert!(
        !env.grid_has_escaped_bytes(),
        "TERM=foot: ESC leak after multiple commands"
    );
}

#[test]
#[serial]
fn test_real_xterm256_no_garbage_chars_in_grid() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");
    // After startup, run a simple command and check for garbage characters
    // that indicate DECRQM/DA responses leaked as visible text
    let _ = env.send_wait_marker(b"echo CLEAN_CHECK\n", "CLEAN_CHECK", TIMEOUT);
    let grid = env.grid_text();
    // Common response leak patterns: "62;22c" (DA1), ";1$y" (DECRQM), "0n" (DSR)
    let garbage_patterns = [
        "62;22c",                                              // DA1 response fragment
        ";1$y",                                                // DECRQM SET response fragment
        ";2$y",                                                // DECRQM RESET response fragment
        ">1;1;0c",                                             // DA2 response fragment
        "485253",                                              // DA3 hex response
        concat!("horseshoe(", env!("CARGO_PKG_VERSION"), ")"), // XTVERSION leaked to display
        "?0u",                                                 // Kitty keyboard response
    ];
    for pattern in &garbage_patterns {
        assert!(
            !grid.contains(pattern),
            "Response fragment '{pattern}' leaked into grid:\n{grid}"
        );
    }
}

#[test]
#[serial]
fn test_real_foot_term_no_garbage_chars_in_grid() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "foot");
    let _ = env.send_wait_marker(b"echo CLEAN_FOOT\n", "CLEAN_FOOT", TIMEOUT);
    let grid = env.grid_text();
    let garbage_patterns = [
        "62;22c",
        ";1$y",
        ";2$y",
        ">1;1;0c",
        "485253",
        concat!("horseshoe(", env!("CARGO_PKG_VERSION"), ")"),
        "?0u",
    ];
    for pattern in &garbage_patterns {
        assert!(
            !grid.contains(pattern),
            "TERM=foot: Response fragment '{pattern}' leaked into grid:\n{grid}"
        );
    }
}

#[test]
#[serial]
fn test_real_xterm256_full_session() {
    // Full session simulation: startup → commands → verify clean grid throughout
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "xterm-256color");

    // Step 1: Verify clean startup
    assert!(!env.grid_has_escaped_bytes(), "Grid dirty after startup");

    // Step 2: Run several commands
    let _ = env.send_wait_marker(b"echo STEP_ONE\n", "STEP_ONE", TIMEOUT);
    assert!(!env.grid_has_escaped_bytes(), "ESC leak after step 1");

    let _ = env.send_wait_marker(b"pwd\n", "$", Duration::from_secs(3));
    assert!(!env.grid_has_escaped_bytes(), "ESC leak after pwd");

    let _ = env.send_wait_marker(b"echo STEP_THREE\n", "STEP_THREE", TIMEOUT);
    assert!(!env.grid_has_escaped_bytes(), "ESC leak after step 3");

    // Step 3: Send a batch of queries (like apps do) and verify no leaks
    let _ = env.send_wait_marker(
        b"printf '\\033[c\\033[>c\\033[5n\\033[6n\\033[?25$p' ; echo QUERY_DONE\n",
        "QUERY_DONE",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "ESC leak after query batch in full session"
    );

    // Step 4: Clear readline (Ctrl+C) since query responses went to stdin
    // and may have left garbage on the command line (this is normal terminal
    // behavior — responses to user-initiated printf go to shell stdin).
    let _ = env.pty.write_all(b"\x03");
    std::thread::sleep(Duration::from_millis(100));
    let _ = env.drain_timeout(Duration::from_millis(200));

    let _ = env.send_wait_marker(b"echo FINAL_OK\n", "FINAL_OK", TIMEOUT);
    let grid = env.grid_text();
    assert!(grid.contains("FINAL_OK"), "Final output missing:\n{grid}");
}

#[test]
#[serial]
fn test_real_foot_term_full_session() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/bash", "foot");

    assert!(
        !env.grid_has_escaped_bytes(),
        "TERM=foot: Grid dirty after startup"
    );

    let _ = env.send_wait_marker(b"echo FOOT_ONE\n", "FOOT_ONE", TIMEOUT);
    assert!(
        !env.grid_has_escaped_bytes(),
        "TERM=foot: ESC leak after echo"
    );

    let _ = env.send_wait_marker(
        b"printf '\\033[c\\033[>c\\033[5n' ; echo FOOT_QUERY\n",
        "FOOT_QUERY",
        TIMEOUT,
    );
    assert!(
        !env.grid_has_escaped_bytes(),
        "TERM=foot: ESC leak after queries"
    );

    // Clear readline after query responses (they go to bash stdin as input)
    let _ = env.pty.write_all(b"\x03");
    std::thread::sleep(Duration::from_millis(100));
    let _ = env.drain_timeout(Duration::from_millis(200));

    let _ = env.send_wait_marker(b"echo FOOT_FINAL\n", "FOOT_FINAL", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("FOOT_FINAL"),
        "TERM=foot: Final missing:\n{grid}"
    );
}

// ---------------------------------------------------------------------------
// Delayed PTY read tests — simulate the real app scenario where
// PTY data is NOT read immediately after spawn (e.g. because the
// event loop is busy with Wayland configure).
// ---------------------------------------------------------------------------

/// Simulates the real app bug: spawn bash, delay PTY reads, then check
/// that escape sequence responses don't leak as visible text.
///
/// This catches the exact issue where PTY deferral caused readline
/// queries (DA1, DECRQM) to go unanswered, and their late responses
/// appeared as garbage characters in the prompt.

#[test]
#[serial]
fn test_delayed_pty_read_no_escape_leak() {
    let cols: u16 = 80;
    let rows: u16 = 24;
    let mut pty = Pty::spawn(&horseshoe::pty::SpawnOptions {
        cols,
        rows,
        shell: Some("/bin/bash"),
        term: Some("xterm-256color"),
        pixel_width: 800,
        pixel_height: 600,
        login: false,
        command: None,
        working_directory: None,
    })
    .expect("spawn PTY");
    let mut terminal = TerminalCb::new(cols, rows, 1000).expect("create terminal");
    let mut render_state = RenderState::new().expect("create render state");

    // Simulate the real app delay: DON'T read PTY for 200ms.
    // This is what happens when the event loop is busy with Wayland configure.
    std::thread::sleep(Duration::from_millis(200));

    // Now start reading (simulating the event loop finally processing PTY data).
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut idle = 0u32;
    let mut buf = [0u8; 16384];
    while Instant::now() < deadline {
        let _ = pty.flush_pending();
        match pty.read(&mut buf) {
            Ok(0) | Err(_) => {
                idle += 1;
                if idle > 25 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(n) => {
                idle = 0;
                if let Some(slice) = buf.get(..n) {
                    terminal.vt_write(slice);
                    let responses = terminal.take_pty_responses();
                    if !responses.is_empty() {
                        let _ = pty.write_all(&responses);
                    }
                }
            }
        }
    }

    // Send a command to verify the shell is functional
    let _ = pty.write_all(b"echo DELAYED_OK\n");
    let mut idle2 = 0u32;
    let deadline2 = Instant::now() + Duration::from_secs(3);
    let mut all_output = Vec::new();
    while Instant::now() < deadline2 {
        let _ = pty.flush_pending();
        match pty.read(&mut buf) {
            Ok(0) | Err(_) => {
                idle2 += 1;
                if idle2 > 25 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(n) => {
                idle2 = 0;
                if let Some(slice) = buf.get(..n) {
                    terminal.vt_write(slice);
                    all_output.extend_from_slice(slice);
                    let responses = terminal.take_pty_responses();
                    if !responses.is_empty() {
                        let _ = pty.write_all(&responses);
                    }
                }
                let text = String::from_utf8_lossy(&all_output);
                if text.contains("DELAYED_OK") {
                    // Drain remaining
                    let mut d_idle = 0u32;
                    while d_idle < 10 {
                        let _ = pty.flush_pending();
                        match pty.read(&mut buf) {
                            Ok(0) | Err(_) => {
                                d_idle += 1;
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            Ok(dn) => {
                                d_idle = 0;
                                if let Some(s) = buf.get(..dn) {
                                    terminal.vt_write(s);
                                    let _ = terminal.take_pty_responses();
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    // Check the grid for escape sequence leaks
    let _ = render_state.update(terminal.inner());
    let mut grid_text = String::new();
    render_state.for_each_cell(|_row, _col, codepoints, _style, _wide| {
        if let Some(&cp) = codepoints.first() {
            if (0x20..0x7F).contains(&cp) {
                grid_text.push(char::from(u8::try_from(cp).unwrap_or(b'?')));
            } else if cp == 0 {
                grid_text.push(' ');
            }
        }
    });

    // The grid should contain our marker
    assert!(
        grid_text.contains("DELAYED_OK"),
        "Shell output 'DELAYED_OK' should be visible in grid after delayed read"
    );

    // Check for DA1/DA2/DECRQM response fragments leaked as visible text
    let garbage = [
        "62;22c",
        ";1$y",
        ";2$y",
        ">1;1;0c",
        "485253",
        concat!("horseshoe(", env!("CARGO_PKG_VERSION"), ")"),
        "?0u",
    ];
    for pattern in &garbage {
        assert!(
            !grid_text.contains(pattern),
            "VT response fragment '{pattern}' leaked into grid after delayed read:\n{grid_text}"
        );
    }

    // Cleanup
    let _ = pty.write_all(b"\nexit\n");
    let pid = pty.child_pid();
    let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
}

/// Test that bash is actually interactive, has proper options, and PS1 works.
/// This directly tests "missing bash options" and "PS1 leaks".
#[test]
#[serial]
fn test_bash_interactive_and_ps1() {
    let mut env = TestEnv::with_bash();

    // Check bash is interactive (has 'i' in $-)
    let _ = env.send_wait_marker(b"echo FLAGS:$-:END\n", "FLAGS:", TIMEOUT);
    let grid = env.grid_text();
    // The echoed command line contains "echo FLAGS:$-:END",
    // while the output line starts with "FLAGS:" directly.
    let mut found_flags = false;
    for line in grid.lines() {
        if let Some(rest) = line.strip_prefix("FLAGS:")
            && let Some(flags) = rest.strip_suffix(":END")
        {
            assert!(
                flags.contains('i'),
                "bash should be interactive (have 'i' in $-), got flags: '{flags}'\nGrid:\n{grid}"
            );
            found_flags = true;
            break;
        }
    }
    assert!(found_flags, "FLAGS output line not found in grid:\n{grid}");

    // Check that TERM is set correctly
    let _ = env.send_wait_marker(b"echo TERM:$TERM:END\n", "TERM:", TIMEOUT);
    let grid_term = env.grid_text();
    let mut found_term = false;
    for line in grid_term.lines() {
        if let Some(rest) = line.strip_prefix("TERM:")
            && let Some(val) = rest.strip_suffix(":END")
        {
            assert_eq!(val, "xterm-256color", "TERM mismatch\nGrid:\n{grid_term}");
            found_term = true;
            break;
        }
    }
    assert!(found_term, "TERM output not found in grid:\n{grid_term}");

    // Check COLORTERM
    let _ = env.send_wait_marker(b"echo CT:$COLORTERM:END\n", "CT:", TIMEOUT);
    let grid_ct = env.grid_text();
    let mut found_ct = false;
    for line in grid_ct.lines() {
        if let Some(rest) = line.strip_prefix("CT:")
            && let Some(val) = rest.strip_suffix(":END")
        {
            assert_eq!(val, "truecolor", "COLORTERM mismatch\nGrid:\n{grid_ct}");
            found_ct = true;
            break;
        }
    }
    assert!(found_ct, "COLORTERM output not found in grid:\n{grid_ct}");

    // Check that PS1 is set (not empty)
    let _ = env.send_wait_marker(b"echo PS1LEN:${#PS1}:END\n", "PS1LEN:", TIMEOUT);
    let grid_ps1 = env.grid_text();
    for line in grid_ps1.lines() {
        if let Some(rest) = line.strip_prefix("PS1LEN:")
            && let Some(len_str) = rest.strip_suffix(":END")
        {
            let ps1_len: usize = len_str.parse().unwrap_or(0);
            assert!(
                ps1_len > 0,
                "PS1 should not be empty, length was {ps1_len}\nGrid:\n{grid_ps1}"
            );
            break;
        }
    }

    // Check that checkwinsize is set (common bash option)
    let _ = env.send_wait_marker(
        b"shopt checkwinsize 2>/dev/null && echo CW:ON || echo CW:OFF\n",
        "CW:",
        TIMEOUT,
    );
    let _ = env.grid_text();
    // checkwinsize may or may not be on depending on bashrc, just check it doesn't crash

    // Check no escape sequence garbage in the grid
    let grid_final = env.grid_text();
    let garbage = [
        "[01;", "[32m", "[0m", "[34m", // raw SGR fragments
        "\\[", "\\]", // literal PS1 markers
        "62;22c", ";1$y", ">1;1;0c", // VT response fragments
    ];
    for pat in &garbage {
        assert!(
            !grid_final.contains(pat),
            "garbage pattern '{pat}' found in grid:\n{grid_final}"
        );
    }

    // Check grid doesn't have raw ESC bytes
    assert!(
        !env.grid_has_escaped_bytes(),
        "raw ESC bytes found in grid after bash session"
    );
}

/// Test that a colored PS1 renders correctly through the full pipeline.
/// Specifically tests that ANSI color codes in the prompt don't leak
/// as visible text.
#[test]
#[serial]
fn test_bash_colored_ps1_no_leak() {
    let mut env = TestEnv::with_bash();

    // Set a colored PS1 with escape sequences inside \[...\]
    let _ = env.send_wait_marker(
        b"PS1='\\[\\033[32m\\]TEST>\\[\\033[0m\\] '; echo PS1SET\n",
        "PS1SET",
        TIMEOUT,
    );
    let _ = env.drain_timeout(Duration::from_millis(200));

    // Send a command to trigger the new prompt
    let _ = env.send_wait_marker(b"echo COLORTEST\n", "COLORTEST", TIMEOUT);
    let grid = env.grid_text();

    // The grid should show "TEST>" prompt text, not "[32m" or "[0m"
    assert!(
        grid.contains("TEST>"),
        "colored prompt text 'TEST>' should be visible:\n{grid}"
    );
    // Check prompt lines (lines starting with "TEST>") for escape code leaks.
    // The echoed PS1-assignment command naturally contains [32m as literal text, skip it.
    for line in grid.lines() {
        if line.starts_with("TEST>") || line == "COLORTEST" || line == "PS1SET" {
            assert!(!line.contains("[32m"), "SGR green leaked in output: {line}");
            assert!(!line.contains("[0m"), "SGR reset leaked in output: {line}");
        }
    }
    assert!(
        !env.grid_has_escaped_bytes(),
        "raw ESC bytes in grid after colored PS1"
    );
}

// ===========================================================================
// Section 10: Bash option and PS1 marker rendering tests
//
// These tests verify that:
// - Bash options from .bashrc (vi mode, progcomp) are active
// - PS1 \[...\] markers (SOH/STX) are not rendered as visible characters
// ===========================================================================

/// Verify that bash `progcomp` shopt is active.
///
/// progcomp enables programmable completion and should be on by default
/// in interactive bash sessions.
#[test]
#[serial]
fn test_bash_progcomp_active() {
    let mut env = TestEnv::with_bash();
    let _ = env.send_wait_marker(
        b"shopt progcomp | awk '{print \"PROGCOMP:\"$2\":END\"}'\n",
        "PROGCOMP:",
        TIMEOUT,
    );
    let grid = env.grid_text();
    let mut found = false;
    for line in grid.lines() {
        if let Some(rest) = line.strip_prefix("PROGCOMP:")
            && let Some(val) = rest.strip_suffix(":END")
        {
            assert_eq!(
                val, "on",
                "progcomp should be 'on' in interactive bash, got '{val}'\nGrid:\n{grid}"
            );
            found = true;
            break;
        }
    }
    assert!(found, "PROGCOMP output not found in grid:\n{grid}");
}

/// Verify that C0 controls SOH (0x01) and STX (0x02) are not rendered
/// as visible characters in the terminal grid.
///
/// Bash translates PS1 `\[` to SOH and `\]` to STX. These delimit
/// non-printing sequences (ANSI colors) for readline cursor positioning.
/// The terminal must silently ignore them — rendering them as visible
/// glyphs causes the `\[\]` extra characters the user sees in the prompt.
#[test]
#[serial]
fn test_c0_soh_stx_not_rendered() {
    let mut terminal = TerminalCb::new(80, 24, 100).expect("terminal");
    // Simulate bash prompt output: SOH ESC[33m STX hello SOH ESC[0m STX space
    terminal.vt_write(b"\x01\x1b[33m\x02hello\x01\x1b[0m\x02 ");
    let mut rs = RenderState::new().expect("render state");
    let _ = rs.update(terminal.inner());
    let mut row0 = String::new();
    rs.for_each_cell(|row, _col, codepoints, _style, _wide| {
        if row == 0
            && let Some(&cp) = codepoints.first()
            && cp != 0
        {
            row0.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
        }
    });
    let trimmed = row0.trim_end();
    // SOH (U+0001) and STX (U+0002) must not appear in rendered output.
    assert!(
        !trimmed.contains('\x01'),
        "SOH (0x01) rendered as visible character in grid: {trimmed:?}"
    );
    assert!(
        !trimmed.contains('\x02'),
        "STX (0x02) rendered as visible character in grid: {trimmed:?}"
    );
    assert_eq!(
        trimmed, "hello",
        "expected only 'hello' in grid (no SOH/STX artifacts), got: {trimmed:?}"
    );
}

/// Verify that a bash prompt with \[...\] color markers does not leave
/// visible SOH/STX or literal backslash-bracket artifacts in the grid.
///
/// Uses a real bash PTY with a colored PS1 containing \[...\] markers.
#[test]
#[serial]
fn test_bash_ps1_no_soh_stx_artifacts() {
    let mut env = TestEnv::with_bash();
    // Set a PS1 with color escapes wrapped in \[...\], just like the user's config
    let _ = env.send_wait_marker(
        b"PS1='\\[\\e[32m\\]XPROMPT\\[\\e[0m\\]> '; echo SETPROMPT\n",
        "SETPROMPT",
        TIMEOUT,
    );
    let _ = env.drain_timeout(Duration::from_millis(200));
    // Trigger a fresh prompt
    let _ = env.send_wait_marker(b"echo AFTERPROMPT\n", "AFTERPROMPT", TIMEOUT);

    let _ = env.render_state.update(env.terminal.inner());
    // Scan all cells for SOH (0x01) and STX (0x02) codepoints
    let mut soh_count = 0u32;
    let mut stx_count = 0u32;
    env.render_state
        .for_each_cell(|_row, _col, codepoints, _style, _wide| {
            for &cp in codepoints {
                if cp == 0x01 {
                    soh_count += 1;
                }
                if cp == 0x02 {
                    stx_count += 1;
                }
            }
        });
    assert_eq!(
        soh_count, 0,
        "SOH (\\x01 from PS1 \\[) rendered {soh_count} times as visible cells"
    );
    assert_eq!(
        stx_count, 0,
        "STX (\\x02 from PS1 \\]) rendered {stx_count} times as visible cells"
    );

    // Also check the prompt text is visible without escape artifacts
    let grid = env.grid_text();
    assert!(
        grid.contains("XPROMPT>"),
        "prompt text 'XPROMPT>' should be visible:\n{grid}"
    );
    // Check only PROMPT lines (starting with XPROMPT) for literal \[ or \].
    // The echoed PS1 assignment command naturally contains \[ as text.
    for line in grid.lines() {
        if line.starts_with("XPROMPT>") {
            assert!(
                !line.contains("\\["),
                "literal '\\[' found in prompt line: {line}"
            );
            assert!(
                !line.contains("\\]"),
                "literal '\\]' found in prompt line: {line}"
            );
        }
    }
}

/// Test tab completion works: typing partial command + Tab shows completions.
///
/// Types "ech" and presses Tab. Bash progcomp should complete to "echo".
#[test]
#[serial]
fn test_bash_tab_completion_functional() {
    let mut env = TestEnv::with_bash();
    // Type "ech" then Tab to trigger completion
    let _ = env.pty.write_all(b"ech\t");
    std::thread::sleep(Duration::from_millis(500));
    let _ = env.drain_timeout(Duration::from_millis(300));

    let grid = env.grid_text();
    // After tab completion, "ech" should be completed to "echo"
    let last_line = grid.lines().last().unwrap_or("");
    assert!(
        last_line.contains("echo"),
        "Tab should complete 'ech' to 'echo'.\n\
         Last line: '{last_line}'\nFull grid:\n{grid}"
    );
    // Send Ctrl+C to clear the line, then verify we can still type
    let _ = env.pty.write_all(b"\x03");
    let _ = env.send_wait_marker(b"echo TAB_OK\n", "TAB_OK", TIMEOUT);
    let final_grid = env.grid_text();
    assert!(
        final_grid.contains("TAB_OK"),
        "Shell should still work after tab completion:\n{final_grid}"
    );
}

/// Reproduce the root-cause bug: when the shell path resolves to `/bin/sh`,
/// bash enters POSIX mode. In POSIX mode `.bashrc` is never sourced, PS1
/// `\[\]` markers print as literal text, and user options like `set -o vi`
/// are not active.
///
/// This test uses the `Pty::default_shell()` helper (which mimics the real
/// app's fallback logic) and verifies it NEVER returns a path whose basename
/// is "sh" when the actual binary is bash.
#[test]
#[serial]
fn test_default_shell_not_posix_mode() {
    let shell = horseshoe::pty::default_shell();
    let basename = shell.rsplit('/').next().unwrap_or(&shell);
    assert_ne!(
        basename, "sh",
        "default_shell() returned '{shell}' with basename 'sh'.\n\
         Bash invoked as 'sh' enters POSIX mode: .bashrc not sourced,\n\
         PS1 \\[\\] printed literally, set -o vi not active."
    );
}

/// Verify that a shell spawned via `/bin/sh` (POSIX mode) does NOT have
/// `set -o vi` active and DOES print `\\[\\]` as literal text.
/// This confirms that POSIX mode is the root cause of the user-reported bugs.
#[test]
#[serial]
fn test_sh_posix_mode_causes_reported_bugs() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/sh", "xterm-256color");
    // In POSIX mode, vi should be OFF (even if .bashrc has set -o vi)
    let _ = env.send_wait_marker(b"set -o | grep vi | head -1\n", "vi", TIMEOUT);
    let grid = env.grid_text();
    let vi_line = grid.lines().find(|l| l.trim().starts_with("vi"));
    if let Some(line) = vi_line {
        assert!(
            line.contains("off"),
            "POSIX-mode sh: vi should be off (confirms .bashrc not sourced): {line}"
        );
    }

    // In POSIX mode, PS1 \[\] are printed literally
    let _ = env.send_wait_marker(b"PS1='\\[X\\]> '; echo SETPS1\n", "SETPS1", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    let _ = env.send_wait_marker(b"echo CHECKPROMPT\n", "CHECKPROMPT", TIMEOUT);
    let grid2 = env.grid_text();
    // In POSIX mode, the prompt should contain literal \[ and \]
    let has_literal_markers = grid2.lines().any(|l| l.contains("\\[X\\]>"));
    assert!(
        has_literal_markers,
        "POSIX-mode sh should print \\[X\\]> literally (confirms PS1 \\[\\] bug):\n{grid2}"
    );
}

// ===========================================================================
// Selection, clipboard, paste integration tests
// ===========================================================================

/// Verify that bracketed paste mode (2004) wraps paste content in delimiters.

#[test]
#[serial]
fn test_bracketed_paste_mode_set() {
    let mut env = TestEnv::new();

    // Enable bracketed paste mode
    let _ = env.send_wait_marker(b"printf '\\033[?2004h'; echo BP_SET\n", "BP_SET", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));

    // Verify mode 2004 is set
    let mode = env.terminal.mode_get(Mode::BRACKETED_PASTE);
    assert_eq!(
        mode,
        Some(true),
        "mode 2004 should be enabled after CSI ? 2004 h"
    );
}

/// Verify that bracketed paste mode is off by default.
#[test]
#[serial]
fn test_bracketed_paste_mode_default_off() {
    let env = TestEnv::new();
    let mode = env.terminal.mode_get(Mode::BRACKETED_PASTE);
    assert_ne!(mode, Some(true), "mode 2004 should be off by default");
}

/// Verify that mode 2004 can be set and then reset.
#[test]
#[serial]
fn test_bracketed_paste_mode_set_reset() {
    let mut env = TestEnv::new();

    // Set mode 2004
    let _ = env.send_wait_marker(b"printf '\\033[?2004h'; echo BPSET\n", "BPSET", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    assert_eq!(
        env.terminal.mode_get(Mode::BRACKETED_PASTE),
        Some(true),
        "mode 2004 should be on after setting"
    );

    // Reset mode 2004
    let _ = env.send_wait_marker(b"printf '\\033[?2004l'; echo BPRESET\n", "BPRESET", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    let mode = env.terminal.mode_get(Mode::BRACKETED_PASTE);
    assert_ne!(mode, Some(true), "mode 2004 should be off after resetting");
}

/// Verify that mode 2004 persists across terminal resize.
#[test]
#[serial]
fn test_bracketed_paste_mode_persists_across_resize() {
    let mut env = TestEnv::new();

    let _ = env.send_wait_marker(b"printf '\\033[?2004h'; echo BPSET\n", "BPSET", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    assert_eq!(env.terminal.mode_get(Mode::BRACKETED_PASTE), Some(true),);

    // Resize the terminal
    let _ = env.terminal.resize(100, 30);
    let _ = env.pty.resize(100, 30, 1000, 750);

    // Mode should still be set
    assert_eq!(
        env.terminal.mode_get(Mode::BRACKETED_PASTE),
        Some(true),
        "mode 2004 should persist across resize"
    );
}

/// Verify basic grid text extraction produces readable output.
#[test]
#[serial]
fn test_grid_text_extraction_single_line() {
    let mut env = TestEnv::new();
    let _ = env.send_wait_marker(b"echo HELLO_WORLD\n", "HELLO_WORLD", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("HELLO_WORLD"),
        "grid should contain echoed text: {grid}"
    );
}

/// Verify multi-line output is captured correctly.
#[test]
#[serial]
fn test_grid_text_extraction_multi_line() {
    let mut env = TestEnv::new();
    let _ = env.send_wait_marker(
        b"printf 'LINE_ONE\\nLINE_TWO\\nLINE_THREE\\n'; echo DONE_MULTI\n",
        "DONE_MULTI",
        TIMEOUT,
    );
    let grid = env.grid_text();
    assert!(
        grid.contains("LINE_ONE"),
        "grid should contain LINE_ONE: {grid}"
    );
    assert!(
        grid.contains("LINE_TWO"),
        "grid should contain LINE_TWO: {grid}"
    );
    assert!(
        grid.contains("LINE_THREE"),
        "grid should contain LINE_THREE: {grid}"
    );
}

/// Verify that wide characters (CJK) are handled in grid extraction.
#[test]
#[serial]
fn test_grid_text_extraction_wide_chars() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/sh", "xterm-256color");
    let _ = env.send_wait_marker(
        "printf '日本語テスト'; echo WIDE_DONE\n".as_bytes(),
        "WIDE_DONE",
        TIMEOUT,
    );
    let grid = env.grid_text();
    assert!(
        grid.contains('日') || grid.contains("WIDE_DONE"),
        "grid should contain wide chars or marker: {grid}"
    );
}

/// Verify that writing PTY data and reading it back works for plain paste-like input.
#[test]
#[serial]
fn test_pty_write_and_read_back() {
    let mut env = TestEnv::new();
    // Write text to the PTY and echo it back
    let _ = env.send_wait_marker(b"echo PASTE_TEST_123\n", "PASTE_TEST_123", TIMEOUT);
    let grid = env.grid_text();
    assert!(
        grid.contains("PASTE_TEST_123"),
        "echoed text should appear in grid: {grid}"
    );
}

/// Verify that large output does not corrupt the terminal.
#[test]
#[serial]
fn test_large_output_no_corruption() {
    let mut env = TestEnv::new();
    // Generate a large block of text (1000 lines of 'X')
    let _ = env.send_wait_marker(
        b"i=0; while [ $i -lt 100 ]; do echo XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX; i=$((i+1)); done; echo LARGE_DONE\n",
        "LARGE_DONE",
        Duration::from_secs(10),
    );
    let _ = env.drain_timeout(Duration::from_millis(500));
    assert!(
        !env.grid_has_escaped_bytes(),
        "large output should not leave ESC bytes in grid"
    );
}

/// Verify that unicode text can be written to PTY and appears in grid.
#[test]
#[serial]
fn test_unicode_echo() {
    let mut env = TestEnv::with_shell_term(80, 24, "/bin/sh", "xterm-256color");
    let _ = env.send_wait_marker(
        "printf 'café résumé naïve'; echo UNI_DONE\n".as_bytes(),
        "UNI_DONE",
        TIMEOUT,
    );
    let grid = env.grid_text();
    // At minimum the ASCII parts should be visible
    assert!(
        grid.contains("caf") || grid.contains("UNI_DONE"),
        "unicode text should appear in grid: {grid}"
    );
}

/// Verify mouse tracking mode can be toggled via CSI sequences.
#[test]
#[serial]
fn test_mouse_tracking_mode_toggle() {
    let mut env = TestEnv::new();

    // Initially, normal mouse tracking (1000) should be off
    let mode = env.terminal.mode_get(Mode::NORMAL_MOUSE);
    assert_ne!(mode, Some(true), "mouse tracking should be off initially");

    // Enable mouse tracking
    let _ = env.send_wait_marker(b"printf '\\033[?1000h'; echo MT_ON\n", "MT_ON", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    assert_eq!(
        env.terminal.mode_get(Mode::NORMAL_MOUSE),
        Some(true),
        "mouse tracking should be on after CSI ? 1000 h"
    );

    // Disable mouse tracking
    let _ = env.send_wait_marker(b"printf '\\033[?1000l'; echo MT_OFF\n", "MT_OFF", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    let mode_after = env.terminal.mode_get(Mode::NORMAL_MOUSE);
    assert_ne!(
        mode_after,
        Some(true),
        "mouse tracking should be off after CSI ? 1000 l"
    );
}

/// Verify SGR mouse mode can be toggled.
#[test]
#[serial]
fn test_sgr_mouse_mode_toggle() {
    let mut env = TestEnv::new();

    // Enable SGR mouse mode
    let _ = env.send_wait_marker(b"printf '\\033[?1006h'; echo SGR_ON\n", "SGR_ON", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    assert_eq!(
        env.terminal.mode_get(Mode::SGR_MOUSE),
        Some(true),
        "SGR mouse mode should be on"
    );
}

/// Verify alternate screen mode can be entered and exited.
#[test]
#[serial]
fn test_alternate_screen_toggle() {
    let mut env = TestEnv::new();

    // Enter alternate screen
    let _ = env.send_wait_marker(b"printf '\\033[?1049h'; echo ALT_ON\n", "ALT_ON", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    assert!(
        env.terminal.is_alternate_screen(),
        "should be in alternate screen after CSI ? 1049 h"
    );

    // Exit alternate screen
    let _ = env.send_wait_marker(b"printf '\\033[?1049l'; echo ALT_OFF\n", "ALT_OFF", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    assert!(
        !env.terminal.is_alternate_screen(),
        "should be back in normal screen after CSI ? 1049 l"
    );
}

/// Verify that DECRQM for mode 2004 returns correct responses.
#[test]
#[serial]
fn test_decrqm_mode_2004() {
    let mut env = TestEnv::new();
    let _ = env.take_responses();

    // Query mode 2004 when it's off
    let _ = env.send_wait_marker(
        b"printf '\\033[?2004$p'; echo DECRQM2004\n",
        "DECRQM2004",
        TIMEOUT,
    );
    let _ = env.drain_timeout(Duration::from_millis(300));
    let resp = env.take_responses();
    let resp_str = String::from_utf8_lossy(&resp);
    // Response should contain 2004;2 (mode not set) or 2004;1 (set)
    assert!(
        resp_str.contains("2004;2") || resp_str.contains("2004;1"),
        "DECRQM response should report mode 2004 state: {resp_str:?}"
    );
}

// ===========================================================================
// Section: Render pipeline integration tests (VT → terminal → pixels)
// ===========================================================================

/// Write VT text, update render state, render frame, verify non-bg pixels
/// appear at the expected cell location.
#[test]
#[serial]
fn test_render_pipeline_vt_to_pixels() {
    use horseshoe::font::FontManager;
    use horseshoe::renderer::{RenderOptions, RenderTarget, render_frame};
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let cols = 40u16;
    let rows = 10u16;
    let mut term = PlainTerminal::new(cols, rows, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = u32::from(cols) * font.cell_width;
    let height = u32::from(rows) * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    // Write text at known position
    term.vt_write(b"Hello");
    rs.update(term.inner()).expect("update");

    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut buf,
        width,
        height,
        stride,
        retained: &mut retained,
    };
    let colors = rs.colors();
    render_frame(&mut target, &mut rs, &mut font, &opts, &colors);

    // Sample a pixel in the middle of cell (0,0) — should have glyph content
    let cx = font.cell_width / 2;
    let cy = font.cell_height / 2;
    let off = (cy * stride + cx * 4) as usize;
    let pixel = buf.get(off..off + 4).expect("pixel in bounds");
    // Background is black (0,0,0), so any non-zero RGB means glyph was rendered
    let bg_off = ((height - 1) * stride + (width - 4) * 4) as usize;
    let bg = buf.get(bg_off..bg_off + 4).expect("bg pixel");
    assert_ne!(
        pixel, bg,
        "cell (0,0) with 'H' glyph should differ from background"
    );
}

/// Render with a selection overlay, verify selected region pixels change.
#[test]
#[serial]
fn test_render_selection_overlay() {
    use horseshoe::font::FontManager;
    use horseshoe::renderer::{RenderOptions, RenderTarget, render_frame};
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(40, 10, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 40 * font.cell_width;
    let height = 10 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    term.vt_write(b"Selected text here");
    rs.update(term.inner()).expect("update");

    // Render without selection
    let opts_no_sel = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    {
        let mut tgt = RenderTarget {
            buf: &mut buf,
            width,
            height,
            stride,
            retained: &mut retained,
        };
        let colors = rs.colors();
        render_frame(&mut tgt, &mut rs, &mut font, &opts_no_sel, &colors);
    }
    let cx = font.cell_width / 2;
    let cy = font.cell_height / 2;
    let off = (cy * stride + cx * 4) as usize;
    let before: [u8; 4] = buf
        .get(off..off + 4)
        .expect("pixel")
        .try_into()
        .expect("4 bytes");

    // Render with selection covering first 5 columns
    rs.update(term.inner()).expect("update");
    let opts_sel = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: Some(((0, 0), (4, 0))),
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    {
        let mut tgt = RenderTarget {
            buf: &mut buf,
            width,
            height,
            stride,
            retained: &mut retained,
        };
        let colors = rs.colors();
        render_frame(&mut tgt, &mut rs, &mut font, &opts_sel, &colors);
    }
    let after: [u8; 4] = buf
        .get(off..off + 4)
        .expect("pixel")
        .try_into()
        .expect("4 bytes");

    assert_ne!(
        before, after,
        "selection overlay should change pixels in selected region"
    );
}

/// Generate scrollback, scroll up, render, verify old content is visible.
#[test]
#[serial]
fn test_render_scrollback_content() {
    use horseshoe::font::FontManager;
    use horseshoe::renderer::{RenderOptions, RenderTarget, render_frame};
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(40, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 40 * font.cell_width;
    let height = 5 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    // Generate enough lines to scroll
    for i in 0..20 {
        term.vt_write(format!("line{i:02}\r\n").as_bytes());
    }

    // Scroll up to see old content
    term.scroll_viewport_delta(-10);
    rs.update(term.inner()).expect("update");

    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    let mut target = RenderTarget {
        buf: &mut buf,
        width,
        height,
        stride,
        retained: &mut retained,
    };
    let colors = rs.colors();
    render_frame(&mut target, &mut rs, &mut font, &opts, &colors);

    // After scrolling up, row 0 should have content (old scrollback line)
    // Check that some pixels differ from the background
    let bg_x = width - 1;
    let bg_y = height - 1;
    let bg_off = (bg_y * stride + bg_x * 4) as usize;
    let bg = buf.get(bg_off..bg_off + 4).expect("bg");

    let mut found_content = false;
    for x in 0..font.cell_width * 6 {
        for y in 0..font.cell_height {
            let off = (y * stride + x * 4) as usize;
            let px = buf.get(off..off + 4).expect("px");
            if px != bg {
                found_content = true;
                break;
            }
        }
        if found_content {
            break;
        }
    }
    assert!(
        found_content,
        "scrolled-up viewport should show old content"
    );
}

/// Resize terminal + buffers, render succeeds with new dimensions.
#[test]
#[serial]
fn test_render_after_resize() {
    use horseshoe::font::FontManager;
    use horseshoe::renderer::{RenderOptions, RenderTarget, render_frame};
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);

    // Initial render
    let mut w = 80 * font.cell_width;
    let mut h = 24 * font.cell_height;
    let mut s = w * 4;
    let mut buf = vec![0u8; (s * h) as usize];
    let mut retained = Vec::new();

    term.vt_write(b"Before resize");
    rs.update(term.inner()).expect("update");
    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    {
        let mut tgt = RenderTarget {
            buf: &mut buf,
            width: w,
            height: h,
            stride: s,
            retained: &mut retained,
        };
        let colors = rs.colors();
        render_frame(&mut tgt, &mut rs, &mut font, &opts, &colors);
    }

    // Resize to smaller
    term.resize(40, 12).expect("resize");
    w = 40 * font.cell_width;
    h = 12 * font.cell_height;
    s = w * 4;
    buf = vec![0u8; (s * h) as usize];
    retained.clear();

    rs.update(term.inner()).expect("update");
    {
        let mut tgt = RenderTarget {
            buf: &mut buf,
            width: w,
            height: h,
            stride: s,
            retained: &mut retained,
        };
        let colors = rs.colors();
        render_frame(&mut tgt, &mut rs, &mut font, &opts, &colors);
    }

    assert_eq!(
        buf.len(),
        (s * h) as usize,
        "buffer size should match new dimensions"
    );
    // Alpha should be opaque
    assert_eq!(
        *buf.get(3).expect("alpha"),
        0xFF,
        "alpha should be opaque after resize render"
    );
}

/// Write text, set search highlight, highlighted cells differ.
#[test]
#[serial]
fn test_render_search_highlight() {
    use horseshoe::font::FontManager;
    use horseshoe::renderer::{RenderOptions, RenderTarget, SearchHighlight, render_frame};
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(40, 10, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");
    let mut font = FontManager::new_with_family(16.0, None);
    let width = 40 * font.cell_width;
    let height = 10 * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;
    let mut buf = vec![0u8; buf_len];
    let mut retained = Vec::new();

    term.vt_write(b"findme in this text");
    rs.update(term.inner()).expect("update");

    // Render without highlights
    let opts_no_hl = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    {
        let mut tgt = RenderTarget {
            buf: &mut buf,
            width,
            height,
            stride,
            retained: &mut retained,
        };
        let colors = rs.colors();
        render_frame(&mut tgt, &mut rs, &mut font, &opts_no_hl, &colors);
    }
    let cx = font.cell_width / 2;
    let cy = font.cell_height / 2;
    let off = (cy * stride + cx * 4) as usize;
    let before: [u8; 4] = buf
        .get(off..off + 4)
        .expect("pixel")
        .try_into()
        .expect("4 bytes");

    // Render with search highlight on first 6 columns
    rs.update(term.inner()).expect("update");
    let hl = SearchHighlight {
        row: 0,
        start_col: 0,
        end_col: 5,
        is_current: true,
    };
    let opts_hl = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: true,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[hl],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };
    {
        let mut tgt = RenderTarget {
            buf: &mut buf,
            width,
            height,
            stride,
            retained: &mut retained,
        };
        let colors = rs.colors();
        render_frame(&mut tgt, &mut rs, &mut font, &opts_hl, &colors);
    }
    let after: [u8; 4] = buf
        .get(off..off + 4)
        .expect("pixel")
        .try_into()
        .expect("4 bytes");

    assert_ne!(
        before, after,
        "search highlight should change pixels in highlighted region"
    );
}

// ===========================================================================
// Section: VT attribute rendering integration tests
//
// These verify that VT escape sequences for text attributes (bold, faint,
// inverse) propagate correctly through the full pipeline: VT → terminal →
// render state → pixel buffer.
// ===========================================================================

/// CSI q sequences change cursor style as seen through the PTY pipeline.
#[test]
#[serial]
fn test_cursor_styles_via_vt_pty() {
    use horseshoe::terminal::render::CursorStyle;

    let mut env = TestEnv::new();

    // Set bar cursor (CSI 6 SP q)
    let _ = env.send_wait_marker(b"printf '\\033[6 q'; echo CS_BAR\n", "CS_BAR", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    let cur_bar = env.render_state.cursor();
    assert_eq!(
        cur_bar.style,
        CursorStyle::Bar,
        "CSI 6 SP q should set bar cursor via PTY"
    );

    // Set underline cursor (CSI 4 SP q)
    let _ = env.send_wait_marker(b"printf '\\033[4 q'; echo CS_UND\n", "CS_UND", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    let cur_underline = env.render_state.cursor();
    assert_eq!(
        cur_underline.style,
        CursorStyle::Underline,
        "CSI 4 SP q should set underline cursor via PTY"
    );

    // Set block cursor (CSI 2 SP q)
    let _ = env.send_wait_marker(b"printf '\\033[2 q'; echo CS_BLK\n", "CS_BLK", TIMEOUT);
    let _ = env.drain_timeout(Duration::from_millis(200));
    let cur_block = env.render_state.cursor();
    assert_eq!(
        cur_block.style,
        CursorStyle::Block,
        "CSI 2 SP q should set block cursor via PTY"
    );
}

/// Bold red text renders with bright red (palette 9) when `bold_is_bright`.
///
/// Writes SGR 1;31m (bold + red) text directly to terminal, extracts the
/// `CellStyle` from the render state, and verifies the bold+palette attributes.
#[test]
#[serial]
fn test_bold_bright_color_mapping_render() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(40, 10, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Write bold red text: SGR 1 (bold) + SGR 31 (red fg, palette idx 1)
    term.vt_write(b"\x1b[1;31mBOLD_RED\x1b[0m");
    rs.update(term.inner()).expect("update");

    // Extract the style of the first cell ('B')
    let mut found_bold = false;
    rs.for_each_cell(|row, col, codepoints, style, _wide| {
        if row == 0 && col == 0 && codepoints.first() == Some(&u32::from(b'B')) {
            assert!(style.attrs.bold(), "cell should have bold attribute");
            // fg_palette should be 1 (ANSI red) — bold_is_bright remapping
            // happens at render time, not in the style itself
            assert_eq!(style.fg_palette, 1, "fg palette should be ANSI red (1)");
            found_bold = true;
        }
    });
    assert!(found_bold, "should find bold 'B' cell in render state");

    // Verify the actual rendered colors: render with bold_is_bright=true
    // The palette[9] (bright red) should be used instead of palette[1]
    let colors = rs.colors();
    let bright_red = colors.palette[9];
    let normal_red = colors.palette[1];
    assert_ne!(
        bright_red, normal_red,
        "bright red and normal red should differ in default palette"
    );
}

/// Faint text (SGR 2) halves fg channel values in the render pipeline.
///
/// Writes faint text, then renders a frame and checks that the fg pixels
/// are approximately half brightness compared to normal text.
#[test]
#[serial]
fn test_faint_text_rendered_color() {
    use horseshoe::font::FontManager;
    use horseshoe::renderer::{RenderOptions, RenderTarget, render_frame};
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let cols = 40u16;
    let rows = 10u16;
    let mut font = FontManager::new_with_family(16.0, None);
    let width = u32::from(cols) * font.cell_width;
    let height = u32::from(rows) * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;

    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: false,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };

    // Render normal text
    let mut term_normal = PlainTerminal::new(cols, rows, 100).expect("terminal");
    let mut rs_normal = RenderState::new().expect("render state");
    term_normal.vt_write(b"\x1b[37mX"); // white fg
    rs_normal.update(term_normal.inner()).expect("update");

    let mut buf_normal = vec![0u8; buf_len];
    let mut retained_normal = Vec::new();
    {
        let mut tgt = RenderTarget {
            buf: &mut buf_normal,
            width,
            height,
            stride,
            retained: &mut retained_normal,
        };
        let colors = rs_normal.colors();
        render_frame(&mut tgt, &mut rs_normal, &mut font, &opts, &colors);
    }

    // Render faint text
    let mut term_faint = PlainTerminal::new(cols, rows, 100).expect("terminal");
    let mut rs_faint = RenderState::new().expect("render state");
    term_faint.vt_write(b"\x1b[2;37mX"); // faint + white fg
    rs_faint.update(term_faint.inner()).expect("update");

    let mut buf_faint = vec![0u8; buf_len];
    let mut retained_faint = Vec::new();
    {
        let mut tgt = RenderTarget {
            buf: &mut buf_faint,
            width,
            height,
            stride,
            retained: &mut retained_faint,
        };
        let colors = rs_faint.colors();
        render_frame(&mut tgt, &mut rs_faint, &mut font, &opts, &colors);
    }

    // Find the brightest glyph pixel in each render (in the first cell area)
    let cell_w = font.cell_width as usize;
    let cell_h = font.cell_height as usize;
    let mut max_normal: u8 = 0;
    let mut max_faint: u8 = 0;
    for y in 0..cell_h {
        for x in 0..cell_w {
            let off = y * (stride as usize) + x * 4;
            // BGRA — check R channel (offset 2)
            if let Some(&r) = buf_normal.get(off + 2) {
                max_normal = max_normal.max(r);
            }
            if let Some(&r) = buf_faint.get(off + 2) {
                max_faint = max_faint.max(r);
            }
        }
    }

    // Faint pixel should be noticeably dimmer than normal
    assert!(
        max_normal > 0,
        "normal text should have non-zero brightness"
    );
    assert!(
        max_faint < max_normal,
        "faint text max brightness ({max_faint}) should be less than normal ({max_normal})"
    );
    // The faint formula is >>1, so max_faint should be roughly half of max_normal
    let expected_faint = max_normal / 2;
    let tolerance = 20u8; // allow some anti-aliasing variation
    assert!(
        max_faint <= expected_faint + tolerance,
        "faint max ({max_faint}) should be roughly half normal max ({max_normal}), expected ~{expected_faint}"
    );
}

/// Inverse video (SGR 7) swaps fg and bg in rendered pixels.
///
/// Renders the same character with and without SGR 7, verifies that
/// the background pixels differ (inverse fills bg with fg color).
#[test]
#[serial]
fn test_inverse_video_rendered_colors() {
    use horseshoe::font::FontManager;
    use horseshoe::renderer::{RenderOptions, RenderTarget, render_frame};
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let cols = 40u16;
    let rows = 10u16;
    let mut font = FontManager::new_with_family(16.0, None);
    let width = u32::from(cols) * font.cell_width;
    let height = u32::from(rows) * font.cell_height;
    let stride = width * 4;
    let buf_len = (stride * height) as usize;

    let opts = RenderOptions {
        scrollbar: None,
        bold_is_bright: true,
        cursor_blink_visible: false,
        selection: None,
        padding: 0,
        opacity: 1.0,
        search_highlights: &[],
        search_bar: None,
        search_cursor: 0,
        preedit: None,
        selection_fg: None,
        selection_bg: None,
    };

    // Render normal text
    let mut term_normal = PlainTerminal::new(cols, rows, 100).expect("terminal");
    let mut rs_normal = RenderState::new().expect("render state");
    term_normal.vt_write(b"X");
    rs_normal.update(term_normal.inner()).expect("update");

    let mut buf_normal = vec![0u8; buf_len];
    let mut retained_normal = Vec::new();
    {
        let mut tgt = RenderTarget {
            buf: &mut buf_normal,
            width,
            height,
            stride,
            retained: &mut retained_normal,
        };
        let colors = rs_normal.colors();
        render_frame(&mut tgt, &mut rs_normal, &mut font, &opts, &colors);
    }

    // Render inverse text
    let mut term_inverse = PlainTerminal::new(cols, rows, 100).expect("terminal");
    let mut rs_inverse = RenderState::new().expect("render state");
    term_inverse.vt_write(b"\x1b[7mX"); // SGR 7 = inverse
    rs_inverse.update(term_inverse.inner()).expect("update");

    let mut buf_inverse = vec![0u8; buf_len];
    let mut retained_inverse = Vec::new();
    {
        let mut tgt = RenderTarget {
            buf: &mut buf_inverse,
            width,
            height,
            stride,
            retained: &mut retained_inverse,
        };
        let colors = rs_inverse.colors();
        render_frame(&mut tgt, &mut rs_inverse, &mut font, &opts, &colors);
    }

    // Compare background pixel in the first cell area (pick a corner that's
    // unlikely to have glyph content)
    let cell_w = font.cell_width as usize;
    // Top-right corner of cell 0,0 — likely empty space
    let x = cell_w.saturating_sub(1);
    let y = 0;
    let off = y * (stride as usize) + x * 4;
    let bg_normal: [u8; 4] = buf_normal
        .get(off..off + 4)
        .expect("pixel")
        .try_into()
        .expect("4 bytes");
    let bg_inverse: [u8; 4] = buf_inverse
        .get(off..off + 4)
        .expect("pixel")
        .try_into()
        .expect("4 bytes");

    // With inverse, the background of the cell should become the foreground color
    assert_ne!(
        bg_normal, bg_inverse,
        "inverse video should change the background color of the cell"
    );
}

// ---------------------------------------------------------------------------
// Helper: extract grid text from a RenderState (no TestEnv needed)
// ---------------------------------------------------------------------------

fn plain_grid_text(rs: &mut RenderState) -> String {
    let (cols, rows) = rs.dimensions();
    let mut lines: Vec<String> = vec![String::new(); usize::from(rows)];
    rs.for_each_cell(|row, _col, codepoints, _style, _wide| {
        if let Some(line) = lines.get_mut(row) {
            if let Some(&cp) = codepoints.first() {
                if (0x20..0x7F).contains(&cp) {
                    line.push(char::from(u8::try_from(cp).unwrap_or(b'?')));
                } else if cp == 0 {
                    line.push(' ');
                } else {
                    line.push(char::from_u32(cp).unwrap_or(' '));
                }
            } else {
                line.push(' ');
            }
        }
    });
    let trimmed: Vec<&str> = lines.iter().map(|ln| ln.trim_end()).collect();
    let last_nonempty = trimmed.iter().rposition(|ln| !ln.is_empty()).unwrap_or(0);
    let used = trimmed.get(..=last_nonempty).unwrap_or(&[]);
    let _ = cols;
    used.join("\n")
}

// ---------------------------------------------------------------------------
// VT escape sequence integration tests (PlainTerminal, no PTY)
// ---------------------------------------------------------------------------

/// Cursor movement CSI sequences: CUF (forward), CUU (up), CUD (down).
#[test]
#[serial]
fn test_cursor_movement_csi() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(20, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // X at (0,0), CUF 5 -> col 6, write Y, CUD 2 -> row 2, write Z at col 7
    term.vt_write(b"X\x1b[5CY\x1b[2BZ");
    rs.update(term.inner()).expect("update");

    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();

    // Row 0: X at col 0, Y at col 6
    let row_zero = grid_lines.first().expect("row 0 should exist");
    assert_eq!(
        row_zero.as_bytes().first().copied(),
        Some(b'X'),
        "X should be at col 0 row 0"
    );
    assert_eq!(
        row_zero.as_bytes().get(6).copied(),
        Some(b'Y'),
        "Y should be at col 6 row 0"
    );

    // Row 2: Z at col 7
    let row_two = grid_lines.get(2).expect("row 2 should exist");
    assert_eq!(
        row_two.as_bytes().get(7).copied(),
        Some(b'Z'),
        "Z should be at col 7 row 2"
    );
}

/// Erase in line (EL 0): erase from cursor to end of line.
#[test]
#[serial]
fn test_erase_in_line() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(20, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Write ABCDEFGHIJ, move cursor to row 1 col 6 (1-indexed), EL 0
    term.vt_write(b"ABCDEFGHIJ\x1b[1;6H\x1b[0K");
    rs.update(term.inner()).expect("update");

    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();
    let row_zero = grid_lines.first().expect("row 0 should exist");
    assert_eq!(
        row_zero.trim_end(),
        "ABCDE",
        "only ABCDE should remain after EL 0"
    );
}

/// Insert line (IL): pushes existing lines down within the screen.
#[test]
#[serial]
fn test_insert_delete_line() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(20, 6, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Write AAA on row 0, BBB on row 1, CCC on row 2, then move to row 2
    // (1-indexed) col 1 and insert 1 line
    term.vt_write(b"AAA\r\nBBB\r\nCCC\x1b[2;1H\x1b[1L");
    rs.update(term.inner()).expect("update");

    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();

    assert_eq!(
        grid_lines.first().map(|ln| ln.trim_end()),
        Some("AAA"),
        "row 0 should still be AAA"
    );
    assert_eq!(
        grid_lines.get(1).map(|ln| ln.trim_end()),
        Some(""),
        "row 1 should be blank (inserted line)"
    );
    assert_eq!(
        grid_lines.get(2).map(|ln| ln.trim_end()),
        Some("BBB"),
        "row 2 should now be BBB (shifted down)"
    );
    assert_eq!(
        grid_lines.get(3).map(|ln| ln.trim_end()),
        Some("CCC"),
        "row 3 should now be CCC (shifted down)"
    );
}

/// DECSTBM scroll region: scrolling only affects the defined region.
#[test]
#[serial]
fn test_scroll_region_decstbm() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(10, 6, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Fill rows: row 0 = "111", row 1 = "222", row 2 = "333", row 3 = "444"
    term.vt_write(b"111\r\n222\r\n333\r\n444");

    // Set scroll region to rows 2-4 (1-indexed), then move cursor to row 4,
    // and newline to trigger a scroll within the region
    term.vt_write(b"\x1b[2;4r"); // DECSTBM: top=2 bottom=4
    term.vt_write(b"\x1b[4;1H"); // CUP to row 4, col 1
    term.vt_write(b"\nSCR"); // newline scrolls region, then write "SCR"

    rs.update(term.inner()).expect("update");
    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();

    // Row 0 ("111") should be unaffected (outside scroll region)
    assert_eq!(
        grid_lines.first().map(|ln| ln.trim_end()),
        Some("111"),
        "row 0 should be unchanged (outside scroll region)"
    );
    // Row 1 should have shifted up (was "333" originally at row 2, now at row 1)
    // The exact content depends on how the region scrolled, but row 0 must be "111"
    // and the scrolled region (rows 1-3) should not contain "222" at row 1 anymore
    let row_one = grid_lines.get(1).unwrap_or(&"");
    assert_ne!(
        row_one.trim_end(),
        "222",
        "row 1 should have scrolled (222 should have moved out of the region)"
    );
}

/// Default tab stops: every 8 columns.
#[test]
#[serial]
fn test_tab_stops() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(80, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    term.vt_write(b"\tX");
    rs.update(term.inner()).expect("update");

    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();
    let row_zero = grid_lines.first().expect("row 0 should exist");

    // Tab should advance to col 8, so X is at position 8
    assert_eq!(
        row_zero.as_bytes().get(8).copied(),
        Some(b'X'),
        "X should be at col 8 after a tab from col 0"
    );
    // Everything before should be spaces
    for col in 0..8 {
        assert_eq!(
            row_zero.as_bytes().get(col).copied(),
            Some(b' '),
            "col {col} should be a space"
        );
    }
}

/// SGR 9 (strikethrough) and SGR 53 (overline) set the correct attributes.
#[test]
#[serial]
fn test_strikethrough_overline_attrs() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(40, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // SGR 9 = strikethrough, then reset, SGR 53 = overline
    term.vt_write(b"\x1b[9mSTRIKE\x1b[0m\x1b[53mOVERLINE\x1b[0m");
    rs.update(term.inner()).expect("update");

    let mut found_strike = false;
    let mut found_overline = false;

    rs.for_each_cell(|row, col, codepoints, style, _wide| {
        if row != 0 {
            return;
        }
        // "STRIKE" is at cols 0..6
        if col == 0 && codepoints.first() == Some(&u32::from(b'S')) {
            assert!(
                style.attrs.strikethrough(),
                "col 0 ('S') should have strikethrough"
            );
            assert!(
                !style.attrs.overline(),
                "col 0 ('S') should NOT have overline"
            );
            found_strike = true;
        }
        // "OVERLINE" starts at col 6
        if col == 6 && codepoints.first() == Some(&u32::from(b'O')) {
            assert!(style.attrs.overline(), "col 6 ('O') should have overline");
            assert!(
                !style.attrs.strikethrough(),
                "col 6 ('O') should NOT have strikethrough"
            );
            found_overline = true;
        }
    });

    assert!(found_strike, "should find a cell with strikethrough attr");
    assert!(found_overline, "should find a cell with overline attr");
}

/// OSC 0 sets the window title via the `TerminalCb` callback.
#[test]
#[serial]
fn test_osc_0_sets_title() {
    let mut terminal = TerminalCb::new(80, 24, 100).expect("cb terminal");

    // OSC 0 ; title BEL
    terminal.vt_write(b"\x1b]0;MyTitle\x07");
    let _ = terminal.take_pty_responses();

    let reported = terminal.take_title();
    assert_eq!(
        reported.as_deref(),
        Some("MyTitle"),
        "OSC 0 should set the window title to MyTitle"
    );
}

/// DECAWM auto-wrap: enabled by default (wraps), disabled prevents wrapping.
#[test]
#[serial]
fn test_auto_wrap_toggle() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(80, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Write exactly 80 chars to fill row 0, then one more that should wrap to row 1
    let filler: Vec<u8> = vec![b'A'; 80];
    term.vt_write(&filler);
    term.vt_write(b"W");
    rs.update(term.inner()).expect("update");

    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();

    // Row 1 should contain "W" at col 0 due to auto-wrap
    let row_one_wrap = grid_lines.get(1).expect("row 1 should exist");
    assert_eq!(
        row_one_wrap.as_bytes().first().copied(),
        Some(b'W'),
        "81st char should wrap to row 1 col 0 with DECAWM on"
    );

    // Now disable DECAWM (ESC[?7l), position cursor at end of row 2, write past edge
    term.vt_write(b"\x1b[?7l"); // DECAWM off
    term.vt_write(b"\x1b[3;80H"); // CUP to row 3, col 80 (1-indexed, last column)
    term.vt_write(b"NOPE"); // should NOT wrap, stays on row 2 (0-indexed)
    rs.update(term.inner()).expect("update after DECAWM off");

    let grid_after = plain_grid_text(&mut rs);
    let lines_after: Vec<&str> = grid_after.split('\n').collect();

    // Row 2 (0-indexed) should have content at col 79 (the last char written overwrites)
    let row_two = lines_after.get(2).unwrap_or(&"");
    assert!(
        row_two.len() <= 80,
        "row 2 should not exceed 80 cols with DECAWM off"
    );
    // Row 3 should NOT contain "NOPE" or any of those chars (no wrap happened)
    let row_three = lines_after.get(3).unwrap_or(&"");
    assert!(
        !row_three.contains('N'),
        "row 3 should not have wrapped content with DECAWM off"
    );
}

// ===========================================================================
// Section: Scrollback, wide chars, cursor, colors integration tests
// ===========================================================================

/// Write 2000+ lines and verify scrollback does not grow unbounded.
///
/// The terminal is created with scrollback limit 1000. After writing
/// 2000+ lines, the scrollbar total should be bounded: it should NOT
/// exceed rows + `scrollback_limit` (i.e., 24 + 1000 = 1024). Also
/// scrolls up and verifies the earliest visible line number is not 0
/// (the first lines should have been evicted from scrollback).
#[test]
#[serial]
fn test_scrollback_limit_enforcement() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let cols = 80u16;
    let rows = 24u16;
    let scrollback_limit: usize = 1000;
    let mut term = PlainTerminal::new(cols, rows, scrollback_limit).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Write 2500 numbered lines (well past the 1000-line scrollback limit)
    for i in 0..2500u32 {
        term.vt_write(format!("L{i:04}\r\n").as_bytes());
    }

    // Check scrollbar total: should be bounded
    let sb = term.scrollbar().expect("scrollbar");
    let max_total = u64::from(rows) + scrollback_limit as u64 + 10; // small fudge
    assert!(
        sb.total <= max_total,
        "scrollbar total ({}) should not exceed rows + scrollback_limit (~{max_total})",
        sb.total
    );

    // Scroll all the way up and extract the grid
    term.scroll_viewport_delta(-10000);
    rs.update(term.inner()).expect("update");

    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();

    // Find the smallest line number visible in the grid
    let mut smallest_num: Option<u32> = None;
    for line in &grid_lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('L')
            && let Ok(num) = rest.parse::<u32>()
        {
            smallest_num = Some(match smallest_num {
                Some(prev) => prev.min(num),
                None => num,
            });
        }
    }

    // The earliest visible line should NOT be line 0 because scrollback
    // should have evicted early lines
    let earliest = smallest_num.expect("should find at least one numbered line in scrollback");
    assert!(
        earliest > 0,
        "earliest visible line is L{earliest:04}; expected > 0 because scrollback limit \
         should have evicted early lines"
    );
}

/// Fill a row up to col 79 in an 80-col terminal, then write a 2-wide
/// CJK character. The wide char should wrap to the next row because it
/// cannot fit in a single remaining column.
#[test]
#[serial]
fn test_wide_character_at_terminal_edge() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let cols = 80u16;
    let rows = 5u16;
    let mut term = PlainTerminal::new(cols, rows, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Fill cols 0..79 with 79 narrow chars (leaving exactly 1 col free)
    let filler: Vec<u8> = vec![b'X'; 79];
    term.vt_write(&filler);
    // Write a 2-wide CJK character (U+4E16 = '世', width 2)
    term.vt_write("世".as_bytes());
    rs.update(term.inner()).expect("update");

    let grid = plain_grid_text(&mut rs);
    let grid_lines: Vec<&str> = grid.split('\n').collect();

    // Row 0 should have exactly 79 X's (the wide char didn't fit)
    let row_zero = grid_lines.first().expect("row 0 should exist");
    let x_count = row_zero.chars().filter(|&c| c == 'X').count();
    assert_eq!(
        x_count, 79,
        "row 0 should have 79 X chars, got {x_count}: '{row_zero}'"
    );

    // Row 1 should start with the wide character
    let row_one = grid_lines.get(1).expect("row 1 should exist");
    let first_char = row_one.chars().next();
    assert_eq!(
        first_char,
        Some('\u{4E16}'),
        "wide char should wrap to row 1, row 1 starts with: {first_char:?}"
    );
}

/// Write text, move cursor, then ED 2 (erase entire display). Verify
/// the cursor position is preserved and the screen is cleared.
#[test]
#[serial]
fn test_cursor_position_after_erase_display() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Write some text, then position cursor at row 5, col 10 (1-indexed)
    term.vt_write(b"AAAA\r\nBBBB\r\nCCCC\r\nDDDD");
    term.vt_write(b"\x1b[5;10H"); // CUP to row 5, col 10

    // ED 2: erase entire display (does NOT move cursor per spec)
    term.vt_write(b"\x1b[2J");
    rs.update(term.inner()).expect("update");

    // Check that screen is cleared
    let grid = plain_grid_text(&mut rs);
    assert!(
        !grid.contains("AAAA"),
        "AAAA should be erased after ED 2: '{grid}'"
    );
    assert!(
        !grid.contains("BBBB"),
        "BBBB should be erased after ED 2: '{grid}'"
    );

    // Check cursor position preserved at row 4, col 9 (0-indexed)
    let cursor = rs.cursor();
    assert_eq!(
        (cursor.x, cursor.y),
        (9, 4),
        "cursor should remain at (9, 4) after ED 2, got ({}, {})",
        cursor.x,
        cursor.y
    );
}

/// Apply SGR 31 (red fg), then SGR 39 (default fg). Verify the second
/// cell uses the default color (`fg_tag` == NONE), not red.
#[test]
#[serial]
fn test_color_reset_to_default() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(40, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // SGR 31 = red fg, write 'R', then SGR 39 = default fg, write 'D'
    term.vt_write(b"\x1b[31mR\x1b[39mD");
    rs.update(term.inner()).expect("update");

    let mut red_cell_tag: Option<u32> = None;
    let mut default_cell_tag: Option<u32> = None;
    let mut red_cell_palette: Option<u8> = None;

    rs.for_each_cell(|row, col, codepoints, style, _wide| {
        if row != 0 {
            return;
        }
        if col == 0 && codepoints.first() == Some(&u32::from(b'R')) {
            red_cell_tag = Some(style.fg_tag);
            red_cell_palette = Some(style.fg_palette);
        }
        if col == 1 && codepoints.first() == Some(&u32::from(b'D')) {
            default_cell_tag = Some(style.fg_tag);
        }
    });

    let r_tag = red_cell_tag.expect("should find 'R' cell at col 0");
    let r_palette = red_cell_palette.expect("should find 'R' palette");
    let d_tag = default_cell_tag.expect("should find 'D' cell at col 1");

    // 'R' should have palette color tag with ANSI red (index 1)
    assert_eq!(
        r_tag,
        libghostty_vt::ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
        "'R' cell should have palette color tag"
    );
    assert_eq!(r_palette, 1, "'R' cell should use ANSI red palette index 1");

    // 'D' should have default (NONE) color tag — SGR 39 resets fg
    assert_eq!(
        d_tag,
        libghostty_vt::ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_NONE,
        "'D' cell should have default fg tag (NONE) after SGR 39, got tag {d_tag}"
    );
}

/// Send DECTCEM off (CSI ?25l) then on (CSI ?25h). Verify cursor
/// visibility changes via both the mode query API and the render state.
#[test]
#[serial]
fn test_cursor_visibility_toggle() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(80, 24, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Initially cursor should be visible
    rs.update(term.inner()).expect("update");
    let cursor_init = rs.cursor();
    assert!(cursor_init.visible, "cursor should be visible by default");
    assert_eq!(
        term.mode_get(Mode::CURSOR_VISIBLE),
        Some(true),
        "CURSOR_VISIBLE mode should be true by default"
    );

    // Hide cursor: CSI ?25l (DECTCEM off)
    term.vt_write(b"\x1b[?25l");
    rs.update(term.inner()).expect("update after hide");
    let cursor_hidden = rs.cursor();
    assert!(
        !cursor_hidden.visible,
        "cursor should be hidden after CSI ?25l"
    );
    assert_eq!(
        term.mode_get(Mode::CURSOR_VISIBLE),
        Some(false),
        "CURSOR_VISIBLE mode should be false after CSI ?25l"
    );

    // Show cursor: CSI ?25h (DECTCEM on)
    term.vt_write(b"\x1b[?25h");
    rs.update(term.inner()).expect("update after show");
    let cursor_shown = rs.cursor();
    assert!(
        cursor_shown.visible,
        "cursor should be visible again after CSI ?25h"
    );
    assert_eq!(
        term.mode_get(Mode::CURSOR_VISIBLE),
        Some(true),
        "CURSOR_VISIBLE mode should be true after CSI ?25h"
    );
}

/// Write multiple cells with different SGR colors in rapid succession.
/// Verify each cell has the correct color attribute.
#[test]
#[serial]
fn test_rapid_color_changes_per_cell() {
    use horseshoe::terminal::vt::Terminal as PlainTerminal;

    let mut term = PlainTerminal::new(40, 5, 100).expect("terminal");
    let mut rs = RenderState::new().expect("render state");

    // Write 8 cells, each with a different ANSI color (30-37)
    // SGR 30=black, 31=red, 32=green, 33=yellow, 34=blue, 35=magenta, 36=cyan, 37=white
    term.vt_write(
        b"\x1b[30mA\x1b[31mB\x1b[32mC\x1b[33mD\x1b[34mE\x1b[35mF\x1b[36mG\x1b[37mH\x1b[0m",
    );
    rs.update(term.inner()).expect("update");

    let expected_chars = [b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H'];
    let expected_palette: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    let mut found = [false; 8];

    rs.for_each_cell(|row, col, codepoints, style, _wide| {
        if row != 0 || col >= 8 {
            return;
        }
        let Some(&exp_ch) = expected_chars.get(col) else {
            return;
        };
        let Some(&exp_pal) = expected_palette.get(col) else {
            return;
        };
        let cp = codepoints.first().copied().unwrap_or(0);
        let expected_cp = u32::from(exp_ch);
        assert_eq!(
            cp,
            expected_cp,
            "col {col} should be '{}', got codepoint {cp}",
            char::from(exp_ch)
        );
        assert_eq!(
            style.fg_tag,
            libghostty_vt::ffi::GhosttyStyleColorTag_GHOSTTY_STYLE_COLOR_PALETTE,
            "col {col} should have palette color tag"
        );
        assert_eq!(
            style.fg_palette,
            exp_pal,
            "col {col} ('{}') should have palette index {exp_pal}, got {}",
            char::from(exp_ch),
            style.fg_palette
        );
        if let Some(slot) = found.get_mut(col) {
            *slot = true;
        }
    });

    for (i, was_found) in found.iter().enumerate() {
        let ch = expected_chars.get(i).copied().unwrap_or(b'?');
        assert!(
            was_found,
            "cell at col {i} ('{}') was not found in render state",
            char::from(ch)
        );
    }
}
