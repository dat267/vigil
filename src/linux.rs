use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

const PR_SET_PDEATHSIG: i32 = 1;
const SIGTERM: i32 = 15;

extern "C" {
    fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
}

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

        let mut cmd = Command::new("systemd-inhibit");
        cmd.args([
            "--what=idle:sleep",
            "--who=vigil",
            "--why=Inhibiting sleep",
            "sleep",
            "365d",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null());

        unsafe {
            cmd.pre_exec(|| {
                prctl(PR_SET_PDEATHSIG, SIGTERM as usize, 0, 0, 0);
                Ok(())
            });
        }

        let child = cmd.spawn()?;
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
