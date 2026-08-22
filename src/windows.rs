use std::io;

const ES_CONTINUOUS: u32 = 0x80000000;
const ES_SYSTEM_REQUIRED: u32 = 0x00000001;
const ES_DISPLAY_REQUIRED: u32 = 0x00000002;

#[link(name = "kernel32")]
extern "system" {
    fn SetThreadExecutionState(flags: u32) -> u32;
}

pub struct InhibitGuard;

impl InhibitGuard {
    pub(crate) fn check(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn new() -> io::Result<Self> {
        unsafe {
            if SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
                == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(InhibitGuard)
    }
}

impl Drop for InhibitGuard {
    fn drop(&mut self) {
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

pub fn start_inhibit() -> io::Result<InhibitGuard> {
    InhibitGuard::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_inhibit_check() {
        let mut guard = InhibitGuard;
        assert!(guard.check().is_ok());
    }
}
