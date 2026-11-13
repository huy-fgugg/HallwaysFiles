//! Custom floors: presets that drive the floor generator. Built-in presets
//! ship with the game; your own are `.hallways` files (plain text) in the
//! `.hallways` folder in your home folder. Make as many as you like, and
//! share them by sending the file.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::data::EntityConfig;
use crate::floor::plan::{Area, GenOptions, FIRST_ROOM, LAST_ROOM};

pub const PRESET_VERSION: u32 = 1;
pub const EXTENSION: &str = "hallways";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    pub version: u32,
    pub name: String,
    /// Empty = a new floor every run.
    pub seed: String,
    /// Number on the final door (10–100).
    pub doors: u32,
    /// Areas in order; they share the floor evenly.
    pub areas: Vec<Area>,
    /// Percentages.
    pub room_size: u32,
    pub brightness: u32,
    pub dark_rooms: u32,
    pub entities: u32,
    pub fake_doors: u32,
    pub stair_rooms: u32,
    pub loot: u32,
    pub surge: bool,
    pub undertow: bool,
    pub whisperer: bool,
    pub gallery: bool,
    pub hush: bool,
    pub listener: bool,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            version: PRESET_VERSION,
            name: "Classic".into(),
            seed: String::new(),
            doors: LAST_ROOM + 1,
            areas: Area::ALL.to_vec(),
            room_size: 100,
            brightness: 100,
            dark_rooms: 100,
            entities: 100,
            fake_doors: 100,
            stair_rooms: 100,
            loot: 100,
            surge: true,
            undertow: true,
            whisperer: true,
            gallery: true,
            hush: true,
            listener: true,
        }
    }
}

/// Which numeric knob a +/- button changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresetField {
    Doors,
    RoomSize,
    Brightness,
    DarkRooms,
    Entities,
    FakeDoors,
    StairRooms,
    Loot,
}

impl PresetField {
    pub const ALL: [PresetField; 8] = [
        PresetField::Doors,
        PresetField::RoomSize,
        PresetField::Brightness,
        PresetField::DarkRooms,
        PresetField::Entities,
        PresetField::FakeDoors,
        PresetField::StairRooms,
        PresetField::Loot,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PresetField::Doors => "Doors",
            PresetField::RoomSize => "Room size",
            PresetField::Brightness => "Brightness",
            PresetField::DarkRooms => "Dark rooms",
            PresetField::Entities => "Entities",
            PresetField::FakeDoors => "Fake doors",
            PresetField::StairRooms => "Stair rooms",
            PresetField::Loot => "Loot",
        }
    }

    /// (min, max, step)
    fn range(self) -> (u32, u32, u32) {
        match self {
            PresetField::Doors => (10, 100, 5),
            PresetField::RoomSize => (60, 160, 10),
            PresetField::Brightness => (20, 160, 10),
            _ => (0, 300, 25),
        }
    }

    pub fn get(self, p: &Preset) -> u32 {
        match self {
            PresetField::Doors => p.doors,
            PresetField::RoomSize => p.room_size,
            PresetField::Brightness => p.brightness,
            PresetField::DarkRooms => p.dark_rooms,
            PresetField::Entities => p.entities,
            PresetField::FakeDoors => p.fake_doors,
            PresetField::StairRooms => p.stair_rooms,
            PresetField::Loot => p.loot,
        }
    }

    fn slot(self, p: &mut Preset) -> &mut u32 {
        match self {
            PresetField::Doors => &mut p.doors,
            PresetField::RoomSize => &mut p.room_size,
            PresetField::Brightness => &mut p.brightness,
            PresetField::DarkRooms => &mut p.dark_rooms,
            PresetField::Entities => &mut p.entities,
            PresetField::FakeDoors => &mut p.fake_doors,
            PresetField::StairRooms => &mut p.stair_rooms,
            PresetField::Loot => &mut p.loot,
        }
    }

    pub fn adjust(self, p: &mut Preset, delta: i32) {
        let (lo, hi, step) = self.range();
        let v = self.slot(p);
        let next = (*v as i64 + delta as i64 * step as i64).clamp(lo as i64, hi as i64);
        *v = next as u32;
    }

    pub fn value_text(self, p: &Preset) -> String {
        let v = self.get(p);
        match self {
            PresetField::Doors => format!("{v}"),
            _ => format!("{v}%"),
        }
    }
}

pub const ENTITY_NAMES: [&str; 6] = ["The Surge", "The Undertow", "The Whisperer", "The Gallery", "The Hush", "The Listener"];

impl Preset {
    pub fn entity_on(&self, i: usize) -> bool {
        [self.surge, self.undertow, self.whisperer, self.gallery, self.hush, self.listener].get(i).copied().unwrap_or(false)
    }

