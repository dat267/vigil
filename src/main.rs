#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{IsTerminal, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

pub(crate) static QUIET: AtomicBool = AtomicBool::new(false);

/// Seconds in the shutdown countdown.
const COUNTDOWN_SECS: i32 = 60;
/// Poll interval for the main loop and the shutdown countdown.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(unix)]
unsafe extern "C" {
    fn geteuid() -> u32;
}

/// True when running as root, which is required (or at least expected) for the
/// -s shutdown feature on Linux and macOS.
#[cfg(unix)]
fn is_root() -> bool {
    unsafe { geteuid() == 0 }
}

/// Ctrl+C / termination detection used to stop the main loop and to cancel the
/// shutdown countdown on every platform.
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
        unsafe extern "C" fn handler(_: i32) {
            record();
        }
        unsafe extern "C" {
            fn signal(signum: i32, handler: usize) -> usize;
        }
        const SIGINT: i32 = 2;
        const SIGTERM: i32 = 15;
        let handler = handler as *const () as usize;
        for signum in [SIGINT, SIGTERM] {
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
        unsafe extern "system" fn handler(_: u32) -> i32 {
            record();
            1
        }
        extern "system" {
            fn SetConsoleCtrlHandler(
                handler: Option<unsafe extern "system" fn(u32) -> i32>,
                add: i32,
            ) -> i32;
        }
        let installed = unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
        if installed == 0 {
            Err("SetConsoleCtrlHandler failed".into())
        } else {
            Ok(())
        }
    }
}

// Windows console API, declared once at module scope.
#[cfg(windows)]
extern "system" {
    fn AttachConsole(dwProcessId: u32) -> i32;
    fn AllocConsole() -> i32;
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut std::ffi::c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: isize,
    ) -> isize;
    fn SetStdHandle(nStdHandle: u32, hHandle: isize) -> i32;
    fn GetStdHandle(nStdHandle: u32) -> isize;
}

/// UTF-16 null-terminated "CONOUT$", the console attached for output. Held as a
/// static so no heap allocation is needed before the standard handles work.
#[cfg(windows)]
const CONOUT_W: [u16; 8] = [
    0x0043, 0x004F, 0x004E, 0x004F, 0x0055, 0x0054, 0x0024, 0x0000,
];

/// CREATE_NO_WINDOW: suppresses the console window of spawned console apps.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Makes stdout/stderr usable for a windows-subsystem exe (which is launched
/// without a console): attach to the parent console, or allocate one, then
/// repoint the invalid standard handles at it.
#[cfg(windows)]
fn init_console() -> Result<(), String> {
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    const NULL_HANDLE: isize = 0;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const INVALID_HANDLE_VALUE: isize = -1;

    let output_was_valid = {
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        handle != NULL_HANDLE && handle != INVALID_HANDLE_VALUE
    };
    let error_was_valid = {
        let handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        handle != NULL_HANDLE && handle != INVALID_HANDLE_VALUE
    };

    if output_was_valid && error_was_valid {
        return Ok(());
    }

    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        // Allocation may fail when a console is already attached but its
        // inherited standard handles are invalid. CONOUT$ below repairs that case.
        let _ = unsafe { AllocConsole() };
    }

    let con_out: isize = unsafe {
        CreateFileW(
            CONOUT_W.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            0,
        )
    };
    if con_out == NULL_HANDLE || con_out == INVALID_HANDLE_VALUE {
        return Err(format!(
            "could not open console output: {}",
            std::io::Error::last_os_error()
        ));
    }
    if !output_was_valid && unsafe { SetStdHandle(STD_OUTPUT_HANDLE, con_out) } == 0 {
        return Err("could not initialize console stdout".into());
    }
    if !error_was_valid && unsafe { SetStdHandle(STD_ERROR_HANDLE, con_out) } == 0 {
        return Err("could not initialize console stderr".into());
    }
    Ok(())
}

