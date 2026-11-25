mod pack;
mod recycle;
mod spatial;
use super::{MANIFEST, MANIFEST_VERSION, nofollow_options, read_manifest, read_text, save};
pub use spatial::Geometry;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use toml::{Table, Value};

const BOARD_LOCK: &str = ".corktop.lock";
const NOTE_LIMIT: usize = 64 * 1024;
const MAX_CARDS: usize = 512;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Debug)]
pub struct Card {
    pub file: String,
    pub text: String,
    pub title: Option<String>,
    pub missing: bool,
    pub folder: bool,
    revision: Option<(SystemTime, u64)>,
    pub position: [i32; 2],
}

pub struct Board {
    root: PathBuf,
    lock: File,
    manifest: Table,
    cards: Vec<Card>,
    strings: Vec<[usize; 2]>,
    inbox: Vec<String>,
    geometry: Geometry,
    history: std::collections::VecDeque<(Table, Table)>,
}

impl Drop for Board {
    fn drop(&mut self) {
        // A concurrently spawned process may briefly share the file description before exec.
        let _ = self.lock.unlock();
    }
}

fn valid_name(name: &str) -> bool {
    let device = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && name.len() <= 240
        && !name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        && !matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && device.as_bytes()[3].is_ascii_digit())
}

fn position(value: Option<&Value>) -> io::Result<i32> {
    value
        .and_then(Value::as_integer)
        .and_then(|v| i32::try_from(v).ok())
        .filter(|v| (0..=1_000_000).contains(v))
        .ok_or_else(|| invalid("invalid card position"))
}

fn valid_position(pos: [i32; 2]) -> io::Result<()> {
    if pos.iter().all(|v| (0..=1_000_000).contains(v)) {
        Ok(())
    } else {
        Err(invalid("invalid card position"))
    }
}

// Card indices are local to an open board. Only validated filenames go on disk.
fn load_strings(manifest: &Table, cards: &[Card]) -> io::Result<Vec<[usize; 2]>> {
    let Some(value) = manifest.get("strings") else {
        return Ok(Vec::new());
    };
    let entries = value.as_array().ok_or_else(|| invalid("invalid strings"))?;
    let mut strings = Vec::with_capacity(entries.len());
    let mut seen = HashSet::new();
    for entry in entries {
        let pair = entry
            .as_array()
            .filter(|p| {
                p.len() == 2
                    || (p.len() == 3 && p[2].as_integer().is_some_and(|v| (0..6).contains(&v)))
            })
            .ok_or_else(|| invalid("invalid string endpoints"))?;
        let endpoint = |v: &Value| {
            cards
                .iter()
                .position(|card| Some(card.file.as_str()) == v.as_str())
                .ok_or_else(|| invalid("unknown string endpoint"))
        };
        let mut ends = [endpoint(&pair[0])?, endpoint(&pair[1])?];
        ends.sort_unstable();
        if ends[0] == ends[1] || !seen.insert(ends) {
            return Err(invalid("invalid or duplicate string"));
        }
        strings.push(ends);
    }
    Ok(strings)
}

fn is_text(file: &str) -> bool {
    file.rsplit_once('.')
        .is_some_and(|(_, ext)| ["md", "txt"].iter().any(|s| ext.eq_ignore_ascii_case(s)))
}

impl Card {
    fn load(root: &Path, file: &str, position: [i32; 2]) -> io::Result<Self> {
        let mut card = Self {
            file: file.into(),
            position,
            text: String::new(),
            title: None,
            missing: true,
            folder: false,
            revision: None,
        };
        card.refresh(root)?;
        Ok(card)
    }

    pub fn revision(&self) -> Option<(SystemTime, u64)> {
        self.revision
    }

