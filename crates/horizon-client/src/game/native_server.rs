//! World helpers for native mods (native hook API 4), on the `Game`s that
//! simulate the world: what a mod adding machines or blocks that react to
//! the player reads and changes when the server events
//! (`native_level_tick`, `native_use_block`, `native_block_changed`,
//! `native_chunk`) reach it. Positions are block positions of the level in
//! place (`native_level`); states are the block registry's ids, the same
//! for the whole game. Kept as stable as the hook points, and free of `cfg`
//! and macros like them (`native_hooks::tests::server_helpers_stay_hookable`).
use super::{Dim, Game, native_events};
use horizon_client::items::ItemStack;
use horizon_core::{geometry::Vec3, position::BlockPos};
use horizon_world::registry::BlockStateId;
use std::path::PathBuf;

/// What a `native_use_block` hook made of the click (`InteractionResult`).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum NativeUse {
    /// Not the mod's: the block and the held item react as usual.
    #[default]
    Pass,
    /// Done: the arm swings and the click ends.
    Success,
    /// Done without a swing.
    Consume,
    /// Refused: the click ends with nothing done.
    Fail,
}

/// A dropped item as a native mod reads it (`native_items_in`) and writes
/// it back (`native_update_item`).
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeItemEntity {
    pub(crate) id: u32,
    pub(crate) item: String,
    /// 0 removes the entity when written back.
    pub(crate) count: u32,
    pub(crate) position: [f64; 3],
    pub(crate) velocity: [f64; 3],
}

