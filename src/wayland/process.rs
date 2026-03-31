use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use horseshoe::paste;

/// Spawn a new terminal instance (fork+exec of self), optionally in `cwd`.
pub(super) fn spawn_terminal_in(cwd: Option<&Path>) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = Command::new(exe);
        if let Some(dir) = cwd {
            let _ = cmd.current_dir(dir);
        }
        unsafe {
            let _ = cmd.pre_exec(|| {
                let _ = nix::unistd::setsid();
                Ok(())
            });
        }
        if let Err(e) = cmd.spawn() {
            eprintln!("Failed to spawn new terminal: {e}");
        }
    }
}

/// Resolve the user's preferred editor from environment variables or PATH.
///
/// Checks `$VISUAL`, then `$EDITOR`, then looks for `vi` and `nano` in `$PATH`.
pub(super) fn resolve_editor() -> Option<String> {
    for var in &["VISUAL", "EDITOR"] {
        if let Ok(val) = std::env::var(var)
            && !val.is_empty()
        {
            return Some(val);
        }
    }
    let path_var = std::env::var("PATH").unwrap_or_default();
    for editor in &["vi", "nano"] {
        for dir in path_var.split(':') {
            let candidate = Path::new(dir).join(editor);
            if candidate.exists() {
                return Some((*editor).to_string());
            }
        }
    }
    None
}

/// Spawn a new terminal window running the given command with arguments.
///
/// Spawns `hs -m -e <editor> <file_path>` as a new process.
pub(super) fn spawn_editor_terminal(editor: &str, file_path: &str) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = Command::new(exe);
        let _ = cmd.args(["-m", "-e", editor, file_path]);
        unsafe {
            let _ = cmd.pre_exec(|| {
                let _ = nix::unistd::setsid();
                Ok(())
            });
        }
        if let Err(e) = cmd.spawn() {
            eprintln!("Failed to spawn scrollback editor: {e}");
        }
    }
}

/// Read a string from a pipe fd with a 2-second timeout.
pub(super) fn read_pipe_with_timeout(
    pipe: &mut (impl std::io::Read + std::os::fd::AsFd),
) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    paste::read_pipe_with_deadline(pipe, deadline)
}
