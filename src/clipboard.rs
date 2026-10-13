//! # System clipboard subprocess driver.
//!
//! Provides one public entry point, [`copy`], that pipes a string into
//! the platform's clipboard tool (`pbcopy`, `clip`, `wl-copy` or
//! `xclip`) through stdin.
//!
//! ## Why subprocesses instead of a clipboard crate?
//!
//! Native clipboard crates (`arboard`, `clipboard-rs`) pull in heavy
//! platform dependencies: X11 / Wayland / AppKit FFI plus a background
//! thread to hold the selection. For a one-shot timestamp copy on a key
//! press the subprocess approach is simpler and pulls no extra deps.
//!
//! ## Error surfacing
//!
//! Each failure (a missing binary, a broken pipe, a failed wait or a
//! non-zero exit) names the program once. When every candidate fails,
//! [`copy`] returns all of their failures in one line for the status
//! bar, so the user can see which tool to install or fix.

use std::io;
use std::io::Write;

/// Copies `text` to the system clipboard, returning `Ok(())` on the
/// first successful candidate or `Err(msg)` if every candidate failed
/// (or no candidate is configured for the current platform).
///
/// The `Err` string goes to the status bar as it is. It names each
/// program that failed and why.
pub fn copy(text: &str) -> Result<(), String> {
    copy_with(clipboard_commands(), text)
}

/// [`copy`] over an explicit list of `(command, args)` candidates, so a
/// test can name commands that exist on every machine.
fn copy_with(candidates: &[(&str, &[&str])], text: &str) -> Result<(), String> {
    if candidates.is_empty() {
        return Err("No clipboard tool configured for this platform".to_string());
    }

    let mut failures = Vec::new();
    for &(cmd, args) in candidates {
        match pipe_to_command(cmd, args, text) {
            Ok(()) => return Ok(()),
            Err(e) => failures.push(e.to_string()),
        }
    }
    Err(failures.join(", "))
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
        Err(io::Error::other(format!("{cmd} failed with {status}")))
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
    // Tests panic on failure by design. See src/app.rs for the rationale
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

        // A parallel test's child is a zombie for the moment between its
        // exit and its wait, so an exact count can be off by a few. A leak
        // here leaves all ten.
        let after = unreaped_children();
        assert!(after < before + 5, "{before} before, {after} after");
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

    /// A tool that fails hands the copy to the next one.
    ///
    /// Given: a tool that exits with an error, then one that works.
    /// When: the text is copied.
    /// Then: the copy succeeds.
    ///
    /// Why it matters: on Linux `wl-copy` fails outside Wayland, and
    /// `xclip` must still get the copy.
    #[test]
    fn a_failed_tool_hands_the_copy_to_the_next() {
        let result = copy_with(&[("false", &[]), ("cat", &[])], "text");

        assert_eq!(result, Ok(()));
    }

    /// When every tool fails, the message names each of them.
    ///
    /// Given: a tool that is not installed, then one that exits with an
    /// error.
    /// When: the text is copied.
    /// Then: the message names both tools.
    ///
    /// Why it matters: the user fixes the clipboard by installing a
    /// tool, so the message must say which one is missing.
    #[test]
    fn when_every_tool_fails_the_message_names_each_of_them() {
        let missing = "lazytimezone-no-such-binary";

        let err = copy_with(&[(missing, &[]), ("false", &[])], "text").unwrap_err();

        assert!(err.contains(missing) && err.contains("false"), "got: {err}");
    }

    /// A failure message says each fact once.
    ///
    /// Given: a tool that reads all of its input, then exits with an
    /// error.
    /// When: the text is copied.
    /// Then: the message names the tool once and the exit status once.
    ///
    /// Why it matters: the status bar is one line, and a repeated word
    /// pushes the useful part past its edge.
    #[test]
    fn a_failure_message_says_each_fact_once() {
        // `false` can exit before the write, and the copy then fails on
        // a broken pipe with no exit status, so this tool reads first.
        let err = copy_with(&[("sh", &["-c", "cat >/dev/null; exit 1"])], "text").unwrap_err();

        assert_eq!(err, "sh failed with exit status: 1");
    }
}
