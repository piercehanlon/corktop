// The blank rail is a real Win32 non-client caption. Windows owns the click
// sequence and drag loop, including the transition out of a maximized window.
use std::cell::Cell;
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{ClientToScreen, ScreenToClient},
    UI::{
        Input::KeyboardAndMouse::{GetDoubleClickTime, ReleaseCapture, SetCapture},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

thread_local! {
    static PRESS: Cell<Option<(HWND,POINT)>> = const {Cell::new(None)};
    static LAST_DOWN: Cell<Option<(HWND,POINT,u32)>> = const {Cell::new(None)};
    static NORMAL: Cell<Option<(HWND,RECT)>> = const {Cell::new(None)};
    static SLOP: Cell<i32> = const {Cell::new(6)};
    static REGION: Cell<Option<(HWND, RECT)>> = const {Cell::new(None)};
}

pub(crate) fn clear() {
    REGION.set(None);
}

pub(crate) fn set(handle: isize, rect: eframe::egui::Rect, pixels_per_point: f32) {
    if handle == 0 || !rect.is_positive() {
        return;
    }
    SLOP.set((6. * pixels_per_point).ceil() as i32);
    let window = handle as HWND;
    let rect = RECT {
        left: (rect.min.x * pixels_per_point).ceil() as i32,
        top: (rect.min.y * pixels_per_point).ceil() as i32,
        right: (rect.max.x * pixels_per_point).floor() as i32,
        bottom: (rect.max.y * pixels_per_point).floor() as i32,
    };
    // Re-registering the same procedure and ID updates the existing subclass.
    if unsafe { SetWindowSubclass(window, Some(procedure), 1, 0) } != 0 {
        REGION.set(Some((window, rect)));
    }
}

pub(crate) fn toggle_maximize(handle: isize) {
    let window = handle as HWND;
    unsafe {
        let mut rect = RECT::default();
        if IsZoomed(window) == 0 {
            if GetWindowRect(window, &mut rect) != 0 {
                NORMAL.set(Some((window, rect)));
            }
            ShowWindow(window, SW_MAXIMIZE);
        } else {
            ShowWindow(window, SW_RESTORE);
            if let Some((owner, saved)) = NORMAL.take()
                && owner == window
                && GetWindowRect(window, &mut rect) != 0
                && (rect.left, rect.top, rect.right, rect.bottom)
                    != (saved.left, saved.top, saved.right, saved.bottom)
            {
                SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    saved.left,
                    saved.top,
                    saved.right - saved.left,
                    saved.bottom - saved.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

fn point(lp: isize) -> POINT {
    POINT {
        x: lp as i16 as i32,
        y: (lp >> 16) as i16 as i32,
    }
}

unsafe extern "system" fn procedure(
    window: HWND,
    msg: u32,
    wp: usize,
    lp: isize,
    id: usize,
    _: usize,
) -> isize {
    unsafe {
        if msg == WM_NCHITTEST
            && let Some((owner, rect)) = REGION.get()
            && owner == window
        {
            let mut p = POINT {
                x: lp as i16 as i32,
                y: (lp >> 16) as i16 as i32,
            };
            if ScreenToClient(window, &mut p) != 0
                && (rect.left..rect.right).contains(&p.x)
                && (rect.top..rect.bottom).contains(&p.y)
            {
                return HTCAPTION as isize;
            }
        }
        if matches!(msg, WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK) && wp == HTCAPTION as usize {
            let p = point(lp);
            let now = GetMessageTime() as u32;
            let double = msg == WM_NCLBUTTONDBLCLK
                || LAST_DOWN.get().is_some_and(|(owner, q, t)| {
                    owner == window
                        && now.wrapping_sub(t) <= GetDoubleClickTime()
                        && (p.x - q.x).abs() <= GetSystemMetrics(SM_CXDOUBLECLK) / 2
                        && (p.y - q.y).abs() <= GetSystemMetrics(SM_CYDOUBLECLK) / 2
                });
            if double {
                LAST_DOWN.set(None);
                toggle_maximize(window as isize);
            } else {
                LAST_DOWN.set(Some((window, p, now)));
                PRESS.set(Some((window, p)));
                SetCapture(window);
            }
            return 0;
        }
        if let Some((owner, start)) = PRESS.get()
            && owner == window
        {
            match msg {
                WM_MOUSEMOVE => {
                    let mut p = point(lp);
                    if ClientToScreen(window, &mut p) != 0
                        && (p.x - start.x).abs().max((p.y - start.y).abs()) >= SLOP.get()
                    {
                        PRESS.set(None);
                        LAST_DOWN.set(None);
                        ReleaseCapture();
                        let screen = ((p.y as u16 as u32) << 16) | p.x as u16 as u32;
                        return DefWindowProcW(
                            window,
                            WM_NCLBUTTONDOWN,
                            HTCAPTION as usize,
                            screen as isize,
                        );
                    }
                    return 0;
                }
                WM_LBUTTONUP => {
                    PRESS.set(None);
                    ReleaseCapture();
                    return 0;
                }
                WM_CANCELMODE | WM_CAPTURECHANGED => {
                    PRESS.set(None);
                    LAST_DOWN.set(None);
                }
                _ => {}
            }
        }
        if msg == WM_NCDESTROY {
            PRESS.set(None);
            LAST_DOWN.set(None);
            NORMAL.set(None);
            RemoveWindowSubclass(window, Some(procedure), id);
            if REGION.get().is_some_and(|(owner, _)| owner == window) {
                clear();
            }
        }
        DefSubclassProc(window, msg, wp, lp)
    }
}
