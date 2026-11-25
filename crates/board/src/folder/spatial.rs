use super::*;
use serde::{Deserialize, Serialize};

pub type Point = [i32; 2];

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Geometry {
    pub pins: Vec<Point>,
    pub dividers: Vec<Divider>,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Divider {
    pub pins: [usize; 2],
    pub color: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub title: String,
    pub pins: Vec<usize>,
}

impl Geometry {
    pub(super) fn load(table: &Table) -> io::Result<Self> {
        let Some(value) = table.get("geometry") else {
            return Ok(Self::default());
        };
        let geometry: Self = value.clone().try_into().map_err(io::Error::other)?;
        geometry.validate()?;
        Ok(geometry)
    }
    fn validate(&self) -> io::Result<()> {
        if self.pins.len() > 1024 || self.dividers.len() > 4096 || self.zones.len() > 128 {
            return Err(invalid("board geometry limit reached"));
        }
        for &p in &self.pins {
            valid_position(p)?;
        }
        for d in &self.dividers {
            if d.pins[0] == d.pins[1]
                || d.pins.iter().any(|i| *i >= self.pins.len())
                || d.color >= 6
            {
                return Err(invalid("invalid divider"));
            }
        }
        for z in &self.zones {
            if z.title.trim().is_empty()
                || z.title.len() > 128
                || z.pins.len() < 3
                || z.pins.iter().any(|i| *i >= self.pins.len())
                || z.pins.iter().collect::<HashSet<_>>().len() != z.pins.len()
            {
                return Err(invalid("invalid zone"));
            }
        }
        Ok(())
    }

    pub fn contains(&self, zone: usize, p: Point) -> bool {
        let Some(zone) = self.zones.get(zone) else {
            return false;
        };
        let mut inside = false;
        for (&i, &j) in zone.pins.iter().zip(zone.pins.iter().cycle().skip(1)) {
            let [ax, ay] = self.pins[i].map(f64::from);
            let [bx, by] = self.pins[j].map(f64::from);
            let [x, y] = p.map(f64::from);
            if (ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax {
                inside = !inside;
            }
        }
        inside
    }

    pub fn center(&self, index: usize) -> Point {
        let corners = &self.zones[index].pins;
        std::array::from_fn(|axis| {
            (corners
                .iter()
                .map(|&i| i64::from(self.pins[i][axis]))
                .sum::<i64>()
                / corners.len() as i64) as i32
        })
    }
}

impl Board {
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    pub(super) fn commit_layout(&mut self, next: Table) -> io::Result<()> {
        self.check_manifest()?;
        self.apply_layout(&next)?;
        self.history
            .push_back((std::mem::replace(&mut self.manifest, next.clone()), next));
        if self.history.len() > 32 {
            self.history.pop_front();
        }
        Ok(())
    }

    fn apply_layout(&mut self, next: &Table) -> io::Result<()> {
        let geometry = Geometry::load(next)?;
        let strings = load_strings(next, &self.cards)?;
        let entries = next
            .get("cards")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if entries.len() != self.cards.len() {
            return Err(invalid("card set changed"));
        }
        let mut positions = Vec::with_capacity(entries.len());
        for (entry, card) in entries.iter().zip(&self.cards) {
            if entry.get("file").and_then(Value::as_str) != Some(card.file.as_str()) {
                return Err(invalid("card set changed"));
            }
            positions.push([position(entry.get("x"))?, position(entry.get("y"))?]);
        }
        save(&self.root, next)?;
        for (card, pos) in self.cards.iter_mut().zip(positions) {
            card.position = pos;
        }
        self.geometry = geometry;
        self.strings = strings;
        Ok(())
    }

    pub fn undo_layout(&mut self) -> io::Result<bool> {
        self.check_manifest()?;
        let Some((before, after)) = self.history.back() else {
            return Ok(false);
        };
        if *after != self.manifest {
            self.history.clear();
            return Ok(false);
        }
        let before = before.clone();
        self.apply_layout(&before)?;
        self.manifest = before;
        self.history.pop_back();
        Ok(true)
    }

    fn edit_geometry(
        &mut self,
        edit: impl FnOnce(&mut Geometry) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut geometry = self.geometry.clone();
        edit(&mut geometry)?;
        let mut next = self.manifest.clone();
        next.insert(
            "geometry".into(),
            Value::try_from(geometry).map_err(io::Error::other)?,
        );
        self.commit_layout(next)
    }

    pub fn add_pin(&mut self, pos: Point) -> io::Result<usize> {
        let id = self.geometry.pins.len();
        self.edit_geometry(|g| {
            g.pins.push(pos);
            Ok(())
        })?;
        Ok(id)
    }

    pub fn move_pin(&mut self, id: usize, pos: Point) -> io::Result<()> {
        self.edit_geometry(|g| {
            *g.pins.get_mut(id).ok_or_else(|| invalid("unknown tack"))? = pos;
            Ok(())
        })
    }

    pub fn tie_divider(
        &mut self,
        start: usize,
        end: Option<usize>,
        drop: Point,
        color: u8,
    ) -> io::Result<usize> {
        let target = end.unwrap_or(self.geometry.pins.len());
        self.edit_geometry(|g| {
            if end.is_none() {
                g.pins.push(drop);
            }
            let mut pins = [start, target];
            pins.sort_unstable();
            if g.dividers.iter().any(|d| d.pins == pins) {
                return Err(invalid("these tacks are already connected"));
            }
            g.dividers.push(Divider { pins, color });
            Ok(())
        })?;
        Ok(target)
    }

    pub fn name_zone(&mut self, pins: Vec<usize>, title: String) -> io::Result<()> {
        self.edit_geometry(|g| {
            for (&a, &b) in pins.iter().zip(pins.iter().cycle().skip(1)) {
                let mut pair = [a, b];
                pair.sort_unstable();
                if !g.dividers.iter().any(|d| d.pins == pair) {
                    return Err(invalid("close the zone with string first"));
                }
            }
            g.zones.push(Zone { pins, title });
            Ok(())
        })
    }

    pub fn move_zone(&mut self, index: usize, delta: Point) -> io::Result<()> {
        let mut g = self.geometry.clone();
        let zone = g.zones.get(index).ok_or_else(|| invalid("unknown zone"))?;
        let mut next = self.manifest.clone();
        for (i, card) in self.cards.iter().enumerate() {
            if g.contains(index, [card.position[0] + 110, card.position[1] + 56]) {
                let pos = [
                    card.position[0].saturating_add(delta[0]),
                    card.position[1].saturating_add(delta[1]),
                ];
                valid_position(pos)?;
                let entry = &mut next["cards"].as_array_mut().unwrap()[i];
                entry["x"] = pos[0].into();
                entry["y"] = pos[1].into();
            }
        }
        for &i in &zone.pins {
            for (axis, d) in delta.iter().enumerate() {
                g.pins[i][axis] = g.pins[i][axis].saturating_add(*d);
            }
        }
        next.insert(
            "geometry".into(),
            Value::try_from(g).map_err(io::Error::other)?,
        );
        self.commit_layout(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zone_move_and_undo_are_atomic_and_restore_the_persisted_layout() {
        let root = tempfile::tempdir().unwrap();
        let mut b = Board::open(root.path()).unwrap();
        b.create("within", [12, 12]).unwrap();
        b.create("outside", [400, 400]).unwrap();
        let a = b.add_pin([0, 0]).unwrap();
        let c = b.tie_divider(a, None, [370, 0], 1).unwrap();
        let d = b.tie_divider(c, None, [0, 350], 3).unwrap();
        b.tie_divider(d, Some(a), [0, 0], 5).unwrap();
        b.name_zone(vec![a, c, d], "Case notes".into()).unwrap();
        b.move_zone(0, [10, 15]).unwrap();
        assert_eq!(b.cards()[0].position, [22, 27]);
        assert_eq!(b.cards()[1].position, [400, 400]);
        assert_eq!(b.geometry().pins[0], [10, 15]);
        assert!(b.undo_layout().unwrap());
        assert_eq!(b.cards()[0].position, [12, 12]);
        assert_eq!(b.geometry().pins[0], [0, 0]);
        b.connect(0, 1).unwrap();
        b.cut(0, 1).unwrap();
        assert!(b.undo_layout().unwrap());
        drop(b);
        let b = Board::open(root.path()).unwrap();
        assert_eq!(b.geometry().zones[0].title, "Case notes");
        assert_eq!(b.geometry().dividers[1].color, 3);
        assert_eq!(b.strings(), [[0, 1]]);
    }
}
