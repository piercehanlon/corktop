use super::*;

impl Board {
    /// Publish the child board before retiring any source file.
    pub fn pack_zone(&mut self, index: usize) -> io::Result<()> {
        self.check_manifest()?;
        let zone = self
            .geometry
            .zones
            .get(index)
            .ok_or_else(|| invalid("unknown zone"))?;
        let name = zone.title.clone();
        if !valid_name(&name)
            || self
                .cards
                .iter()
                .any(|c| c.file.eq_ignore_ascii_case(&name))
        {
            return Err(invalid(
                "choose an unused, portable folder name for the zone",
            ));
        }
        for entry in fs::read_dir(&self.root)? {
            if entry?
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&name)
            {
                return Err(invalid("zone folder already exists"));
            }
        }
        let packed: Vec<_> = self
            .cards
            .iter()
            .filter(|c| {
                self.geometry
                    .contains(index, [c.position[0] + 110, c.position[1] + 56])
            })
            .map(|c| c.file.clone())
            .collect();
        if packed.is_empty() {
            return Err(invalid("the zone contains no cards"));
        }
        let inside: HashSet<_> = packed.iter().map(String::as_str).collect();
        for card in self
            .cards
            .iter()
            .filter(|c| inside.contains(c.file.as_str()))
        {
            if card.folder || !self.root.join(&card.file).symlink_metadata()?.is_file() {
                return Err(invalid("pack regular file cards into a zone"));
            }
        }
        let center = self.geometry.center(index);
        let pos = [(center[0] - 110).max(0), (center[1] - 56).max(0)];
        let mut next = self.manifest.clone();
        let cards = next["cards"].as_array_mut().unwrap();
        let chosen = |c: &Value| {
            c.get("file")
                .and_then(Value::as_str)
                .is_some_and(|s| inside.contains(s))
        };
        let moved = cards.iter().filter(|c| chosen(c)).cloned().collect();
        cards.retain(|c| !chosen(c));
        cards.push(Value::Table(Table::from_iter([
            ("file".into(), name.clone().into()),
            ("x".into(), pos[0].into()),
            ("y".into(), pos[1].into()),
        ])));
        let mut child_links = Vec::new();
        let mut outer_links = Vec::new();
        let mut seen = HashSet::new();
        for (index, &[a, b]) in self.strings.iter().enumerate() {
            let (a, b) = (&self.cards[a].file, &self.cards[b].file);
            let (ia, ib) = (inside.contains(a.as_str()), inside.contains(b.as_str()));
            if ia && ib {
                child_links.push(self.manifest["strings"].as_array().unwrap()[index].clone());
            } else {
                let mut ends = [if ia { &name } else { a }, if ib { &name } else { b }];
                ends.sort_unstable();
                if seen.insert(ends) {
                    let mut values = ends.map(|f| Value::String(f.clone())).to_vec();
                    let color = self.thread_color(index);
                    if color != 0 {
                        values.push((color as i64).into());
                    }
                    outer_links.push(Value::Array(values));
                }
            }
        }
        next.insert("strings".into(), Value::Array(outer_links));
        let mut geom = self.geometry.clone();
        geom.anchors.retain(|a| !inside.contains(a.file.as_str()));
        geom.remove_zone(index);
        next.insert(
            "geometry".into(),
            Value::try_from(&geom).map_err(io::Error::other)?,
        );
        let child = Table::from_iter([
            ("schema_version".into(), MANIFEST_VERSION.into()),
            ("cards".into(), Value::Array(moved)),
            ("strings".into(), Value::Array(child_links)),
        ]);
        let path = self.root.join(&name);
        // Exclusive directory creation also protects an empty, concurrently created folder.
        fs::create_dir(&path)?;
        let mut copied = Vec::new();
        let publish = (|| {
            for file in &packed {
                let target = path.join(file);
                recycle::copy_without_overwrite(&self.root.join(file), &target)?;
                copied.push(target);
            }
            save(&path, &child)?;
            copied.push(path.join(MANIFEST));
            let portal = Card::load(&self.root, &name, pos)?;
            self.check_manifest()?;
            save(&self.root, &next)?;
            Ok::<_, io::Error>(portal)
        })();
        let portal = match publish {
            Ok(card) => card,
            Err(e) => {
                for dest in copied {
                    fs::remove_file(dest)?;
                }
                fs::remove_dir(path)?;
                return Err(e);
            }
        };
        self.cards.retain(|c| !inside.contains(c.file.as_str()));
        self.cards.push(portal);
        self.strings = load_strings(&next, &self.cards)?;
        self.geometry = geom;
        self.manifest = next;
        self.history.clear();
        for file in packed {
            fs::remove_file(self.root.join(file))?;
        }
        Ok(())
    }
}

