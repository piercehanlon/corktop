use super::*;

pub struct ListEntry {
    pub board: PathBuf,
    pub file: String,
    pub title: String,
    pub snippet: String,
    pub kind: String,
    pub zone: String,
    pub folder: bool,
    pub tone: Option<u8>,
}

fn list(root: &Path, cards: &[Card], geometry: &Geometry) -> Vec<ListEntry> {
    cards
        .iter()
        .map(|card| {
            let title = card.display_title().chars().take(120).collect();
            let zone = geometry
                .zones
                .iter()
                .enumerate()
                .find(|(i, _)| {
                    geometry.contains(*i, [card.position[0] + 110, card.position[1] + 56])
                })
                .map(|(_, z)| z.title.clone())
                .unwrap_or_default();
            let kind = if card.missing {
                "Missing".into()
            } else if card.folder {
                "Folder".into()
            } else {
                Path::new(&card.file)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("file")
                    .to_ascii_uppercase()
            };
            let snippet = if card.title.is_some() {
                card.text.as_str()
            } else {
                card.text.split_once('\n').map_or("", |(_, s)| s)
            };
            ListEntry {
                board: root.into(),
                file: card.file.clone(),
                title,
                snippet: snippet.chars().take(240).collect(),
                kind,
                zone,
                folder: card.folder,
                tone: card.tone,
            }
        })
        .collect()
}
impl Board {
    pub fn list_entries(&self) -> Vec<ListEntry> {
        list(&self.root, &self.cards, &self.geometry)
    }

    /// Read existing child manifests without taking board locks or creating metadata.
    pub fn visit_child_entries(
        root: &Path,
        mut visit: impl FnMut(io::Result<Vec<ListEntry>>) -> bool,
    ) {
        let mut pending = vec![root.to_owned()];
        while let Some(dir) = pending.pop() {
            let scan = (|| -> io::Result<Vec<ListEntry>> {
                if !dir.symlink_metadata()?.is_dir() {
                    return Ok(vec![]);
                }
                let mut children = Vec::new();
                for entry in fs::read_dir(&dir)? {
                    let e = entry?;
                    if e.file_type()?.is_dir() && e.file_name().to_str().is_some_and(valid_name) {
                        children.push(e.path());
                    }
                }
                children.sort_unstable_by(|a, b| b.cmp(a));
                pending.extend(children);
                if dir == root {
                    return Ok(vec![]);
                }
                let manifest = match read_manifest(&dir) {
                    Ok(m) => m,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
                    Err(e) => return Err(e),
                };
                Ok(list(
                    &dir,
                    &load_cards(&dir, &manifest)?,
                    &Geometry::load(&manifest)?,
                ))
            })();
            if !visit(scan) {
                return;
            }
        }
    }
}
