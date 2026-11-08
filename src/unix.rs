use std::io;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

// kill() is POSIX (Linux and macOS); prctl()/getppid() are only used by the
// Linux parent-death linkage.
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn prctl(option: i32, ...) -> i32;
    fn getppid() -> i32;
}

const SIGKILL: i32 = 9;
#[cfg(target_os = "linux")]
const PR_SET_PDEATHSIG: i32 = 1;
#[cfg(target_os = "linux")]
const SIGTERM: i32 = 15;
/// `ESRCH` ("no such process") is 3 on Linux and macOS.
#[cfg(target_os = "linux")]
const ESRCH: i32 = 3;

/// Ask the kernel to signal the child if vigil dies. Linux implements this via
/// PR_SET_PDEATHSIG; elsewhere it is a no-op. The uniform signature keeps the
/// call site cfg-free and the platform delta in one auditable place.
#[cfg(target_os = "linux")]
fn apply_parent_death_signal(command: &mut Command) {
    // SAFETY: This closure runs in the child process after fork(). getppid()
    // and prctl() are async-signal-safe functions suitable for use after
    // fork(). The parent PID is captured before prctl and verified after to
    // detect the parent exiting before prctl took effect (an edge-case race).
    // Errors surface from spawn().
    unsafe {
        command.pre_exec(|| {
            let parent_pid = getppid();
            if prctl(PR_SET_PDEATHSIG, SIGTERM) != 0 {
                return Err(io::Error::last_os_error());
            }
            if getppid() != parent_pid {
                // pre_exec runs after fork, where allocation is not
                // async-signal-safe. Report the race with a raw OS error
                // instead of io::Error::other, which boxes a string.
                return Err(io::Error::from_raw_os_error(ESRCH));
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn apply_parent_death_signal(_command: &mut Command) {}

/// Bytes of inhibitor stderr kept for diagnostics. Anything older is dropped:
/// the last write is the one that reports the failure.
const STDERR_TAIL: usize = 4096;

/// Drains `pipe` to EOF, keeping only the last `STDERR_TAIL` bytes. Draining
/// rather than closing keeps a chatty child from blocking on write, and the
/// bound keeps its output from growing vigil's memory for the whole session.
fn drain_stderr(mut pipe: impl Read, sender: mpsc::Sender<String>) {
    let mut tail = Vec::with_capacity(STDERR_TAIL);
    let mut chunk = [0u8; 1024];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                tail.extend_from_slice(&chunk[..n]);
                if tail.len() > STDERR_TAIL {
                    tail.drain(..tail.len() - STDERR_TAIL);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    let _ = sender.send(String::from_utf8_lossy(&tail).into_owned());
}

/// Owns an inhibitor child process: detects exit, kills + reaps on Drop.
pub(crate) struct ProcessGuard {
    child: Option<Child>,
    stderr: Option<Receiver<String>>,
}

impl ProcessGuard {
    pub(crate) fn spawn(command: &mut Command) -> io::Result<Self> {
        // The child leads its own process group, so Drop can kill the whole
        // group and reap any grandchild the inhibitor spawned.
        command.process_group(0);
        apply_parent_death_signal(command);
        command.stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = command.spawn()?;
        // Drain stderr on its own thread so a full pipe can never block the
        // inhibitor, and so its own explanation is available when it fails.
        let stderr = child.stderr.take().map(|pipe| {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || drain_stderr(pipe, sender));
            receiver
        });
        Ok(ProcessGuard {
            child: Some(child),
            stderr,
        })
    }

    #[cfg(test)]
    pub(crate) fn id(&self) -> u32 {
        self.child.as_ref().expect("guard child").id()
    }

    pub(crate) fn check(&mut self) -> Result<(), String> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| "sleep inhibitor is not running".to_string())?;
        match child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => {
                self.child = None;
                match self.take_stderr() {
                    Some(detail) => Err(format!("inhibitor exited ({status}): {detail}")),
                    None => Err(format!("inhibitor exited ({status})")),
                }
            }
            Err(error) => Err(format!("could not check inhibitor: {error}")),
        }
    }

    /// The inhibitor's stderr, collapsed to one line, once it has exited.
    /// Bounded: a lingering grandchild could hold the pipe open, and reporting
    /// must never hang the main loop.
    fn take_stderr(&mut self) -> Option<String> {
        let receiver = self.stderr.take()?;
        let text = receiver.recv_timeout(Duration::from_millis(250)).ok()?;
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!text.is_empty()).then_some(text)
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            // The child is its own process-group leader (process_group(0)), so
            // killing the group also reaps any sleep grandchild that the
            // inhibitor spawned, leaving nothing behind.
            let pid = c.id();
            if pid > 0 && pid <= i32::MAX as u32 {
                let process_group = -(pid as i32);
                // SAFETY: kill() is a POSIX function. The pid is a negative
                // process group ID (negated child PID), which kills the entire
                // process group. The PID is validated to be in range for i32
                // and positive before negation.
                unsafe {
                    kill(process_group, SIGKILL);
                }
            }
            let _ = c.wait();
        }
    }
}