impl Geometry {
    fn remove_zone(&mut self, index: usize) {
        let zone = self.zones.remove(index);
        let ring: HashSet<_> = zone
            .pins
            .iter()
            .zip(zone.pins.iter().cycle().skip(1))
            .map(|(&a, &b)| [a.min(b), a.max(b)])
            .collect();
        let shared: HashSet<_> = self
            .zones
            .iter()
            .flat_map(|z| z.pins.iter().zip(z.pins.iter().cycle().skip(1)))
            .map(|(&a, &b)| [a.min(b), a.max(b)])
            .collect();
        self.dividers
            .retain(|d| !ring.contains(&d.pins) || shared.contains(&d.pins));
        let used: HashSet<_> = self
            .dividers
            .iter()
            .flat_map(|d| d.pins)
            .chain(self.anchors.iter().map(|a| a.pin))
            .chain(self.zones.iter().flat_map(|z| z.pins.iter().copied()))
            .collect();
        let mut remap = vec![0; self.pins.len()];
        self.pins = std::mem::take(&mut self.pins)
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !zone.pins.contains(i) || used.contains(i))
            .enumerate()
            .map(|(new, (old, p))| {
                remap[old] = new;
                p
            })
            .collect();
        for d in &mut self.dividers {
            d.pins = d.pins.map(|i| remap[i]);
        }
        for a in &mut self.anchors {
            a.pin = remap[a.pin];
        }
        for z in &mut self.zones {
            for i in &mut z.pins {
                *i = remap[*i];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_files_and_internal_links_survive_restart_with_an_external_portal_link() {
        let root = tempfile::tempdir().unwrap();
        let mut b = Board::open(root.path()).unwrap();
        b.create("Alpha", [15, 12]).unwrap();
        b.create("Beta", [40, 20]).unwrap();
        b.create("Outside", [800, 500]).unwrap();
        b.connect_colored(0, 1, 3).unwrap();
        b.connect_colored(1, 2, 2).unwrap();
        let a = b.add_pin([0, 0]).unwrap();
        let c = b.tie_divider(a, None, [500, 0], 1).unwrap();
        let d = b.tie_divider(c, None, [0, 470], 2).unwrap();
        b.tie_divider(d, Some(a), [0, 0], 0).unwrap();
        b.name_zone(vec![a, c, d], "Evidence".into()).unwrap();
        fs::create_dir(root.path().join("Evidence")).unwrap();
        assert!(b.pack_zone(0).is_err());
        assert_eq!(b.cards().len(), 3);
        fs::remove_dir(root.path().join("Evidence")).unwrap();
        b.pack_zone(0).unwrap();
        drop(b);
        let mut parent = Board::open(root.path()).unwrap();
        assert!(parent.geometry().pins.is_empty());
        assert_eq!(parent.strings(), [[0, 1]]);
        let child = Board::open(&parent.child_path(1).unwrap()).unwrap();
        assert_eq!(child.strings(), [[0, 1]]);
        assert_eq!(child.thread_color(0), 3);
        assert_eq!(parent.thread_color(0), 2);
        assert_eq!(child.note_text(0).unwrap(), "Alpha");
        assert_eq!(child.note_text(1).unwrap(), "Beta");
        assert!(!root.path().join(&child.cards()[0].file).exists());
        parent.refit([400, 300]).unwrap();
        assert_eq!(parent.cards()[0].position, [180, 188]);
        assert_eq!(parent.canvas_size(), [400, 300]);
        assert!(parent.undo_layout().unwrap());
        assert_eq!(parent.cards()[0].position, [800, 500]);
    }
}
