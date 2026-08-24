//! Per-monitor DPI and work-area helpers (Windows).

use crate::{BALL_CENTER_Y, BALL_R, H_DETAIL, W};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub dpi: u32,
    pub primary: bool,
}

impl WorkArea {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    fn center(&self) -> (i32, i32) {
        (
            self.left + (self.right - self.left) / 2,
            self.top + (self.bottom - self.top) / 2,
        )
    }
}

/// True once Explorer's notification area exists (needed after logon).
pub fn shell_tray_ready() -> bool {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn FindWindowW(class: *const u16, name: *const u16) -> *mut std::ffi::c_void;
        }
        static CLASS: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
        let class = CLASS.get_or_init(|| "Shell_TrayWnd\0".encode_utf16().collect());
        !unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) }.is_null()
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

/// Return false if another WeatherBall instance is already running.
pub fn take_instance_lock() -> bool {
    #[cfg(target_os = "windows")]
    {
        const ERROR_ALREADY_EXISTS: u32 = 183;
        extern "system" {
            fn CreateMutexW(
                sa: *mut std::ffi::c_void,
                initial_owner: i32,
                name: *const u16,
            ) -> *mut std::ffi::c_void;
            fn CloseHandle(h: *mut std::ffi::c_void) -> i32;
            fn GetLastError() -> u32;
        }
        let name: Vec<u16> = "Local\\WeatherBallNativeSingleton\0".encode_utf16().collect();
        let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr()) };
        if handle.is_null() {
            return true;
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return false;
        }
        // Keep the mutex handle for the process lifetime (not CloseHandle).
        static INSTANCE: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
            std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
        INSTANCE.store(handle, std::sync::atomic::Ordering::Relaxed);
        true
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

