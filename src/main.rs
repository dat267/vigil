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

#[cfg(windows)]
mod sig {
    use std::sync::atomic::{AtomicBool, Ordering};
    static HIT: AtomicBool = AtomicBool::new(false);
    pub fn check() -> bool {
        HIT.swap(false, Ordering::Relaxed)
    }
    pub fn install() -> Result<(), String> {
        unsafe extern "system" fn handler(_: u32) -> i32 {
            HIT.store(true, Ordering::Relaxed);
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

#[cfg(windows)]
fn init_console() -> Result<(), String> {
    unsafe {
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
        const ATTACH_PARENT_PROCESS: u32 = 0xFFFFFFFF;
        const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5u32;
        const STD_ERROR_HANDLE: u32 = 0xFFFFFFF4u32;
        const NULL_HANDLE: isize = 0;
        const GENERIC_READ: u32 = 0x80000000;
        const GENERIC_WRITE: u32 = 0x40000000;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        const OPEN_EXISTING: u32 = 3;
        const INVALID_HANDLE_VALUE: isize = -1;

        let output_was_valid = {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            handle != NULL_HANDLE && handle != INVALID_HANDLE_VALUE
        };
        let error_was_valid = {
            let handle = GetStdHandle(STD_ERROR_HANDLE);
            handle != NULL_HANDLE && handle != INVALID_HANDLE_VALUE
        };

        if output_was_valid && error_was_valid {
            return Ok(());
        }

        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            // Allocation may fail when a console is already attached but its
            // inherited standard handles are invalid. CONOUT$ below repairs that case.
            let _ = AllocConsole();
        }

        let con_out: isize = CreateFileW(
            [
                0x0043u16, 0x004F, 0x004E, 0x004F, 0x0055, 0x0054, 0x0024, 0x0000,
            ]
            .as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            0,
        );
        if con_out == NULL_HANDLE || con_out == INVALID_HANDLE_VALUE {
            return Err(format!(
                "could not open console output: {}",
                std::io::Error::last_os_error()
            ));
        }
        if !output_was_valid && SetStdHandle(STD_OUTPUT_HANDLE, con_out) == 0 {
            return Err("could not initialize console stdout".into());
        }
        if !error_was_valid && SetStdHandle(STD_ERROR_HANDLE, con_out) == 0 {
            return Err("could not initialize console stderr".into());
        }
    }
    Ok(())
}

fn help() {
    println!(
        "\
Usage: vigil [-t <duration>] [-s] [-q]

Keep your system awake.

Flags:
  -t, --timeout <duration>  Stay awake for this long (e.g. 2h, 45m, 30s); \
                             --timeout=<duration> and -t=<duration> are \
                             also accepted. Infinite by default.
  -s, --shutdown            Shutdown when the timeout expires (requires \
                              --timeout).
  -q, --quiet               Suppress all output; hide console window on \
                               Windows.
  -V, --version             Print the version."
    );
}

fn version() {
    println!("vigil {}", env!("CARGO_PKG_VERSION"));
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    if s.is_empty() {
        return Err("empty duration".into());
    }
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
            match c {
                'h' => {
                    total = total
                        .checked_add(n.checked_mul(3600).ok_or("overflow")?)
                        .ok_or("overflow")?
                }
                'm' => {
                    total = total
                        .checked_add(n.checked_mul(60).ok_or("overflow")?)
                        .ok_or("overflow")?
                }
                's' => total = total.checked_add(n).ok_or("overflow")?,
                _ => return Err(format!("unknown unit '{c}' in duration")),
            }
            n = 0;
            has_digits = false;
        }
    }
    if has_digits {
        return Err("trailing digits in duration".into());
    }
    Ok(Duration::from_secs(total))
}