/// Maps a failure message to an actionable hint. Uniform across platforms so
/// the call site in main() stays cfg-free.
pub fn explain_failure(message: &str) -> Option<&'static str> {
    let message = message.to_ascii_lowercase();
    // Only a failure to launch the inhibitor can mean the tool is absent or
    // not executable. A runtime failure embeds the inhibitor's own stderr,
    // which may mention the same words for an unrelated reason.
    if message.starts_with("failed to start ") {
        let caffeinate = message.contains("caffeinate");
        if message.contains("not found") || message.contains("no such file") {
            // Pick the hint from the tool named in the error, not from the
            // build target: the message is the fact being explained, and this
            // keeps the mapping testable on every platform.
            return Some(if caffeinate {
                "caffeinate is built into macOS; check that it is on your PATH"
            } else {
                "systemd-inhibit comes from systemd; install it or use a systemd host"
            });
        }
        if message.contains("permission denied") || message.contains("access denied") {
            return Some(if caffeinate {
                "check that caffeinate is executable"
            } else {
                "check that systemd-inhibit is executable and readable"
            });
        }
    }
    if message.contains("access denied")
        || message.contains("permission denied")
        || message.contains("failed to inhibit")
    {
        return Some(
            "systemd-inhibit needs a local seat session; run from a desktop terminal, or with sudo",
        );
    }
    if message.contains("connection refused")
        || message.contains("failed to connect")
        || message.contains("no such device")
    {
        return Some("logind is not reachable; check that systemd-logind is running");
    }
    None
}

