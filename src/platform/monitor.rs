use egui::{Pos2, Rect, Vec2};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTONULL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};

const MARGIN: f32 = 12.0;

/// Bottom-right corner of the *work area* (i.e. excluding the taskbar) of the
/// monitor containing `widget`, inset by a margin.
///
/// Using the work area rather than raw screen dimensions is what makes this
/// correct with the taskbar on any edge, on a secondary monitor, and at non-100%
/// DPI scaling. Works in physical pixels, which is what
/// `ViewportBuilder::with_position` scales by `pixels_per_point`, so the caller
/// must divide the result by it.
pub fn bottom_right_for(widget: Rect, size: Vec2) -> Pos2 {
    let centre = widget.center();
    let point = POINT {
        x: centre.x as i32,
        y: centre.y as i32,
    };

    unsafe {
        let hmon = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut info).as_bool() {
            let work = info.rcWork;
            return Pos2::new(
                work.right as f32 - size.x - MARGIN,
                work.bottom as f32 - size.y - MARGIN,
            );
        }
        eprintln!("sundial: GetMonitorInfoW failed; falling back to widget-relative placement");
    }

    Pos2::new(widget.right() - size.x, widget.top() - size.y - MARGIN)
}

fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut core::ffi::c_void)
}

/// A window's outer rect in physical pixels.
///
/// Window positions are kept in physical pixels throughout. egui reports and
/// applies positions in points, scaled by whichever monitor's DPI it last saw,
/// so a position saved on a 100% monitor and restored at 125% lands 25% further
/// out - far enough to put the widget off every screen.
pub fn window_rect(raw: isize) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd(raw), &mut r).ok()? };
    Some(Rect::from_min_max(
        Pos2::new(r.left as f32, r.top as f32),
        Pos2::new(r.right as f32, r.bottom as f32),
    ))
}

/// Moves a window's top-left corner to `pos`, in physical pixels.
pub fn move_window(raw: isize, pos: Pos2) {
    unsafe {
        let _ = SetWindowPos(
            hwnd(raw),
            None,
            pos.x as i32,
            pos.y as i32,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Whether any part of `rect` (physical pixels) is on a connected monitor. A
/// saved position can go stale when a monitor is unplugged or rearranged.
pub fn is_on_screen(rect: Rect) -> bool {
    let r = RECT {
        left: rect.left() as i32,
        top: rect.top() as i32,
        right: rect.right() as i32,
        bottom: rect.bottom() as i32,
    };
    unsafe { !MonitorFromRect(&r, MONITOR_DEFAULTTONULL).is_invalid() }
}

/// Where `rect` must move so it lies inside `area`. Where it can't fit, its
/// top-left edge wins, keeping the start of the widget visible.
fn clamp_into(rect: Rect, area: Rect) -> Pos2 {
    let x = rect.min.x.min(area.max.x - rect.width()).max(area.min.x);
    let y = rect.min.y.min(area.max.y - rect.height()).max(area.min.y);
    Pos2::new(x, y)
}

/// Pulls a window back inside the work area of the monitor it is mostly on.
/// Growing the widget near a screen edge would otherwise push part of it
/// off-screen.
pub fn keep_on_work_area(raw: isize) {
    let Some(rect) = window_rect(raw) else { return };
    let r = RECT {
        left: rect.left() as i32,
        top: rect.top() as i32,
        right: rect.right() as i32,
        bottom: rect.bottom() as i32,
    };
    let work = unsafe {
        let hmon = MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut info).as_bool() {
            return;
        }
        info.rcWork
    };
    let area = Rect::from_min_max(
        Pos2::new(work.left as f32, work.top as f32),
        Pos2::new(work.right as f32, work.bottom as f32),
    );
    let target = clamp_into(rect, area);
    if target != rect.min {
        move_window(raw, target);
    }
}

/// A top-level window by exact title. egui does not expose the native handles
/// of the extra viewports it creates, so this is how they are found.
pub fn find_window(title: &str) -> Option<isize> {
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let found = unsafe { FindWindowW(None, windows::core::PCWSTR(wide.as_ptr())).ok()? };
    Some(found.0 as isize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_max(Pos2::new(1920.0, -330.0), Pos2::new(3840.0, 710.0))
    }

    #[test]
    fn rect_already_inside_stays_put() {
        let r = Rect::from_min_size(Pos2::new(2000.0, 0.0), Vec2::new(210.0, 60.0));
        assert_eq!(clamp_into(r, area()), r.min);
    }

    /// Growing the widget at the right edge would push it off the monitor.
    #[test]
    fn rect_past_the_right_edge_is_pulled_back() {
        let r = Rect::from_min_size(Pos2::new(3655.0, -296.0), Vec2::new(210.0, 60.0));
        assert_eq!(clamp_into(r, area()), Pos2::new(3630.0, -296.0));
    }

    #[test]
    fn rect_past_the_bottom_and_left_is_pulled_back() {
        let r = Rect::from_min_size(Pos2::new(1900.0, 700.0), Vec2::new(210.0, 60.0));
        assert_eq!(clamp_into(r, area()), Pos2::new(1920.0, 650.0));
    }

    /// A rect larger than the area pins to its top-left rather than panicking.
    #[test]
    fn rect_larger_than_area_pins_to_top_left() {
        let r = Rect::from_min_size(Pos2::new(3000.0, 0.0), Vec2::new(5000.0, 60.0));
        assert_eq!(clamp_into(r, area()), Pos2::new(1920.0, 0.0));
    }
}
