use super::*;

fn references(pair: &Value, name: &str) -> bool {
    pair.as_array()
        .is_some_and(|ends| ends.iter().any(|end| end.as_str() == Some(name)))
}

fn entries<'a>(table: &'a mut Table, key: &str) -> io::Result<&'a mut Vec<Value>> {
    table
        .entry(key.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| invalid("invalid board records"))
}

fn copy_without_overwrite(source: &Path, destination: &Path) -> io::Result<()> {
    let dest_dir = destination
        .parent()
        .ok_or_else(|| invalid("missing destination directory"))?;
    if !source.symlink_metadata()?.is_file() {
        return Err(invalid("expected a regular file"));
    }
    let mut input = nofollow_options().read(true).open(source)?;
    let mut staged = tempfile::NamedTempFile::new_in(dest_dir)?;
    io::copy(&mut input, &mut staged)?;
    staged.as_file().sync_all()?;
    staged.persist_noclobber(destination)?;
    Ok(())
}

impl Board {
    fn trash_folder(&self) -> io::Result<PathBuf> {
        let path = self.root.join(".trash");
        match fs::create_dir(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
        if !path.symlink_metadata()?.is_dir() {
            return Err(invalid("invalid trash folder"));
        }
        Ok(path)
    }

    pub fn trashed_files(&self) -> Vec<String> {
        self.manifest
            .get("trash")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|record| record.get("card")?.get("file")?.as_str().map(str::to_owned))
            .collect()
    }

    pub fn remove_card(&mut self, idx: usize) -> io::Result<()> {
        self.check_manifest()?;
        let card = self.cards.get(idx).ok_or_else(|| invalid("unknown card"))?;
        let name = card.file.clone();
        let missing = card.missing;
        let source = self.root.join(&name);
        let mut next = self.manifest.clone();
        let entry = entries(&mut next, "cards")?.remove(idx);
        let strings = entries(&mut next, "strings")?;
        let saved_links: Vec<_> = strings
            .iter()
            .filter(|s| references(s, &name))
            .cloned()
            .collect();
        strings.retain(|s| !references(s, &name));
        let destination = if missing {
            if source.try_exists()? {
                return Err(invalid("file reappeared; refresh the board"));
            }
            None
        } else {
            let dest = self.trash_folder()?.join(&name);
            entries(&mut next, "trash")?.push(Value::Table(Table::from_iter([
                ("card".into(), entry),
                ("strings".into(), Value::Array(saved_links)),
            ])));
            copy_without_overwrite(&source, &dest)?;
            Some(dest)
        };
        if let Err(e) = save(&self.root, &next) {
            if let Some(path) = destination {
                fs::remove_file(path)?;
            }
            return Err(e);
        }
        self.cards.remove(idx);
        self.strings = load_strings(&next, &self.cards)?;
        self.manifest = next;
        if !missing {
            fs::remove_file(source)?;
        }
        Ok(())
    }

    pub fn restore(&mut self, name: &str) -> io::Result<()> {
        self.check_manifest()?;
        if !valid_name(name) || self.cards.len() >= MAX_CARDS {
            return Err(invalid("cannot restore card"));
        }
        for entry in fs::read_dir(&self.root)? {
            if entry?
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
            {
                return Err(invalid("restore filename is occupied"));
            }
        }
        let mut next = self.manifest.clone();
        let trash = entries(&mut next, "trash")?;
        let idx = trash
            .iter()
            .position(|r| {
                r.get("card")
                    .and_then(|c| c.get("file"))
                    .and_then(Value::as_str)
                    == Some(name)
            })
            .ok_or_else(|| invalid("unknown trash record"))?;
        let record = trash.remove(idx);
        let entry = record
            .get("card")
            .ok_or_else(|| invalid("invalid trash record"))?;
        let pos = [position(entry.get("x"))?, position(entry.get("y"))?];
        entries(&mut next, "cards")?.push(entry.clone());
        let links = entries(&mut next, "strings")?;
        for pair in record
            .get("strings")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("invalid trash links"))?
        {
            if references(pair, name)
                && pair.as_array().is_some_and(|p| {
                    p.len() == 2
                        && p.iter().all(|v| {
                            v.as_str().is_some_and(|f| {
                                f == name || self.cards.iter().any(|c| c.file == f)
                            })
                        })
                })
                && !links.contains(pair)
            {
                links.push(pair.clone());
            }
        }
        let trash_folder = self.trash_folder()?;
        let card = Card::load(&trash_folder, name, pos)?;
        let source = trash_folder.join(name);
        let dest = self.root.join(name);
        copy_without_overwrite(&source, &dest)?;
        self.cards.push(card);
        let result = load_strings(&next, &self.cards).and_then(|strings| {
            save(&self.root, &next)?;
            Ok(strings)
        });
        match result {
            Ok(strings) => {
                self.strings = strings;
                self.manifest = next;
                fs::remove_file(source)
            }
            Err(e) => {
                self.cards.pop();
                fs::remove_file(dest)?;
                Err(e)
            }
        }
    }
}
