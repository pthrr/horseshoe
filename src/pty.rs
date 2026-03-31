use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::pty::{Winsize, openpty};
use nix::sys::signal::{SigHandler, SigSet, SigmaskHow, Signal, signal, sigprocmask};
use nix::unistd::{ForkResult, User, execvp, fork, getuid, setsid};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

/// Set an environment variable via libc in the child process after fork.
///
/// Uses `libc::setenv` directly instead of `std::env::set_var` to avoid the
/// disallowed-methods lint (`set_var` is disallowed because it's not thread-safe,
/// but in a post-fork child there's only one thread).
fn child_setenv(key: &str, value: &str) {
    if let (Ok(ck), Ok(cv)) = (CString::new(key), CString::new(value)) {
        unsafe {
            let _ = libc::setenv(ck.as_ptr(), cv.as_ptr(), 1);
        }
    }
}

/// Remove stale terminal-emulator environment variables from other emulators.
fn child_clean_env() {
    const STALE_VARS: &[&str] = &[
        "WEZTERM_EXECUTABLE",
        "WEZTERM_CONFIG_DIR",
        "WEZTERM_CONFIG_FILE",
        "WEZTERM_PANE",
        "WEZTERM_UNIX_SOCKET",
        "KITTY_WINDOW_ID",
        "KITTY_PID",
        "KITTY_INSTALLATION_DIR",
        "ALACRITTY_LOG",
        "ALACRITTY_SOCKET",
        "ALACRITTY_WINDOW_ID",
        "FOOT_SERVER_SOCKET",
        "GHOSTTY_RESOURCES_DIR",
        "GHOSTTY_BIN_DIR",
        "VTE_VERSION",
        "TERMINAL_EMULATOR",
        "TERM_PROGRAM",
        "TERM_PROGRAM_VERSION",
        // Prevent parent environment from altering shell mode.
        // POSIXLY_CORRECT forces bash into POSIX mode where .bashrc is
        // not sourced, PS1 \[\] markers print literally, and progcomp
        // is disabled.
        "POSIXLY_CORRECT",
        // ENV/BASH_ENV: startup files sourced in POSIX/non-interactive
        // mode — clear to prevent unexpected side effects.
        "ENV",
        "BASH_ENV",
    ];
    for var in STALE_VARS {
        if let Ok(ckey) = CString::new(*var) {
            unsafe {
                let _ = libc::unsetenv(ckey.as_ptr());
            }
        }
    }
}

/// Determine the user's default shell, avoiding POSIX-mode `/bin/sh`.
///
/// Priority:
/// 1. `$SHELL` environment variable (set by login manager from `/etc/passwd`)
/// 2. Login shell from `/etc/passwd` via `getpwuid(getuid())`
/// 3. `/bin/bash` as last resort (NOT `/bin/sh` — bash invoked as "sh" enters
///    POSIX mode where `.bashrc` is not sourced and PS1 `\[\]` markers are
///    printed literally)
pub fn default_shell() -> String {
    // Try $SHELL first (most common case)
    if let Ok(shell) = std::env::var("SHELL")
        && !shell.is_empty()
    {
        return shell;
    }
    // Fall back to /etc/passwd via getpwuid
    if let Ok(Some(user)) = User::from_uid(getuid())
        && let Some(s) = user.shell.to_str()
        && !s.is_empty()
    {
        return s.to_string();
    }
    "/bin/bash".to_string()
}

/// Options for spawning a PTY.
pub struct SpawnOptions<'a> {
    pub cols: u16,
    pub rows: u16,
    pub shell: Option<&'a str>,
    pub term: Option<&'a str>,
    pub pixel_width: u16,
    pub pixel_height: u16,
    pub login: bool,
    pub command: Option<&'a [String]>,
    pub working_directory: Option<&'a std::path::Path>,
}

pub struct Pty {
    master: OwnedFd,
    child_pid: nix::unistd::Pid,
    write_buf: Vec<u8>,
}