    pub fn toggle_entity(&mut self, i: usize) {
        match i {
            0 => self.surge = !self.surge,
            1 => self.undertow = !self.undertow,
            2 => self.whisperer = !self.whisperer,
            3 => self.gallery = !self.gallery,
            4 => self.hush = !self.hush,
            _ => self.listener = !self.listener,
        }
    }

    /// Toggles an area, keeping the floor's area order and at least one area.
    pub fn toggle_area(&mut self, a: Area) {
        if self.areas.contains(&a) {
            if self.areas.len() > 1 {
                self.areas.retain(|x| *x != a);
            }
        } else {
            self.areas.push(a);
            self.areas.sort_by_key(|x| Area::ALL.iter().position(|y| y == x));
        }
    }

    /// Clamps every field (hand-edited files can say anything).
    pub fn sanitized(mut self) -> Self {
        self.version = PRESET_VERSION;
        self.name = clean_name(&self.name);
        self.seed.truncate(24);
        self.doors = self.doors.clamp(10, LAST_ROOM + 1);
        self.areas.dedup();
        if self.areas.is_empty() {
            self.areas = Area::ALL.to_vec();
        }
        self.room_size = self.room_size.clamp(60, 160);
        self.brightness = self.brightness.clamp(20, 160);
        for v in [&mut self.dark_rooms, &mut self.entities, &mut self.fake_doors, &mut self.stair_rooms, &mut self.loot] {
            *v = (*v).min(300);
        }
        self
    }

    /// Generator options for this preset. The classic preset keeps the
    /// classic area layout exactly.
    pub fn gen_options(&self) -> GenOptions {
        let classic_areas = self.areas == Area::ALL && self.doors == LAST_ROOM + 1;
        GenOptions {
            last_room: (self.doors - 1).max(FIRST_ROOM + 4),
            areas: if classic_areas { None } else { Some(self.areas.clone()) },
            size: self.room_size as f32 / 100.0,
            darkness: self.dark_rooms as f32 / 100.0,
            fake_doors: self.fake_doors as f32 / 100.0,
            stairs: self.stair_rooms as f32 / 100.0,
            loot: self.loot as f32 / 100.0,
        }
    }

    /// Lamp brightness multiplier.
    pub fn light_scale(&self) -> f32 {
        self.brightness as f32 / 100.0
    }

    /// Applies the entity rate and on/off switches to the tuning.
    pub fn apply_entities(&self, cfg: &mut EntityConfig) {
        let k = self.entities as f32 / 100.0;
        let tunings = [&mut cfg.surge, &mut cfg.undertow, &mut cfg.whisperer, &mut cfg.gallery, &mut cfg.hush, &mut cfg.listener];
        for (i, t) in tunings.into_iter().enumerate() {
            t.chance = if self.entity_on(i) { (t.chance * k).min(0.95) } else { 0.0 };
            if k > 1.0 {
                t.cooldown = (t.cooldown as f32 / k).round() as u32;
            }
        }
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    pub fn from_ron(text: &str) -> Option<Self> {
        ron::from_str::<Preset>(text).ok().map(Preset::sanitized)
    }
}

fn clean_name(s: &str) -> String {
    let n: String = s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(24).collect();
    let n = n.trim().to_string();
    if n.is_empty() { "My Floor".into() } else { n }
}

/// Presets that ship with the game.
pub fn builtin() -> Vec<Preset> {
    let d = Preset::default();
    vec![
        d.clone(),
        Preset { name: "Night Gardens".into(), doors: 40, areas: vec![Area::Grounds], ..d.clone() },
        Preset { name: "Short Shift".into(), doors: 25, ..d.clone() },
        Preset { name: "Blackout".into(), brightness: 30, dark_rooms: 250, ..d.clone() },
        Preset { name: "Quiet Halls".into(), entities: 0, ..d.clone() },
        Preset { name: "Grand Tour".into(), doors: 60, room_size: 150, areas: vec![Area::Archive, Area::Grounds, Area::Atrium], ..d.clone() },
        Preset { name: "Liars Gallery".into(), doors: 50, fake_doors: 300, stair_rooms: 250, ..d.clone() },
        Preset { name: "Nightmare".into(), entities: 250, fake_doors: 200, dark_rooms: 200, loot: 50, brightness: 70, ..d },
    ]
}

/// The `.hallways` folder in your home folder.
pub fn presets_dir() -> PathBuf {
    if cfg!(target_os = "android") {
        return crate::settings::config_dir().join(".hallways");
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".hallways")
}

/// One preset in the list: built-in (no file) or yours (a `.hallways` file).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub preset: Preset,
    pub path: Option<PathBuf>,
}

