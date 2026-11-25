//! Persistence probe for disposable Pilot boards.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use toml::{Table, Value};

pub const MANIFEST_VERSION: u32 = 1;
pub const MARKER: &str = "corktop-gate-2-fixture";
const MANIFEST: &str = ".board.toml";
const LOCK: &str = ".probe.lock";
const LIMIT: u64 = 64 * 1024;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid synthetic board")
}

fn load(root: &Path) -> io::Result<Table> {
    let path = root.join(MANIFEST);
    if !path.symlink_metadata()?.is_file() {
        return Err(invalid());
    }
    let mut text = String::new();
    File::open(path)?
        .take(LIMIT + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > LIMIT {
        return Err(invalid());
    }
    let table: Table = toml::from_str(&text).map_err(|_| invalid())?;
    if table.get("marker").and_then(Value::as_str) != Some(MARKER) {
        return Err(invalid());
    }
    Ok(table)
}

fn save(root: &Path, table: &Table) -> io::Result<()> {
    let mut pending = tempfile::NamedTempFile::new_in(root)?;
    pending.write_all(toml::to_string(table).map_err(|_| invalid())?.as_bytes())?;
    pending.as_file().sync_all()?;
    pending.persist(root.join(MANIFEST))?;
    Ok(())
}

/// Create a new scratch directory or reopen a previously initialized probe.
/// Existing unrelated directories are rejected before opening any file for writing.
pub fn probe(root: &Path) -> io::Result<()> {
    let lock_path = root.join(LOCK);
    let lock = match fs::create_dir(root) {
        Ok(()) => OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&lock_path)?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if !root.symlink_metadata()?.is_dir()
                || !lock_path.symlink_metadata()?.is_file()
                || lock_path.metadata()?.len() != 0
            {
                return Err(invalid());
            }
            OpenOptions::new().read(true).write(true).open(&lock_path)?
        }
        Err(e) => return Err(e),
    };
    lock.lock()?;
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name();
        if name != LOCK && name != MANIFEST {
            return Err(invalid());
        }
    }
    match root.join(MANIFEST).symlink_metadata() {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let mut initial = Table::new();
            initial.insert("marker".into(), MARKER.into());
            initial.insert("schema_version".into(), 0.into());
            save(root, &initial)?;
        }
        Err(e) => return Err(e),
        Ok(_) => (),
    }
    let mut table = load(root)?;
    match table.get("schema_version").and_then(Value::as_integer) {
        Some(0) => {
            table.insert("schema_version".into(), MANIFEST_VERSION.into());
            save(root, &table)?;
        }
        Some(v) if v == i64::from(MANIFEST_VERSION) => (),
        _ => return Err(invalid()),
    }
    if load(root)? != table {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_reopens_without_rewriting() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().join("board");
        probe(&root).unwrap();
        let manifest = root.join(MANIFEST);
        let first = fs::read(&manifest).unwrap();
        assert_eq!(load(&root).unwrap()["schema_version"].as_integer(), Some(1));
        probe(&root).unwrap();
        assert_eq!(fs::read(&manifest).unwrap(), first);
        fs::write(
            &manifest,
            format!("schema_version=0\nmarker='{MARKER}'\nextra=7"),
        )
        .unwrap();
        probe(&root).unwrap();
        assert_eq!(load(&root).unwrap()["extra"].as_integer(), Some(7));
    }

    #[test]
    fn rejects_invalid_and_unrelated_inputs_without_modifying_them() {
        let scratch = tempfile::tempdir().unwrap();
        assert!(probe(scratch.path()).is_err());
        assert_eq!(fs::read_dir(scratch.path()).unwrap().count(), 0);
        let root = scratch.path().join("board");
        probe(&root).unwrap();
        let manifest = root.join(MANIFEST);
        for text in [
            "[".into(),
            format!("schema_version=2\nmarker='{MARKER}'"),
            " ".repeat(LIMIT as usize + 1),
        ] {
            fs::write(&manifest, &text).unwrap();
            assert!(probe(&root).is_err());
            assert_eq!(fs::read_to_string(&manifest).unwrap(), text);
        }
    }
}
