//! Persistent player profile (Phase 5): the Lumens bank, shop purchases for
//! the next run, run modifiers, a display name for online rooms and
//! lifetime stats. Saved as versioned RON next to the settings file.
//! Lumens are cosmetic progress only: the shop sells a head start for one
//! run, never randomised rewards.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::items::ItemKind;
use crate::run::RunState;
use crate::state::Overlay;

/// Bump when the format changes; `migrate` upgrades older saves.
pub const PROFILE_VERSION: u32 = 2;
pub const LOADOUT_MAX: usize = 3;

/// Optional challenge rules. Each one adds a Lumens bonus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    /// Every room's lamps are much dimmer.
    LightsLow,
    /// Entities show up more often.
    Restless,
    /// You take 50% more damage.
    Fragile,
}

impl Modifier {
    pub const ALL: [Modifier; 3] = [Modifier::LightsLow, Modifier::Restless, Modifier::Fragile];

    pub fn name(self) -> &'static str {
        match self {
            Modifier::LightsLow => "Lights Low",
            Modifier::Restless => "Restless",
            Modifier::Fragile => "Fragile",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Modifier::LightsLow => "Lamps burn at half strength",
            Modifier::Restless => "Entities appear more often",
            Modifier::Fragile => "Take 50% more damage",
        }
    }

    /// Extra Lumens share when this modifier is on.
    pub fn bonus(self) -> f32 {
        match self {
            Modifier::LightsLow => 0.25,
            Modifier::Restless => 0.35,
            Modifier::Fragile => 0.3,
        }
    }
}