/// Set up the child side of the PTY: session, terminal, fds, env.
///
/// Separated from `Pty::spawn` to keep each function within the line limit.
fn child_setup_pty(slave: &OwnedFd) {
    let _ = setsid().ok();

    // Restore default signal handling for child process.
    let _ = sigprocmask(SigmaskHow::SIG_SETMASK, Some(&SigSet::empty()), None);
    unsafe {
        let _ = signal(Signal::SIGHUP, SigHandler::SigDfl);
        let _ = signal(Signal::SIGPIPE, SigHandler::SigDfl);
    }

    // Set controlling terminal
    let _ = unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSCTTY, 0) };

    // Set IUTF8 for proper kernel UTF-8 line editing in canonical mode.
    if let Ok(mut termios) = nix::sys::termios::tcgetattr(slave) {
        termios
            .input_flags
            .insert(nix::sys::termios::InputFlags::IUTF8);
        let _ = nix::sys::termios::tcsetattr(slave, nix::sys::termios::SetArg::TCSANOW, &termios);
    }

    // Dup slave to stdin/stdout/stderr
    let slave_fd = slave.as_raw_fd();
    unsafe {
        let _ = libc::dup2(slave_fd, 0);
        let _ = libc::dup2(slave_fd, 1);
        let _ = libc::dup2(slave_fd, 2);
    }

    // Close all inherited file descriptors > 2.
    for fd in 3..1024_i32 {
        let _ = unsafe { libc::close(fd) };
    }
}