pub fn start_inhibit() -> io::Result<ProcessGuard> {
    #[cfg(target_os = "linux")]
    {
        let mut command = Command::new("systemd-inhibit");
        command.args([
            "--what=idle:sleep",
            "--who=vigil",
            "--why=Inhibiting sleep",
            "sleep",
            "2147483647",
        ]);
        ProcessGuard::spawn(&mut command).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    error.kind(),
                    "failed to start systemd-inhibit: not found on PATH",
                )
            } else {
                io::Error::new(
                    error.kind(),
                    format!("failed to start systemd-inhibit: {error}"),
                )
            }
        })
    }
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id();
        let mut command = Command::new("caffeinate");
        command.args(["-d", "-i", "-w", &pid.to_string()]);
        ProcessGuard::spawn(&mut command).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    error.kind(),
                    "failed to start caffeinate: not found on PATH",
                )
            } else {
                io::Error::new(error.kind(), format!("failed to start caffeinate: {error}"))
            }
        })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(io::Error::other("unsupported platform"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_reports_the_inhibitors_stderr() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("echo 'Access denied: no seat' >&2; exit 3");
        let mut guard = ProcessGuard::spawn(&mut command).unwrap();
        let mut message = String::new();
        for _ in 0..500 {
            match guard.check() {
                Ok(()) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => {
                    message = error;
                    break;
                }
            }
        }
        assert!(
            message.contains("Access denied: no seat"),
            "check() must surface the inhibitor's own error, got: {message:?}"
        );
    }

    #[test]
    fn check_caps_the_captured_stderr() {
        // 64 KiB of diagnostics: a real inhibitor writes a line or two, but a
        // chatty one must not grow vigil's memory for the whole session.
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("head -c 65536 /dev/zero | tr '\\0' x >&2; echo TAIL >&2");
        let mut guard = ProcessGuard::spawn(&mut command).unwrap();
        let mut message = String::new();
        for _ in 0..500 {
            match guard.check() {
                Ok(()) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => {
                    message = error;
                    break;
                }
            }
        }
        assert!(!message.is_empty(), "the exit must be reported");
        assert!(
            message.len() <= 8192,
            "captured stderr must be bounded, got {} bytes",
            message.len()
        );
        assert!(
            message.contains("TAIL"),
            "the end of the error message must be kept"
        );
    }

    #[test]
    fn explain_failure_does_not_blame_a_missing_tool_for_runtime_stderr() {
        let message =
            "sleep inhibition failed: inhibitor exited (exit status: 1): No such file or directory";
        assert_eq!(
            explain_failure(message),
            None,
            "the inhibitor launched, so its stderr must not trigger the missing-binary hint"
        );
    }

    #[test]
    fn explain_failure_points_at_executable_for_a_non_executable_inhibitor() {
        let hint =
            explain_failure("failed to start systemd-inhibit: Permission denied (os error 13)")
                .expect("a spawn permission error must have a hint");
        assert!(hint.contains("executable"), "got: {hint}");
    }

    #[test]
    fn explain_failure_points_at_executable_for_a_non_executable_caffeinate() {
        let hint = explain_failure("failed to start caffeinate: Permission denied (os error 13)")
            .expect("a spawn permission error must have a hint");
        assert!(hint.contains("executable"), "got: {hint}");
    }

    #[test]
    fn explain_failure_points_at_caffeinate_for_a_missing_caffeinate() {
        let hint = explain_failure("failed to start caffeinate: not found on PATH")
            .expect("missing binary must have a hint");
        assert!(hint.contains("caffeinate"), "got: {hint}");
    }

    #[test]
    fn explain_failure_points_at_the_seat_for_access_denied() {
        let hint =
            explain_failure("inhibitor exited (exit status: 1): Failed to inhibit: Access denied")
                .expect("access denied must have a hint");
        assert!(hint.contains("seat"), "got: {hint}");
    }

    #[test]
    fn explain_failure_points_at_systemd_for_a_missing_binary() {
        let hint = explain_failure(
            "failed to start systemd-inhibit: No such file or directory (os error 2)",
        )
        .expect("missing binary must have a hint");
        assert!(hint.contains("systemd"), "got: {hint}");
    }

    #[test]
    fn explain_failure_points_at_logind_when_the_bus_is_unreachable() {
        let hint = explain_failure(
            "Failed to connect to system scope bus via local transport: Connection refused",
        )
        .expect("unreachable bus must have a hint");
        assert!(hint.contains("logind"), "got: {hint}");
    }

    #[test]
    fn explain_failure_has_no_hint_for_unrecognized_errors() {
        assert_eq!(explain_failure("inhibitor exited (exit status: 1)"), None);
    }

    #[test]
    fn detects_an_exited_inhibitor() {
        let mut command = Command::new("true");
        let mut guard = ProcessGuard::spawn(&mut command).unwrap();
        // `true` exits immediately; the exit may already be visible on the
        // first check, so poll (bounded) until it is reported.
        for _ in 0..500 {
            match guard.check() {
                Ok(()) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => {
                    assert!(guard.check().is_err(), "must stay reported");
                    return;
                }
            }
        }
        panic!("exited inhibitor was never reported");
    }

    #[test]
    fn drop_kills_a_running_inhibitor() {
        let mut command = Command::new("sleep");
        command.arg("30");
        let guard = ProcessGuard::spawn(&mut command).unwrap();
        let pid = guard.id() as i32;
        drop(guard);
        // SAFETY: kill(pid, 0) probes for existence; no signal is sent.
        let alive = unsafe { kill(pid, 0) } == 0;
        assert!(!alive, "dropped guard must have killed the inhibitor");
    }

    #[test]
    fn group_kill_reaps_the_inhibitors_grandchild() {
        // The Linux production config: the inhibitor is its own process-group
        // leader, so Drop must kill the whole group — including any sleep
        // grandchild the inhibitor spawned — not just the direct child.
        let pid_file =
            std::env::temp_dir().join(format!("vigil-grandchild-{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pid_file);
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let mut command = Command::new("sh");
        command.arg("-c").arg(&script);
        let guard = ProcessGuard::spawn(&mut command).unwrap();

        // Wait for the shell to report the grandchild PID.
        let mut grandchild: Option<i32> = None;
        for _ in 0..200 {
            if let Ok(text) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    grandchild = Some(pid);
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let grandchild = grandchild.expect("shell never reported the grandchild PID");

        drop(guard);

        // SIGKILL delivery and init's reap of the reparented grandchild are
        // asynchronous; poll for the reap instead of probing once.
        let mut reaped = false;
        for _ in 0..200 {
            // SAFETY: kill(pid, 0) probes for existence; no signal is sent.
            if unsafe { kill(grandchild, 0) } != 0 {
                reaped = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(reaped, "dropped guard must have reaped the grandchild too");
        let _ = std::fs::remove_file(&pid_file);
    }
}
