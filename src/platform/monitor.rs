use egui::{Pos2, Rect, Vec2};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};

const MARGIN: f32 = 12.0;

/// Bottom-right corner of the *work area* (i.e. excluding the taskbar) of the
/// monitor containing `widget`, inset by a margin.
///
/// Using the work area rather than raw screen dimensions is what makes this
/// correct with the taskbar on any edge, on a secondary monitor, and at non-100%
/// DPI scaling. Works in physical pixels, which is what
/// `ViewportBuilder::with_position` expects - do not scale by `pixels_per_point`.
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
