use std::{io, path::Path};

#[cfg(not(windows))]
pub(super) fn open(path: &Path) -> io::Result<()> {
    let mut child = std::process::Command::new("xdg-open").arg(path).spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(windows)]
pub(super) fn open(path: &Path) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr::null};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let file: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // The shell chooses the registered file association. The path is a single UTF-16 argument.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            null(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        return Err(io::Error::other(format!(
            "Windows could not open the Markdown file (shell code {result})"
        )));
    }
    Ok(())
}
