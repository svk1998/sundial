use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject};

/// Named event another process sets to ask Sundial to save and exit. The
/// installer uses it before replacing or removing the exe: a WM_CLOSE reaches
/// only visible windows, so with the widget hidden (or just the analytics panel
/// up) it never asks the app to quit, and the fallback force-kill would drop
/// unsaved time. Per-session, like the single-instance mutex.
const EVENT_NAME: windows::core::PCWSTR = w!("Local\\Sundial.Quit");

pub struct QuitSignal(HANDLE);

// The handle is a kernel object; signalling and waiting on it is thread-safe.
unsafe impl Send for QuitSignal {}

impl Drop for QuitSignal {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

impl QuitSignal {
    /// `None` if the event can't be created; the app then simply can't be
    /// asked to quit this way, and the installer falls back to force-closing.
    pub fn create() -> Option<QuitSignal> {
        // Manual reset, so a signal set while the engine is mid-tick is still
        // seen on the next check. If the event already exists (a setter still
        // holds it open), it may be left signalled from the previous run; clear
        // it so this run doesn't quit at once.
        unsafe {
            let handle = CreateEventW(None, true, false, EVENT_NAME).ok()?;
            let _ = ResetEvent(handle);
            Some(QuitSignal(handle))
        }
    }

    pub fn is_set(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}
