//! Data native mods keep with the world, as Fabric's data attachments do:
//! values by mod and key for the whole world, and for one block of a level.
//! A block's values last while that block stays: once another block takes
//! its place, it reads as having none, and the save leaves them out. Saved
//! with the world, in its folder's `horizon-native-data.json`.
use super::{Game, Result};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

pub(super) const FILE: &str = "horizon-native-data.json";

#[derive(Default)]
pub(super) struct NativeData {
    /// The world folder the data belongs to; None until a mod asks.
    root: Option<PathBuf>,
    dirty: bool,
    /// By `<mod>:<key>`.
    world: BTreeMap<String, String>,
    /// By level, then position.
    blocks: BTreeMap<String, BTreeMap<[i32; 3], BlockData>>,
}

/// One block's values, and the block they were set on.
#[derive(Clone, Debug, Default, PartialEq)]
struct BlockData {
    block: String,
    values: BTreeMap<String, String>,
}

fn key(mod_id: &str, key: &str) -> String {
    format!("{mod_id}:{key}")
}

impl NativeData {
    /// The data of the world in `root`, read once; a world opened after
    /// another first saves the other's.
    pub(super) fn open(&mut self, root: &Path) {
        if self.root.as_deref() == Some(root) {
            return;
        }
        if let Err(e) = self.save() {
            eprintln!("native mod data: {e}");
        }
        *self = Self::read(root);
    }

    fn read(root: &Path) -> Self {
        let mut data = Self {
            root: Some(root.to_path_buf()),
            ..Self::default()
        };
        let Ok(text) = std::fs::read_to_string(root.join(FILE)) else {
            return data;
        };
        let Ok(Value::Object(top)) = serde_json::from_str::<Value>(&text) else {
            eprintln!("native mod data: {} is not a JSON object", FILE);
            return data;
        };
        if let Some(Value::Object(world)) = top.get("world") {
            data.world = strings(world);
        }
        if let Some(Value::Object(levels)) = top.get("levels") {
            for (level, entries) in levels {
                let Value::Array(entries) = entries else {
                    continue;
                };
                let blocks = data.blocks.entry(level.clone()).or_default();
                for entry in entries {
                    let pos = entry.get("pos").and_then(Value::as_array).and_then(|p| {
                        let c = |i: usize| p.get(i)?.as_i64()?.try_into().ok();
                        Some([c(0)?, c(1)?, c(2)?])
                    });
                    let block = entry.get("block").and_then(Value::as_str);
                    let values = entry.get("values").and_then(Value::as_object);
                    if let (Some(pos), Some(block), Some(values)) = (pos, block, values) {
                        let values = strings(values);
                        if !values.is_empty() {
                            let block = block.to_owned();
                            blocks.insert(pos, BlockData { block, values });
                        }
                    }
                }
            }
        }
        data
    }

    /// Write the data when it changed since the last save.
    pub(super) fn save(&mut self) -> io::Result<()> {
        let Some(root) = self.root.as_ref().filter(|_| self.dirty) else {
            return Ok(());
        };
        let mut levels = Map::new();
        for (level, blocks) in &self.blocks {
            let entries: Vec<Value> = blocks
                .iter()
                .map(|(pos, data)| {
                    let mut entry = Map::new();
                    entry.insert("pos".into(), Value::from(pos.to_vec()));
                    entry.insert("block".into(), Value::from(data.block.clone()));
                    entry.insert("values".into(), object(&data.values));
                    Value::Object(entry)
                })
                .collect();
            if !entries.is_empty() {
                levels.insert(level.clone(), Value::Array(entries));
            }
        }
        let mut top = Map::new();
        top.insert("world".into(), object(&self.world));
        top.insert("levels".into(), Value::Object(levels));
        let text = serde_json::to_string_pretty(&Value::Object(top))?;
        // Written beside, then moved over: a crash mid-write keeps the last save.
        let path = root.join(FILE);
        let partial = root.join(format!("{FILE}.partial"));
        std::fs::write(&partial, text + "\n")?;
        std::fs::rename(&partial, &path)?;
        self.dirty = false;
        Ok(())
    }

    pub(super) fn world(&self, mod_id: &str, name: &str) -> Option<&str> {
        self.world.get(&key(mod_id, name)).map(String::as_str)
    }

    pub(super) fn set_world(&mut self, mod_id: &str, name: &str, value: Option<&str>) {
        let k = key(mod_id, name);
        let changed = match value {
            Some(v) => self.world.insert(k, v.to_owned()).as_deref() != Some(v),
            None => self.world.remove(&k).is_some(),
        };
        self.dirty |= changed;
    }

