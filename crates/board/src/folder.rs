use super::{MANIFEST, MANIFEST_VERSION, nofollow_options, read_manifest, read_text, save};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
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
    pub position: [i32; 2],
}

pub struct Board {
    root: PathBuf,
    lock: File,
    manifest: Table,
    cards: Vec<Card>,
    strings: Vec<[usize; 2]>,
}

impl Drop for Board {
    fn drop(&mut self) {
        // A concurrently spawned process may briefly share the file description before exec.
        let _ = self.lock.unlock();
    }
}

fn valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let device = stem
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    !stem.is_empty()
        && !stem.starts_with('.')
        && name.len() <= 240
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" ._-".contains(&b))
        && !matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(device.len() == 4
            && matches!(&device[..3], "COM" | "LPT")
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
            .filter(|p| p.len() == 2)
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

impl Board {
    pub fn open(root: &Path) -> io::Result<Self> {
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
        if !has_manifest && has_other_files {
            return Err(invalid("choose an empty folder or an existing board"));
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
                cards.push(Card {
                    file: file.into(),
                    text: read_text(&root.join(file), NOTE_LIMIT as u64)?,
                    position: [position(entry.get("x"))?, position(entry.get("y"))?],
                });
            }
        }
        let strings = load_strings(&manifest, &cards)?;
        Ok(Self {
            strings,
            root,
            lock,
            manifest,
            cards,
        })
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    pub fn strings(&self) -> &[[usize; 2]] {
        &self.strings
    }

    fn save_strings(&mut self, strings: Vec<[usize; 2]>) -> io::Result<()> {
        self.check_manifest()?;
        let mut updated = self.manifest.clone();
        let entries = strings
            .iter()
            .map(|pair| {
                Value::Array(
                    pair.iter()
                        .map(|&i| Value::String(self.cards[i].file.clone()))
                        .collect(),
                )
            })
            .collect();
        updated.insert("strings".into(), Value::Array(entries));
        save(&self.root, &updated)?;
        self.manifest = updated;
        self.strings = strings;
        Ok(())
    }

    pub fn connect(&mut self, a: usize, b: usize) -> io::Result<()> {
        if a == b || a >= self.cards.len() || b >= self.cards.len() {
            return Err(invalid("choose two different cards"));
        }
        let pair = [a.min(b), a.max(b)];
        if self.strings.contains(&pair) {
            return Ok(());
        }
        let mut strings = self.strings.clone();
        strings.push(pair);
        self.save_strings(strings)
    }

    pub fn cut(&mut self, a: usize, b: usize) -> io::Result<()> {
        let pair = [a.min(b), a.max(b)];
        let Some(index) = self.strings.iter().position(|ends| ends == &pair) else {
            return Ok(());
        };
        let mut strings = self.strings.clone();
        strings.remove(index);
        self.save_strings(strings)
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
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let names = fs::read_dir(&self.root)?
            .map(|e| e.map(|f| f.file_name().to_string_lossy().to_ascii_lowercase()))
            .collect::<io::Result<HashSet<_>>>()?;
        let mut pending = tempfile::NamedTempFile::new_in(&self.root)?;
        pending.write_all(text.as_bytes())?;
        pending.as_file().sync_all()?;
        for number in 1u32.. {
            let suffix = if number == 1 {
                String::new()
            } else {
                format!("_{number}")
            };
            let file = format!("{stamp}_{title}{suffix}.md");
            if names.contains(&file) {
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
            self.cards.push(Card {
                file,
                text: text.into(),
                position: pos,
            });
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
        save(&self.root, &updated)?;
        self.manifest = updated;
        self.cards[index].position = pos;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

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
