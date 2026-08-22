use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, ...) -> i32;
    fn getppid() -> i32;
}

const PR_SET_PDEATHSIG: i32 = 1;
const SIGKILL: i32 = 9;
const SIGTERM: i32 = 15;

pub struct InhibitGuard(Option<Child>);

impl InhibitGuard {
    pub(crate) fn check(&mut self) -> Result<(), String> {
        let child = self
            .0
            .as_mut()
            .ok_or_else(|| "sleep inhibitor is not running".to_string())?;
        match child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(format!("systemd-inhibit exited ({status})")),
            Err(error) => Err(format!("could not check systemd-inhibit: {error}")),
        }
    }

    fn new() -> io::Result<Self> {
        let mut command = Command::new("systemd-inhibit");
        command.args([
            "--what=idle:sleep",
            "--who=vigil",
            "--why=Inhibiting sleep",
            "sleep",
            "2147483647",
        ]);
        command.process_group(0);
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
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(InhibitGuard(Some(child)))
    }
}

impl Drop for InhibitGuard {
    fn drop(&mut self) {
        if let Some(ref mut c) = self.0 {
            // The child is its own process-group leader (process_group(0)), so
            // killing the group also reaps the sleep grandchild that
            // systemd-inhibit spawns, leaving nothing behind.
            let process_group = -(c.id() as i32);
            unsafe {
                kill(process_group, SIGKILL);
            }
            let _ = c.wait();
        }
    }
}

pub fn start_inhibit() -> io::Result<InhibitGuard> {
    InhibitGuard::new().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to start systemd-inhibit: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_an_exited_inhibitor() {
        let child = Command::new("true").spawn().unwrap();
        let mut guard = InhibitGuard(Some(child));
        guard.0.as_mut().unwrap().wait().unwrap();
        assert!(guard.check().is_err());
    }
}