struct Options {
    quiet: bool,
    show_help: bool,
    show_version: bool,
    timeout: Option<Duration>,
    shutdown: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        quiet: false,
        show_help: false,
        show_version: false,
        timeout: None,
        shutdown: false,
    };
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "start" {
            i += 1;
            continue;
        }
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
                options.timeout = Some(
                    parse_duration(&args[i]).map_err(|e| format!("error: invalid timeout: {e}"))?,
                );
            }
            _ if arg.starts_with("--timeout=") => {
                let value = &arg["--timeout=".len()..];
                options.timeout = Some(
                    parse_duration(value).map_err(|e| format!("error: invalid timeout: {e}"))?,
                );
            }
            _ if arg.starts_with("-t=") => {
                let value = &arg[3..];
                options.timeout = Some(
                    parse_duration(value).map_err(|e| format!("error: invalid timeout: {e}"))?,
                );
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

/// Runs the 60-second shutdown countdown, then issues the shutdown command.
/// Returns `true` if the flow completed or was cancelled by the user,
/// `false` if the shutdown command failed to execute.
fn trigger_shutdown() -> bool {
    if !QUIET.load(Ordering::Relaxed) {
        println!("\nShutting down in 60 seconds. Press Ctrl+C to cancel.");
    }
    let tty = std::io::stdout().is_terminal();
    for i in (1..=60).rev() {
        #[cfg(windows)]
        if sig::check() {
            if !QUIET.load(Ordering::Relaxed) {
                println!("\nShutdown cancelled.");
            }
            return true;
        }
        if tty && !QUIET.load(Ordering::Relaxed) {
            print!("\rShutting down in {i}s... ");
            std::io::stdout().flush().ok();
        } else if !tty && !QUIET.load(Ordering::Relaxed) && (i == 60 || i <= 5 || i % 10 == 0) {
            println!("Shutting down in {i}s...");
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    #[cfg(windows)]
    if sig::check() {
        if !QUIET.load(Ordering::Relaxed) {
            println!("\nShutdown cancelled.");
        }
        return true;
    }
    if !QUIET.load(Ordering::Relaxed) {
        println!("\nShutting down...");
    }
    #[cfg(target_os = "windows")]
    let mut command = Command::new("shutdown");
    #[cfg(target_os = "windows")]
    command.args(["/s", "/t", "0"]);
    #[cfg(not(target_os = "windows"))]
    let mut command = Command::new("shutdown");
    #[cfg(not(target_os = "windows"))]
    command.args(["-h", "now"]);
    if QUIET.load(Ordering::Relaxed) {
        command.stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x08000000);
    }
    let r = command.status();
    match r {
        Ok(s) if s.success() => true,
        _ => {
            if !QUIET.load(Ordering::Relaxed) {
                eprintln!("warning: shutdown command failed; elevated privileges may be required");
            }
            false
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let quiet_hint = args
        .iter()
        .skip(1)
        .any(|arg| arg == "-q" || arg == "--quiet");
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
            let _ = writeln!(std::io::stderr(), "error: {error}");
            return ExitCode::from(1);
        }
    }

    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(error) => {
            if !quiet_hint {
                let _ = writeln!(std::io::stderr(), "{error}");
            }
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

    #[cfg(windows)]
    if let Err(error) = sig::install() {
        if !options.quiet {
            eprintln!("error: {error}");
        }
        return ExitCode::from(1);
    }
    let mut _guard = match platform::start_inhibit() {
        Ok(guard) => guard,
        Err(error) => {
            if !options.quiet {
                eprintln!("error: {error}");
            }
            return ExitCode::from(1);
        }
    };

    let start = Instant::now();
    let tty = std::io::stdout().is_terminal();

    if tty && !options.quiet {
        println!("Vigil started. Press Ctrl+C to stop.");
    }

    loop {
        #[cfg(windows)]
        if sig::check() {
            if tty && !options.quiet {
                println!("\rStopped.              ");
            }
            break;
        }

        if let Err(error) = _guard.check() {
            if !options.quiet {
                eprintln!("error: sleep inhibition was lost: {error}");
            }
            return ExitCode::from(1);
        }

        if let Some(dur) = options.timeout {
            if start.elapsed() >= dur {
                if tty && !options.quiet {
                    println!("\rTimeout reached.        ");
                }
                if options.shutdown {
                    let shutdown_ok = trigger_shutdown();
                    drop(_guard);
                    if !shutdown_ok {
                        return ExitCode::from(1);
                    }
                    break;
                }
                drop(_guard);
                break;
            }
        }

        std::thread::sleep(Duration::from_secs(1));
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
    fn test_parse_start_is_accepted_after_flags() {
        let args = vec!["vigil".into(), "--quiet".into(), "start".into()];
        assert!(parse_args(&args).unwrap().quiet);
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