    fn refresh(&mut self, root: &Path) -> io::Result<()> {
        let path = root.join(&self.file);
        let meta = match path.symlink_metadata() {
            Ok(meta) if meta.is_file() || meta.is_dir() => meta,
            Ok(_) => return Err(invalid("expected a regular note file")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.missing = true;
                self.revision = None;
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let revision = meta.modified().ok().map(|mtime| (mtime, meta.len()));
        if self.missing || revision.is_none() || self.revision != revision {
            self.folder = meta.is_dir();
            self.text = if self.folder {
                let mut count = 0;
                for entry in fs::read_dir(&path)? {
                    let entry = entry?;
                    let kind = entry.file_type()?;
                    if (kind.is_file() || kind.is_dir())
                        && entry.file_name().to_str().is_some_and(valid_name)
                    {
                        count += 1;
                    }
                }
                format!("{}\n{count} items", self.file)
            } else if is_text(&self.file) {
                use std::io::Read;
                let mut bytes = Vec::new();
                nofollow_options()
                    .read(true)
                    .open(&path)?
                    .take(4096)
                    .read_to_end(&mut bytes)?;
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                self.file.clone()
            };
            self.revision = revision;
            self.missing = false;
        }
        Ok(())
    }
}

impl Board {
    pub fn open(root: &Path) -> io::Result<Self> {
        Self::open_folder(root, false)
    }

    pub fn stage_folder(root: &Path) -> io::Result<Self> {
        Self::open_folder(root, true)
    }

    fn open_folder(root: &Path, stage: bool) -> io::Result<Self> {
        if !root.symlink_metadata()?.is_dir() {
            return Err(invalid("select a board directory"));
        }
        let root = root.canonicalize()?;
        let mut names = HashSet::new();
        let mut has_manifest = false;
        let mut has_other_files = false;
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let folded = name.to_ascii_lowercase();
            if folded == MANIFEST || folded == BOARD_LOCK {
                if folded != name || !entry.file_type()?.is_file() {
                    return Err(invalid("invalid board metadata file"));
                }
                has_manifest |= folded == MANIFEST;
            } else {
                has_other_files = true;
            }
            if !names.insert(folded) {
                return Err(invalid("ambiguous board filenames"));
            }
        }
        if !has_manifest && has_other_files && !stage {
            return Err(io::Error::new(
                io::ErrorKind::DirectoryNotEmpty,
                "choose how to handle the existing files",
            ));
        }
        let lock = nofollow_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(BOARD_LOCK))?;
        lock.try_lock()
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "board is already open"))?;
        let manifest = match read_manifest(&root) {
            Ok(table) => table,
            Err(e) if !has_manifest && e.kind() == io::ErrorKind::NotFound => {
                let mut table = Table::new();
                table.insert("schema_version".into(), MANIFEST_VERSION.into());
                table.insert("cards".into(), Value::Array(Vec::new()));
                save(&root, &table)?;
                table
            }
            Err(e) => return Err(e),
        };
        if manifest.get("schema_version").and_then(Value::as_integer)
            != Some(i64::from(MANIFEST_VERSION))
        {
            return Err(invalid("unsupported board version"));
        }
        let mut cards = Vec::new();
        let mut seen = HashSet::new();
        if let Some(entries) = manifest.get("cards") {
            let entries = entries.as_array().ok_or_else(|| invalid("invalid cards"))?;
            if entries.len() > MAX_CARDS {
                return Err(invalid("too many cards"));
            }
            for entry in entries {
                let file = entry
                    .get("file")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("missing card filename"))?;
                if !valid_name(file) || !seen.insert(file.to_ascii_lowercase()) {
                    return Err(invalid("invalid or duplicate card filename"));
                }
                let mut card = Card::load(
                    &root,
                    file,
                    [position(entry.get("x"))?, position(entry.get("y"))?],
                )?;
                card.title = entry
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                cards.push(card);
            }
        }
        let strings = load_strings(&manifest, &cards)?;
        let mut board = Self {
            inbox: Vec::new(),
            geometry: Geometry::load(&manifest)?,
            history: Default::default(),
            strings,
            root,
            lock,
            manifest,
            cards,
        };
        board.scan_inbox()?;
        Ok(board)
    }

    fn scan_inbox(&mut self) -> io::Result<()> {
        let known: HashSet<_> = self
            .cards
            .iter()
            .map(|c| c.file.to_ascii_lowercase())
            .collect();
        let mut inbox = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if let Ok(name) = entry.file_name().into_string()
                && valid_name(&name)
                && {
                    let kind = entry.file_type()?;
                    kind.is_file() || kind.is_dir()
                }
                && !known.contains(&name.to_ascii_lowercase())
            {
                inbox.push(name);
            }
        }
        inbox.sort();
        self.inbox = inbox;
        Ok(())
    }

    pub fn inbox(&self) -> &[String] {
        &self.inbox
    }

    pub fn pin_files(&mut self, files: &[String]) -> io::Result<()> {
        if files.len() + self.cards.len() > MAX_CARDS {
            return Err(invalid("board card limit reached"));
        }
        self.check_manifest()?;
        let mut updated = self.manifest.clone();
        let entries = updated
            .entry("cards")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| invalid("invalid cards"))?;
        let mut added = Vec::new();
        let mut seen = HashSet::new();
        for name in files {
            if !self.inbox.contains(name) || !seen.insert(name.to_ascii_lowercase()) {
                return Err(invalid("item unavailable in inbox"));
            }
            let i = entries.len() as i32;
            let pos = [16 + 236 * (i % 3), 16 + 128 * (i / 3)];
            let card = Card::load(&self.root, name, pos)?;
            if card.missing {
                return Err(invalid("item disappeared from inbox"));
            }
            entries.push(Value::Table(Table::from_iter([
                ("file".into(), name.clone().into()),
                ("x".into(), pos[0].into()),
                ("y".into(), pos[1].into()),
            ])));
            added.push(card);
        }
        save(&self.root, &updated)?;
        self.manifest = updated;
        self.cards.extend(added);
        self.inbox
            .retain(|name| !seen.contains(&name.to_ascii_lowercase()));
        Ok(())
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        for card in &mut self.cards {
            card.refresh(&self.root)?;
        }
        self.scan_inbox()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn child_path(&self, index: usize) -> io::Result<PathBuf> {
        let card = self
            .cards
            .get(index)
            .ok_or_else(|| invalid("unknown folder card"))?;
        let path = self.root.join(&card.file);
        if !card.folder || !path.symlink_metadata()?.is_dir() {
            return Err(invalid("folder is unavailable"));
        }
        Ok(path)
    }

    pub fn note_path(&self, index: usize) -> io::Result<PathBuf> {
        let card = self
            .cards
            .get(index)
            .ok_or_else(|| invalid("unknown card"))?;
        let path = self.root.join(&card.file);
        if !path.symlink_metadata()?.is_file() {
            return Err(invalid("expected a regular note file"));
        }
        Ok(path)
    }

    pub fn open_target(&self, index: usize) -> io::Result<PathBuf> {
        let path = self.note_path(index)?;
        match path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("url") => {
                let raw = read_text(&path, NOTE_LIMIT as u64)?;
                let value = raw
                    .lines()
                    .find_map(|l| l.strip_prefix("URL="))
                    .ok_or_else(|| invalid("invalid bookmark"))?;
                let url = url::Url::parse(value).map_err(|_| invalid("invalid bookmark URL"))?;
                if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
                    return Err(invalid("unsupported bookmark URL"));
                }
                Ok(PathBuf::from(url.as_str()))
            }
            Some("corklink") => {
                let target = PathBuf::from(read_text(&path, NOTE_LIMIT as u64)?);
                if !target.is_absolute() || !target.symlink_metadata()?.is_file() {
                    return Err(invalid("linked file unavailable"));
                }
                Ok(target)
            }
            _ => Ok(path),
        }
    }

    pub fn note_text(&self, index: usize) -> io::Result<String> {
        if self.cards.get(index).is_some_and(|c| c.folder) {
            return Ok(self.child_path(index)?.to_string_lossy().into_owned());
        }
        let path = self.note_path(index)?;
        if is_text(&self.cards[index].file) {
            read_text(&path, NOTE_LIMIT as u64)
        } else {
            Ok(self.open_target(index)?.to_string_lossy().into_owned())
        }
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    pub fn strings(&self) -> &[[usize; 2]] {
        &self.strings
    }

    pub fn thread_color(&self, index: usize) -> usize {
        self.manifest
            .get("strings")
            .and_then(Value::as_array)
            .and_then(|v| v.get(index))
            .and_then(Value::as_array)
            .and_then(|p| p.get(2))
            .and_then(Value::as_integer)
            .unwrap_or(0) as usize
    }

    pub fn connect(&mut self, a: usize, b: usize) -> io::Result<()> {
        self.connect_colored(a, b, 0)
    }

    pub fn connect_colored(&mut self, a: usize, b: usize, color: u8) -> io::Result<()> {
        if color >= 6 {
            return Err(invalid("invalid thread color"));
        }
        if a == b || a >= self.cards.len() || b >= self.cards.len() {
            return Err(invalid("choose two different cards"));
        }
        let pair = [a.min(b), a.max(b)];
        if self.strings.contains(&pair) {
            return Ok(());
        }
        let mut next = self.manifest.clone();
        let mut ends = pair
            .map(|i| Value::String(self.cards[i].file.clone()))
            .to_vec();
        if color != 0 {
            ends.push(i64::from(color).into());
        }
        next.entry("strings")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .unwrap()
            .push(Value::Array(ends));
        self.commit_layout(next)
    }

    pub fn cut(&mut self, a: usize, b: usize) -> io::Result<()> {
        let pair = [a.min(b), a.max(b)];
        let Some(index) = self.strings.iter().position(|ends| ends == &pair) else {
            return Ok(());
        };
        let mut next = self.manifest.clone();
        next["strings"].as_array_mut().unwrap().remove(index);
        self.commit_layout(next)
    }

    fn check_manifest(&self) -> io::Result<()> {
        if read_manifest(&self.root)? != self.manifest {
            return Err(invalid("board changed on disk; reopen it"));
        }
        Ok(())
    }

    pub fn create(&mut self, text: &str, pos: [i32; 2]) -> io::Result<()> {
        valid_position(pos)?;
        if text.trim().is_empty() || text.len() > NOTE_LIMIT || self.cards.len() >= MAX_CARDS {
            return Err(invalid("note is empty or board limit reached"));
        }
        self.check_manifest()?;
        let title = text
            .lines()
            .next()
            .unwrap_or("")
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
            .to_ascii_lowercase();
        let title = if title.is_empty() {
            "note"
        } else {
            &title[..title.len().min(64)]
        };
        self.store_payload(title, ".md", text.as_bytes(), pos)
    }

    pub fn paste_text(&mut self, text: &str, pos: [i32; 2]) -> io::Result<()> {
        if !text.trim().chars().any(char::is_whitespace)
            && let Ok(url) = url::Url::parse(text.trim())
            && matches!(url.scheme(), "https" | "http")
            && let Some(host) = url.host_str()
        {
            if text.len() > NOTE_LIMIT {
                return Err(invalid("bookmark exceeds size limit"));
            }
            let body = format!("[InternetShortcut]\nURL={}\n", url.as_str());
            self.store_payload(host, ".url", body.as_bytes(), pos)
        } else {
            self.create(text, pos)
        }
    }

    pub fn import_file(
        &mut self,
        source: &Path,
        pos: [i32; 2],
        link: bool,
        move_source: bool,
    ) -> io::Result<()> {
        if !source.symlink_metadata()?.is_file() {
            return Err(invalid("select a regular file"));
        }
        let source = source.canonicalize()?;
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| valid_name(n))
            .ok_or_else(|| invalid("nonportable filename"))?;
        if link {
            let target = source
                .to_str()
                .ok_or_else(|| invalid("nonportable link target"))?;
            self.store_payload(name, ".corklink", target.as_bytes(), pos)
        } else {
            if move_source && source.parent() == Some(&self.root) {
                return Err(invalid("use Inbox to pin this file"));
            }
            let (stem, suffix) = name.rfind('.').map_or((name, ""), |i| name.split_at(i));
            let mut input = nofollow_options().read(true).open(&source)?;
            self.store_payload(stem, suffix, &mut input, pos)?;
            if move_source {
                fs::remove_file(&source)?;
            }
            Ok(())
        }
    }

    pub fn store_payload(
        &mut self,
        title: &str,
        ext: &str,
        mut input: impl io::Read,
        pos: [i32; 2],
    ) -> io::Result<()> {
        valid_position(pos)?;
        if self.cards.len() >= MAX_CARDS {
            return Err(invalid("board card limit reached"));
        }
        self.check_manifest()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let names = fs::read_dir(&self.root)?
            .map(|e| e.map(|f| f.file_name().to_string_lossy().to_ascii_lowercase()))
            .collect::<io::Result<HashSet<_>>>()?;
        let mut pending = tempfile::NamedTempFile::new_in(&self.root)?;
        io::copy(&mut input, &mut pending)?;
        pending.as_file().sync_all()?;
        for number in 1u32.. {
            let suffix = if number == 1 {
                String::new()
            } else {
                format!("_{number}")
            };
            let file = format!("{stamp}_{title}{suffix}{ext}");
            if !valid_name(&file) {
                return Err(invalid("invalid payload name"));
            }
            if names.contains(&file.to_ascii_lowercase())
                || self
                    .cards
                    .iter()
                    .any(|c| c.file.eq_ignore_ascii_case(&file))
            {
                continue;
            }
            let path = self.root.join(&file);
            match pending.persist_noclobber(&path) {
                Ok(_) => (),
                Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
                    pending = e.file;
                    continue;
                }
                Err(e) => return Err(e.error),
            }
            let card = match Card::load(&self.root, &file, pos) {
                Ok(card) => card,
                Err(error) => {
                    fs::remove_file(path)?;
                    return Err(error);
                }
            };
            let mut updated = self.manifest.clone();
            let mut entry = Table::new();
            entry.insert("file".into(), file.clone().into());
            entry.insert("x".into(), pos[0].into());
            entry.insert("y".into(), pos[1].into());
            updated
                .entry("cards")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .unwrap()
                .push(entry.into());
            if let Err(error) = save(&self.root, &updated) {
                fs::remove_file(path)?;
                return Err(error);
            }
            self.manifest = updated;
            self.cards.push(card);
            return Ok(());
        }
        Err(invalid("no available card filename"))
    }

    pub fn move_card(&mut self, index: usize, pos: [i32; 2]) -> io::Result<()> {
        valid_position(pos)?;
        let card = self
            .cards
            .get(index)
            .ok_or_else(|| invalid("unknown card"))?;
        if card.position == pos {
            return Ok(());
        }
        self.check_manifest()?;
        let mut updated = self.manifest.clone();
        let entry = &mut updated["cards"].as_array_mut().unwrap()[index];
        entry["x"] = pos[0].into();
        entry["y"] = pos[1].into();
        self.commit_layout(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn restore_link_when_both_endpoints_were_recycled_and_only_purge_tracked_files() {
        let root = tempfile::tempdir().unwrap();
        let mut board = Board::open(root.path()).unwrap();
        board.create("one", [1, 2]).unwrap();
        board.create("two", [3, 4]).unwrap();
        board.connect(0, 1).unwrap();
        board.remove_card(0).unwrap();
        board.remove_card(0).unwrap();
        let names = board.trashed_files();
        board.restore(&names[0]).unwrap();
        board.restore(&names[1]).unwrap();
        assert_eq!(board.strings(), [[0, 1]]);
        board.remove_card(0).unwrap();
        let unrelated = root.path().join(".trash/untracked");
        fs::write(&unrelated, "leave this").unwrap();
        board.empty_trash().unwrap();
        assert!(board.trashed_files().is_empty());
        assert!(unrelated.is_file());
        assert!(!root.path().join(".trash").join(&names[0]).exists());
    }

    #[test]
    fn folder_transfer_preserves_payload_and_title_and_only_unlinks_local_strands() {
        let root = tempfile::tempdir().unwrap();
        let mut board = Board::open(root.path()).unwrap();
        board.create("First", [12, 34]).unwrap();
        board.create("Second", [32, 42]).unwrap();
        fs::create_dir(root.path().join("Archive")).unwrap();
        board.refresh().unwrap();
        board.pin_files(&["Archive".into()]).unwrap();
        board.connect(0, 1).unwrap();
        board.connect(1, 2).unwrap();
        board.edit_title(0, "Revised title", false).unwrap();
        let name = board.cards()[0].file.clone();
        assert!(name.ends_with("_revised-title.md"));
        let child = Board::open(&board.child_path(2).unwrap()).unwrap();
        assert!(board.move_into_folder(0, 2).is_err());
        assert_eq!(board.cards().len(), 3);
        drop(child);
        board.move_into_folder(0, 2).unwrap();
        drop(board);
        let board = Board::open(root.path()).unwrap();
        assert_eq!(board.strings(), [[0, 1]]);
        assert!(!root.path().join(&name).exists());
        let child = Board::open(&board.child_path(1).unwrap()).unwrap();
        assert_eq!(child.cards()[0].file, name);
        assert_eq!(child.cards()[0].title.as_deref(), Some("Revised title"));
        assert_eq!(child.note_text(0).unwrap(), "First");
    }

    #[test]
    fn portable_preferences_survive_reopen_and_preserve_unrelated_keys() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = crate::Preferences::load(root.path()).unwrap();
        assert!(!settings.flag("link_file_drops", false));
        settings.set("link_file_drops", true.into()).unwrap();
        let mut second = crate::Preferences::load(root.path()).unwrap();
        second.set("display_only_titles", true.into()).unwrap();
        settings.set("tutorial_seen", true.into()).unwrap();
        let restored = crate::Preferences::load(root.path()).unwrap();
        for key in ["tutorial_seen", "link_file_drops", "display_only_titles"] {
            assert!(restored.flag(key, false));
        }
    }

    #[test]
    fn trash_restore_keeps_contents_position_and_string_and_protects_collisions() {
        let root = tempfile::tempdir().unwrap();
        let mut board = Board::open(root.path()).unwrap();
        board.create("first", [12, 34]).unwrap();
        board.create("second", [22, 44]).unwrap();
        board.connect(0, 1).unwrap();
        let name = board.cards()[0].file.clone();
        board.remove_card(0).unwrap();
        drop(board);
        let mut board = Board::open(root.path()).unwrap();
        assert_eq!(
            board.trashed_files().as_slice(),
            std::slice::from_ref(&name)
        );
        assert!(board.strings().is_empty());
        let file = root.path().join(&name);
        fs::write(&file, "occupied").unwrap();
        assert!(board.restore(&name).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "occupied");
        fs::remove_file(file).unwrap();
        board.restore(&name).unwrap();
        drop(board);
        let board = Board::open(root.path()).unwrap();
        assert_eq!(board.note_text(1).unwrap(), "first");
        assert_eq!(board.cards()[1].position, [12, 34]);
        assert_eq!(board.strings(), [[0, 1]]);
    }

    #[test]
    fn imported_files_links_and_bookmarks_round_trip() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let file = external.path().join("An image.png");
        fs::write(&file, [0, 255, 1]).unwrap();
        let mut board = Board::open(root.path()).unwrap();
        board.import_file(&file, [1, 2], false, false).unwrap();
        board.import_file(&file, [3, 4], false, false).unwrap();
        assert_ne!(board.cards()[0].file, board.cards()[1].file);
        assert_eq!(fs::read(board.note_path(1).unwrap()).unwrap(), [0, 255, 1]);
        board.import_file(&file, [5, 6], true, false).unwrap();
        board
            .paste_text("https://example.org/a?q=1", [7, 8])
            .unwrap();
        drop(board);
        let mut board = Board::open(root.path()).unwrap();
        assert_eq!(board.open_target(2).unwrap(), file.canonicalize().unwrap());
        assert_eq!(board.note_text(3).unwrap(), "https://example.org/a?q=1");
        board.import_file(&file, [9, 10], false, true).unwrap();
        assert!(!file.exists());
        assert!(board.open_target(2).is_err());
        assert_eq!(fs::read(board.note_path(4).unwrap()).unwrap(), [0, 255, 1]);
    }

    #[test]
    fn stage_and_pin_existing_files_without_touching_the_payloads() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Report.pdf"), [0, 255, 1]).unwrap();
        fs::write(root.path().join("Notes.md"), "external text").unwrap();
        fs::write(root.path().join(".private"), "ignored").unwrap();
        assert!(Board::open(root.path()).is_err());
        assert!(!root.path().join(MANIFEST).exists());
        let mut board = Board::stage_folder(root.path()).unwrap();
        assert_eq!(board.inbox(), ["Notes.md", "Report.pdf"]);
        let files = board.inbox().to_vec();
        board.pin_files(&files).unwrap();
        assert_eq!(board.cards()[0].text, "external text");
        assert!(board.inbox().is_empty());
        drop(board);
        let mut board = Board::open(root.path()).unwrap();
        assert_eq!(board.cards().len(), 2);
        assert_eq!(
            fs::read(root.path().join("Report.pdf")).unwrap(),
            [0, 255, 1]
        );
        fs::write(root.path().join("New.txt"), "new").unwrap();
        board.refresh().unwrap();
        assert_eq!(board.inbox(), ["New.txt"]);
    }

    #[test]
    fn external_edit_removal_and_reappearance_preserve_layout_and_links() {
        let root = tempfile::tempdir().unwrap();
        let mut board = Board::open(root.path()).unwrap();
        board.create("first", [7, 9]).unwrap();
        board.create("second", [8, 10]).unwrap();
        board.connect(0, 1).unwrap();
        let file = board.note_path(0).unwrap();
        let manifest = fs::read(root.path().join(MANIFEST)).unwrap();
        fs::write(&file, "externally edited").unwrap();
        board.refresh().unwrap();
        assert_eq!(board.cards()[0].text, "externally edited");
        fs::remove_file(&file).unwrap();
        board.refresh().unwrap();
        assert!(board.cards()[0].missing);
        drop(board);
        let mut board = Board::open(root.path()).unwrap();
        assert!(board.cards()[0].missing);
        fs::write(&file, "restored").unwrap();
        board.refresh().unwrap();
        assert!(!board.cards()[0].missing);
        assert_eq!(board.cards()[0].text, "restored");
        assert_eq!(board.cards()[0].position, [7, 9]);
        assert_eq!(board.strings(), &[[0, 1]]);
        assert_eq!(manifest, fs::read(root.path().join(MANIFEST)).unwrap());
    }

    #[test]
    fn markdown_round_trip_and_exclusive_writer() {
        let root = tempfile::tempdir().unwrap();
        let mut board = Board::open(root.path()).unwrap();
        assert!(Board::open(root.path()).is_err());
        let text = "Field note\n\nA **red** thread.";
        board.create(text, [12, 34]).unwrap();
        board.create(text, [0, 0]).unwrap();
        assert_ne!(board.cards()[0].file, board.cards()[1].file);
        assert_eq!(
            fs::read_to_string(root.path().join(&board.cards()[0].file)).unwrap(),
            text
        );
        board.move_card(0, [86, 105]).unwrap();
        drop(board);
        let board = Board::open(root.path()).unwrap();
        assert_eq!(board.cards()[0].position, [86, 105]);
        assert_eq!(board.cards()[0].text, text);
        assert_eq!(board.cards()[1].position, [0, 0]);
    }

    #[test]
    fn interrupted_move() {
        if let Some(root) = std::env::var_os("CORKTOP_TEST_INTERRUPT_ROOT") {
            let mut board = Board::open(Path::new(&root)).unwrap();
            board.move_card(0, [88, 99]).unwrap();
            panic!("save returned despite interruption");
        }
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().canonicalize().unwrap();
        let mut board = Board::open(&root).unwrap();
        board.create("Case 37\nMarker survives.", [7, 8]).unwrap();
        drop(board);
        let initial = fs::read(root.join(MANIFEST)).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "folder::tests::interrupted_move"])
            .env("CORKTOP_TEST_INTERRUPT_ROOT", &root)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(73));
        assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), initial);
        let mut reopened = Board::open(&root).unwrap();
        assert_eq!(reopened.cards()[0].text, "Case 37\nMarker survives.");
        assert_eq!(reopened.cards()[0].position, [7, 8]);
        reopened.move_card(0, [88, 99]).unwrap();
        drop(reopened);
        assert_eq!(Board::open(&root).unwrap().cards()[0].position, [88, 99]);
    }

    #[test]
    fn strings_use_board_filenames_and_preserve_cards() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        fs::write(root.join("first.md"), "First").unwrap();
        fs::write(root.join("second.md"), "Second").unwrap();
        fs::write(root.join(MANIFEST), "schema_version=1\n[[cards]]\nfile='first.md'\nx=1\ny=2\n[[cards]]\nfile='second.md'\nx=3\ny=4").unwrap();
        let mut board = Board::open(root).unwrap();
        assert!(board.strings().is_empty());
        assert!(board.connect(0, 0).is_err());
        board.connect(1, 0).unwrap();
        board.connect(0, 1).unwrap();
        board.create("Third", [7, 8]).unwrap();
        board.connect(1, 2).unwrap();
        board.move_card(1, [34, 56]).unwrap();
        drop(board);
        let mut table = read_manifest(root).unwrap();
        assert_eq!(table["strings"][0][0].as_str(), Some("first.md"));
        assert_eq!(table["strings"][0][1].as_str(), Some("second.md"));
        // Reordering the cards on disk must not retarget their link.
        table["cards"].as_array_mut().unwrap().reverse();
        save(root, &table).unwrap();
        let mut board = Board::open(root).unwrap();
        assert_eq!(board.strings(), &[[1, 2], [0, 1]]);
        board.cut(2, 1).unwrap();
        drop(board);
        let board = Board::open(root).unwrap();
        assert_eq!(board.strings(), &[[0, 1]]);
        assert_eq!(board.cards()[0].text, "Third");
        assert_eq!(board.cards()[1].text, "Second");
        assert_eq!(board.cards()[1].position, [34, 56]);
        assert_eq!(board.cards()[2].text, "First");
        assert_eq!(board.cards()[2].position, [1, 2]);
        drop(board);
        for bad in ["../first.md", "missing.md", "first.md"] {
            table.insert(
                "strings".into(),
                Value::Array(vec![Value::Array(vec!["first.md".into(), bad.into()])]),
            );
            save(root, &table).unwrap();
            assert!(Board::open(root).is_err());
            assert_eq!(read_manifest(root).unwrap(), table);
        }
    }

    #[test]
    fn interrupted_string_updates() {
        if let Some(root) = std::env::var_os("CORKTOP_TEST_INTERRUPT_ROOT") {
            let mut board = Board::open(Path::new(&root)).unwrap();
            if board.strings().is_empty() {
                board.connect(0, 1).unwrap();
            } else {
                board.cut(0, 1).unwrap();
            }
            panic!("save returned despite interruption");
        }
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().canonicalize().unwrap();
        let mut board = Board::open(&root).unwrap();
        board.create("Alpha", [10, 20]).unwrap();
        board.create("Beta", [30, 40]).unwrap();
        for expected_count in [0, 1] {
            drop(board);
            let before = fs::read(root.join(MANIFEST)).unwrap();
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "folder::tests::interrupted_string_updates"])
                .env("CORKTOP_TEST_INTERRUPT_ROOT", &root)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(73));
            assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), before);
            board = Board::open(&root).unwrap();
            assert_eq!(board.strings().len(), expected_count);
            assert_eq!(board.cards()[0].text, "Alpha");
            assert_eq!(board.cards()[1].text, "Beta");
            board.connect(0, 1).unwrap();
        }
        board.cut(0, 1).unwrap();
    }

    #[test]
    fn rejects_unsafe_references_and_preserves_manifest() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join(MANIFEST);
        for name in [
            "../note.md",
            "/note.md",
            "C:/note.md",
            ".hidden.md",
            "aux.md",
            "COM1.any.md",
            "folder/note.md",
        ] {
            let contents = format!("schema_version=1\n[[cards]]\nfile='{name}'\nx=1\ny=1");
            fs::write(&manifest, &contents).unwrap();
            assert!(Board::open(root.path()).is_err());
            assert_eq!(fs::read_to_string(&manifest).unwrap(), contents);
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_note_and_case_collision() {
        let root = tempfile::tempdir().unwrap();
        let mut board = Board::open(root.path()).unwrap();
        board.create("Title", [0, 0]).unwrap();
        let file = board.cards()[0].file.clone();
        drop(board);
        fs::write(root.path().join(file.to_uppercase()), "unrelated").unwrap();
        assert!(Board::open(root.path()).is_err());
        fs::remove_file(root.path().join(file.to_uppercase())).unwrap();
        fs::remove_file(root.path().join(&file)).unwrap();
        std::os::unix::fs::symlink(root.path().join(MANIFEST), root.path().join(file)).unwrap();
        assert!(Board::open(root.path()).is_err());
    }
}
