use std::{
    io,
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Foundation::{GetLastError, HWND, RECT, SetLastError},
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

pub(super) struct Placement {
    style: isize,
    extended: isize,
    rect: RECT,
    owner: HWND,
}

unsafe extern "system" fn find_icon_host(window: HWND, output: isize) -> i32 {
    unsafe {
        let mut class = [0u16; 32];
        let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        let name = &class[..len.max(0) as usize];
        if IsWindowVisible(window) != 0
            && (name.iter().copied().eq("WorkerW".encode_utf16())
                || name.iter().copied().eq("Progman".encode_utf16()))
            && !FindWindowExW(window, null_mut(), w!("SHELLDLL_DefView"), null()).is_null()
        {
            *(output as *mut HWND) = window;
            return 0;
        }
    }
    1
}

fn style(window: HWND, index: i32, value: isize) -> io::Result<()> {
    unsafe {
        SetLastError(0);
        if SetWindowLongPtrW(window, index, value) == 0 && GetLastError() != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

// Explorer may request minimization directly or through the system menu.
// Restore without activating, so Show Desktop keeps keyboard focus in the shell.
unsafe extern "system" fn desktop_proc(
    window: HWND,
    msg: u32,
    wp: usize,
    lp: isize,
    id: usize,
    _: usize,
) -> isize {
    unsafe {
        match msg {
            WM_SYSCOMMAND if (wp as u32 & 0xfff0) == SC_MINIMIZE => return 0,
            WM_SIZE if wp == SIZE_MINIMIZED as usize => {
                ShowWindowAsync(window, SW_SHOWNOACTIVATE);
            }
            WM_STYLECHANGING => {
                let change = &mut *(lp as *mut STYLESTRUCT);
                match wp as i32 {
                    GWL_EXSTYLE => {
                        change.styleNew = (change.styleNew | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW
                    }
                    GWL_STYLE => change.styleNew &= !WS_MINIMIZEBOX,
                    _ => {}
                }
            }
            WM_WINDOWPOSCHANGING => {
                let pos = &mut *(lp as *mut WINDOWPOS);
                if pos.flags & SWP_NOZORDER == 0 {
                    pos.hwndInsertAfter = HWND_BOTTOM;
                    pos.flags |= SWP_NOOWNERZORDER;
                }
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(window, Some(desktop_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(window, msg, wp, lp)
    }
}

impl Placement {
    fn restore(&self, window: HWND) -> io::Result<()> {
        unsafe {
            RemoveWindowSubclass(window, Some(desktop_proc), 2);
        }
        style(window, GWLP_HWNDPARENT, self.owner as isize)?;
        style(window, GWL_STYLE, self.style)?;
        style(window, GWL_EXSTYLE, self.extended)?;
        unsafe {
            if SetWindowPos(
                window,
                HWND_TOP,
                self.rect.left,
                self.rect.top,
                self.rect.right - self.rect.left,
                self.rect.bottom - self.rect.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOOWNERZORDER,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

pub(super) fn set(
    handle: isize,
    desktop: bool,
    original: &mut Option<Placement>,
) -> io::Result<()> {
    let window = handle as HWND;
    if !desktop {
        if let Some(saved) = original.as_ref() {
            saved.restore(window)?;
            *original = None;
        }
        return Ok(());
    }
    unsafe {
        let mut host: HWND = null_mut();
        EnumWindows(Some(find_icon_host), (&mut host as *mut HWND) as isize);
        if host.is_null() {
            return Err(io::Error::other(
                "Explorer icon host unavailable. Standard window is active.",
            ));
        }
        let mut rect = RECT::default();
        if GetWindowRect(window, &mut rect) == 0 {
            return Err(io::Error::last_os_error());
        }
        let saved = Placement {
            style: GetWindowLongPtrW(window, GWL_STYLE),
            extended: GetWindowLongPtrW(window, GWL_EXSTYLE),
            owner: GetWindow(window, GW_OWNER),
            rect,
        };
        // Keep the renderer top-level. Ownership keeps the board above the
        // desktop icon host even as the shell moves it forward for Show Desktop.
        let attach = (|| {
            style(window, GWLP_HWNDPARENT, host as isize)?;
            style(window, GWL_STYLE, saved.style & !(WS_MINIMIZEBOX as isize))?;
            style(
                window,
                GWL_EXSTYLE,
                (saved.extended & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize,
            )?;
            if SetWindowSubclass(window, Some(desktop_proc), 2, 0) == 0 {
                return Err(io::Error::other(
                    "Could not install desktop placement handler",
                ));
            }
            if SetWindowPos(
                window,
                HWND_BOTTOM,
                0,
                0,
                0,
                0,
                SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOACTIVATE
                    | SWP_FRAMECHANGED
                    | SWP_SHOWWINDOW
                    | SWP_NOOWNERZORDER,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        })();
        if let Err(error) = attach {
            saved.restore(window)?;
            return Err(error);
        }
        *original = Some(saved);
    }
    Ok(())
}
