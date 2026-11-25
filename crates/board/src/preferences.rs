use crate::{LIMIT, read_text};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use toml::Table;

/// Synced preferences for a portable board tree. Unknown keys survive edits.
pub struct Preferences {
    directory: PathBuf,
    values: Table,
}

impl Preferences {
    pub fn load(root: &Path) -> io::Result<Self> {
        let directory = root.join(".corktop");
        let values = match directory.symlink_metadata() {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Table::new(),
            Ok(m) if m.is_dir() => match read_text(&directory.join("config.toml"), LIMIT) {
                Ok(raw) => toml::from_str(&raw).map_err(io::Error::other)?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => Table::new(),
                Err(e) => return Err(e),
            },
            Ok(_) => return Err(io::Error::other("invalid portable config directory")),
            Err(e) => return Err(e),
        };
        Ok(Self { directory, values })
    }

    pub fn flag(&self, key: &str, default: bool) -> bool {
        self.values
            .get(key)
            .and_then(toml::Value::as_bool)
            .unwrap_or(default)
    }

    pub fn set(&mut self, key: &str, value: toml::Value) -> io::Result<()> {
        match fs::create_dir(&self.directory) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
        if !self.directory.symlink_metadata()?.is_dir() {
            return Err(io::Error::other("invalid portable config directory"));
        }
        let mut fresh = Self::load(self.directory.parent().unwrap())?.values;
        fresh.insert(key.into(), value);
        let data = toml::to_string(&fresh).map_err(io::Error::other)?;
        if data.len() as u64 > LIMIT {
            return Err(io::Error::other("portable config exceeds size limit"));
        }
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
        temp.write_all(data.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(self.directory.join("config.toml"))?;
        self.values = fresh;
        Ok(())
    }
}
