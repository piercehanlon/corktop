//! Preview and copy a folder board without modifying the source.
use crate::{nofollow_options, read_text};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(PartialEq)]
enum Entry {
    Directory,
    File,
    Link(String),
    Skip,
}

#[derive(PartialEq)]
pub struct ExportPlan {
    root: PathBuf,
    entries: Vec<(PathBuf, Entry)>,
}

fn portable_link(target: &Path) -> bool {
    target.starts_with(".corktop/linked")
        && target
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

pub(crate) fn resolve_link(board: &Path, raw: &str) -> io::Result<PathBuf> {
    let target = Path::new(raw);
    let resolved = if target.is_absolute() {
        target.to_owned()
    } else if portable_link(target) {
        let path = board.join(target).canonicalize()?;
        if !path.starts_with(board.canonicalize()?) {
            return Err(io::Error::other("linked file is outside its board"));
        }
        path
    } else {
        return Err(io::Error::other("invalid linked file path"));
    };
    if !resolved.symlink_metadata()?.is_file() {
        return Err(io::Error::other("linked file unavailable"));
    }
    Ok(resolved)
}

impl ExportPlan {
    pub fn preview(root: &Path) -> io::Result<Self> {
        let mut plan = Self {
            root: root.canonicalize()?,
            entries: Vec::new(),
        };
        plan.scan(Path::new(""))?;
        Ok(plan)
    }

    fn scan(&mut self, sub: &Path) -> io::Result<()> {
        let mut children = fs::read_dir(self.root.join(sub))?.collect::<io::Result<Vec<_>>>()?;
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let name = child.file_name();
            let rel = sub.join(&name);
            let kind = child.file_type()?;
            let entry = if name == ".corktop.lock"
                || name == ".probe.lock"
                || (name.to_string_lossy().starts_with('.')
                    && !matches!(name.to_str(), Some(".board.toml" | ".corktop" | ".trash")))
            {
                Entry::Skip
            } else if kind.is_dir() {
                self.entries.push((rel.clone(), Entry::Directory));
                self.scan(&rel)?;
                continue;
            } else if !kind.is_file() {
                Entry::Skip
            } else if rel
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("corklink"))
                && !rel.components().any(|c| c.as_os_str() == ".corktop")
            {
                Entry::Link(read_text(&child.path(), 65536)?)
            } else {
                Entry::File
            };
            self.entries.push((rel, entry));
        }
        Ok(())
    }

    pub fn contents(&self, include: bool) -> impl Iterator<Item = String> + '_ {
        self.entries.iter().map(move |(p, entry)| match entry {
            Entry::Link(raw) if Path::new(raw).is_absolute() => format!(
                "{} → {} ({})",
                p.display(),
                raw,
                if include {
                    "include file"
                } else {
                    "reference only"
                }
            ),
            Entry::Skip => format!("{} (skipped)", p.display()),
            _ => p.display().to_string(),
        })
    }

    pub fn write(&self, parent: &Path, include: bool) -> io::Result<PathBuf> {
        let parent = parent.canonicalize()?;
        if parent.starts_with(&self.root) {
            return Err(io::Error::other("Choose a destination outside this board."));
        }
        if Self::preview(&self.root)? != *self {
            return Err(io::Error::other(
                "Board contents changed. Reopen the export preview.",
            ));
        }
        let stage = tempfile::Builder::new()
            .prefix("corktop-board-")
            .tempdir_in(parent)?;
        for (rel, entry) in &self.entries {
            let dest = stage.path().join(rel);
            match entry {
                Entry::Skip => (),
                Entry::Directory => fs::create_dir_all(dest)?,
                Entry::Link(raw) if include && Path::new(raw).is_absolute() => {
                    let source = resolve_link(&self.root, raw)
                        .map_err(|e| io::Error::other(format!("{raw}: {e}")))?;
                    let dir = rel.parent().unwrap();
                    let board = if dir.ends_with(".trash") {
                        dir.parent().unwrap()
                    } else {
                        dir
                    };
                    let store = stage.path().join(board).join(".corktop/linked");
                    fs::create_dir_all(&store)?;
                    let group = tempfile::Builder::new()
                        .prefix("file-")
                        .tempdir_in(&store)?
                        .keep();
                    let bundled = group.join(source.file_name().unwrap());
                    copy_file(&source, &bundled)?;
                    let reference = bundled.strip_prefix(stage.path().join(board)).unwrap();
                    fs::write(
                        dest,
                        reference
                            .to_string_lossy()
                            .replace(std::path::MAIN_SEPARATOR, "/"),
                    )?;
                }
                _ => copy_file(&self.root.join(rel), &dest)?,
            }
        }
        Ok(stage.keep())
    }
}

fn copy_file(source: &Path, dest: &Path) -> io::Result<()> {
    let mut input = nofollow_options().read(true).open(source)?;
    if !input.metadata()?.is_file() {
        return Err(io::Error::other("source file changed during export"));
    }
    let mut out = fs::File::create_new(dest)?;
    io::copy(&mut input, &mut out)?;
    out.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_link_survives_relocation_and_source_removal() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let board = temp.path().join("original");
        fs::create_dir(&board)?;
        let target = temp.path().join("sample.txt");
        fs::write(&target, "evidence")?;
        fs::write(board.join("item.corklink"), target.to_str().unwrap())?;
        fs::write(board.join(".board.toml"), "schema_version = 1")?;
        let plan = ExportPlan::preview(&board)?;
        assert!(plan.write(&board, true).is_err());
        let plain = plan.write(temp.path(), false)?;
        assert_eq!(
            fs::read(plain.join("item.corklink"))?,
            fs::read(board.join("item.corklink"))?
        );
        let dest = temp.path().join("relocated");
        fs::rename(plan.write(temp.path(), true)?, &dest)?;
        fs::remove_file(target)?;
        let raw = fs::read_to_string(dest.join("item.corklink"))?;
        assert_eq!(fs::read_to_string(resolve_link(&dest, &raw)?)?, "evidence");
        assert!(plan.write(temp.path(), true).is_err());
        assert!(resolve_link(&dest, ".corktop/linked/../../sample.txt").is_err());
        Ok(())
    }
}
