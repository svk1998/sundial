use super::SystemEvent;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::time::Duration;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Power::{
    RegisterSuspendResumeNotification, UnregisterSuspendResumeNotification,
};
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::UI::WindowsAndMessaging::*;

static TX: OnceLock<Sender<SystemEvent>> = OnceLock::new();

const WTS_SESSION_LOCK: u32 = 0x7;
const WTS_SESSION_UNLOCK: u32 = 0x8;
const WTS_SESSION_LOGON: u32 = 0x5;
const WTS_SESSION_LOGOFF: u32 = 0x6;

/// How long `spawn` waits for the session thread to report its startup verdict.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

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
///
/// Returns `Ok(())` only once the thread has registered the window class, created
/// the window, and successfully registered for BOTH session-change and
/// suspend/resume notifications. Any failure is reported here rather than
/// leaving the caller with a thread pumping an empty queue forever.
pub fn spawn(tx: Sender<SystemEvent>) -> Result<(), String> {
    let _ = TX.set(tx);

    let (status_tx, status_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    let join = std::thread::Builder::new()
        .name("sundial-session".into())
        .spawn(move || unsafe {
            // Reports the failure to `spawn` and returns from the thread. If the
            // caller has already given up (timed out), the send fails silently,
            // so also print the diagnostic.
            macro_rules! fail {
                ($($arg:tt)*) => {{
                    let m = format!($($arg)*);
                    eprintln!("sundial: {}", m);
                    let _ = status_tx.send(Err(m));
                    return;
                }};
            }

            let class = w!("SundialSessionWatcher");
            let hinstance =
                match windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null()) {
                    Ok(h) => h,
                    Err(e) => fail!("GetModuleHandleW failed: {e}"),
                };

            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                // A second spawn() in the same process would hit
                // ERROR_CLASS_ALREADY_EXISTS; that is not a real failure.
                let err = GetLastError();
                if err != ERROR_CLASS_ALREADY_EXISTS {
                    fail!("RegisterClassW failed: {err:?}");
                }
            }

            let hwnd = match CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Sundial"),
                WINDOW_STYLE(0),
                0, 0, 0, 0,
                HWND_MESSAGE,
                None,
                hinstance,
                None,
            ) {
                Ok(h) => h,
                Err(e) => fail!("CreateWindowExW failed: {e}"),
            };

            // Lock/unlock. WTS posts directly to this HWND, so a message-only
            // window is fine. Without this registration nothing is ever
            // delivered and every locked hour counts as present.
            if let Err(e) = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) {
                let _ = DestroyWindow(hwnd);
                fail!("WTSRegisterSessionNotification failed: {e}");
            }

            // Suspend/resume. WM_POWERBROADCAST is a broadcast and is NOT
            // delivered to message-only windows; this opt-in registration is
            // what makes the power manager post directly to this HWND.
            // `power_notify` must stay registered as long as the window, so it
            // is bound here (not a temporary) and only unregistered once the
            // pump has exited. HPOWERNOTIFY is a plain Copy handle with no
            // Drop, so the release is explicit.
            let power_notify =
                match RegisterSuspendResumeNotification(HANDLE(hwnd.0), DEVICE_NOTIFY_WINDOW_HANDLE)
                {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = WTSUnRegisterSessionNotification(hwnd);
                        let _ = DestroyWindow(hwnd);
                        fail!("RegisterSuspendResumeNotification failed: {e}");
                    }
                };

            let _ = status_tx.send(Ok(()));

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = UnregisterSuspendResumeNotification(power_notify);
        });

    if let Err(e) = join {
        let m = format!("could not start session thread: {e}");
        eprintln!("sundial: {m}");
        return Err(m);
    }

    match status_rx.recv_timeout(STARTUP_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(m)) => Err(m),
        Err(e) => {
            let m = format!("session thread did not report startup within {STARTUP_TIMEOUT:?}: {e}");
            eprintln!("sundial: {m}");
            Err(m)
        }
    }
}
