use std::io;
use std::process::{Child, Command, Stdio};

pub struct InhibitGuard(Option<Child>);

impl InhibitGuard {
    fn new() -> io::Result<Self> {
        let dry = Command::new("systemd-inhibit")
            .args(["--what=idle:sleep", "--who=vigil", "--why=check", "true"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(ref s) = dry {
            if !s.success() {
                eprintln!("warning: systemd-inhibit dry-run failed");
            }
        }

        let child = Command::new("systemd-inhibit")
            .args([
                "--what=idle:sleep",
                "--who=vigil",
                "--why=Inhibiting sleep",
                "sleep",
                "365d",
            ])
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
    InhibitGuard::new().expect("failed to start systemd-inhibit")
}
