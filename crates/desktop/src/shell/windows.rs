use std::{
    io,
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Foundation::{GetLastError, HWND, RECT, SetLastError},
        UI::WindowsAndMessaging::*,
    },
    core::w,
};

pub(super) struct Placement {
    style: isize,
    rect: RECT,
    parent: isize,
}

unsafe extern "system" fn find_worker(window: HWND, output: isize) -> i32 {
    // Explorer places the wallpaper WorkerW immediately below the desktop icon host.
    unsafe {
        if !FindWindowExW(window, null_mut(), w!("SHELLDLL_DefView"), null()).is_null() {
            let worker = FindWindowExW(null_mut(), window, w!("WorkerW"), null());
            if !worker.is_null() {
                *(output as *mut HWND) = worker;
                return 0;
            }
        }
    }
    1
}

pub(super) fn set(
    handle: isize,
    desktop: bool,
    original: &mut Option<Placement>,
) -> io::Result<()> {
    let window = handle as HWND;
    // All changes to style and parent target this process's own live window.
    unsafe {
        if desktop {
            let shell = FindWindowW(w!("Progman"), null());
            if shell.is_null() {
                return Err(io::Error::other("Windows desktop host unavailable"));
            }
            SendMessageTimeoutW(shell, 0x052c, 0, 0, SMTO_ABORTIFHUNG, 300, null_mut());
            let mut worker: HWND = null_mut();
            EnumWindows(Some(find_worker), (&mut worker as *mut HWND) as isize);
            if worker.is_null() {
                return Err(io::Error::other(
                    "Wallpaper layer unavailable. Standard window is active.",
                ));
            }
            let mut rect = RECT::default();
            if GetWindowRect(window, &mut rect) == 0 {
                return Err(io::Error::last_os_error());
            }
            let style = GetWindowLongPtrW(window, GWL_STYLE);
            let parent = GetParent(window);
            SetWindowLongPtrW(
                window,
                GWL_STYLE,
                (style & !(WS_POPUP as isize)) | WS_CHILD as isize,
            );
            SetLastError(0);
            if SetParent(window, worker).is_null() && GetLastError() != 0 {
                let err = io::Error::last_os_error();
                SetWindowLongPtrW(window, GWL_STYLE, style);
                return Err(err);
            }
            *original = Some(Placement {
                style,
                rect,
                parent: parent as isize,
            });
        } else if let Some(p) = original.as_ref() {
            SetLastError(0);
            if SetParent(window, p.parent as HWND).is_null() && GetLastError() != 0 {
                return Err(io::Error::last_os_error());
            }
            SetWindowLongPtrW(window, GWL_STYLE, p.style);
            SetWindowPos(
                window,
                HWND_TOP,
                p.rect.left,
                p.rect.top,
                p.rect.right - p.rect.left,
                p.rect.bottom - p.rect.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
            *original = None;
        }
    }
    Ok(())
}
