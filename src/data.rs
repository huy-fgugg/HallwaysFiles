//! Data-driven tuning loaded from `assets/data/*.ron`, with compiled-in
//! fallbacks so a broken edit never breaks the game.

use bevy::prelude::*;
use serde::Deserialize;

use crate::music::asset_root;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct EntityTuning {
    pub min_room: u32,
    pub chance: f32,
    pub cooldown: u32,
    pub warn: f32,
    pub warn_min: f32,
    pub window: f32,
    pub damage: f32,
}

impl EntityTuning {
    /// Warning time for a given room: shrinks linearly over the floor.
    pub fn warn_for(&self, room: u32) -> f32 {
        let k = (room as f32 / 100.0).clamp(0.0, 1.0);
        (self.warn - (self.warn - self.warn_min) * k).max(self.warn_min)
    }
}

#[derive(Resource, Debug, Clone, Deserialize, PartialEq)]
pub struct EntityConfig {
    pub surge: EntityTuning,
    pub undertow: EntityTuning,
    pub whisperer: EntityTuning,
    pub gallery: EntityTuning,
    pub hush: EntityTuning,
    /// Blind outdoor patroller that hunts by sound. Older files without it get the default.
    #[serde(default = "default_listener")]
    pub listener: EntityTuning,
}

fn default_listener() -> EntityTuning {
    EntityTuning { min_room: 51, chance: 0.45, cooldown: 2, warn: 2.0, warn_min: 2.0, window: 0.0, damage: 100.0 }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ItemTuning {
    pub name: String,
    pub weight: f32,
    pub max_stack: u32,
    pub power: f32,
}

#[derive(Resource, Debug, Clone, Deserialize, PartialEq)]
pub struct ItemConfig {
    pub item_chance: f32,
    pub flashlight: ItemTuning,
    pub lockpick: ItemTuning,
    pub vitamins: ItemTuning,
    pub bandage: ItemTuning,
    pub skeleton_key: ItemTuning,
}

const ENTITIES_BUILTIN: &str = include_str!("../assets/data/entities.ron");
const ITEMS_BUILTIN: &str = include_str!("../assets/data/items.ron");

fn load<T: for<'de> Deserialize<'de>>(file: &str, builtin: &str) -> T {
    let path = asset_root().join("data").join(file);
    if let Ok(text) = std::fs::read_to_string(&path) {
        match ron::from_str::<T>(&text) {
            Ok(v) => return v,
            Err(e) => warn!("{}: {e} — using built-in values", path.display()),
        }
    }
    ron::from_str(builtin).expect("built-in data file must parse")
}

impl Default for EntityConfig {
    fn default() -> Self {
        ron::from_str(ENTITIES_BUILTIN).expect("built-in entities.ron must parse")
    }
}

impl Default for ItemConfig {
    fn default() -> Self {
        ron::from_str(ITEMS_BUILTIN).expect("built-in items.ron must parse")
    }
}

pub struct DataPlugin;

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load::<EntityConfig>("entities.ron", ENTITIES_BUILTIN))
            .insert_resource(load::<ItemConfig>("items.ron", ITEMS_BUILTIN));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_files_parse() {
        let e = EntityConfig::default();
        assert!(e.surge.chance > 0.0);
        let i = ItemConfig::default();
        assert_eq!(i.flashlight.max_stack, 1);
    }

    #[test]
    fn warn_shrinks_but_respects_minimum() {
        let t = EntityConfig::default().surge;
        assert!(t.warn_for(6) > t.warn_for(90));
        assert!(t.warn_for(200) >= t.warn_min);
    }
}