/// Built-ins first, then every readable `.hallways` file in `dir`, A–Z.
pub fn load_all(dir: &Path) -> Vec<Entry> {
    let mut list: Vec<Entry> = builtin().into_iter().map(|preset| Entry { preset, path: None }).collect();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION))).collect())
        .unwrap_or_default();
    files.sort();
    for path in files {
        if let Some(preset) = std::fs::read_to_string(&path).ok().and_then(|t| Preset::from_ron(&t)) {
            list.push(Entry { preset, path: Some(path) });
        } else {
            warn!("skipping unreadable preset {}", path.display());
        }
    }
    list
}

/// File name for a preset name ("My Floor!" → "My Floor.hallways").
pub fn file_name(name: &str) -> String {
    format!("{}.{EXTENSION}", clean_name(name))
}

/// Writes the preset as `<name>.hallways` in `dir`; returns the path.
pub fn save(dir: &Path, p: &Preset) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file_name(&p.name));
    std::fs::write(&path, p.to_ron())?;
    Ok(path)
}

/// A name not used by any preset in the list ("My Floor 3").
pub fn unique_name(list: &[Entry], base: &str) -> String {
    let base = clean_name(base);
    let taken = |n: &str| list.iter().any(|e| e.preset.name.eq_ignore_ascii_case(n));
    if !taken(&base) {
        return base;
    }
    (2..).map(|i| format!("{} {i}", base.chars().take(20).collect::<String>())).find(|n| !taken(n)).unwrap()
}

/// Opens the presets folder in the system file browser.
pub fn open_folder(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(dir).spawn();
}

/// The custom preset for the next run (`None` = the classic floor).
#[derive(Resource, Debug, Default, Clone)]
pub struct ActivePreset(pub Option<Preset>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_sanitize() {
        let p = Preset { name: "Spooky <3".into(), doors: 999, room_size: 5, areas: vec![], ..Preset::default() };
        let back = Preset::from_ron(&p.to_ron()).unwrap();
        assert_eq!(back.name, "Spooky 3");
        assert_eq!(back.doors, 100);
        assert_eq!(back.room_size, 60);
        assert_eq!(back.areas, Area::ALL.to_vec());
        // Missing fields fall back to defaults (old or hand-written files).
        let small = Preset::from_ron("(name: \"Tiny\", doors: 20)").unwrap();
        assert_eq!(small.doors, 20);
        assert_eq!(small.entities, 100);
        assert!(Preset::from_ron("garbage").is_none());
    }

    #[test]
    fn classic_preset_is_the_classic_floor() {
        let o = Preset::default().gen_options();
        assert_eq!(o, GenOptions::default());
        let g = builtin()[1].gen_options();
        assert_eq!(g.last_room, 39);
        assert_eq!(g.areas, Some(vec![Area::Grounds]));
    }

    #[test]
    fn adjust_clamps_and_toggles() {
        let mut p = Preset::default();
        PresetField::Doors.adjust(&mut p, 1);
        assert_eq!(p.doors, 100);
        PresetField::Doors.adjust(&mut p, -3);
        assert_eq!(p.doors, 85);
        PresetField::Entities.adjust(&mut p, -10);
        assert_eq!(p.entities, 0);
        p.toggle_area(Area::Archive);
        assert!(!p.areas.contains(&Area::Archive));
        p.toggle_area(Area::Archive);
        assert_eq!(p.areas, Area::ALL.to_vec(), "order is restored");
        for a in Area::ALL {
            p.toggle_area(a);
        }
        assert_eq!(p.areas.len(), 1, "at least one area stays on");
    }

    #[test]
    fn entities_switch_off() {
        let mut cfg = EntityConfig::default();
        let p = Preset { surge: false, entities: 200, ..Preset::default() };
        let before = cfg.hush.chance;
        p.apply_entities(&mut cfg);
        assert_eq!(cfg.surge.chance, 0.0);
        assert!(cfg.hush.chance > before);
    }

    #[test]
    fn files_save_list_and_names() {
        let dir = std::env::temp_dir().join(format!("hallways-presets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = Preset { name: "Test Floor".into(), doors: 30, ..Preset::default() };
        let path = save(&dir, &p).unwrap();
        assert_eq!(path.file_name().unwrap(), "Test Floor.hallways");
        std::fs::write(dir.join("broken.hallways"), "nope").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let list = load_all(&dir);
        assert_eq!(list.len(), builtin().len() + 1);
        assert_eq!(list.last().unwrap().preset.doors, 30);
        assert_eq!(unique_name(&list, "Test Floor"), "Test Floor 2");
        assert_eq!(unique_name(&list, "Fresh"), "Fresh");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
