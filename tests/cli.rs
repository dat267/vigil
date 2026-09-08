//! UX tests: exercise the CLI the way a user does — by running the binary and
//! checking exit codes, which stream each message lands on, and message text.

use std::process::{Command, Output};
use std::sync::OnceLock;
use std::thread::sleep;
use std::time::{Duration, Instant};

fn vigil(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vigil"))
        .args(args)
        .output()
        .expect("failed to spawn vigil binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Whether the platform can actually inhibit: the inhibitor must survive at
/// least one poll cycle (-t 2s), because -t 0s exits before the main loop
/// would notice a broken inhibitor. Cached; runs once per test binary.
fn inhibitor_available() -> bool {
    static WORKS: OnceLock<bool> = OnceLock::new();
    *WORKS.get_or_init(|| vigil(&["-t", "2s"]).status.success())
}

fn skip_if_unavailable() -> bool {
    if inhibitor_available() {
        return true;
    }
    eprintln!("skipping: no working sleep inhibitor on this host");
    false
}

// ---------------------------------------------------------------- version

#[test]
fn version_flag_prints_version_to_stdout() {
    let out = vigil(&["--version"]);
    assert!(out.status.success());
    assert_eq!(
        stdout(&out),
        format!("vigil {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(stderr(&out), "", "version output must not pollute stderr");
}

#[test]
fn short_version_flag_prints_version() {
    let out = vigil(&["-V"]);
    assert!(out.status.success());
    assert!(stdout(&out).starts_with("vigil "));
}

// ---------------------------------------------------------------- help

#[test]
fn help_flag_prints_usage_to_stdout() {
    let out = vigil(&["--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Usage: vigil"), "got: {text:?}");
    for flag in ["-t, --timeout", "-h, --help", "-V, --version"] {
        assert!(
            text.contains(flag),
            "help must document {flag}, got: {text:?}"
        );
    }
    assert!(
        !text.contains("-q"),
        "-q was removed; help must not document it"
    );
    assert_eq!(stderr(&out), "", "help output must not pollute stderr");
}

#[test]
fn short_help_flag_prints_usage() {
    let out = vigil(&["-h"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: vigil"));
}

#[test]
fn bare_help_word_prints_usage() {
    let out = vigil(&["help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: vigil"));
}

// ---------------------------------------------------------------- errors

#[test]
fn unknown_argument_is_reported_on_stderr_with_exit_code_1() {
    let out = vigil(&["--bogus"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "", "errors must not leak to stdout");
    assert!(stderr(&out).contains("error: unknown argument '--bogus'"));
}

#[test]
fn unknown_argument_error_names_the_offending_argument() {
    let out = vigil(&["--frobnicate"]);
    assert!(stderr(&out).contains("--frobnicate"));
}

#[test]
fn timeout_flag_without_value_reports_error() {
    let out = vigil(&["-t"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("requires a value"));
}

#[test]
fn timeout_flag_with_value_starting_with_dash_reports_error() {
    let out = vigil(&["-t", "-2h"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("requires a value"));
}

#[test]
fn timeout_at_end_of_args_without_value_reports_error() {
    let out = vigil(&["--timeout"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("requires a value"));
}

#[test]
fn invalid_duration_unit_is_reported_with_exit_code_1() {
    let out = vigil(&["-t", "5x"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("error: invalid timeout"));
    assert_eq!(stdout(&out), "");
}

#[test]
fn trailing_digits_without_unit_are_rejected() {
    let out = vigil(&["-t", "5h30"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("error: invalid timeout"));
}

#[test]
fn repeated_units_are_rejected() {
    let out = vigil(&["-t", "1h1h"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("error: invalid timeout"));
}

#[test]
fn out_of_order_units_are_rejected() {
    let out = vigil(&["-t", "1m1h"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("error: invalid timeout"));
}

#[test]
fn overflowing_duration_is_rejected_not_panicking() {
    let out = vigil(&["-t", "99999999999999999999h"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("error: invalid timeout"));
}

#[test]
fn empty_timeout_value_is_rejected() {
    let out = vigil(&["--timeout="]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("error: invalid timeout"));
}

// ------------------------------------------------- removed -q flag

#[test]
fn quiet_flag_is_rejected_as_unknown_argument() {
    let out = vigil(&["-q"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("error: unknown argument '-q'"));
}

#[test]
fn quiet_long_form_is_rejected_as_unknown_argument() {
    let out = vigil(&["--quiet"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("error: unknown argument '--quiet'"));
}

// ------------------------------------------------- run behavior (inhibits)

#[test]
fn successful_run_is_silent_on_piped_streams_and_exits_zero() {
    if !skip_if_unavailable() {
        return;
    }
    let out = vigil(&["-t", "0s"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "", "non-TTY runs must stay silent");
    assert_eq!(stderr(&out), "");
}

#[test]
fn timeout_error_message_goes_to_stderr_when_inhibition_fails() {
    // UX contract: when inhibition cannot start, the message lands on stderr
    // with a nonzero exit — never silently, never on stdout.
    let out = vigil(&["-t", "0s"]);
    if out.status.success() {
        assert_eq!(stderr(&out), "");
    } else {
        assert!(!stderr(&out).is_empty(), "failure must be reported");
        assert_eq!(stdout(&out), "");
    }
}

#[test]
fn running_process_stays_alive_until_timeout() {
    if !skip_if_unavailable() {
        return;
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_vigil"))
        .args(["-t", "30m"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn vigil");
    sleep(Duration::from_millis(500));
    let still_running = matches!(child.try_wait(), Ok(None));
    assert!(still_running, "vigil with -t 30m must still be running");
    child.kill().expect("failed to kill vigil");
    let _ = child.wait();
}

#[test]
fn zero_timeout_exits_promptly() {
    if !skip_if_unavailable() {
        return;
    }
    let start = Instant::now();
    let out = vigil(&["-t", "0s"]);
    assert!(out.status.success());
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "-t 0s must exit promptly, took {:?}",
        start.elapsed()
    );
}

#[test]
fn short_timeout_exits_on_its_own() {
    if !skip_if_unavailable() {
        return;
    }
    let out = vigil(&["-t", "1s"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
}
