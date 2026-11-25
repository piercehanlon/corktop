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

impl Preferences {
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> io::Result<Option<T>> {
        self.values
            .get(key)
            .cloned()
            .map(|v| v.try_into().map_err(io::Error::other))
            .transpose()
    }
    pub fn store<T: serde::Serialize>(&mut self, key: &str, value: &T) -> io::Result<()> {
        self.set(key, toml::Value::try_from(value).map_err(io::Error::other)?)
    }
}

impl Preferences {
    fn overrides(&self) -> io::Result<PathBuf> {
        let dir = self.directory.join("overrides");
        for p in [&self.directory, &dir] {
            if !p.symlink_metadata()?.is_dir() {
                return Err(io::Error::other("invalid overrides directory"));
            }
        }
        Ok(dir)
    }
    pub fn override_files(&self) -> io::Result<Vec<String>> {
        let dir = match self.overrides() {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut files = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && let Ok(name) = entry.file_name().into_string()
            {
                files.push(name);
            }
        }
        files.sort_unstable();
        Ok(files)
    }
    pub fn read_override(&self, name: &str) -> io::Result<Vec<u8>> {
        use std::io::Read;
        let name = Path::new(name);
        if Some(name.as_os_str()) != name.file_name() {
            return Err(io::Error::other("invalid override filename"));
        }
        let path = self.overrides()?.join(name);
        if !path.symlink_metadata()?.is_file() {
            return Err(io::Error::other("expected a regular override file"));
        }
        let file = crate::nofollow_options().read(true).open(path)?;
        let limit = 16 * 1024 * 1024;
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() > limit {
            return Err(io::Error::other("override exceeds size limit"));
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        file.take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err(io::Error::other("override exceeds size limit"));
        }
        Ok(bytes)
    }
}
