use super::control::{Action, Sender};
use std::{io, path::PathBuf};

pub fn choose(sender: Sender) -> io::Result<()> {
    std::thread::Builder::new()
        .name("folder-picker".into())
        .spawn(move || {
            let result = folder();
            sender.send(match result {
                Ok(Some(p)) => Action::Chosen(p),
                Ok(None) => Action::PickerClosed,
                Err(e) => Action::Error(e.to_string()),
            });
        })?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn folder() -> io::Result<Option<PathBuf>> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, process::Command};
    let kde = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("kde");
    let zenity = (
        "zenity",
        &[
            "--file-selection",
            "--directory",
            "--title=Open board folder",
        ][..],
    );
    let kdialog = (
        "kdialog",
        &[
            "--getexistingdirectory",
            ".",
            "--title",
            "Open board folder",
        ][..],
    );
    for (program, args) in if kde {
        [kdialog, zenity]
    } else {
        [zenity, kdialog]
    } {
        match Command::new(program).args(args).output() {
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
            Ok(out) if out.status.success() => {
                let mut bytes = out.stdout;
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                }
                return Ok((!bytes.is_empty()).then(|| PathBuf::from(OsString::from_vec(bytes))));
            }
            Ok(out) if out.status.code() == Some(1) => return Ok(None),
            Ok(_) => continue,
        }
    }
    Err(io::Error::other(
        "Folder picker unavailable. Paste a directory path into the board folder field.",
    ))
}

#[cfg(windows)]
fn folder() -> io::Result<Option<PathBuf>> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr::null};
    use windows_sys::{
        Win32::{System::Com::*, UI::Shell::*},
        core::w,
    };
    unsafe {
        let hr = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
        if hr < 0 {
            return Err(io::Error::other("Could not initialize the folder picker"));
        }
        let info = BROWSEINFOW {
            lpszTitle: w!("Open board folder"),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            ..Default::default()
        };
        let item = SHBrowseForFolderW(&info);
        let result = if item.is_null() {
            Ok(None)
        } else {
            let mut path = vec![0u16; 32768];
            let ok = SHGetPathFromIDListEx(item, path.as_mut_ptr(), path.len() as u32, 0);
            CoTaskMemFree(item.cast());
            if ok == 0 {
                Err(io::Error::other("Selected folder has no filesystem path"))
            } else {
                let len = path.iter().position(|c| *c == 0).unwrap_or(path.len());
                Ok(Some(PathBuf::from(OsString::from_wide(&path[..len]))))
            }
        };
        CoUninitialize();
        result
    }
}
