use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// Per-session, not machine-wide: two copies *in the same session* would
/// double-count. A second user (RDP, fast user switching) has their own
/// `%APPDATA%` and their own presence to track, so `Global\` would wrongly
/// block them — and would also drag in `SeCreateGlobalPrivilege` and
/// default-DACL access-denied problems.
const MUTEX_NAME: windows::core::PCWSTR = w!("Local\\Sundial.SingleInstance");

/// Holds the named mutex for the life of the process. Two running copies would
/// double-count, so a second instance must exit.
///
/// The handle is optional: when the mutex could not be created for a reason
/// other than "already running", the app still starts (fail-open) and the guard
/// holds `None`.
pub struct Guard(Option<HANDLE>);

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

/// `Some(guard)` means this process may run. `None` means another instance in
/// this session already holds the mutex and this one should exit.
///
/// Any other failure returns a guard anyway: failing closed would mean the app
/// silently never starts, which is strictly worse than the edge-case risk of a
/// second copy.
pub fn acquire() -> Option<Guard> {
    unsafe {
        match CreateMutexW(None, true, MUTEX_NAME) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = CloseHandle(handle);
                    return None;
                }
                Some(Guard(Some(handle)))
            }
            Err(e) if e.code() == ERROR_ALREADY_EXISTS.to_hresult() => None,
            Err(e) => {
                eprintln!(
                    "sundial: could not create the single-instance mutex ({e}); \
                     starting anyway without single-instance protection"
                );
                Some(Guard(None))
            }
        }
    }
}
