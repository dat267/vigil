use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

/// Poll interval for the main loop.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Ctrl+C / termination detection used to stop the main loop on every platform.
mod sig {
    use std::sync::atomic::{AtomicBool, Ordering};

    static HIT: AtomicBool = AtomicBool::new(false);

    pub fn check() -> bool {
        HIT.swap(false, Ordering::Relaxed)
    }

    /// Async-signal-safe: called from the platform signal/console handler.
    fn record() {
        HIT.store(true, Ordering::Relaxed);
    }

    #[cfg(unix)]
    pub fn install() -> Result<(), String> {
        // SAFETY: The handler only performs an atomic store, which is
        // async-signal-safe on all POSIX platforms. The function pointer
        // is passed to signal() which invokes it with the signal number.
        unsafe extern "C" fn handler(_: i32) {
            record();
        }
        // SAFETY: signal() is a standard POSIX C function. The handler
        // pointer is a valid function pointer cast to usize, matching the
        // C signature of signal().
        unsafe extern "C" {
            fn signal(signum: i32, handler: usize) -> usize;
        }
        const SIGINT: i32 = 2;
        const SIGTERM: i32 = 15;
        let handler = handler as *const () as usize;
        for signum in [SIGINT, SIGTERM] {
            // SAFETY: handler is a valid function pointer obtained from an
            // unsafe extern "C" fn, and signum is a valid POSIX signal number.
            // signal() is safe to call from this context; the only race is that
            // the handler may temporarily be SIG_DFL on some implementations,
            // which is benign for our use case.
            if unsafe { signal(signum, handler) } == usize::MAX {
                return Err(format!(
                    "could not install signal handler for signal {signum}"
                ));
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn install() -> Result<(), String> {
        // SAFETY: This handler is called by the OS console control handler
        // mechanism. It only performs an atomic store, which is safe in any
        // context. The function signature matches the required
        // PHANDLER_ROUTINE callback type.
        unsafe extern "system" fn handler(_: u32) -> i32 {
            record();
            1
        }
        // SAFETY: SetConsoleCtrlHandler is a documented Windows API. The
        // function pointer type matches the PHANDLER_ROUTINE signature.
        extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }
        // SAFETY: handler is a valid function pointer matching the
        // PHANDLER_ROUTINE signature. add=1 means install the handler.
        let installed = unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
        if installed == 0 {
            Err("SetConsoleCtrlHandler failed".into())
        } else {
            Ok(())
        }
    }
}

/// Non-panicking stdout output: broken pipes are ignored instead of aborting
/// the process (the release profile uses panic = "abort").
macro_rules! outln {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

/// Reports a fatal error on stderr.
fn report_error(message: &str) {
    let _ = writeln!(std::io::stderr(), "{message}");
}

fn help() {
    // Keep the flag list and descriptions in sync with README.md.
    outln!(
        "\
Usage: vigil [-t <duration>] [-V] [-h]

Keep your system awake. Vigil holds a sleep inhibitor until you stop it with
Ctrl+C, or until its timeout elapses.

Options:
  -t, --timeout <duration>  How long to stay awake: 30s, 45m, 2h, 1h30m.
                            Units are h, m, and s, in that order.
                            Omit to stay awake until stopped.
  -h, --help                Print this help.
  -V, --version             Print the version.

Examples:
  vigil                     stay awake until Ctrl+C
  vigil -t 2h               stay awake for 2 hours
  vigil --timeout=1h30m     same, using the long form

Status output appears only in a terminal; piped output stays silent."
    );
}

fn version() {
    outln!("{}", version_string());
}

fn version_string() -> String {
    format!("vigil {}", env!("CARGO_PKG_VERSION"))
}

/// Compact, human-readable duration: "45s", "1m30s", "2h", "1h2m3s".
fn fmt_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    if total == 0 {
        return "0s".into();
    }
    let (hours, minutes, seconds) = (total / 3600, total % 3600 / 60, total % 60);
    let mut text = String::new();
    if hours > 0 {
        text.push_str(&format!("{hours}h"));
    }
    if minutes > 0 {
        text.push_str(&format!("{minutes}m"));
    }
    if seconds > 0 {
        text.push_str(&format!("{seconds}s"));
    }
    text
}

/// One status line for the TTY, e.g. "Awake 1m5s | 58m55s left".
fn render_status(elapsed: Duration, timeout: Option<Duration>) -> String {
    match timeout {
        Some(limit) => format!(
            "Awake {} | {} left",
            fmt_duration(elapsed),
            fmt_duration(limit.saturating_sub(elapsed))
        ),
        None => format!("Awake {}", fmt_duration(elapsed)),
    }
}

/// The text needed to overwrite `previous` columns with `line`. Padding (rather
/// than ANSI erase) keeps this working on consoles without VT processing.
fn overwrite(previous: usize, line: &str) -> String {
    let padding = previous.saturating_sub(line.chars().count());
    format!("\r{line}{}", " ".repeat(padding))
}

/// The single status line shown on a terminal, redrawn in place.
#[derive(Default)]
struct StatusLine {
    width: usize,
}

impl StatusLine {
    fn new() -> Self {
        Self::default()
    }

    fn draw(&mut self, line: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{}", overwrite(self.width, line));
        let _ = stdout.flush();
        self.width = line.chars().count();
    }

    /// Blanks the line and returns the cursor to its start.
    fn erase(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "\r{}\r", " ".repeat(self.width));
        let _ = stdout.flush();
        self.width = 0;
    }

    /// Replaces the line with a summary and moves to a fresh line.
    fn finish(&mut self, line: &str) {
        let mut stdout = std::io::stdout();
        let _ = writeln!(stdout, "{}", overwrite(self.width, line));
        let _ = stdout.flush();
        self.width = 0;
    }
}

/// Sleeps until the next poll, waking early when the timeout expires sooner.
fn wait_for_next_poll(elapsed: Duration, timeout: Option<Duration>) -> Duration {
    match timeout {
        Some(limit) => POLL_INTERVAL.min(limit.saturating_sub(elapsed)),
        None => POLL_INTERVAL,
    }
}

/// Reports a fatal runtime error, plus an actionable hint when one is known.
fn fail(message: &str) -> ExitCode {
    report_error(&format!("error: {message}"));
    if let Some(hint) = platform::explain_failure(message) {
        report_error(&format!("help: {hint}"));
    }
    ExitCode::from(1)
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    if s.is_empty() {
        return Err("empty duration".into());
    }
    // Units must appear once, in the order h, m, s. "1h1h" or "1m1h" are typos,
    // not sums.
    let mut stage = 0u8; // 0 = before h, 1 = before m, 2 = before s, 3 = finish
    let mut total = 0u64;
    let mut n = 0u64;
    let mut has_digits = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            has_digits = true;
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add((c as u8 - b'0') as u64))
                .ok_or("overflow")?;
        } else {
            if !has_digits {
                return Err(format!("missing number before unit '{c}'"));
            }
            let (seconds, new_stage): (u64, u8) = match c {
                'h' => (n.checked_mul(3600).ok_or("overflow")?, 1),
                'm' => (n.checked_mul(60).ok_or("overflow")?, 2),
                's' => (n, 3),
                _ => return Err(format!("unknown unit '{c}' in duration")),
            };
            if stage >= new_stage {
                return Err(format!(
                    "unit '{c}' is out of order or repeated (expected order: h, m, s)"
                ));
            }
            stage = new_stage;
            total = total.checked_add(seconds).ok_or("overflow")?;
            n = 0;
            has_digits = false;
        }
    }
    if has_digits {
        return Err("digits must be followed by a unit (h, m, or s)".into());
    }
    Ok(Duration::from_secs(total))
}

