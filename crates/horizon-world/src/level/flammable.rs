//! Fire odds native mods give blocks (Fabric's `FlammableBlockRegistry`), for
//! every game of the process, ahead of the blocks' own `burnOdds` and
//! `igniteOdds`.
use std::{
    collections::BTreeMap,
    sync::{
        RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

static ODDS: RwLock<BTreeMap<String, (i32, i32)>> = RwLock::new(BTreeMap::new());
/// Whether any block has odds of a mod's: the fire tick asks many times.
static ANY: AtomicBool = AtomicBool::new(false);

/// Make block `name` burn with `odds` (burn, ignite: `FireBlock.setFlammable`'s
/// `burnOdds`, `igniteOdds`), or (None) leave it to its own.
pub fn set_mod_flammable(name: &str, odds: Option<(i32, i32)>) {
    let mut table = ODDS.write().unwrap_or_else(|e| e.into_inner());
    match odds {
        Some(o) => table.insert(name.to_owned(), (o.0.max(0), o.1.max(0))),
        None => table.remove(name),
    };
    ANY.store(!table.is_empty(), Ordering::Relaxed);
}

/// The (burn, ignite) odds a native mod gave block `name`.
pub fn mod_flammable(name: &str) -> Option<(i32, i32)> {
    if !ANY.load(Ordering::Relaxed) {
        return None;
    }
    let table = ODDS.read().unwrap_or_else(|e| e.into_inner());
    table.get(name).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mods_odds_come_and_go() {
        assert_eq!(mod_flammable("test:plank"), None);
        set_mod_flammable("test:plank", Some((5, 20)));
        assert_eq!(mod_flammable("test:plank"), Some((5, 20)));
        set_mod_flammable("test:plank", None);
        assert_eq!(mod_flammable("test:plank"), None);
    }
}
