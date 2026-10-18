use super::SystemEvent;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::UI::WindowsAndMessaging::*;

static TX: OnceLock<Sender<SystemEvent>> = OnceLock::new();

const WTS_SESSION_LOCK: u32 = 0x7;
const WTS_SESSION_UNLOCK: u32 = 0x8;
const WTS_SESSION_LOGON: u32 = 0x5;
const WTS_SESSION_LOGOFF: u32 = 0x6;

fn emit(e: SystemEvent) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(e);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_WTSSESSION_CHANGE => {
            match wp.0 as u32 {
                WTS_SESSION_LOCK | WTS_SESSION_LOGOFF => emit(SystemEvent::Locked),
                WTS_SESSION_UNLOCK | WTS_SESSION_LOGON => emit(SystemEvent::Unlocked),
                _ => {}
            }
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            match wp.0 as u32 {
                PBT_APMSUSPEND => emit(SystemEvent::Suspend),
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => emit(SystemEvent::Resume),
                _ => {}
            }
            LRESULT(1)
        }
        WM_DESTROY => {
            let _ = WTSUnRegisterSessionNotification(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Spawns a thread owning a message-only window that receives session and power
/// notifications. The thread runs a message pump for the life of the process.
pub fn spawn(tx: Sender<SystemEvent>) {
    let _ = TX.set(tx);
    std::thread::Builder::new()
        .name("sundial-session".into())
        .spawn(|| unsafe {
            let class = w!("SundialSessionWatcher");
            let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null())
                .expect("GetModuleHandleW");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Sundial"),
                WINDOW_STYLE(0),
                0, 0, 0, 0,
                HWND_MESSAGE,
                None,
                hinstance,
                None,
            )
            .expect("CreateWindowExW");

            let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn session thread");
}
