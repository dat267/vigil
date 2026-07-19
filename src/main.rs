#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::IsTerminal;
use std::process::{Command, ExitCode};
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
    pub fn install() {
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
        unsafe {
            SetConsoleCtrlHandler(Some(handler), 1);
        }
    }
}

#[cfg(windows)]
fn init_console() {
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
        }
        const ATTACH_PARENT_PROCESS: u32 = 0xFFFFFFFF;
        const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5u32;
        const STD_ERROR_HANDLE: u32 = 0xFFFFFFF4u32;
        const GENERIC_READ: u32 = 0x80000000;
        const GENERIC_WRITE: u32 = 0x40000000;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        const OPEN_EXISTING: u32 = 3;
        const INVALID_HANDLE_VALUE: isize = -1;

        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            AllocConsole();
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
        if con_out != INVALID_HANDLE_VALUE {
            SetStdHandle(STD_OUTPUT_HANDLE, con_out);
            SetStdHandle(STD_ERROR_HANDLE, con_out);
        }
    }
}

fn help() {
    println!(
        "\
Usage: vigil [-t <duration>] [-s] [-q]

Keep your system awake.

Flags:
  -t, --timeout <duration>  Stay awake for this long (e.g. 2h, 45m, 30s). \
                              Infinite by default.
  -s, --shutdown            Shutdown when the timeout expires (requires \
                              --timeout).
  -q, --quiet               Suppress all output; hide console window on \
                              Windows."
    );
}

fn print_elapsed(d: Duration) {
    if QUIET.load(Ordering::Relaxed) {
        return;
    }
    let s = d.as_secs();
    let h = s / 3600;
    let m = s % 3600 / 60;
    let sec = s % 60;
    use std::io::Write;
    if h > 0 {
        print!("\r{h}h {m}m {sec}s elapsed");
    } else if m > 0 {
        print!("\r{m}m {sec}s elapsed");
    } else {
        print!("\r{sec}s elapsed");
    }
    std::io::stdout().flush().ok();
}

fn print_elapsed_np(d: Duration) {
    if QUIET.load(Ordering::Relaxed) {
        return;
    }
    let s = d.as_secs();
    let h = s / 3600;
    let m = s % 3600 / 60;
    let sec = s % 60;
    if h > 0 {
        println!("{h}h {m}m {sec}s elapsed");
    } else if m > 0 {
        println!("{m}m {sec}s elapsed");
    } else {
        println!("{sec}s elapsed");
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    if s.is_empty() {
        return Err("empty duration".into());
    }
    let mut total = 0u64;
    let mut n = 0u64;
    for c in s.chars() {
        if c.is_ascii_digit() {
            n = n * 10 + (c as u8 - b'0') as u64;
        } else {
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
        }
    }
    if n != 0 {
        return Err("trailing digits in duration".into());
    }
    Ok(Duration::from_secs(total))
}

fn trigger_shutdown() {
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
            return;
        }
        if tty && !QUIET.load(Ordering::Relaxed) {
            use std::io::Write;
            print!("\rShutting down in {i}s... ");
            std::io::stdout().flush().ok();
        } else if !tty && !QUIET.load(Ordering::Relaxed) && (i == 60 || i <= 5 || i % 10 == 0) {
            println!("Shutting down in {i}s...");
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    if !QUIET.load(Ordering::Relaxed) {
        println!("\nShutting down...");
    }
    #[cfg(target_os = "windows")]
    let r = Command::new("shutdown").args(["/s", "/t", "0"]).status();
    #[cfg(not(target_os = "windows"))]
    let r = Command::new("shutdown").args(["-h", "now"]).status();
    match r {
        Ok(s) if s.success() => {}
        _ => {
            if !QUIET.load(Ordering::Relaxed) {
                eprintln!("warning: shutdown command failed");
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    let mut quiet = false;
    let mut show_help = false;
    for arg in &args[1..] {
        match arg.as_str() {
            "-q" | "--quiet" => quiet = true,
            "-h" | "--help" | "help" => show_help = true,
            _ => {}
        }
    }

    if quiet {
        QUIET.store(true, Ordering::Relaxed);
    }

    #[cfg(windows)]
    if !quiet {
        init_console();
    }

    if show_help {
        help();
        return ExitCode::SUCCESS;
    }

    let mut i = 1;
    if i < args.len() && args[i] == "start" {
        i += 1;
    }

    let mut timeout: Option<Duration> = None;
    let mut shutdown = false;

    while i < args.len() {
        match args[i].as_str() {
            "-t" | "--timeout" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --timeout requires a value");
                    return ExitCode::from(1);
                }
                match parse_duration(&args[i]) {
                    Ok(d) => timeout = Some(d),
                    Err(e) => {
                        eprintln!("error: invalid timeout: {e}");
                        return ExitCode::from(1);
                    }
                }
            }
            "-s" | "--shutdown" => shutdown = true,
            "-q" | "--quiet" => {}
            "-h" | "--help" => {
                help();
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("error: unknown argument '{}'", args[i]);
                return ExitCode::from(1);
            }
        }
        i += 1;
    }

    if shutdown && timeout.is_none() {
        eprintln!("error: --shutdown requires --timeout");
        return ExitCode::from(1);
    }

    #[cfg(windows)]
    sig::install();
    let _guard = platform::start_inhibit();

    let start = Instant::now();
    let tty = std::io::stdout().is_terminal();
    let mut last_report = Duration::ZERO;

    if tty && !quiet {
        println!("Vigil started. Press Ctrl+C to stop.");
    }

    loop {
        #[cfg(windows)]
        if sig::check() {
            if tty && !quiet {
                println!("\rStopped.              ");
            }
            break;
        }

        let elapsed = start.elapsed();

        if let Some(dur) = timeout {
            if elapsed >= dur {
                if tty && !quiet {
                    println!("\rTimeout reached.        ");
                }
                drop(_guard);
                if shutdown {
                    trigger_shutdown();
                }
                break;
            }
        }

        let delta = elapsed - last_report;
        let report_tty = tty && delta >= Duration::from_secs(1) && !quiet;
        let report_np = !tty && !quiet && {
            let near_end = timeout
                .map(|d| {
                    let rem = if d > elapsed {
                        d - elapsed
                    } else {
                        Duration::ZERO
                    };
                    rem <= Duration::from_secs(5)
                })
                .unwrap_or(false);
            let interval = if near_end {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(10)
            };
            delta >= interval
        };

        if report_tty {
            print_elapsed(elapsed);
            last_report = elapsed;
        } else if report_np {
            print_elapsed_np(elapsed);
            last_report = elapsed;
        }

        std::thread::sleep(Duration::from_millis(200));
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
}
