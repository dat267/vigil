use std::io;
use std::process::{Child, Command, Stdio};

pub struct InhibitGuard(Option<Child>);

impl InhibitGuard {
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

pub fn start_inhibit() -> InhibitGuard {
    InhibitGuard::new().expect("failed to start caffeinate")
}
