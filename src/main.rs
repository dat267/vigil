#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{IsTerminal, Write};
use std::process::ExitCode;
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

// SAFETY: These are well-documented Windows API functions (kernel32). The
// function signatures are correct for the declared ABI.
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
        // SAFETY: GetStdHandle is a documented Windows API. The constants
        // STD_OUTPUT_HANDLE and STD_ERROR_HANDLE are correct. The function
        // returns a pseudo-handle that does not need closing.
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        handle != NULL_HANDLE && handle != INVALID_HANDLE_VALUE
    };
    let error_was_valid = {
        // SAFETY: Same as above, for the standard error handle.
        let handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        handle != NULL_HANDLE && handle != INVALID_HANDLE_VALUE
    };

    if output_was_valid && error_was_valid {
        return Ok(());
    }

    // SAFETY: AttachConsole is a documented Windows API. The
    // ATTACH_PARENT_PROCESS constant (0xFFFFFFFF) is the correct value to
    // attach to the parent process's console.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        // Allocation may fail when a console is already attached but its
        // inherited standard handles are invalid. CONOUT$ below repairs that case.
        // SAFETY: AllocConsole allocates a new console if none is attached.
        // Failure is expected when a console is already present, so the
        // return value is intentionally discarded.
        let _ = unsafe { AllocConsole() };
    }

    // SAFETY: CreateFileW is a documented Windows API. CONOUT_W is a valid
    // null-terminated UTF-16 string. The access and sharing flags are
    // correct for opening the console output device. The returned handle
    // does not need explicit closing (it is a console handle).
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
    // SAFETY: SetStdHandle is a documented Windows API. con_out is a valid
    // handle returned by CreateFileW. The standard handle constants are
    // correct. Only replaced if the original handle was invalid.
    if !output_was_valid && unsafe { SetStdHandle(STD_OUTPUT_HANDLE, con_out) } == 0 {
        return Err("could not initialize console stdout".into());
    }
    // SAFETY: Same as above, for the standard error handle.
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
Usage: vigil [-t <duration>] [-q] [-V] [-h]

Keep your system awake.

Flags:
  -t, --timeout <duration>  Stay awake for this long (e.g. 2h, 45m, 30s); \
                             --timeout=<duration> and -t=<duration> are \
                             also accepted. Infinite by default.
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
    Ok(options)
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
        let args = vec!["vigil".into(), "--timeout".into(), "--quiet".into()];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_parse_does_not_hide_unknown_argument_behind_help() {
        let args = vec!["vigil".into(), "--unknown".into(), "--help".into()];
        assert!(parse_args(&args).is_err());
    }
}
