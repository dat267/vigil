use std::io::IsTerminal;
use std::process::{Command, ExitCode};
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

const VERSION: &str = {
    if let Some(v) = option_env!("VIGIL_VERSION") {
        v
    } else {
        env!("CARGO_PKG_VERSION")
    }
};

#[cfg(unix)]
mod sig {
    use std::sync::atomic::{AtomicBool, Ordering};
    static HIT: AtomicBool = AtomicBool::new(false);
    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    extern "C" fn handler(_: i32) {
        HIT.store(true, Ordering::Relaxed);
    }
    pub fn install() {
        extern "C" {
            fn signal(sig: i32, cb: extern "C" fn(i32)) -> usize;
        }
        unsafe {
            signal(SIGINT, handler);
            signal(SIGTERM, handler);
        }
    }
    pub fn check() -> bool {
        HIT.swap(false, Ordering::Relaxed)
    }
}

#[cfg(windows)]
mod sig {
    use std::sync::atomic::{AtomicBool, Ordering};
    static HIT: AtomicBool = AtomicBool::new(false);
    unsafe extern "system" fn handler(_: u32) -> i32 {
        HIT.store(true, Ordering::Relaxed);
        1
    }
    pub fn install() {
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
    pub fn check() -> bool {
        HIT.swap(false, Ordering::Relaxed)
    }
}

fn help() {
    println!(
        "\
Usage: vigil [start] [-t <duration>] [-s]
       vigil version
       vigil --help

Start a vigil (stay awake) session.

Flags:
  -t, --timeout <duration>  Stay awake for this long (e.g. 2h, 45m, 30s). \
                              Infinite by default.
  -s, --shutdown            Shutdown when the timeout expires (requires \
                              --timeout)."
    );
}

fn print_elapsed(d: Duration) {
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
    println!("\nShutting down in 60 seconds. Press Ctrl+C to cancel.");
    let tty = std::io::stdout().is_terminal();
    for i in (1..=60).rev() {
        if sig::check() {
            println!("\nShutdown cancelled.");
            return;
        }
        if tty {
            use std::io::Write;
            print!("\rShutting down in {i}s... ");
            std::io::stdout().flush().ok();
        } else if i == 60 || i <= 5 || i % 10 == 0 {
            println!("Shutting down in {i}s...");
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    println!("\nShutting down...");
    #[cfg(target_os = "windows")]
    let r = Command::new("shutdown").args(["/s", "/t", "0"]).status();
    #[cfg(not(target_os = "windows"))]
    let r = Command::new("shutdown").args(["-h", "now"]).status();
    match r {
        Ok(s) if s.success() => {}
        _ => eprintln!("warning: shutdown command failed"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;

    if i >= args.len() || args[i] == "-h" || args[i] == "--help" || args[i] == "help" {
        help();
        return ExitCode::SUCCESS;
    }

    if args[i] == "version" {
        println!("vigil {VERSION}");
        return ExitCode::SUCCESS;
    }

    let mut timeout: Option<Duration> = None;
    let mut shutdown = false;

    if i < args.len() && args[i] == "start" {
        i += 1;
    }

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

    sig::install();
    let _guard = platform::start_inhibit();

    let start = Instant::now();
    let tty = std::io::stdout().is_terminal();
    let mut last_report = Duration::ZERO;

    if tty {
        println!("Vigil started. Press Ctrl+C to stop.");
    }

    loop {
        if sig::check() {
            if tty {
                println!("\rStopped.              ");
            }
            break;
        }

        let elapsed = start.elapsed();

        if let Some(dur) = timeout {
            if elapsed >= dur {
                if tty {
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
        let report_tty = tty && delta >= Duration::from_secs(1);
        let report_np = !tty && {
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
