const ES_CONTINUOUS: u32 = 0x80000000;
const ES_SYSTEM_REQUIRED: u32 = 0x00000001;
const ES_DISPLAY_REQUIRED: u32 = 0x00000002;

#[link(name = "kernel32")]
extern "system" {
    fn SetThreadExecutionState(flags: u32) -> u32;
}

pub struct InhibitGuard;

impl InhibitGuard {
    fn new() -> Self {
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        }
        InhibitGuard
    }
}

impl Drop for InhibitGuard {
    fn drop(&mut self) {
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

pub fn start_inhibit() -> InhibitGuard {
    InhibitGuard::new()
}
