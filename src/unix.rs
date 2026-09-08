use std::io;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, ...) -> i32;
    fn getppid() -> i32;
}

#[cfg(target_os = "linux")]
const PR_SET_PDEATHSIG: i32 = 1;
#[cfg(target_os = "linux")]
const SIGKILL: i32 = 9;
#[cfg(target_os = "linux")]
const SIGTERM: i32 = 15;

/// Knobs for the child-process lifecycle; the only platform delta.
#[derive(Default)]
pub(crate) struct Options {
    /// Make the child its own process-group leader so Drop can kill the
    /// whole group (reaping grandchildren).
    pub(crate) process_group: bool,
    /// Linux only: ask the kernel to signal the child if vigil dies.
    pub(crate) pdeathsig: bool,
}

/// Owns an inhibitor child process: detects exit, kills + reaps on Drop.
pub(crate) struct ProcessGuard {
    child: Option<Child>,
    options: Options,
}

impl ProcessGuard {
    pub(crate) fn spawn(command: &mut Command, options: Options) -> io::Result<Self> {
        if options.process_group {
            command.process_group(0);
        }
        #[cfg(target_os = "linux")]
        if options.pdeathsig {
            // SAFETY: This closure runs in the child process after fork().
            // getppid() and prctl() are async-signal-safe POSIX/Linux functions
            // suitable for use after fork(). The parent PID is captured before
            // prctl and verified after to detect if the parent exited before
            // prctl took effect (an edge-case race).
            unsafe {
                command.pre_exec(|| {
                    let parent_pid = getppid();
                    if prctl(PR_SET_PDEATHSIG, SIGTERM) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if getppid() != parent_pid {
                        return Err(io::Error::other("parent process exited"));
                    }
                    Ok(())
                });
            }
        }
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(ProcessGuard {
            child: Some(child),
            options,
        })
    }

    #[cfg(all(test, target_os = "linux"))]
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
                Err(format!("inhibitor exited ({status})"))
            }
            Err(error) => Err(format!("could not check inhibitor: {error}")),
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            if self.options.process_group {
                // The child is its own process-group leader (process_group(0)),
                // so killing the group also reaps any sleep grandchild that the
                // inhibitor spawned, leaving nothing behind.
                let pid = c.id();
                if pid > 0 && pid <= i32::MAX as u32 {
                    let process_group = -(pid as i32);
                    // SAFETY: kill() is a POSIX function. The pid is a negative
                    // process group ID (negated child PID), which kills the
                    // entire process group. The PID is validated to be in
                    // range for i32 and positive before negation.
                    unsafe {
                        kill(process_group, SIGKILL);
                    }
                }
            } else {
                let _ = c.kill();
            }
            let _ = c.wait();
        }
    }
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
        ProcessGuard::spawn(
            &mut command,
            Options {
                process_group: true,
                pdeathsig: true,
            },
        )
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to start systemd-inhibit: {error}"),
            )
        })
    }
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id();
        let mut command = Command::new("caffeinate");
        command.args(["-d", "-i", "-w", &pid.to_string()]);
        ProcessGuard::spawn(&mut command, Options::default()).map_err(|error| {
            io::Error::new(error.kind(), format!("failed to start caffeinate: {error}"))
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
    fn detects_an_exited_inhibitor() {
        let mut command = Command::new("true");
        let mut guard = ProcessGuard::spawn(&mut command, Options::default()).unwrap();
        assert!(guard.check().is_ok());
        // `true` exits immediately; wait for the exit to become visible.
        loop {
            match guard.check() {
                Ok(()) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        assert!(guard.check().is_err(), "exited inhibitor must be reported");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn drop_kills_a_running_inhibitor() {
        let mut command = Command::new("sleep");
        command.arg("30");
        let guard = ProcessGuard::spawn(&mut command, Options::default()).unwrap();
        let pid = guard.id() as i32;
        drop(guard);
        // SAFETY: kill(pid, 0) probes for existence; no signal is sent.
        let alive = unsafe { kill(pid, 0) } == 0;
        assert!(!alive, "dropped guard must have killed the inhibitor");
    }
}