/// Server helpers for native mods; nothing in Horizon calls them.
#[allow(dead_code)]
impl Game<'_> {
    /// The level in place: `minecraft:overworld`, `minecraft:the_nether`,
    /// `minecraft:the_end`. The events of one tick are all of one level.
    pub(super) fn native_level(&self) -> &'static str {
        self.dim.id()
    }
    /// The world's save folder: a mod keeps its per-world data here.
    pub(super) fn native_world_dir(&self) -> PathBuf {
        self.world_root.clone()
    }
    /// Whether `pos` is in the level's height and its chunk is resident.
    pub(super) fn native_loaded(&self, pos: [i32; 3]) -> bool {
        (self.dim.min_y()..=self.dim.max_y()).contains(&pos[1]) && self.world.loaded(pos[0], pos[2])
    }
    /// The block state at `pos`; air where nothing is resident.
    pub(super) fn native_block(&self, pos: [i32; 3]) -> BlockStateId {
        self.world.block(pos[0], pos[1], pos[2])
    }
    /// The block of a state (`minecraft:oak_log`).
    pub(super) fn native_state_name(&self, state: BlockStateId) -> Option<&str> {
        let s = self.pack.assets.registry.state(state)?;
        Some(s.name.as_str())
    }
    /// A property of a state (`axis` gives `y`); None when it has none.
    pub(super) fn native_state_property(&self, state: BlockStateId, key: &str) -> Option<&str> {
        let s = self.pack.assets.registry.state(state)?;
        s.properties.get(key).map(String::as_str)
    }
    /// The default state of block `name` (`Block.defaultBlockState`).
    pub(super) fn native_default_state(&self, name: &str) -> Option<BlockStateId> {
        self.pack.assets.registry.default_state(name)
    }
    /// `state` with property `key` set to `value` (`BlockState.setValue`);
    /// None when the block has no such property or value.
    pub(super) fn native_state_with(
        &self,
        state: BlockStateId,
        key: &str,
        value: &str,
    ) -> Option<BlockStateId> {
        let registry = self.pack.assets.registry;
        let mut s = registry.state(state)?.clone();
        let slot = s.properties.get_mut(key)?;
        *slot = value.to_owned();
        registry.id(&s)
    }
    /// Have `native_block_changed` report each write of block `name`'s
    /// states (to them or from them). The number of states, 0 for an
    /// unknown block. Call it when the world opens (`WorldOpenReady`): a
    /// watch lasts as long as the `Game`.
    pub(super) fn native_watch_block(&mut self, name: &str) -> usize {
        // The world records the writes of `world.watched`, which sculk's
        // tables also fill when they are built (`sculk_prepare` keeps these).
        let registry = self.pack.assets.registry;
        let n = registry.len();
        self.native_watched.resize(n, false);
        if self.world.watched.len() < n {
            self.world.watched.resize(n, false);
        }
        let mut count = 0;
        for i in 0..n {
            let state = registry.state(BlockStateId(i as u32));
            if state.is_some_and(|s| s.name == name) {
                self.native_watched[i] = true;
                self.world.watched[i] = true;
                count += 1;
            }
        }
        count
    }
    /// Set `pos` to `state` as `Level.setBlock` with `UPDATE_ALL` does:
    /// neighbours update and react. Drops nothing.
    pub(super) fn native_set_block(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
    ) -> Result<(), String> {
        self.level_set(pos, state).map_err(|e| e.to_string())
    }
    /// Break the block at `pos` as `Level.destroyBlock` with drops does.
    pub(super) fn native_break_block(&mut self, pos: [i32; 3]) -> bool {
        self.destroy_block_dropping(pos).is_ok()
    }
    /// Whether redstone powers the block at `pos` (`hasNeighborSignal`).
    pub(super) fn native_powered(&mut self, pos: [i32; 3]) -> bool {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.has_neighbor_signal(level, ticks, at))
    }
    /// The dropped items whose position is in the box `lo..hi`.
    pub(super) fn native_items_in(&self, lo: [f64; 3], hi: [f64; 3]) -> Vec<NativeItemEntity> {
        let inside = |p: &Vec3| {
            p.x >= lo[0]
                && p.x < hi[0]
                && p.y >= lo[1]
                && p.y < hi[1]
                && p.z >= lo[2]
                && p.z < hi[2]
        };
        self.dropped
            .iter()
            .filter(|e| inside(&e.position))
            .map(|e| NativeItemEntity {
                id: e.id,
                item: e.stack.item.to_string(),
                count: e.stack.count,
                position: [e.position.x, e.position.y, e.position.z],
                velocity: [e.velocity.x, e.velocity.y, e.velocity.z],
            })
            .collect()
    }
    /// Write back a dropped item's count, position and velocity (not its
    /// item); a count of 0 removes it. False when it is gone.
    pub(super) fn native_update_item(&mut self, item: &NativeItemEntity) -> bool {
        if item.count == 0 {
            let before = self.dropped.len();
            self.dropped.retain(|e| e.id != item.id);
            return self.dropped.len() < before;
        }
        let Some(e) = self.dropped.iter_mut().find(|e| e.id == item.id) else {
            return false;
        };
        e.stack.count = item.count;
        e.position = Vec3::new(item.position[0], item.position[1], item.position[2]);
        e.velocity = Vec3::new(item.velocity[0], item.velocity[1], item.velocity[2]);
        true
    }
    /// Drop `count` of `item` at `position`, moving at `velocity`, with
    /// the usual pickup delay of a block's drops.
    pub(super) fn native_spawn_item(
        &mut self,
        item: &str,
        count: u32,
        position: [f64; 3],
        velocity: [f64; 3],
    ) {
        self.spawn_item(ItemStack::new(item, count), position, velocity, 10);
    }
    /// The most of `item` a stack holds (64 for an unknown item).
    pub(super) fn native_max_stack(&self, item: &str) -> u32 {
        self.res.items().get(item).map_or(64, |i| i.max_stack)
    }
    /// What cooking one `item` gives in a recipe of type `kind`
    /// (`minecraft:smelting`, `minecraft:smoking`, `minecraft:blasting`,
    /// `minecraft:campfire_cooking`): the result and its count.
    pub(super) fn native_cook(&self, kind: &str, item: &str) -> Option<(String, u32)> {
        let cooking = self.res.recipes().cook(kind, &ItemStack::new(item, 1))?;
        Some((cooking.result.item.to_string(), cooking.result.count))
    }
    /// The item in the hand acting now (the one `native_use_block` names)
    /// and its count.
    pub(super) fn native_held(&self) -> Option<(String, u32)> {
        let stack = self.held()?;
        Some((stack.item.to_string(), stack.count))
    }
    /// Whether the player acting now sneaks (holds sneak).
    pub(super) fn native_sneaking(&self) -> bool {
        self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft)
    }
    /// A value mod `mod_id` keeps with the world under `key` (Fabric's
    /// world attachments), saved with the world.
    pub(super) fn native_world_data(&mut self, mod_id: &str, key: &str) -> Option<String> {
        let root = self.world_root.clone();
        self.native_data.open(&root);
        self.native_data.world(mod_id, key).map(str::to_owned)
    }
    /// Keep `value` under `key` with the world, or forget it (None).
    pub(super) fn native_set_world_data(&mut self, mod_id: &str, key: &str, value: Option<&str>) {
        let root = self.world_root.clone();
        self.native_data.open(&root);
        self.native_data.set_world(mod_id, key, value);
    }
    /// A value mod `mod_id` keeps on the block at `pos` (Fabric's block
    /// entity attachments, for any block): None once another block took
    /// its place, or where nothing is resident.
    pub(super) fn native_block_data(
        &mut self,
        pos: [i32; 3],
        mod_id: &str,
        key: &str,
    ) -> Option<String> {
        let block = self.native_resident_block(pos)?;
        let root = self.world_root.clone();
        self.native_data.open(&root);
        let level = self.dim.id();
        let value = self.native_data.block(level, pos, &block, mod_id, key)?;
        Some(value.to_owned())
    }
    /// Keep `value` on the block at `pos` until it is replaced, or forget it
    /// (None). False where nothing is resident.
    pub(super) fn native_set_block_data(
        &mut self,
        pos: [i32; 3],
        mod_id: &str,
        key: &str,
        value: Option<&str>,
    ) -> bool {
        let Some(block) = self.native_resident_block(pos) else {
            return false;
        };
        let root = self.world_root.clone();
        self.native_data.open(&root);
        let level = self.dim.id();
        self.native_data
            .set_block(level, pos, &block, mod_id, key, value);
        true
    }
    /// The block's name at `pos`, where its chunk is resident.
    fn native_resident_block(&self, pos: [i32; 3]) -> Option<String> {
        if !self.native_loaded(pos) {
            return None;
        }
        let state = self.native_block(pos);
        self.native_state_name(state).map(str::to_owned)
    }
    /// The type of entity `entity` (its network id) in the level in place
    /// (`minecraft:pig`), None when there is none. Players aren't found.
    pub(super) fn native_entity_type(&self, entity: i32) -> Option<String> {
        let target = self.native_target(entity)?;
        Some(self.target_type(target))
    }
    /// Where entity `entity` is (its feet).
    pub(super) fn native_entity_position(&self, entity: i32) -> Option<[f64; 3]> {
        let target = self.native_target(entity)?;
        self.target_position(target)
    }
    /// The player whose action, command or damage runs now on a server:
    /// their connection, name and uuid.
    pub(super) fn native_acting_player(&self) -> Option<(u32, String, u128)> {
        let conn = self.lan_acting?;
        let name = self.lan_acting_name.clone()?;
        Some((conn, name, self.lan_acting_uuid?))
    }
    /// The players on the server (`native_player_join` to
    /// `native_player_leave`): connection, name and uuid.
    pub(super) fn native_players(&self) -> Vec<(u32, String, u128)> {
        let players = self.native_events.players.iter();
        players.map(|(c, (n, u))| (*c, n.clone(), *u)).collect()
    }
    /// The permission level (0 to 4) of the command running now: mod
    /// commands get no check of their own.
    pub(super) fn native_permission(&self) -> u8 {
        self.ui.registries.permission
    }
    /// Send `data` on `channel` (`mod:name`) to player `to` (on a server)
    /// or to the server (None, on a client), at the end of the tick. False
    /// for a bad channel or one too big (32767 bytes to a server, 1 MiB to
    /// a player). Unknown channels are dropped on the other side.
    pub(super) fn native_send_payload(
        &mut self,
        to: Option<u32>,
        channel: &str,
        data: &[u8],
    ) -> bool {
        let max = match to {
            Some(_) => native_events::MAX_TO_CLIENT,
            None => native_events::MAX_TO_SERVER,
        };
        if !native_events::valid_channel(channel) || data.len() > max {
            return false;
        }
        let payload = (to, channel.to_owned(), data.to_vec());
        self.native_events.payloads.push(payload);
        true
    }
    /// Move player `conn` to `pos` in `level` (`minecraft:the_end`) at the
    /// end of the tick. False for an unknown level.
    pub(super) fn native_teleport(&mut self, conn: u32, level: &str, pos: [f64; 3]) -> bool {
        let dim = Dim::from_id(level);
        if dim.id() != level || pos.iter().any(|v| !v.is_finite()) {
            return false;
        }
        self.native_events.teleports.push((conn, dim, pos));
        true
    }
    /// Hold players' chat for `native_allow_chat` (on), or send it at once
    /// as vanilla does (off, the default).
    pub(super) fn native_filter_chat(&mut self, on: bool) {
        self.native_events.filter_chat = on;
    }
    /// Make `item` a furnace fuel burning `ticks` (FuelRegistry), or leave
    /// it to its components (None). For every game of the process.
    pub(super) fn native_set_fuel(&self, item: &str, ticks: Option<i32>) {
        horizon_client::items::set_mod_fuel(item, ticks);
    }
    /// Make `item` compostable with `chance` (CompostingChanceRegistry), or
    /// leave it to its components (None).
    pub(super) fn native_set_compostable(&self, item: &str, chance: Option<f32>) {
        horizon_client::items::set_mod_compostable(item, chance);
    }
    /// Answer the command being run (`native_command`), as its source's
    /// feedback: red when `error`. Outside a command it shows only in the
    /// window of the `Game` that runs it.
    pub(super) fn native_reply(&mut self, text: &str, error: bool) {
        let text = horizon_client::text::Text::literal(text);
        if error {
            self.command_failure(text);
        } else {
            self.system_message(text);
        }
    }
}