/// Per-monitor V2 so the orb stays sharp when dragged between mixed-DPI screens.
pub fn enable_per_monitor_dpi() {
    #[cfg(target_os = "windows")]
    unsafe {
        const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn dpi_at_point(x: i32, y: i32) -> u32 {
    #[cfg(target_os = "windows")]
    {
        dpi_for_monitor(monitor_from_point(x, y)).max(96)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (x, y);
        96
    }
}

pub fn dpi_for_window(hwnd: isize) -> u32 {
    #[cfg(target_os = "windows")]
    {
        if hwnd == 0 {
            return 96;
        }
        extern "system" {
            fn GetDpiForWindow(hwnd: *mut std::ffi::c_void) -> u32;
        }
        let dpi = unsafe { GetDpiForWindow(hwnd as *mut std::ffi::c_void) };
        if dpi >= 96 {
            dpi
        } else if let Some((x, y, _, _)) = window_rect(hwnd) {
            dpi_at_point(x, y)
        } else {
            96
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        96
    }
}

pub fn scale_for_window(hwnd: isize) -> f32 {
    dpi_for_window(hwnd) as f32 / 96.0
}

/// Physical top-left and DPI scale of the native window.
pub fn window_origin_and_scale(hwnd: isize) -> Option<(f32, f32, f32)> {
    let (x, y, _, _) = window_rect(hwnd)?;
    Some((x as f32, y as f32, scale_for_window(hwnd).max(0.5)))
}

pub fn window_rect(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        if hwnd == 0 {
            return None;
        }
        let mut r = WinRect::zero();
        extern "system" {
            fn GetWindowRect(hwnd: *mut std::ffi::c_void, rect: *mut WinRect) -> i32;
        }
        unsafe {
            if GetWindowRect(hwnd as *mut std::ffi::c_void, &mut r) != 0 {
                Some((r.left, r.top, (r.right - r.left).max(1), (r.bottom - r.top).max(1)))
            } else {
                None
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = hwnd;
        None
    }
}

/// Default: top-right of the monitor that currently has the cursor (else primary).
pub fn default_orb_pos() -> (i32, i32) {
    let areas = work_areas();
    let target = cursor_work_area(&areas)
        .or_else(|| areas.iter().find(|a| a.primary).copied())
        .or_else(|| areas.first().copied());
    let Some(m) = target else {
        return (80, 80);
    };
    let scale = m.dpi as f32 / 96.0;
    let win_w = (W * scale).round() as i32;
    let win_h = (H_DETAIL * scale).round() as i32;
    let margin = (40.0 * scale).round() as i32;
    let x = m.right - win_w - margin;
    let y = m.top + margin;
    clamp_sized(x, y, win_w, win_h, scale, false)
}

/// Clamp a saved physical origin using estimated window size at that point's DPI.
pub fn clamp_saved_pos(x: i32, y: i32) -> (i32, i32) {
    let scale = dpi_at_point(x, y) as f32 / 96.0;
    let win_w = (W * scale).round() as i32;
    let win_h = (H_DETAIL * scale).round() as i32;
    clamp_sized(x, y, win_w, win_h, scale, false)
}

pub fn clamp_hwnd_pos(hwnd: isize, x: i32, y: i32) -> (i32, i32) {
    clamp_hwnd_pos_inner(hwnd, x, y, false)
}

/// Re-query monitors (unplug / work-area change).
pub fn clamp_hwnd_pos_live(hwnd: isize, x: i32, y: i32) -> (i32, i32) {
    clamp_hwnd_pos_inner(hwnd, x, y, true)
}

fn clamp_hwnd_pos_inner(hwnd: isize, x: i32, y: i32, fresh: bool) -> (i32, i32) {
    let scale = scale_for_window(hwnd).max(0.5);
    let (win_w, win_h) = window_rect(hwnd)
        .map(|(_, _, w, h)| (w, h))
        .unwrap_or_else(|| {
            (
                (W * scale).round() as i32,
                (H_DETAIL * scale).round() as i32,
            )
        });
    clamp_sized(x, y, win_w, win_h, scale, fresh)
}

pub fn physical_to_logical_pos(x: i32, y: i32) -> [f32; 2] {
    let dpi = dpi_at_point(x, y) as f32;
    [x as f32 * 96.0 / dpi, y as f32 * 96.0 / dpi]
}

fn clamp_sized(x: i32, y: i32, win_w: i32, win_h: i32, scale: f32, fresh: bool) -> (i32, i32) {
    let monitors = work_areas_cached(fresh);
    if monitors.is_empty() {
        return (x, y);
    }
    let ball_cx = win_w / 2;
    let ball_cy = (BALL_CENTER_Y * scale).round() as i32;
    let radius = ((BALL_R + 8.0) * scale).round() as i32;
    let bx = x + ball_cx;
    let by = y + ball_cy;
    let target = pick_monitor(&monitors, bx, by, x, y, win_w, win_h);
    let min_bx = target.left + radius;
    let max_bx = (target.right - radius).max(min_bx);
    let min_by = target.top + radius;
    let max_by = (target.bottom - radius).max(min_by);
    (bx.clamp(min_bx, max_bx) - ball_cx, by.clamp(min_by, max_by) - ball_cy)
}

fn pick_monitor(
    monitors: &[WorkArea],
    bx: i32,
    by: i32,
    x: i32,
    y: i32,
    win_w: i32,
    win_h: i32,
) -> WorkArea {
    if let Some(&m) = monitors.iter().find(|m| m.contains(bx, by)) {
        return m;
    }
    let mut best = monitors[0];
    let mut best_area = -1i64;
    for &m in monitors {
        let area = intersect_area(x, y, win_w, win_h, &m);
        if area > best_area {
            best_area = area;
            best = m;
        }
    }
    if best_area > 0 {
        return best;
    }
    let mut nearest = monitors[0];
    let mut nearest_d = i64::MAX;
    for &m in monitors {
        let (cx, cy) = m.center();
        let dx = (bx - cx) as i64;
        let dy = (by - cy) as i64;
        let d = dx * dx + dy * dy;
        if d < nearest_d {
            nearest_d = d;
            nearest = m;
        }
    }
    nearest
}

fn intersect_area(x: i32, y: i32, w: i32, h: i32, m: &WorkArea) -> i64 {
    let x1 = x.max(m.left);
    let y1 = y.max(m.top);
    let x2 = (x + w).min(m.right);
    let y2 = (y + h).min(m.bottom);
    (x2 - x1).max(0) as i64 * (y2 - y1).max(0) as i64
}

fn cursor_work_area(areas: &[WorkArea]) -> Option<WorkArea> {
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct Point {
            x: i32,
            y: i32,
        }
        extern "system" {
            fn GetCursorPos(point: *mut Point) -> i32;
        }
        let mut pt = Point { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } != 0 {
            return areas.iter().copied().find(|a| a.contains(pt.x, pt.y));
        }
    }
    let _ = areas;
    None
}

struct WorkAreaCache {
    at: Instant,
    areas: Vec<WorkArea>,
}

fn work_areas() -> Vec<WorkArea> {
    work_areas_cached(false)
}

fn work_areas_cached(fresh: bool) -> Vec<WorkArea> {
    #[cfg(target_os = "windows")]
    {
        const TTL: Duration = Duration::from_millis(2000);
        static CACHE: Mutex<Option<WorkAreaCache>> = Mutex::new(None);

        if !fresh {
            let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = cache.as_ref() {
                if c.at.elapsed() < TTL && !c.areas.is_empty() {
                    return c.areas.clone();
                }
            }
        }

        let areas = enumerate_work_areas();
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        *cache = Some(WorkAreaCache {
            at: Instant::now(),
            areas: areas.clone(),
        });
        areas
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = fresh;
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
fn enumerate_work_areas() -> Vec<WorkArea> {
    let mut out = Vec::new();
    extern "system" {
        fn EnumDisplayMonitors(
            hdc: *mut std::ffi::c_void,
            clip: *mut WinRect,
            proc: Option<
                unsafe extern "system" fn(
                    *mut std::ffi::c_void,
                    *mut std::ffi::c_void,
                    *mut WinRect,
                    isize,
                ) -> i32,
            >,
            lparam: isize,
        ) -> i32;
    }
    unsafe {
        let _ = EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            Some(enum_monitors_cb),
            &mut out as *mut Vec<WorkArea> as isize,
        );
    }
    out
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_monitors_cb(
    hmon: *mut std::ffi::c_void,
    _hdc: *mut std::ffi::c_void,
    _rect: *mut WinRect,
    lparam: isize,
) -> i32 {
    const MONITORINFOF_PRIMARY: u32 = 1;
    extern "system" {
        fn GetMonitorInfoW(hmon: *mut std::ffi::c_void, info: *mut MonitorInfo) -> i32;
    }
    let out = &mut *(lparam as *mut Vec<WorkArea>);
    let mut info = MonitorInfo {
        cb_size: std::mem::size_of::<MonitorInfo>() as u32,
        rc_monitor: WinRect::zero(),
        rc_work: WinRect::zero(),
        dw_flags: 0,
    };
    if GetMonitorInfoW(hmon, &mut info) != 0 {
        out.push(WorkArea {
            left: info.rc_work.left,
            top: info.rc_work.top,
            right: info.rc_work.right,
            bottom: info.rc_work.bottom,
            dpi: dpi_for_monitor(hmon),
            primary: info.dw_flags & MONITORINFOF_PRIMARY != 0,
        });
    }
    1
}

#[cfg(target_os = "windows")]
fn monitor_from_point(x: i32, y: i32) -> *mut std::ffi::c_void {
    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    extern "system" {
        fn MonitorFromPoint(pt: Point, flags: u32) -> *mut std::ffi::c_void;
    }
    unsafe { MonitorFromPoint(Point { x, y }, MONITOR_DEFAULTTONEAREST) }
}

#[cfg(target_os = "windows")]
fn dpi_for_monitor(hmon: *mut std::ffi::c_void) -> u32 {
    if hmon.is_null() {
        return 96;
    }
    const MDT_EFFECTIVE_DPI: u32 = 0;
    #[link(name = "shcore")]
    extern "system" {
        fn GetDpiForMonitor(
            hmon: *mut std::ffi::c_void,
            dpi_type: u32,
            dpi_x: *mut u32,
            dpi_y: *mut u32,
        ) -> i32;
    }
    let mut dx = 0u32;
    let mut dy = 0u32;
    let hr = unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
    if hr == 0 && dx >= 96 {
        dx
    } else {
        96
    }
}

#[repr(C)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl WinRect {
    fn zero() -> Self {
        Self {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    }
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct MonitorInfo {
    cb_size: u32,
    rc_monitor: WinRect,
    rc_work: WinRect,
    dw_flags: u32,
}

/// Hide while a game is exclusive-fullscreen or a video covers this monitor.
pub fn should_auto_hide(orb_hwnd: isize) -> bool {
    #[cfg(target_os = "windows")]
    {
        if session_wants_overlay_hidden() {
            return true;
        }
        foreign_fullscreen_over_orb(orb_hwnd)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = orb_hwnd;
        false
    }
}

#[cfg(target_os = "windows")]
fn session_wants_overlay_hidden() -> bool {
    const QUNS_RUNNING_D3D_FULL_SCREEN: i32 = 3;
    const QUNS_PRESENTATION_MODE: i32 = 4;
    #[link(name = "shell32")]
    extern "system" {
        fn SHQueryUserNotificationState(state: *mut i32) -> i32;
    }
    let mut state = 0i32;
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    hr == 0 && matches!(state, QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE)
}

#[cfg(target_os = "windows")]
fn foreign_fullscreen_over_orb(orb_hwnd: isize) -> bool {
    use std::ffi::c_void;
    const MONITOR_DEFAULTTONEAREST: u32 = 2;

    extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
        fn GetClassNameW(hwnd: *mut c_void, buf: *mut u16, max: i32) -> i32;
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
        fn GetWindowRect(hwnd: *mut c_void, rect: *mut WinRect) -> i32;
        fn MonitorFromWindow(hwnd: *mut c_void, flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(hmon: *mut c_void, info: *mut MonitorInfo) -> i32;
        fn GetCurrentProcessId() -> u32;
        fn GetWindowLongPtrW(hwnd: *mut c_void, index: i32) -> isize;
    }

    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return false;
    }
    if orb_hwnd != 0 && fg as isize == orb_hwnd {
        return false;
    }

    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(fg, &mut pid) };
    if pid == unsafe { GetCurrentProcessId() } {
        return false;
    }
    if unsafe { IsWindowVisible(fg) } == 0 {
        return false;
    }

    let mut buf = [0u16; 96];
    let n = unsafe { GetClassNameW(fg, buf.as_mut_ptr(), buf.len() as i32) };
    if n > 0 {
        let cls = String::from_utf16_lossy(&buf[..n as usize]);
        match cls.as_str() {
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            | "XamlExplorerHostIslandWindow" | "NotifyIconOverflowWindow"
            | "ForegroundStaging" | "Windows.UI.Core.CoreWindow" => {
                return false;
            }
            _ => {}
        }
    }

    let mut wr = WinRect::zero();
    if unsafe { GetWindowRect(fg, &mut wr) } == 0 {
        return false;
    }
    let fg_mon = unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) };
    if fg_mon.is_null() {
        return false;
    }
    if orb_hwnd != 0 {
        let orb_mon =
            unsafe { MonitorFromWindow(orb_hwnd as *mut c_void, MONITOR_DEFAULTTONEAREST) };
        if !orb_mon.is_null() && orb_mon != fg_mon {
            return false;
        }
    }

    let mut mi = MonitorInfo {
        cb_size: std::mem::size_of::<MonitorInfo>() as u32,
        rc_monitor: WinRect::zero(),
        rc_work: WinRect::zero(),
        dw_flags: 0,
    };
    if unsafe { GetMonitorInfoW(fg_mon, &mut mi) } == 0 {
        return false;
    }
    let mr = mi.rc_monitor;
    let mw = (mr.right - mr.left).max(0);
    let mh = (mr.bottom - mr.top).max(0);
    if mw < 320 || mh < 240 {
        return false;
    }
    let ww = (wr.right - wr.left).max(0);
    let wh = (wr.bottom - wr.top).max(0);
    if ww <= 0 || wh <= 0 {
        return false;
    }
    // Fullscreen covers the monitor; maximized apps usually stop at the taskbar (rcWork).
    if (ww as i64) * (wh as i64) < (mw as i64) * (mh as i64) * 97 / 100 {
        return false;
    }
    if wr.left > mr.left + 8 || wr.top > mr.top + 8 {
        return false;
    }
    if wr.right < mr.right - 8 || wr.bottom < mr.bottom - 8 {
        return false;
    }

    // Maximized captioned windows can fill the monitor when the taskbar auto-hides.
    const GWL_STYLE: i32 = -16;
    const WS_CAPTION: isize = 0x00C0_0000;
    const WS_POPUP: isize = 0x8000_0000;
    let style = unsafe { GetWindowLongPtrW(fg, GWL_STYLE) };
    let captioned = style & WS_CAPTION == WS_CAPTION;
    let popup = style & WS_POPUP != 0;
    if captioned && !popup {
        return false;
    }
    true
}
