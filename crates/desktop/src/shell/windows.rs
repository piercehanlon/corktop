use std::{
    io,
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Foundation::{GetLastError, HWND, POINT, RECT, SetLastError},
        Graphics::Gdi::ScreenToClient,
        UI::WindowsAndMessaging::*,
    },
    core::w,
};

pub(super) struct Placement {
    style: isize,
    extended: isize,
    rect: RECT,
    parent: HWND,
}

unsafe extern "system" fn find_icon_host(window: HWND, output: isize) -> i32 {
    unsafe {
        let mut class = [0u16; 32];
        let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        if !class[..len.max(0) as usize]
            .iter()
            .copied()
            .eq("WorkerW".encode_utf16())
        {
            return 1;
        }
        if !FindWindowExW(window, null_mut(), w!("SHELLDLL_DefView"), null()).is_null() {
            *(output as *mut HWND) = window;
            return 0;
        }
    }
    1
}

fn reparent(window: HWND, parent: HWND) -> io::Result<()> {
    unsafe {
        SetLastError(0);
        if SetParent(window, parent).is_null() && GetLastError() != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
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

fn position(window: HWND, rect: RECT, origin: POINT) -> io::Result<()> {
    unsafe {
        if SetWindowPos(
            window,
            HWND_TOP,
            origin.x,
            origin.y,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

impl Placement {
    fn restore(&self, window: HWND) -> io::Result<()> {
        reparent(window, self.parent)?;
        style(window, GWL_STYLE, self.style)?;
        style(window, GWL_EXSTYLE, self.extended)?;
        position(
            window,
            self.rect,
            POINT {
                x: self.rect.left,
                y: self.rect.top,
            },
        )
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
        let progman = FindWindowW(w!("Progman"), null());
        if progman.is_null() {
            return Err(io::Error::other(
                "Explorer desktop unavailable. Standard window is active.",
            ));
        }
        // Current Explorer keeps DefView directly under Progman. Older shells host
        // it in a top-level WorkerW. Use the icon host itself, so a board-sized child
        // can accept input above DefView while ordinary apps remain above the desktop.
        let mut host = progman;
        if FindWindowExW(progman, null_mut(), w!("SHELLDLL_DefView"), null()).is_null() {
            host = null_mut();
            EnumWindows(Some(find_icon_host), (&mut host as *mut HWND) as isize);
        }
        if host.is_null() {
            return Err(io::Error::other(
                "Explorer icon host unavailable. Standard window is active.",
            ));
        }
        let mut rect = RECT::default();
        if GetWindowRect(window, &mut rect) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut origin = POINT {
            x: rect.left,
            y: rect.top,
        };
        if ScreenToClient(host, &mut origin) == 0 {
            return Err(io::Error::last_os_error());
        }
        let saved = Placement {
            style: GetWindowLongPtrW(window, GWL_STYLE),
            extended: GetWindowLongPtrW(window, GWL_EXSTYLE),
            parent: GetParent(window),
            rect,
        };
        let attach = (|| {
            style(
                window,
                GWL_STYLE,
                (saved.style & !(WS_POPUP as isize)) | (WS_CHILD | WS_CLIPSIBLINGS) as isize,
            )?;
            style(
                window,
                GWL_EXSTYLE,
                (saved.extended & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize,
            )?;
            reparent(window, host)?;
            position(window, rect, origin)
        })();
        if let Err(error) = attach {
            saved.restore(window)?;
            return Err(error);
        }
        *original = Some(saved);
    }
    Ok(())
}
