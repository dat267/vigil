use std::io;
use std::process::{Child, Command, Stdio};

pub struct InhibitGuard(Option<Child>);

impl InhibitGuard {
    pub(crate) fn check(&mut self) -> Result<(), String> {
        let child = self
            .0
            .as_mut()
            .ok_or_else(|| "sleep inhibitor is not running".to_string())?;
        match child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(format!("caffeinate exited ({status})")),
            Err(error) => Err(format!("could not check caffeinate: {error}")),
        }
    }

    fn new() -> io::Result<Self> {
        let pid = std::process::id();
        let child = Command::new("caffeinate")
            .args(["-d", "-i", "-w", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(InhibitGuard(Some(child)))
    }
}

impl Drop for InhibitGuard {
    fn drop(&mut self) {
        if let Some(ref mut c) = self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub fn start_inhibit() -> io::Result<InhibitGuard> {
    InhibitGuard::new().map_err(|error| {
        io::Error::new(error.kind(), format!("failed to start caffeinate: {error}"))
    })
}
