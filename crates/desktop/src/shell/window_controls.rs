// Dispatch tray and launch requests on the window thread even while there is
// no drawable frame. The queue owns the actions; the message is only a wakeup.
use crate::BoardWindow;
use eframe::egui;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

const WAKE: u32 = WM_APP + 0x4c1;
const QUIT: u32 = WM_APP + 0x4c3;

pub fn quit(hwnd: isize) {
    unsafe {
        PostMessageW(hwnd as HWND, QUIT, 0, 0);
    }
}

thread_local! {
    static RECEIVER: RefCell<Option<(Weak<RefCell<BoardWindow>>, egui::Context)>> = const { RefCell::new(None) };
}

pub fn attach(hwnd: isize, app: &Rc<RefCell<BoardWindow>>, ctx: &egui::Context) -> bool {
    if unsafe { SetWindowSubclass(hwnd as HWND, Some(procedure), 3, 0) } == 0 {
        return false;
    }
    RECEIVER.replace(Some((Rc::downgrade(app), ctx.clone())));
    true
}

pub fn wake(hwnd: isize) {
    if hwnd != 0 {
        unsafe {
            PostMessageW(hwnd as HWND, WAKE, 0, 0);
        }
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
        if msg == QUIT {
            // winit maps destruction of the root window to a normal eframe exit.
            DestroyWindow(window);
            return 0;
        }
        if msg == WAKE {
            let receiver = RECEIVER.with_borrow(Clone::clone);
            if let Some((weak, ctx)) = receiver
                && let Some(app) = weak.upgrade()
                && let Ok(mut app) = app.try_borrow_mut()
            {
                app.process_controls(&ctx);
                ctx.request_repaint();
            }
            return 0;
        }
        if msg == WM_NCDESTROY {
            RECEIVER.take();
            RemoveWindowSubclass(window, Some(procedure), id);
        }
        DefSubclassProc(window, msg, wp, lp)
    }
}
