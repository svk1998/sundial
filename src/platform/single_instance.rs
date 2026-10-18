use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// Holds the named mutex for the life of the process. Two running copies would
/// double-count, so a second instance must exit.
pub struct Guard(HANDLE);

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn acquire() -> Option<Guard> {
    unsafe {
        let handle = CreateMutexW(None, true, w!("Global\\Sundial.SingleInstance")).ok()?;
        if windows::Win32::Foundation::GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            return None;
        }
        Some(Guard(handle))
    }
}