    /// A value of the block at `pos` of `level`, which is `block` now.
    pub(super) fn block(
        &self,
        level: &str,
        pos: [i32; 3],
        block: &str,
        mod_id: &str,
        name: &str,
    ) -> Option<&str> {
        let data = self.blocks.get(level)?.get(&pos)?;
        if data.block != block {
            return None;
        }
        data.values.get(&key(mod_id, name)).map(String::as_str)
    }

    pub(super) fn set_block(
        &mut self,
        level: &str,
        pos: [i32; 3],
        block: &str,
        mod_id: &str,
        name: &str,
        value: Option<&str>,
    ) {
        let blocks = self.blocks.entry(level.to_owned()).or_default();
        let data = blocks.entry(pos).or_default();
        if data.block != block {
            // The values of the block that stood here are gone with it.
            data.block = block.to_owned();
            data.values.clear();
        }
        let k = key(mod_id, name);
        let changed = match value {
            Some(v) => data.values.insert(k, v.to_owned()).as_deref() != Some(v),
            None => data.values.remove(&k).is_some(),
        };
        if data.values.is_empty() {
            blocks.remove(&pos);
        }
        self.dirty |= changed;
    }

    /// Leave out the values of blocks of `level` that another block replaced,
    /// where `now` knows the block (its chunk is resident).
    pub(super) fn prune(&mut self, level: &str, now: impl Fn([i32; 3]) -> Option<String>) {
        let Some(blocks) = self.blocks.get_mut(level) else {
            return;
        };
        let before = blocks.len();
        blocks.retain(|pos, data| now(*pos).is_none_or(|block| block == data.block));
        self.dirty |= blocks.len() != before;
    }
}

impl Game<'_> {
    /// Save what native mods keep, with the world (each save, the autosave
    /// included), leaving out the values of blocks replaced in the level in place.
    pub(super) fn save_native_data(&mut self) -> Result<()> {
        if self.native_data.root != Some(self.world_root.clone()) {
            return Ok(());
        }
        let (world, registry, dim) = (&self.world, self.pack.assets.registry, self.dim);
        let now = |pos: [i32; 3]| {
            if pos[1] < dim.min_y() || pos[1] > dim.max_y() || !world.loaded(pos[0], pos[2]) {
                return None;
            }
            let state = world.block(pos[0], pos[1], pos[2]);
            registry.state(state).map(|s| s.name.clone())
        };
        self.native_data.prune(dim.id(), now);
        self.native_data.save()?;
        Ok(())
    }
}

fn strings(map: &Map<String, Value>) -> BTreeMap<String, String> {
    map.iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
        .collect()
}

fn object(map: &BTreeMap<String, String>) -> Value {
    Value::Object(
        map.iter()
            .map(|(k, v)| (k.clone(), Value::from(v.clone())))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "horizon-native-data-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn values_survive_a_save_and_a_read() {
        let root = scratch();
        let mut data = NativeData::default();
        data.open(&root);
        data.set_world("create", "motors", Some("3"));
        let pos = [1, 64, -2];
        data.set_block(
            "minecraft:overworld",
            pos,
            "create:shaft",
            "create",
            "speed",
            Some("32"),
        );
        data.save().unwrap();
        let mut again = NativeData::default();
        again.open(&root);
        assert_eq!(again.world("create", "motors"), Some("3"));
        let speed = again.block(
            "minecraft:overworld",
            pos,
            "create:shaft",
            "create",
            "speed",
        );
        assert_eq!(speed, Some("32"));
        assert_eq!(again.world("other", "motors"), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_blocks_values_go_with_it() {
        let mut data = NativeData::default();
        let (level, pos) = ("minecraft:overworld", [0, 0, 0]);
        data.set_block(level, pos, "create:shaft", "create", "speed", Some("16"));
        assert_eq!(
            data.block(level, pos, "minecraft:stone", "create", "speed"),
            None
        );
        data.set_block(level, pos, "minecraft:stone", "mod", "mark", Some("x"));
        assert_eq!(
            data.block(level, pos, "create:shaft", "create", "speed"),
            None
        );
        data.prune(level, |_| Some("minecraft:dirt".into()));
        assert_eq!(
            data.block(level, pos, "minecraft:stone", "mod", "mark"),
            None
        );
    }

    #[test]
    fn removing_the_last_value_removes_the_entry() {
        let mut data = NativeData::default();
        let (level, pos) = ("minecraft:the_nether", [5, 5, 5]);
        data.set_block(level, pos, "b", "m", "k", Some("v"));
        data.set_block(level, pos, "b", "m", "k", None);
        assert!(data.blocks[level].is_empty());
    }
}