/// Non-panicking stdout output: broken pipes are ignored instead of aborting
/// the process (the release profile uses panic = "abort").
macro_rules! outln {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

macro_rules! out {
    ($($arg:tt)*) => {{
        let _ = write!(std::io::stdout(), $($arg)*);
        let _ = std::io::stdout().flush();
    }};
}

/// Reports a fatal error on stderr. On Windows this first initializes the
/// console so the message is visible even in quiet mode, instead of writing to
/// an invalid standard handle and vanishing.
fn report_error(message: &str) {
    #[cfg(windows)]
    let _ = init_console();
    let _ = writeln!(std::io::stderr(), "{message}");
}

fn help() {
    // Keep the flag list and descriptions in sync with README.md.
    outln!(
        "\
Usage: vigil [-t <duration>] [-s] [-q] [-V] [-h]

Keep your system awake.

Flags:
  -t, --timeout <duration>  Stay awake for this long (e.g. 2h, 45m, 30s); \
                             --timeout=<duration> and -t=<duration> are \
                             also accepted. Infinite by default.
  -s, --shutdown            Shutdown when the timeout expires (requires \
                              --timeout). May require elevated privileges.
  -q, --quiet               Suppress normal output (errors are still shown); \
                               hide console window on Windows.
  -h, --help                Print this help.
  -V, --version             Print the version."
    );
}

fn version() {
    outln!("{}", version_string());
}

fn version_string() -> String {
    format!("vigil {}", env!("CARGO_PKG_VERSION"))
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
    quiet: bool,
    show_help: bool,
    show_version: bool,
    timeout: Option<Duration>,
    shutdown: bool,
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
            "-q" | "--quiet" => options.quiet = true,
            "-h" | "--help" | "help" => options.show_help = true,
            "-V" | "--version" => options.show_version = true,
            "-s" | "--shutdown" => options.shutdown = true,
            "-t" | "--timeout" => {
                i += 1;
                if i >= args.len() || args[i].starts_with('-') {
                    return Err(format!("error: {arg} requires a value"));
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
    if options.shutdown && options.timeout.is_none() {
        return Err("error: --shutdown requires --timeout".into());
    }
    Ok(options)
}

/// Issues the platform shutdown command. Returns whether it was accepted.
#[cfg(target_os = "windows")]
fn run_shutdown() -> std::io::Result<bool> {
    let mut command = Command::new("shutdown");
    command.args(["/s", "/t", "0"]);
    if QUIET.load(Ordering::Relaxed) {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    // Never let shutdown.exe flash its own console window.
    command.creation_flags(CREATE_NO_WINDOW);
    match command.status() {
        Ok(s) => Ok(s.success()),
        Err(_) => Ok(false),
    }
}

#[cfg(target_os = "linux")]
fn run_shutdown() -> std::io::Result<bool> {
    // Root: classic shutdown. Non-root: prefer polkit-aware systemctl, then
    // fall back to the plain shutdown command (e.g. on non-systemd distros).
    let candidates: &[&[&str]] = if is_root() {
        &[&["shutdown", "-h", "now"]]
    } else {
        &[&["systemctl", "poweroff"], &["shutdown", "-h", "now"]]
    };
    for candidate in candidates {
        let mut command = Command::new(candidate[0]);
        command.args(&candidate[1..]);
        if QUIET.load(Ordering::Relaxed) {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
        match command.status() {
            Ok(s) if s.success() => return Ok(true),
            _ => continue,
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn run_shutdown() -> std::io::Result<bool> {
    let mut command = Command::new("shutdown");
    command.args(["-h", "now"]);
    if QUIET.load(Ordering::Relaxed) {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    match command.status() {
        Ok(s) => Ok(s.success()),
        Err(_) => Ok(false),
    }
}

/// Runs the 60-second shutdown countdown, then issues the shutdown command.
/// Returns true if the flow completed or was cancelled by the user, false if
/// the shutdown command failed to execute.
fn trigger_shutdown() -> bool {
    if !QUIET.load(Ordering::Relaxed) {
        outln!("\nShutting down in 60 seconds. Press Ctrl+C to cancel.");
    }
    #[cfg(unix)]
    if !QUIET.load(Ordering::Relaxed) && !is_root() {
        outln!(
            "warning: shutting down usually requires elevated privileges; \
             if it fails, rerun vigil with sudo"
        );
    }
    let tty = std::io::stdout().is_terminal();
    for i in (1..=COUNTDOWN_SECS).rev() {
        if sig::check() {
            if !QUIET.load(Ordering::Relaxed) {
                outln!("\nShutdown cancelled.");
            }
            return true;
        }
        if tty && !QUIET.load(Ordering::Relaxed) {
            out!("\rShutting down in {i}s... ");
        } else if !tty
            && !QUIET.load(Ordering::Relaxed)
            && (i == COUNTDOWN_SECS || i <= 5 || i % 10 == 0)
        {
            outln!("Shutting down in {i}s...");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    if sig::check() {
        if !QUIET.load(Ordering::Relaxed) {
            outln!("\nShutdown cancelled.");
        }
        return true;
    }
    if !QUIET.load(Ordering::Relaxed) {
        outln!("\nShutting down...");
    }
    match run_shutdown() {
        Ok(true) => true,
        Ok(false) | Err(_) => {
            if !QUIET.load(Ordering::Relaxed) {
                outln!(
                    "warning: shutdown command failed; your user may lack shutdown \
                     privileges. The system will stay awake (try running vigil with sudo)."
                );
            }
            false
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let quiet_hint = args
        .iter()
        .skip(1)
        .any(|arg| arg == "-q" || arg == "--quiet");
    // A windows-subsystem exe is launched without a console; decide up front
    // whether output is needed (help/version/parse errors) so it is visible.
    #[cfg(windows)]
    let wants_console = !quiet_hint
        || args
            .iter()
            .skip(1)
            .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help" | "-V" | "--version"));
    QUIET.store(quiet_hint, Ordering::Relaxed);

    #[cfg(windows)]
    if wants_console {
        if let Err(error) = init_console() {
            report_error(&format!("error: {error}"));
            return ExitCode::from(1);
        }
    }

    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(error) => {
            report_error(&error);
            return ExitCode::from(1);
        }
    };
    QUIET.store(options.quiet, Ordering::Relaxed);

    if options.show_help {
        help();
        return ExitCode::SUCCESS;
    }
    if options.show_version {
        version();
        return ExitCode::SUCCESS;
    }

    if let Err(error) = sig::install() {
        report_error(&format!("error: {error}"));
        return ExitCode::from(1);
    }
    let mut guard = match platform::start_inhibit() {
        Ok(guard) => guard,
        Err(error) => {
            report_error(&format!("error: {error}"));
            return ExitCode::from(1);
        }
    };

    let start = Instant::now();
    let tty = std::io::stdout().is_terminal();

    if tty && !options.quiet {
        outln!("Vigil started. Press Ctrl+C to stop.");
    }

    loop {
        if sig::check() {
            if tty && !options.quiet {
                outln!("\rStopped.              ");
            }
            break;
        }

        if let Err(error) = guard.check() {
            report_error(&format!("error: sleep inhibition was lost: {error}"));
            return ExitCode::from(1);
        }

        if let Some(dur) = options.timeout {
            if start.elapsed() >= dur {
                if tty && !options.quiet {
                    outln!("\rTimeout reached.        ");
                }
                if options.shutdown {
                    let shutdown_ok = trigger_shutdown();
                    drop(guard);
                    if !shutdown_ok {
                        return ExitCode::from(1);
                    }
                    break;
                }
                drop(guard);
                break;
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let args = vec!["vigil".into(), "--timeout".into(), "--shutdown".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_parse_does_not_hide_unknown_argument_behind_help() {
        let args = vec!["vigil".into(), "--unknown".into(), "--help".into()];
        assert!(parse_args(&args).is_err());
    }
}
