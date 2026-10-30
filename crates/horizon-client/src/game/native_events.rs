//! Horizon's side of the native hook API 4 events: deciding what a hook's
//! answer does, and what native mods ask of the network (payloads, command
//! trees, teleports), queued while a hook runs and sent when the server's
//! network tick holds the host. Nothing here is a hook point.
use super::commands::Target;
use super::lan::LanHost;
use super::native_server::NativeUse;
use super::server_travel::LevelTeleport;
use super::{Dim, Game};
use horizon_protocol::packets::{
    configuration::CustomPayload,
    play::{Clientbound as C, Serverbound as S},
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// The largest payload a client may send (`ServerboundCustomPayloadPacket`).
pub(super) const MAX_TO_SERVER: usize = 32767;
/// The largest payload a server may send (`ClientboundCustomPayloadPacket`).
pub(super) const MAX_TO_CLIENT: usize = 1 << 20;

/// What native mods asked to send, until the network can.
#[derive(Default)]
pub(super) struct NativeEvents {
    /// To a player's connection on a server, or to the server (None).
    pub(super) payloads: Vec<(Option<u32>, String, Vec<u8>)>,
    pub(super) teleports: Vec<(u32, Dim, [f64; 3])>,
    /// Whether the mods' creative items are in the tabs.
    creative_filled: bool,
    /// Whether a mod reads players' chat before it goes out
    /// (`native_filter_chat`).
    pub(super) filter_chat: bool,
    /// The players on the server, by connection: name and uuid.
    pub(super) players: BTreeMap<u32, (String, u128)>,
}

/// A channel is a resource location: `namespace:path`.
pub(super) fn valid_channel(channel: &str) -> bool {
    let Some((namespace, path)) = channel.split_once(':') else {
        return false;
    };
    let ok = |s: &str, extra: &str| {
        !s.is_empty()
            && s.chars().all(|c| {
                c.is_ascii_lowercase()
                    || c.is_ascii_digit()
                    || "_-.".contains(c)
                    || extra.contains(c)
            })
    };
    ok(namespace, "") && ok(path, "/")
}

/// A mod command's name: a literal Brigadier accepts as typed.
fn valid_command(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-".contains(c))
}

/// Whether `node` is a command node in the commands report's format: a
/// literal, or an argument naming its parser (`brigadier:integer`...), each
/// maybe `executable`, with children of the same kind.
fn valid_node(node: &Value, depth: usize) -> bool {
    let Some(map) = node.as_object() else {
        return false;
    };
    let kind = match map.get("type").and_then(Value::as_str) {
        Some("literal") => true,
        Some("argument") => map
            .get("parser")
            .and_then(Value::as_str)
            .is_some_and(|p| p.contains(':')),
        _ => false,
    };
    let children = match map.get("children") {
        None => true,
        Some(Value::Object(children)) => children.iter().all(|(name, child)| {
            !name.is_empty() && !name.contains(char::is_whitespace) && valid_node(child, depth + 1)
        }),
        Some(_) => false,
    };
    kind && children && depth < 32
}

/// The tree of the mods' commands in the commands report's format: each
/// name runs alone or with any text after it (a greedy string), unless its
/// mod gave its syntax.
pub(super) fn command_tree(names: &[String], syntax: &[(String, Value)]) -> Value {
    let mut children = Map::new();
    for name in names {
        let mut args = Map::new();
        args.insert("type".into(), "argument".into());
        args.insert("parser".into(), "brigadier:string".into());
        let mut properties = Map::new();
        properties.insert("type".into(), "greedy".into());
        args.insert("properties".into(), Value::Object(properties));
        args.insert("executable".into(), true.into());
        let mut args_child = Map::new();
        args_child.insert("args".into(), Value::Object(args));
        let mut literal = Map::new();
        literal.insert("type".into(), "literal".into());
        literal.insert("executable".into(), true.into());
        literal.insert("children".into(), Value::Object(args_child));
        children.insert(name.clone(), Value::Object(literal));
    }
    for (name, node) in syntax {
        children.insert(name.clone(), node.clone());
    }
    let mut root = Map::new();
    root.insert("type".into(), "root".into());
    root.insert("children".into(), Value::Object(children));
    Value::Object(root)
}

impl Game<'_> {
    /// A use hook's answer: whether the click ends here (the arm swings on
    /// `Success`).
    pub(super) fn native_use_ends(&mut self, result: NativeUse) -> bool {
        match result {
            NativeUse::Pass => false,
            NativeUse::Success => {
                self.swing_arm();
                true
            }
            NativeUse::Consume | NativeUse::Fail => true,
        }
    }

    /// The entity of network id `id` in the level in place, if any.
    pub(super) fn native_target(&self, id: i32) -> Option<Target> {
        let id = u32::try_from(id).ok()?;
        [
            Target::Mob(id),
            Target::Item(id),
            Target::Orb(id),
            Target::Projectile(id),
            Target::Stand(id),
            Target::Frame(id),
            Target::Painting(id),
            Target::Boat(id),
            Target::Cart(id),
            Target::Crystal(id),
            Target::Dragon(id),
            Target::Other(id),
        ]
        .into_iter()
        .find(|t| self.target_position(*t).is_some())
    }

    /// The end of the server's network tick: the mods' commands into every
    /// player's tree when they changed, their payloads to the players, and
    /// the teleports they asked for.
    pub(super) fn native_lan_tick(&mut self, host: &mut LanHost) {
        let mut names = Vec::new();
        self.native_commands(&mut names);
        names.retain(|n| valid_command(n));
        names.sort();
        names.dedup();
        let mut written = Vec::new();
        self.native_command_syntax(&mut written);
        written.sort();
        written.dedup_by(|a, b| a.0 == b.0);
        let mut hasher = DefaultHasher::new();
        (&names, &written).hash(&mut hasher);
        let key = hasher.finish();
        // With no mod commands, the tree stays as it was sent.
        let unchanged = host
            .hub
            .mod_commands_key()
            .map_or(names.is_empty(), |k| k == key);
        host.hub.hold_chat = self.native_events.filter_chat;
        if !unchanged {
            let mut syntax = Vec::new();
            for (name, json) in written {
                let node = serde_json::from_str::<Value>(&json).ok();
                match node
                    .filter(|n| valid_command(&name) && n["type"] == "literal" && valid_node(n, 0))
                {
                    Some(node) => syntax.push((name, node)),
                    None => log::warn!(
                        "a native mod's syntax for /{name} is not a literal command node"
                    ),
                }
            }
            host.hub
                .set_mod_commands(key, command_tree(&names, &syntax));
        }
        for (to, channel, data) in std::mem::take(&mut self.native_events.payloads) {
            if let Some(conn) = to {
                host.hub
                    .send_to(conn, &C::CustomPayload(CustomPayload { channel, data }));
            }
        }
        for (conn, dim, at) in std::mem::take(&mut self.native_events.teleports) {
            let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
                continue;
            };
            if Dim::from_id(&host.hub.guests[gi].dimension) != dim {
                self.level_teleports.push(LevelTeleport {
                    conn,
                    dim,
                    at,
                    rotation: None,
                });
                continue;
            }
            host.hub.guests[gi].motion = None;
            host.hub.teleport(gi, Some(at));
        }
    }

    /// The mods' items into the creative tabs, the first time they open.
    pub(super) fn native_fill_creative(&mut self) {
        if std::mem::replace(&mut self.native_events.creative_filled, true) {
            return;
        }
        let tabs: Vec<String> = self
            .creative_screen
            .tabs
            .iter()
            .map(|t| t.id.clone())
            .collect();
        for tab in tabs {
            let mut items = Vec::new();
            self.native_creative_items(&tab, &mut items);
            if !items.is_empty() {
                self.creative_screen.add_items(&tab, items);
            }
        }
    }

    /// The server is stopping: each player leaves (DISCONNECT).
    pub(super) fn native_server_stopping(&mut self) {
        let players: Vec<(u32, String, u128)> = self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter(|g| g.placed)
                .map(|g| (g.conn, g.name.clone(), g.uuid))
                .collect()
        });
        for (conn, name, uuid) in players {
            self.native_left(conn, &name, uuid);
        }
    }

    /// A player stands in the world: JOIN.
    pub(super) fn native_joined(&mut self, conn: u32, name: &str, uuid: u128) {
        self.native_events
            .players
            .insert(conn, (name.to_owned(), uuid));
        self.native_player_join(conn, name, uuid);
    }

    /// A player who joined left: DISCONNECT, once.
    pub(super) fn native_left(&mut self, conn: u32, name: &str, uuid: u128) {
        if self.native_events.players.remove(&conn).is_some() {
            self.native_player_leave(conn, name, uuid);
        }
    }

    /// After the final save: every level unloads (ServerWorldEvents.UNLOAD).
    pub(super) fn native_levels_closed(&mut self) {
        let mut dims = vec![self.dim];
        dims.extend(self.levels.parked_dims());
        for dim in dims {
            self.native_level_loaded(dim.id(), false);
        }
    }

    /// A player's chat went out: server mods hear of it (CHAT_MESSAGE),
    /// and its rate counts toward the spam kick.
    pub(super) fn native_chat_sent(&mut self, host: &mut LanHost, sender: u128, text: &str) {
        let conn = host
            .hub
            .guests
            .iter()
            .find(|g| g.uuid == sender)
            .map(|g| g.conn);
        if let Some(conn) = conn {
            self.native_chat_message(conn, text);
            self.detect_rate_spam(host, conn, false);
        }
    }

    /// A client's tick: the mods' payloads to the server.
    pub(super) fn native_remote_tick(&mut self) {
        for (to, channel, data) in std::mem::take(&mut self.native_events.payloads) {
            if to.is_none() {
                self.send(S::CustomPayload(CustomPayload { channel, data }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_are_resource_locations() {
        assert!(valid_channel("create:kinetics"));
        assert!(valid_channel("my_mod:sync/blocks"));
        assert!(!valid_channel("nonamespace"));
        assert!(!valid_channel("Upper:case"));
        assert!(!valid_channel("mod:"));
    }

    #[test]
    fn mod_commands_run_alone_or_with_text() {
        let tree = command_tree(&["create".into()], &[]);
        let create = &tree["children"]["create"];
        assert_eq!(create["type"], "literal");
        assert_eq!(create["executable"], true);
        let args = &create["children"]["args"];
        assert_eq!(args["parser"], "brigadier:string");
        assert_eq!(args["properties"]["type"], "greedy");
        assert!(valid_command("create") && !valid_command("Create") && !valid_command("a b"));
    }

    #[test]
    fn a_mods_syntax_replaces_the_free_text() {
        let node: Value = serde_json::from_str(
            r#"{"type": "literal", "children": {"motor": {"type": "literal", "children":
                {"rpm": {"type": "argument", "parser": "brigadier:integer",
                         "properties": {"min": -256, "max": 256}, "executable": true}}}}}"#,
        )
        .unwrap();
        assert!(valid_node(&node, 0));
        let tree = command_tree(&["create".into()], &[("create".into(), node)]);
        let rpm = &tree["children"]["create"]["children"]["motor"]["children"]["rpm"];
        assert_eq!(rpm["parser"], "brigadier:integer");
        assert!(tree["children"]["create"]["children"]["args"].is_null());
        let bad: Value = serde_json::from_str(r#"{"type": "argument"}"#).unwrap();
        assert!(!valid_node(&bad, 0));
    }
}
