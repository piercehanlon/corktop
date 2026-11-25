use super::control::{Action, Sender};
#[cfg(target_os = "linux")]
pub struct Tray(pub Sender);

#[cfg(target_os = "linux")]
impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "corktop".into()
    }
    fn title(&self) -> String {
        "corktop".into()
    }
    fn icon_name(&self) -> String {
        "user-bookmarks".into()
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.0.send(Action::ToggleVisible);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        type Item = (&'static str, fn() -> Action);
        let actions: [Item; 6] = [
            ("Show / Hide", || Action::ToggleVisible),
            ("Toggle desktop layer", || Action::ToggleLayer),
            ("Switch folder…", || Action::ChooseFolder),
            ("New panel", || Action::Panel),
            ("Settings", || Action::Settings),
            ("Quit", || Action::Quit),
        ];
        actions
            .into_iter()
            .map(|(title, action)| {
                ksni::menu::StandardItem {
                    label: title.into(),
                    activate: Box::new(move |this: &mut Self| this.0.send(action())),
                    ..Default::default()
                }
                .into()
            })
            .collect()
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        io,
        ptr::{null, null_mut},
        sync::mpsc,
    };
    use windows_sys::{
        Win32::{
            Foundation::*,
            System::LibraryLoader::GetModuleHandleW,
            UI::{Shell::*, WindowsAndMessaging::*},
        },
        core::w,
    };
    const EVENT: u32 = WM_APP + 4;

    pub struct Handle(isize);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                PostMessageW(self.0 as HWND, WM_CLOSE, 0, 0);
            }
        }
    }
    struct WindowData {
        sender: Sender,
        restart: u32,
    }
    fn icon(hwnd: HWND, operation: u32) -> bool {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: EVENT,
            ..Default::default()
        };
        unsafe {
            data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        }
        for (p, c) in data.szTip.iter_mut().zip("corktop".encode_utf16()) {
            *p = c;
        }
        unsafe { Shell_NotifyIconW(operation, &data) != 0 }
    }

    unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
        unsafe {
            let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowData;
            if let Some(d) = data.as_ref() {
                if msg == d.restart && msg != 0 {
                    icon(hwnd, NIM_ADD);
                    return 0;
                }
                if msg == EVENT {
                    match lp as u32 {
                        WM_LBUTTONUP => d.sender.send(Action::ToggleVisible),
                        WM_RBUTTONUP | WM_CONTEXTMENU => {
                            let menu = CreatePopupMenu();
                            if menu.is_null() {
                                return 0;
                            }
                            for (i, title) in [
                                "Show / Hide",
                                "Toggle desktop layer",
                                "Switch folder...",
                                "New panel",
                                "Settings",
                                "Quit",
                            ]
                            .iter()
                            .enumerate()
                            {
                                let wide: Vec<_> = title.encode_utf16().chain([0]).collect();
                                AppendMenuW(menu, MF_STRING, i + 1, wide.as_ptr());
                            }
                            let mut pt = POINT::default();
                            GetCursorPos(&mut pt);
                            SetForegroundWindow(hwnd);
                            let choice = TrackPopupMenu(
                                menu,
                                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                                pt.x,
                                pt.y,
                                0,
                                hwnd,
                                null(),
                            );
                            DestroyMenu(menu);
                            PostMessageW(hwnd, WM_NULL, 0, 0);
                            let action = match choice {
                                1 => Action::ToggleVisible,
                                2 => Action::ToggleLayer,
                                3 => Action::ChooseFolder,
                                4 => Action::Panel,
                                5 => Action::Settings,
                                6 => Action::Quit,
                                _ => return 0,
                            };
                            d.sender.send(action);
                        }
                        _ => (),
                    }
                    return 0;
                }
            }
            if msg == WM_DESTROY {
                icon(hwnd, NIM_DELETE);
                PostQuitMessage(0);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }
    pub fn start(sender: Sender) -> io::Result<Handle> {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("notification-area".into())
            .spawn(move || unsafe {
                let module = GetModuleHandleW(null());
                let class = WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: module,
                    lpszClassName: w!("corktopTrayWindow"),
                    ..Default::default()
                };
                if RegisterClassW(&class) == 0 {
                    let _ = tx.send(Err(io::Error::last_os_error()));
                    return;
                }
                let hwnd = CreateWindowExW(
                    0,
                    class.lpszClassName,
                    w!("corktop tray"),
                    0,
                    0,
                    0,
                    0,
                    0,
                    null_mut(),
                    null_mut(),
                    module,
                    null(),
                );
                if hwnd.is_null() {
                    let _ = tx.send(Err(io::Error::last_os_error()));
                    return;
                }
                // This thread owns both the window and its state through the final dispatched message.
                let data = Box::new(WindowData {
                    sender,
                    restart: RegisterWindowMessageW(w!("TaskbarCreated")),
                });
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&*data as *const WindowData) as isize);
                if !icon(hwnd, NIM_ADD) {
                    let _ = tx.send(Err(io::Error::other("notification area unavailable")));
                    DestroyWindow(hwnd);
                    return;
                }
                if tx.send(Ok(Handle(hwnd as isize))).is_err() {
                    DestroyWindow(hwnd);
                    return;
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })?;
        rx.recv().map_err(io::Error::other)?
    }
}
#[cfg(windows)]
pub use native::{Handle, start};
