use std::time::Duration;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// Time since the last keyboard or mouse input, system-wide.
/// Returns ZERO if the call fails, which fails safe toward "present".
pub fn since_last_input() -> Duration {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if GetLastInputInfo(&mut info).as_bool() {
            let now = GetTickCount();
            Duration::from_millis(now.wrapping_sub(info.dwTime) as u64)
        } else {
            Duration::ZERO
        }
    }
}