/// Price in banked Lumens of each item when bought ahead of a run.
pub fn shop_price(kind: ItemKind) -> u32 {
    match kind {
        ItemKind::Flashlight => 60,
        ItemKind::Lockpick => 45,
        ItemKind::Vitamins => 30,
        ItemKind::Bandage => 35,
        ItemKind::SkeletonKey => 90,
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub version: u32,
    pub name: String,
    pub lumens: u32,
    pub runs: u32,
    pub wins: u32,
    pub best_room: u32,
    /// Items bought for the next run (consumed when it starts).
    pub loadout: Vec<ItemKind>,
    pub modifiers: Vec<Modifier>,
    /// Last seed typed on the Play screen ("" = random each run).
    pub seed_text: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self { version: PROFILE_VERSION, name: "Player".into(), lumens: 0, runs: 0, wins: 0, best_room: 0, loadout: vec![], modifiers: vec![], seed_text: String::new() }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BuyResult {
    Bought,
    TooExpensive,
    LoadoutFull,
}

impl Profile {
    pub fn has(&self, m: Modifier) -> bool {
        self.modifiers.contains(&m)
    }

    pub fn toggle(&mut self, m: Modifier) {
        if let Some(i) = self.modifiers.iter().position(|x| *x == m) {
            self.modifiers.remove(i);
        } else {
            self.modifiers.push(m);
        }
    }

    /// Multiplier applied to Lumens banked at the end of a run.
    pub fn lumen_multiplier(&self) -> f32 {
        1.0 + self.modifiers.iter().map(|m| m.bonus()).sum::<f32>()
    }

    pub fn buy(&mut self, kind: ItemKind) -> BuyResult {
        if self.loadout.len() >= LOADOUT_MAX {
            return BuyResult::LoadoutFull;
        }
        let price = shop_price(kind);
        if self.lumens < price {
            return BuyResult::TooExpensive;
        }
        self.lumens -= price;
        self.loadout.push(kind);
        BuyResult::Bought
    }

    /// Refund the last purchase (changed your mind before the run).
    pub fn refund_last(&mut self) -> bool {
        if let Some(k) = self.loadout.pop() {
            self.lumens += shop_price(k);
            true
        } else {
            false
        }
    }

    /// Banks a finished run. Returns the Lumens added.
    pub fn bank_run(&mut self, run: &RunState, won: bool) -> u32 {
        let earned = (run.lumens as f32 * self.lumen_multiplier()).round() as u32;
        self.lumens = self.lumens.saturating_add(earned);
        self.runs += 1;
        if won {
            self.wins += 1;
        }
        self.best_room = self.best_room.max(run.best_room).max(run.current_room);
        earned
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    /// Parses a saved profile, upgrading old versions. Corrupt files give a
    /// fresh profile (the caller keeps a backup of the bad file).
    pub fn from_ron(text: &str) -> Option<Self> {
        let mut p: Profile = ron::from_str(text).ok()?;
        p.migrate();
        p.sanitize();
        Some(p)
    }

    fn migrate(&mut self) {
        // v1 had no name/seed fields; serde defaults fill them in.
        if self.version < 2 {
            if self.name.trim().is_empty() {
                self.name = "Player".into();
            }
            self.version = 2;
        }
    }

    fn sanitize(&mut self) {
        self.loadout.truncate(LOADOUT_MAX);
        self.modifiers.dedup();
        self.name = clean_name(&self.name);
        self.seed_text.truncate(24);
    }
}

/// Names shown to other players: letters, digits, space, - and _ ; 1–16 chars.
pub fn clean_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(16).collect();
    let s = s.trim().to_string();
    if s.is_empty() { "Player".into() } else { s }
}

fn profile_path() -> std::path::PathBuf {
    crate::settings::config_dir().join("profile.ron")
}

pub fn load_profile() -> Profile {
    let path = profile_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => Profile::from_ron(&text).unwrap_or_else(|| {
            warn!("profile unreadable; starting fresh (old file kept as profile.bad.ron)");
            let _ = std::fs::copy(&path, path.with_extension("bad.ron"));
            Profile::default()
        }),
        Err(_) => Profile::default(),
    }
}

pub fn save_profile(p: &Profile) {
    if std::env::var_os("HALLWAYS_CAPTURE").is_some() {
        return; // QA runs never touch the real save
    }
    let path = profile_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("ron.tmp");
    if std::fs::write(&tmp, p.to_ron()).is_ok() {
        if let Err(err) = std::fs::rename(&tmp, &path) {
            warn!("could not save profile: {err}");
        }
    }
}

/// Lumens banked by the run that just ended (shown on the death/summary screens).
#[derive(Resource, Debug, Default)]
pub struct LastBanked(pub u32);

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_profile())
            .init_resource::<LastBanked>()
            .add_systems(OnEnter(Overlay::Dead), |mut p: ResMut<Profile>, mut run: ResMut<RunState>, mut last: ResMut<LastBanked>| {
                if !run.banked {
                    run.banked = true;
                    last.0 = p.bank_run(&run, false);
                    save_profile(&p);
                }
            })
            .add_systems(OnEnter(Overlay::Summary), |mut p: ResMut<Profile>, mut run: ResMut<RunState>, mut last: ResMut<LastBanked>| {
                if !run.banked {
                    run.banked = true;
                    last.0 = p.bank_run(&run, true);
                    save_profile(&p);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_migration() {
        let mut p = Profile { lumens: 120, ..default() };
        p.toggle(Modifier::Fragile);
        p.loadout.push(ItemKind::Bandage);
        let back = Profile::from_ron(&p.to_ron()).unwrap();
        assert_eq!(back, p);
        // A v1 save without the newer fields still loads.
        let old = Profile::from_ron("(version: 1, lumens: 40, runs: 3)").unwrap();
        assert_eq!(old.lumens, 40);
        assert_eq!(old.version, PROFILE_VERSION);
        assert_eq!(old.name, "Player");
        assert!(Profile::from_ron("not ron at all").is_none());
    }

    #[test]
    fn shop_rules() {
        let mut p = Profile { lumens: 100, ..default() };
        assert_eq!(p.buy(ItemKind::Flashlight), BuyResult::Bought);
        assert_eq!(p.lumens, 40);
        assert_eq!(p.buy(ItemKind::Flashlight), BuyResult::TooExpensive);
        assert_eq!(p.buy(ItemKind::Vitamins), BuyResult::Bought);
        assert!(p.refund_last());
        assert_eq!(p.lumens, 40);
        p.lumens = 1000;
        for _ in 0..3 {
            p.buy(ItemKind::Bandage);
        }
        assert_eq!(p.buy(ItemKind::Bandage), BuyResult::LoadoutFull);
    }

    #[test]
    fn modifiers_raise_rewards() {
        let mut p = Profile::default();
        let run = RunState { lumens: 100, best_room: 12, ..RunState::new() };
        assert_eq!(p.bank_run(&run, false), 100);
        p.toggle(Modifier::Restless);
        p.toggle(Modifier::Fragile);
        assert_eq!(p.bank_run(&run, true), 165);
        assert_eq!(p.runs, 2);
        assert_eq!(p.wins, 1);
        assert_eq!(p.best_room, 12);
        p.toggle(Modifier::Fragile);
        assert!(!p.has(Modifier::Fragile));
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  <b>Hi</b> there!!  "), "bHib there");
        assert_eq!(clean_name(""), "Player");
        assert_eq!(clean_name("abcdefghijklmnopqrstuvwxyz").len(), 16);
    }
}
