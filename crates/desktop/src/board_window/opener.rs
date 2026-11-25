use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc,
};

// Launchers and association dialogs can block. Report failures back to the event loop.
pub(super) fn launch(
    path: PathBuf,
    errors: mpsc::Sender<io::Error>,
    ctx: eframe::egui::Context,
) -> io::Result<()> {
    std::thread::Builder::new()
        .name("open-note".into())
        .spawn(move || {
            if let Err(error) = open(&path) {
                let _ = errors.send(error);
                ctx.request_repaint();
            }
        })?;
    Ok(())
}

#[cfg(not(windows))]
fn open(path: &Path) -> io::Result<()> {
    let status = std::process::Command::new("xdg-open").arg(path).status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "Desktop opener exited with {status}"
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn open(path: &Path) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr::null};
    use windows_sys::Win32::{
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
        UI::{
            Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    };

    let file: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut request = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // Finish shell dispatch on this worker before releasing its COM apartment.
        fMask: SEE_MASK_NOASYNC,
        lpFile: file.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // All pointers remain valid throughout dispatch. Each worker owns its COM apartment.
    let hr = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };
    if hr < 0 {
        return Err(io::Error::other(format!(
            "Could not initialize Windows shell dispatch ({hr:#x})"
        )));
    }
    let result = if unsafe { ShellExecuteExW(&mut request) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    };
    unsafe { CoUninitialize() };
    result
}
