use std::{io, path::Path};

#[cfg(target_os = "linux")]
pub fn set(enabled: bool, exe: &Path) -> io::Result<()> {
    use std::{fs, io::Write, path::PathBuf};
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| io::Error::other("missing user config directory"))?;
    let dir = config.join("autostart");
    let dest = dir.join("corktop.desktop");
    if !enabled {
        return match fs::remove_file(dest) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    }
    let path = exe
        .to_str()
        .ok_or_else(|| io::Error::other("invalid executable path"))?;
    if path.chars().any(char::is_control) {
        return Err(io::Error::other("invalid executable path"));
    }
    let mut quoted = String::new();
    for c in path.chars() {
        match c {
            '%' => quoted.push_str("%%"),
            '\\' | '"' | '$' | '`' => {
                quoted.push('\\');
                quoted.push(c)
            }
            _ => quoted.push(c),
        }
    }
    // Desktop-entry strings unescape before the Exec argument parser.
    let quoted = quoted.replace('\\', "\\\\");
    fs::create_dir_all(&dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
    writeln!(
        temp,
        "[Desktop Entry]\nType=Application\nName=corktop\nExec=\"{quoted}\"\nTerminal=false"
    )?;
    temp.as_file().sync_all()?;
    temp.persist(dest)?;
    Ok(())
}

#[cfg(windows)]
pub fn set(enabled: bool, exe: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::{
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
            System::Registry::*,
        },
        core::w,
    };
    let mut key = std::ptr::null_mut();
    unsafe {
        let code = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            KEY_SET_VALUE,
            &mut key,
        );
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let code = if enabled {
            let mut command = vec![34u16];
            command.extend(exe.as_os_str().encode_wide());
            command.extend([34, 0]);
            RegSetValueExW(
                key,
                w!("corktop"),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, w!("corktop"))
        };
        RegCloseKey(key);
        if code != ERROR_SUCCESS && (enabled || code != ERROR_FILE_NOT_FOUND) {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
    }
    Ok(())
}