/// Set up child environment and exec the command or shell.
fn child_exec(opts: &SpawnOptions<'_>) -> io::Result<()> {
    child_clean_env();
    child_setenv("TERM", opts.term.unwrap_or("xterm-256color"));
    child_setenv("COLORTERM", "truecolor");
    child_setenv("TERM_PROGRAM", "horseshoe");
    child_setenv("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));

    if let Some(dir) = opts.working_directory {
        let _ = nix::unistd::chdir(dir);
    }

    if let Some(cmd) = opts.command
        && let Some(prog) = cmd.first()
    {
        let prog_c = CString::new(prog.as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let args_c: Vec<CString> = cmd
            .iter()
            .map(|a| CString::new(a.as_bytes()))
            .collect::<Result<_, _>>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let _ = execvp(&prog_c, &args_c);
    } else {
        let shell_path = opts.shell.map_or_else(default_shell, String::from);
        let shell_cstr = CString::new(shell_path.as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let basename = shell_path.rsplit('/').next().unwrap_or(&shell_path);
        let argv0_str = if opts.login {
            format!("-{basename}")
        } else {
            basename.to_string()
        };
        let argv0 = CString::new(argv0_str.as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let _ = execvp(&shell_cstr, &[argv0]);
    }
    Ok(())
}

impl Pty {
    /// Spawn a shell (or command) in a new PTY with the given dimensions.
    pub fn spawn(opts: &SpawnOptions<'_>) -> io::Result<Self> {
        let pty = openpty(None, None).map_err(io::Error::other)?;

        // Set initial size
        let ws = Winsize {
            ws_row: opts.rows,
            ws_col: opts.cols,
            ws_xpixel: opts.pixel_width,
            ws_ypixel: opts.pixel_height,
        };
        let _ = unsafe { libc::ioctl(pty.master.as_raw_fd(), libc::TIOCSWINSZ, &ws) };

        match unsafe { fork() }.map_err(io::Error::other)? {
            ForkResult::Child => {
                drop(pty.master);
                child_setup_pty(&pty.slave);
                // slave fd is closed by child_setup_pty's fd cleanup loop (>2)
                let _ = child_exec(opts);
                unsafe { libc::_exit(1) };
            }
            ForkResult::Parent { child } => {
                drop(pty.slave);
                let raw_flags = fcntl(&pty.master, FcntlArg::F_GETFL).map_err(io::Error::other)?;
                let oflags = OFlag::from_bits_truncate(raw_flags);
                let _ = fcntl(&pty.master, FcntlArg::F_SETFL(oflags | OFlag::O_NONBLOCK))
                    .map_err(io::Error::other)?;
                Ok(Pty {
                    master: pty.master,
                    child_pid: child,
                    write_buf: Vec::with_capacity(4096),
                })
            }
        }
    }

    /// Get the master fd for polling.
    pub fn master_fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }

    /// Read available data from the PTY (non-blocking).
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match nix::unistd::read(&self.master, buf) {
            Ok(n) => Ok(n),
            Err(nix::errno::Errno::EAGAIN) => Ok(0),
            Err(e) => Err(io::Error::from(e)),
        }
    }

    /// Queue data to write to the PTY and flush what we can.
    ///
    /// Data is appended to an internal buffer and flushed non-blockingly.
    /// Any data that cannot be written immediately is retained and can be
    /// flushed later via `flush_pending()`.
    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_buf.extend_from_slice(data);
        self.flush_pending()
    }

    /// Try to flush any pending write data to the PTY (non-blocking).
    ///
    /// Called from the event loop to drain queued data without blocking.
    pub fn flush_pending(&mut self) -> io::Result<()> {
        while !self.write_buf.is_empty() {
            match nix::unistd::write(&self.master, &self.write_buf) {
                Ok(n) => {
                    let _ = self.write_buf.drain(..n);
                }
                Err(nix::errno::Errno::EAGAIN) => return Ok(()),
                Err(e) => return Err(io::Error::from(e)),
            }
        }
        Ok(())
    }

    /// Returns true if there is pending write data.
    pub const fn has_pending_writes(&self) -> bool {
        !self.write_buf.is_empty()
    }

    /// Resize the PTY window.
    pub fn resize(
        &self,
        cols: u16,
        rows: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) -> io::Result<()> {
        let ws = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: pixel_width,
            ws_ypixel: pixel_height,
        };
        let ret = unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &ws) };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Get the child PID.
    pub const fn child_pid(&self) -> nix::unistd::Pid {
        self.child_pid
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Send SIGHUP to the child.
        let _ = nix::sys::signal::kill(self.child_pid, Signal::SIGHUP);

        // Non-blocking reap with timeout. If the child ignores SIGHUP (e.g. a
        // login shell trapping signals), a blocking waitpid would hang forever.
        let wnohang = Some(nix::sys::wait::WaitPidFlag::WNOHANG);
        for attempt in 0..20u32 {
            match nix::sys::wait::waitpid(self.child_pid, wnohang) {
                Ok(nix::sys::wait::WaitStatus::StillAlive) => {
                    if attempt == 10 {
                        // Escalate: SIGKILL after 100ms of SIGHUP.
                        let _ = nix::sys::signal::kill(self.child_pid, Signal::SIGKILL);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                _ => return, // Exited, signaled, or already reaped (ECHILD).
            }
        }
        // Final blocking reap after SIGKILL.
        let _ = nix::sys::wait::waitpid(self.child_pid, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_spawn() {
        let pty_result = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        });
        assert!(pty_result.is_ok());
        let pty = pty_result.expect("spawn should succeed");
        assert!(pty.master_fd() >= 0);
    }

    #[test]
    fn test_pty_child_pid() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        // PID should be a positive number
        assert!(pty.child_pid().as_raw() > 0);
    }

    #[test]
    fn test_pty_resize() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        assert!(pty.resize(120, 40, 1200, 1000).is_ok());
        assert!(pty.resize(1, 1, 10, 25).is_ok());
        assert!(pty.resize(300, 100, 3000, 2500).is_ok());
    }

    #[test]
    fn test_pty_read_empty() {
        let mut pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        // Immediately try to read without writing anything meaningful
        // Should return Ok(0) due to non-blocking and EAGAIN
        let mut buf = [0u8; 1024];
        let result = pty.read(&mut buf);
        // Either Ok(0) from EAGAIN/empty, or Ok(n) from shell prompt
        assert!(result.is_ok());
    }

    #[test]
    fn test_login_shell_basename_absolute() {
        let path = "/usr/bin/bash";
        let basename = path.rsplit('/').next().unwrap_or(path);
        assert_eq!(basename, "bash");
        assert_eq!(format!("-{basename}"), "-bash");
    }

    #[test]
    fn test_login_shell_basename_plain() {
        let path = "zsh";
        let basename = path.rsplit('/').next().unwrap_or(path);
        assert_eq!(basename, "zsh");
        assert_eq!(format!("-{basename}"), "-zsh");
    }

    #[test]
    fn test_pty_master_fd_valid() {
        use std::os::fd::BorrowedFd;
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        let fd = pty.master_fd();
        // Verify the fd is valid by checking fcntl
        let result = fcntl(unsafe { BorrowedFd::borrow_raw(fd) }, FcntlArg::F_GETFL);
        assert!(result.is_ok());
    }

    #[test]
    fn test_spawn_explicit_shell() {
        let pty_result = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: Some("/bin/sh"),
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        });
        assert!(
            pty_result.is_ok(),
            "spawning with explicit /bin/sh should succeed"
        );
        let pty = pty_result.expect("spawn");
        assert!(pty.child_pid().as_raw() > 0);
    }

    #[test]
    fn test_spawn_custom_term() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: Some("dumb"),
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        });
        assert!(pty.is_ok(), "spawning with TERM=dumb should succeed");
    }

    #[test]
    fn test_spawn_login_shell() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: true,
            command: None,
            working_directory: None,
        });
        assert!(pty.is_ok(), "spawning login shell should succeed");
    }

    #[test]
    fn test_pty_write_all_basic() {
        let mut pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        // Give the shell a moment to start
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Write a command that produces known output
        pty.write_all(b"echo hello\n")
            .expect("write should succeed");

        // Give time for the shell to process and produce output
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Read back all available data
        let mut output = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match pty.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Some(slice) = buf.get(..n) {
                        output.extend_from_slice(slice);
                    }
                }
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("hello"),
            "PTY output should contain 'hello', got: {text}"
        );
    }

    #[test]
    fn test_pty_fd_closed_on_drop() {
        use std::os::fd::BorrowedFd;
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        let fd = pty.master_fd();
        // Verify fd is valid before drop
        assert!(
            fcntl(unsafe { BorrowedFd::borrow_raw(fd) }, FcntlArg::F_GETFL).is_ok(),
            "fd should be valid before drop"
        );
        drop(pty);
        // After drop, the OwnedFd closes the master fd
        let result = fcntl(unsafe { BorrowedFd::borrow_raw(fd) }, FcntlArg::F_GETFL);
        assert!(result.is_err(), "fd should be invalid after Pty is dropped");
    }

    #[test]
    fn test_pty_drop_reaps_child() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        let pid = pty.child_pid();
        // Drop triggers SIGHUP + waitpid in the Drop impl
        drop(pty);
        // The child should already be reaped by Drop, so waitpid should fail with ECHILD
        let result = nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG));
        assert!(
            result.is_err(),
            "waitpid should return Err(ECHILD) because Drop already reaped the child"
        );
    }

    #[test]
    fn test_spawn_with_command() {
        let cmd = vec!["echo".to_string(), "test_output".to_string()];
        let mut pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: Some(&cmd),
            working_directory: None,
        })
        .expect("spawn with command should succeed");

        // Wait for the command to run
        std::thread::sleep(std::time::Duration::from_millis(500));

        let mut output = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match pty.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Some(slice) = buf.get(..n) {
                        output.extend_from_slice(slice);
                    }
                }
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("test_output"),
            "command output should contain 'test_output', got: {text}"
        );
    }

    #[test]
    fn test_resize_pixel_dimensions() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");

        // Resize with explicit pixel dimensions
        assert!(
            pty.resize(120, 40, 1200, 1000).is_ok(),
            "resize with pixel dims should succeed"
        );
        // Zero pixel dimensions should also work
        assert!(
            pty.resize(80, 24, 0, 0).is_ok(),
            "resize with zero pixel dims should succeed"
        );
    }

    #[test]
    fn test_pty_rapid_resize() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        for i in 0u16..50 {
            let cols = 10 + (i * 3);
            let rows = 5 + i;
            assert!(
                pty.resize(cols, rows, 0, 0).is_ok(),
                "resize {i} should succeed"
            );
        }
    }

    #[test]
    fn test_pty_has_pending_writes_initially_false() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        assert!(
            !pty.has_pending_writes(),
            "fresh PTY should have no pending writes"
        );
    }

    #[test]
    fn test_pty_flush_pending_when_empty() {
        let mut pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        pty.flush_pending()
            .expect("flush_pending on fresh PTY should return Ok(())");
    }

    #[test]
    fn test_pty_write_all_empty_data() {
        let mut pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        pty.write_all(&[])
            .expect("write_all with empty data should return Ok(())");
        assert!(
            !pty.has_pending_writes(),
            "no pending writes after writing empty data"
        );
    }

    #[test]
    fn test_pty_has_pending_writes_false_initially() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        assert!(
            !pty.has_pending_writes(),
            "fresh PTY must not have pending writes"
        );
    }

    #[test]
    fn test_pty_master_fd_consistency() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        let fd1 = pty.master_fd();
        let fd2 = pty.master_fd();
        assert_eq!(
            fd1, fd2,
            "master_fd() must return the same value on consecutive calls"
        );
    }

    #[test]
    fn test_pty_child_pid_consistency() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        let pid1 = pty.child_pid();
        let pid2 = pty.child_pid();
        assert_eq!(
            pid1, pid2,
            "child_pid() must return the same value on consecutive calls"
        );
    }

    #[test]
    fn test_pty_read_nonblocking_empty() {
        let mut pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        // Read immediately before the shell has a chance to produce output.
        // The non-blocking fd should yield Ok(0) from EAGAIN or Ok(n) if the
        // shell prompt arrived fast enough — both are acceptable.
        let mut buf = [0u8; 256];
        let result = pty.read(&mut buf);
        assert!(
            result.is_ok(),
            "read on fresh PTY should not return an error"
        );
    }

    #[test]
    fn test_pty_resize_boundary() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        pty.resize(1, 1, 0, 0)
            .expect("resize to 1x1 with zero pixel dims should succeed");
    }

    #[test]
    fn test_pty_resize_large() {
        let pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: None,
            working_directory: None,
        })
        .expect("spawn should succeed");
        pty.resize(500, 200, 4000, 3200)
            .expect("resize to large dimensions should succeed");
    }

    #[test]
    fn test_default_shell_returns_string() {
        let shell = default_shell();
        assert!(
            !shell.is_empty(),
            "default_shell() must return a non-empty string"
        );
    }

    #[test]
    fn test_default_shell_is_absolute_path() {
        let shell = default_shell();
        assert!(
            shell.starts_with('/'),
            "default_shell() must return an absolute path starting with '/', got: {shell}"
        );
    }

    #[test]
    fn test_spawn_with_working_directory() {
        let cmd = vec!["pwd".to_string()];
        let dir = std::path::Path::new("/tmp");
        let mut pty = Pty::spawn(&SpawnOptions {
            cols: 80,
            rows: 24,
            shell: None,
            term: None,
            pixel_width: 800,
            pixel_height: 600,
            login: false,
            command: Some(&cmd),
            working_directory: Some(dir),
        })
        .expect("spawn with working_directory should succeed");

        // Wait for the command to run and produce output
        std::thread::sleep(std::time::Duration::from_millis(500));

        let mut output = Vec::new();
        let mut buf = [0u8; 4096];
        for _ in 0..20 {
            match pty.read(&mut buf) {
                Ok(0) => {
                    if !output.is_empty() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Ok(n) => {
                    if let Some(slice) = buf.get(..n) {
                        output.extend_from_slice(slice);
                    }
                }
                Err(_) => break,
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("/tmp"),
            "pwd output should contain '/tmp' when working_directory is /tmp, got: {text}"
        );
    }
}