#[derive(Default)]
struct Options {
    show_help: bool,
    show_version: bool,
    timeout: Option<Duration>,
}

/// Parses and stores a timeout value, translating parse errors into CLI errors.
fn set_timeout(options: &mut Options, value: &str) -> Result<(), String> {
    options.timeout =
        Some(parse_duration(value).map_err(|e| format!("error: invalid timeout: {e}"))?);
    Ok(())
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-h" | "--help" | "help" => options.show_help = true,
            "-V" | "--version" => options.show_version = true,
            "-t" | "--timeout" => {
                i += 1;
                if i >= args.len() || args[i].starts_with('-') {
                    return Err(format!(
                        "error: {arg} requires a value (for example: -t 2h)"
                    ));
                }
                set_timeout(&mut options, &args[i])?;
            }
            _ if arg.starts_with("--timeout=") => {
                set_timeout(&mut options, &arg["--timeout=".len()..])?;
            }
            _ if arg.starts_with("-t=") => {
                set_timeout(&mut options, &arg[3..])?;
            }
            _ => return Err(format!("error: unknown argument '{arg}'")),
        }
        i += 1;
    }
    Ok(options)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(error) => {
            report_error(&error);
            if error.contains("invalid timeout") {
                report_error("help: durations look like 30s, 45m, 2h, or 1h30m");
            } else {
                report_error("help: run 'vigil --help' for usage");
            }
            return ExitCode::from(1);
        }
    };

    if options.show_help {
        help();
        return ExitCode::SUCCESS;
    }
    if options.show_version {
        version();
        return ExitCode::SUCCESS;
    }

    if let Err(error) = sig::install() {
        return fail(&error);
    }
    let mut guard = match platform::start_inhibit() {
        Ok(guard) => guard,
        Err(error) => return fail(&error.to_string()),
    };

    let start = Instant::now();
    let tty = std::io::stdout().is_terminal();
    let mut status = StatusLine::new();

    if tty {
        match options.timeout {
            Some(limit) => outln!(
                "Vigil: preventing sleep for {} (Ctrl+C to stop).",
                fmt_duration(limit)
            ),
            None => outln!("Vigil: preventing sleep (Ctrl+C to stop)."),
        }
    }

    loop {
        if sig::check() {
            if tty {
                status.finish(&format!(
                    "Awake {}. Stopped.",
                    fmt_duration(start.elapsed())
                ));
            }
            break;
        }

        if let Err(error) = guard.check() {
            if tty {
                status.erase();
            }
            return fail(&format!("sleep inhibition failed: {error}"));
        }

        let elapsed = start.elapsed();
        if let Some(limit) = options.timeout {
            if elapsed >= limit {
                if tty {
                    status.finish(&format!(
                        "Awake {}. Timeout reached.",
                        fmt_duration(elapsed)
                    ));
                }
                break;
            }
        }

        if tty {
            status.draw(&render_status(elapsed, options.timeout));
        }
        std::thread::sleep(wait_for_next_poll(elapsed, options.timeout));
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_duration_is_compact() {
        assert_eq!(fmt_duration(Duration::from_secs(0)), "0s");
        assert_eq!(fmt_duration(Duration::from_secs(45)), "45s");
        assert_eq!(fmt_duration(Duration::from_secs(90)), "1m30s");
        assert_eq!(fmt_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(fmt_duration(Duration::from_secs(5400)), "1h30m");
        assert_eq!(fmt_duration(Duration::from_secs(3723)), "1h2m3s");
        assert_eq!(fmt_duration(Duration::from_secs(90000)), "25h");
    }

    #[test]
    fn test_overwrite_pads_shorter_lines() {
        assert_eq!(overwrite(10, "abc"), "\rabc       ");
        assert_eq!(overwrite(3, "abcdef"), "\rabcdef");
        assert_eq!(overwrite(0, "abc"), "\rabc");
    }

    #[test]
    fn test_render_status_counts_down() {
        assert_eq!(
            render_status(Duration::from_secs(65), Some(Duration::from_secs(3600))),
            "Awake 1m5s | 58m55s left"
        );
    }

    #[test]
    fn test_render_status_without_timeout() {
        assert_eq!(render_status(Duration::from_secs(65), None), "Awake 1m5s");
    }

    #[test]
    fn test_render_status_never_shows_negative_time() {
        assert_eq!(
            render_status(Duration::from_secs(10), Some(Duration::from_secs(5))),
            "Awake 10s | 0s left"
        );
    }

    #[test]
    fn test_parse_hours() {
        assert_eq!(parse_duration("2h").unwrap().as_secs(), 7200);
    }

    #[test]
    fn test_parse_minutes() {
        assert_eq!(parse_duration("45m").unwrap().as_secs(), 2700);
    }

    #[test]
    fn test_parse_seconds() {
        assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
    }

    #[test]
    fn test_parse_combined() {
        assert_eq!(parse_duration("1h30m").unwrap().as_secs(), 5400);
    }

    #[test]
    fn test_parse_all() {
        assert_eq!(parse_duration("1h2m3s").unwrap().as_secs(), 3723);
    }

    #[test]
    fn test_parse_invalid_unit() {
        assert!(parse_duration("5x").is_err());
    }

    #[test]
    fn test_parse_trailing_digits() {
        assert!(parse_duration("5h30").is_err());
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn test_parse_zero() {
        assert_eq!(parse_duration("0s").unwrap().as_secs(), 0);
    }

    #[test]
    fn test_parse_overflow() {
        assert!(parse_duration("9999999999999999999h").is_err());
    }

    #[test]
    fn test_parse_digit_overflow() {
        assert!(parse_duration("18446744073709551616h").is_err());
    }

    #[test]
    fn test_parse_requires_digits_before_unit() {
        assert!(parse_duration("h").is_err());
        assert!(parse_duration("1hh").is_err());
    }

    #[test]
    fn test_parse_timeout_equals_form() {
        let args = vec!["vigil".into(), "--timeout=2h".into()];
        let options = parse_args(&args).unwrap();
        assert_eq!(options.timeout.unwrap().as_secs(), 7200);
    }

    #[test]
    fn test_parse_short_timeout_equals_form() {
        let args = vec!["vigil".into(), "-t=30m".into()];
        let options = parse_args(&args).unwrap();
        assert_eq!(options.timeout.unwrap().as_secs(), 1800);
    }

    #[test]
    fn test_parse_rejects_repeated_units() {
        assert!(parse_duration("1h1h").is_err());
        assert!(parse_duration("1m1m").is_err());
        assert!(parse_duration("1s1s").is_err());
    }

    #[test]
    fn test_parse_rejects_out_of_order_units() {
        assert!(parse_duration("1m1h").is_err());
        assert!(parse_duration("1s1m").is_err());
        assert!(parse_duration("1s1h").is_err());
    }

    #[test]
    fn test_parse_rejects_unit_after_finish() {
        assert!(parse_duration("1h2m3s4m").is_err());
    }

    #[test]
    fn test_parse_rejects_unknown_start_subcommand() {
        let args = vec!["vigil".into(), "start".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_version_string() {
        assert!(version_string().starts_with("vigil "));
        assert!(version_string().contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn test_parse_rejects_missing_timeout_value() {
        let args = vec!["vigil".into(), "--timeout".into(), "--version".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_parse_does_not_hide_unknown_argument_behind_help() {
        let args = vec!["vigil".into(), "--unknown".into(), "--help".into()];
        assert!(parse_args(&args).is_err());
    }
}
