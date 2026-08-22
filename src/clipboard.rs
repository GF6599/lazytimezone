//! # System clipboard subprocess driver.
//!
//! Provides a single public entry point — [`copy`] — that pipes a string
//! into the platform-appropriate clipboard tool (`pbcopy` / `clip` /
//! `wl-copy` / `xclip`) via stdin and returns a structured result.
//!
//! ## Why subprocesses instead of a clipboard crate?
//!
//! Native clipboard crates (`arboard`, `clipboard-rs`) pull in heavy
//! platform dependencies — X11 / Wayland / AppKit FFI plus a background
//! thread to hold the selection. For a one-shot timestamp copy on a key
//! press the subprocess approach is simpler and pulls no extra deps.
//!
//! ## Error surfacing
//!
//! Every distinct failure mode — missing binary (spawn error), broken
//! pipe (write error), wait error, non-zero exit — produces a
//! descriptive `io::Error` that the caller can route to the status bar.
//! The program name is embedded in every error so the user can see which
//! candidate (e.g. `pbcopy` vs `xclip`) failed.

use std::io;
use std::io::Write;

/// Copies `text` to the system clipboard, returning `Ok(())` on the
/// first successful candidate or `Err(msg)` if every candidate failed
/// (or no candidate is configured for the current platform).
///
/// The returned `Err` string is suitable for direct surfacing in the
/// status bar — it carries both the program name and the underlying OS
/// error message.
pub fn copy(text: &str) -> Result<(), String> {
    let candidates = clipboard_commands();
    if candidates.is_empty() {
        return Err("No clipboard tool configured for this platform".to_string());
    }

    let mut last_error: Option<String> = None;
    for (cmd, args) in candidates {
        match pipe_to_command(cmd, args, text) {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(format!("{cmd}: {e}")),
        }
    }

    Err(last_error.unwrap_or_else(|| "clipboard copy failed".to_string()))
}

/// Pipes `text` into a command's stdin, returning a structured error if
/// anything goes wrong.
fn pipe_to_command(cmd: &str, args: &[&str], text: &str) -> io::Result<()> {
    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to spawn {cmd}: {e}")))?;

    // `Child` has no `Drop`, so returning before `wait` leaves a zombie
    // for the life of the process. A clipboard tool that exits without
    // reading (wl-copy with no Wayland session) fails the write on every
    // keypress, which is how that accumulates.
    let written = write_to_stdin(&mut child, cmd, text);

    // `wait` closes stdin itself, which is what lets the child see EOF.
    let status = child
        .wait()
        .map_err(|e| io::Error::new(e.kind(), format!("failed to wait on {cmd}: {e}")))?;

    written?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{cmd} exited with status {status}"
        )))
    }
}

fn write_to_stdin(child: &mut std::process::Child, cmd: &str, text: &str) -> io::Result<()> {
    let stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| io::Error::other(format!("failed to open stdin for {cmd}")))?;
    stdin
        .write_all(text.as_bytes())
        .map_err(|e| io::Error::new(e.kind(), format!("failed to write to {cmd}: {e}")))
}

/// Returns the platform-appropriate clipboard command(s) to try, in priority order.
#[cfg(target_os = "macos")]
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[("pbcopy", &[])]
}

#[cfg(target_os = "windows")]
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[("clip", &[])]
}

#[cfg(target_os = "linux")]
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn clipboard_commands() -> &'static [(&'static str, &'static [&'static str])] {
    &[]
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — see src/app.rs for the rationale
    // on why the production panic lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// `std::process::Child` has no `Drop` impl, so a child that is
    /// dropped without `wait` stays in the table until the parent exits.
    fn unreaped_children() -> usize {
        let output = std::process::Command::new("ps")
            .args(["-ax", "-o", "stat=,ppid="])
            .output()
            .expect("ps is available");
        let listing = String::from_utf8_lossy(&output.stdout);
        // A silently empty listing would make every assertion below pass
        // for the wrong reason.
        assert!(
            listing.lines().count() > 1,
            "ps returned no process listing, so this test proves nothing"
        );
        let ours = std::process::id().to_string();
        listing
            .lines()
            .filter(|line| line.trim_start().starts_with('Z'))
            .filter(|line| line.split_whitespace().nth(1) == Some(ours.as_str()))
            .count()
    }

    #[test]
    fn a_command_that_never_reads_stdin_is_still_reaped() {
        // `false` exits before consuming its input. A payload past the
        // pipe buffer therefore fails the write rather than racing it.
        let payload = "x".repeat(4 * 1024 * 1024);
        let before = unreaped_children();

        for _ in 0..10 {
            let _ = pipe_to_command("false", &[], &payload);
        }

        assert_eq!(unreaped_children(), before);
    }

    #[test]
    fn a_non_zero_exit_is_reported_with_the_program_name() {
        let err = pipe_to_command("false", &[], "irrelevant").unwrap_err();

        assert!(err.to_string().contains("false"), "got: {err}");
    }

    #[test]
    fn a_missing_binary_is_reported_with_the_program_name() {
        let err = pipe_to_command("lazytimezone-no-such-binary", &[], "x").unwrap_err();

        assert!(
            err.to_string().contains("lazytimezone-no-such-binary"),
            "got: {err}"
        );
    }
}
