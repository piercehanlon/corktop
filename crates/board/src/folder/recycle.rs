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

    pub fn edit_title(&mut self, idx: usize, title: &str, display_only: bool) -> io::Result<()> {
        let title = title.trim();
        if title.is_empty() || title.len() > 128 || title.chars().any(char::is_control) {
            return Err(invalid("enter a title of at most 128 bytes"));
        }
        self.check_manifest()?;
        let card = self.cards.get(idx).ok_or_else(|| invalid("unknown card"))?;
        let old = card.file.clone();
        let mut next = self.manifest.clone();
        next["cards"].as_array_mut().unwrap()[idx]
            .as_table_mut()
            .unwrap()
            .insert("title".into(), title.into());
        let renamed = if display_only {
            None
        } else {
            let source = self.note_path(idx)?;
            let stamp = old
                .split_once('_')
                .filter(|(stamp, _)| stamp.chars().all(|c| c.is_ascii_digit()))
                .map(|(s, _)| format!("{s}_"))
                .unwrap_or_default();
            let slug = title
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-")
                .to_lowercase();
            let ext = source
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{e}"))
                .unwrap_or_default();
            let name = format!("{stamp}{slug}{ext}");
            if slug.is_empty() || !valid_name(&name) {
                return Err(invalid("title cannot produce a portable filename"));
            }
            if name == old {
                None
            } else {
                for entry in fs::read_dir(&self.root)? {
                    if entry?
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&name)
                    {
                        return Err(invalid("title filename already exists"));
                    }
                }
                if self
                    .cards
                    .iter()
                    .any(|c| c.file.eq_ignore_ascii_case(&name))
                {
                    return Err(invalid("title filename already used by a card"));
                }
                next["cards"].as_array_mut().unwrap()[idx]["file"] = name.clone().into();
                for pair in entries(&mut next, "strings")? {
                    for end in pair
                        .as_array_mut()
                        .ok_or_else(|| invalid("invalid string"))?
                    {
                        if end.as_str() == Some(&old) {
                            *end = name.clone().into();
                        }
                    }
                }
                if let Some(bin) = next.get_mut("trash").and_then(Value::as_array_mut) {
                    for record in bin {
                        if let Some(pairs) = record.get_mut("strings").and_then(Value::as_array_mut)
                        {
                            for end in pairs.iter_mut().filter_map(Value::as_array_mut).flatten() {
                                if end.as_str() == Some(&old) {
                                    *end = name.clone().into();
                                }
                            }
                        }
                    }
                }
                copy_without_overwrite(&source, &self.root.join(&name))?;
                Some(name)
            }
        };
        if let Err(e) = save(&self.root, &next) {
            if let Some(name) = renamed {
                fs::remove_file(self.root.join(name))?;
            }
            return Err(e);
        }
        self.cards[idx].title = Some(title.into());
        self.manifest = next;
        if let Some(name) = renamed {
            self.cards[idx].file = name;
            fs::remove_file(self.root.join(old))?;
        }
        Ok(())
    }

    pub fn move_into_folder(&mut self, idx: usize, folder: usize) -> io::Result<()> {
        let source = self.note_path(idx)?;
        let name = self.cards[idx].file.clone();
        self.check_manifest()?;
        let mut child = Board::stage_folder(&self.child_path(folder)?)?;
        if child.cards.len() >= MAX_CARDS
            || child
                .cards
                .iter()
                .any(|c| c.file.eq_ignore_ascii_case(&name))
            || child.inbox.iter().any(|f| f.eq_ignore_ascii_case(&name))
        {
            return Err(invalid("destination is full or filename is occupied"));
        }
        let mut next = self.manifest.clone();
        entries(&mut next, "cards")?.remove(idx);
        entries(&mut next, "strings")?.retain(|pair| !references(pair, &name));
        let dest = child.root.join(&name);
        copy_without_overwrite(&source, &dest)?;
        child.inbox.push(name.clone());
        if let Err(e) = child.pin_files(std::slice::from_ref(&name)) {
            fs::remove_file(dest)?;
            return Err(e);
        }
        if let Some(title) = &self.cards[idx].title {
            child.edit_title(child.cards.len() - 1, title, true)?;
        }
        // Commit the destination before unlinking the original. An interrupted transfer
        // may leave both copies; it must always leave at least one readable payload.
        save(&self.root, &next)?;
        self.cards.remove(idx);
        self.strings = load_strings(&next, &self.cards)?;
        self.manifest = next;
        fs::remove_file(source)
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
        let mut card = Card::load(&trash_folder, name, pos)?;
        card.title = entry
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned);
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
