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
    /// A hand-built map (Map Builder): its rooms in order. Empty = generated.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rooms: Vec<crate::mapfile::MapRoom>,
    /// How often each monster comes (Mode Creator): 0 off, 1 rare, 2 normal,
    /// 3 often, 4 constant. Empty = from the on/off switches (on = normal).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<u8>,
    /// Mode Creator extras (all optional in files; defaults = the normal game).
    /// Door you start at (0 = the beginning).
    pub start_door: u32,
    /// The two Hollow chases happen.
    pub chases: bool,
    /// Areas come in a shuffled order (false = in the order listed).
    pub shuffle_areas: bool,
    /// How often exits are locked (key in a drawer), percent.
    pub locks: u32,
    /// Player health, speed, hiding time, Lumens earned (percent; hide 0 = no limit).
    pub health: u32,
    pub speed: u32,
    pub hide_time: u32,
    pub lumens: u32,
    /// Starting items: 0 normal (flashlight, bandage), 1 nothing, 2 extra supplies, 3 the full kit.
    pub start_kit: u8,
    /// Extra lives: after dying you can get back up this many times.
    pub revives: u8,
    /// Sprinting allowed.
    pub sprint: bool,
    /// Items that never turn up in drawers (bit per [`crate::items::ItemKind::ALL`]).
    pub items_off: u8,
    /// How often each structure appears (lofts, stairs, zigzags, pillars,
    /// streams), as monster-style levels 0..4 (2 = normal).
    pub structures: [u8; 5],
    /// Room the Reading Hall is at (0 = no Reading Hall).
    pub reading_room: u32,
    /// The mode's own rushing monster.
    pub custom: CustomMonster,
}

/// A monster made in the Mode Creator: it rushes through the rooms like the
/// Surge, in its own colour and with its own rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomMonster {
    pub on: bool,
    pub name: String,
    /// Index into [`MONSTER_COLORS`].
    pub color: u8,
    /// Speed in m/s, slowest and fastest (each run picks in between).
    pub speed_min: u8,
    pub speed_max: u8,
    /// Times it turns around and comes back.
    pub passes: u8,
    /// Damage when it catches you (100 = kills).
    pub damage: u8,
    /// Warning before it arrives, tenths of a second.
    pub warning: u8,
    /// Chance per room, percent.
    pub chance: u8,
    /// First room it can appear in.
    pub first_room: u32,
}

impl Default for CustomMonster {
    fn default() -> Self {
        Self { on: false, name: "The Stranger".into(), color: 0, speed_min: 16, speed_max: 24, passes: 0, damage: 100, warning: 40, chance: 15, first_room: 10 }
    }
}

pub const MONSTER_COLORS: [(&str, [f32; 3]); 8] = [
    ("Red", [1.0, 0.2, 0.15]),
    ("Orange", [1.0, 0.55, 0.15]),
    ("Yellow", [1.0, 0.9, 0.3]),
    ("Green", [0.35, 1.0, 0.4]),
    ("Cyan", [0.3, 0.95, 1.0]),
    ("Blue", [0.3, 0.45, 1.0]),
    ("Purple", [0.75, 0.35, 1.0]),
    ("White", [0.95, 0.95, 1.0]),
];

impl CustomMonster {
    pub fn rgb(&self) -> Color {
        let c = MONSTER_COLORS[(self.color as usize) % MONSTER_COLORS.len()].1;
        Color::srgb(c[0], c[1], c[2])
    }
}

pub const STRUCTURE_NAMES: [&str; 5] = ["Key lofts", "Stairs", "Zigzag walls", "Pillar halls", "Streams"];

pub const KIT_NAMES: [&str; 4] = ["Normal", "Nothing", "Extra", "Full kit"];

/// The rules of the run being played, from its mode (all 1.0 for the normal game).
#[derive(Resource, Debug, Clone, Copy)]
pub struct ModeRules {
    /// Damage taken is multiplied by this (100 / health%).
    pub damage: f32,
    pub speed: f32,
    /// Hiding timer speed (0 = no limit).
    pub hide_rate: f32,
    pub lumens: f32,
    pub kit: u8,
    pub sprint: bool,
    pub revives: u8,
}

impl Default for ModeRules {
    fn default() -> Self {
        Self { damage: 1.0, speed: 1.0, hide_rate: 1.0, lumens: 1.0, kit: 0, sprint: true, revives: 0 }
    }
}

impl ModeRules {
    pub fn of(p: Option<&Preset>) -> Self {
        let Some(p) = p else { return Self::default() };
        Self {
            damage: 100.0 / p.health.max(25) as f32,
            speed: p.speed as f32 / 100.0,
            hide_rate: if p.hide_time == 0 { 0.0 } else { 100.0 / p.hide_time as f32 },
            lumens: p.lumens as f32 / 100.0,
            kit: p.start_kit,
            sprint: p.sprint,
            revives: p.revives,
        }
    }
}

/// Monster frequency names (Mode Creator).
pub const LEVEL_NAMES: [&str; 5] = ["Off", "Rare", "Normal", "Often", "Constant"];
const LEVEL_RATE: [f32; 5] = [0.0, 0.45, 1.0, 1.7, 3.0];

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
            rooms: Vec::new(),
            levels: Vec::new(),
            start_door: 0,
            chases: true,
            shuffle_areas: true,
            locks: 100,
            health: 100,
            speed: 100,
            hide_time: 100,
            lumens: 100,
            start_kit: 0,
            revives: 0,
            sprint: true,
            items_off: 0,
            structures: [2; 5],
            reading_room: crate::floor::plan::READING_HALL,
            custom: CustomMonster::default(),
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
    /// A monster's frequency level (0 off .. 4 constant).
    pub fn level(&self, i: usize) -> u8 {
        match self.levels.get(i) {
            Some(&l) => l.min(4),
            None => if self.entity_on(i) { 2 } else { 0 },
        }
    }

    /// Sets a monster's level (and its on/off switch to match).
    pub fn set_level(&mut self, i: usize, l: u8) {
        if self.levels.len() < ENTITY_NAMES.len() {
            self.levels = (0..ENTITY_NAMES.len()).map(|k| self.level(k)).collect();
        }
        self.levels[i] = l.min(4);
        if self.entity_on(i) != (l > 0) {
            self.toggle_entity(i);
        }
    }

    /// 0..100: how hard this mode is, for the creator's meter.
    pub fn difficulty(&self) -> u32 {
        let monsters: f32 = (0..ENTITY_NAMES.len()).map(|i| LEVEL_RATE[self.level(i) as usize]).sum::<f32>() / 6.0 * self.entities as f32 / 100.0;
        let dark = (1.0 - self.brightness as f32 / 160.0) * 0.6 + self.dark_rooms as f32 / 300.0 * 0.6;
        let tricks = self.fake_doors as f32 / 300.0 * 0.5 + self.stair_rooms as f32 / 300.0 * 0.15;
        let supplies = (1.0 - self.loot as f32 / 200.0).max(0.0) * 0.4;
        let length = self.doors as f32 / 100.0 * 0.5;
        // Scaled so the classic floor reads HARD (it is), all-out reads IMPOSSIBLE.
        ((monsters * 0.9 + dark + tricks + supplies + length) / 4.2 * 100.0).clamp(0.0, 100.0) as u32
    }

    /// A name for the difficulty meter.
    pub fn difficulty_name(&self) -> &'static str {
        match self.difficulty() {
            0..=20 => "EASY",
            21..=40 => "NORMAL",
            41..=60 => "HARD",
            61..=80 => "NIGHTMARE",
            _ => "IMPOSSIBLE",
        }
    }

    /// True for anything a player made (not one of the game's built-in floors).
    pub fn player_made(&self) -> bool {
        !self.rooms.is_empty() || !builtin().iter().any(|b| b.name == self.name && b.doors == self.doors && b.areas == self.areas && b.entities == self.entities)
    }

    /// A short code that carries the mode's settings (not its rooms or name),
    /// to type into another player's game: "HW-" and base-32 letters.
    pub fn share_code(&self) -> String {
        let mut bytes = vec![
            1u8,
            self.doors.min(255) as u8,
            (self.room_size / 10) as u8,
            (self.brightness / 10) as u8,
            (self.dark_rooms / 25) as u8,
            (self.fake_doors / 25) as u8,
            (self.stair_rooms / 25) as u8,
            (self.loot / 25) as u8,
            (self.entities / 25) as u8,
        ];
        let mask = Area::ALL.iter().enumerate().fold(0u8, |m, (i, a)| if self.areas.contains(a) { m | 1 << i } else { m });
        bytes.push(mask);
        let lv: u32 = (0..6).fold(0u32, |acc, i| acc | (self.level(i) as u32) << (i * 3));
        bytes.extend_from_slice(&lv.to_le_bytes()[..3]);
        bytes[0] = 2;
        bytes.extend_from_slice(&[
            self.start_door.min(255) as u8,
            (self.chases as u8) | (self.shuffle_areas as u8) << 1 | self.start_kit.min(3) << 2,
            (self.locks / 25) as u8,
            (self.health / 25) as u8,
            (self.speed / 10) as u8,
            (self.hide_time / 25) as u8,
            (self.lumens / 25) as u8,
        ]);
        bytes[0] = 3;
        let st = self.structures.iter().enumerate().fold(0u16, |m, (i, l)| m | (u16::from((*l).min(4)) << (i * 3)));
        let c = &self.custom;
        bytes.extend_from_slice(&[
            self.revives.min(5) | (self.sprint as u8) << 3 | (c.on as u8) << 4,
            self.items_off,
            st as u8,
            (st >> 8) as u8,
            self.reading_room.min(255) as u8,
            c.color,
            c.speed_min,
            c.speed_max,
            c.passes,
            c.damage,
            c.warning,
            c.chance,
            c.first_room.min(255) as u8,
        ]);
        let sum = bytes.iter().fold(0u8, |a, b| a.wrapping_mul(31).wrapping_add(*b));
        bytes.push(sum);
        format!("HW-{}", base32(&bytes))
    }

    /// Reads a share code back into a mode (None if it's mistyped).
    pub fn from_share_code(code: &str) -> Option<Self> {
        let clean: String = code.trim().to_ascii_uppercase().trim_start_matches("HW-").chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let b = unbase32(&clean)?;
        let n = match b.first() {
            Some(1) => 13,
            Some(2) => 20,
            Some(3) => 33,
            _ => return None,
        };
        if b.len() < n + 1 {
            return None;
        }
        let sum = b[..n].iter().fold(0u8, |a, x| a.wrapping_mul(31).wrapping_add(*x));
        if sum != b[n] {
            return None;
        }
        let mut p = Preset { name: format!("Shared {}", &clean[..clean.len().min(5)]), doors: b[1] as u32, room_size: b[2] as u32 * 10, brightness: b[3] as u32 * 10, dark_rooms: b[4] as u32 * 25, fake_doors: b[5] as u32 * 25, stair_rooms: b[6] as u32 * 25, loot: b[7] as u32 * 25, entities: b[8] as u32 * 25, ..Preset::default() };
        p.areas = Area::ALL.iter().enumerate().filter(|(i, _)| b[9] & (1 << i) != 0).map(|(_, a)| *a).collect();
        let lv = u32::from_le_bytes([b[10], b[11], b[12], 0]);
        for i in 0..6 {
            p.set_level(i, ((lv >> (i * 3)) & 7) as u8);
        }
        if n >= 20 {
            p.start_door = b[13] as u32;
            p.chases = b[14] & 1 != 0;
            p.shuffle_areas = b[14] & 2 != 0;
            p.start_kit = (b[14] >> 2) & 3;
            p.locks = b[15] as u32 * 25;
            p.health = b[16] as u32 * 25;
            p.speed = b[17] as u32 * 10;
            p.hide_time = b[18] as u32 * 25;
            p.lumens = b[19] as u32 * 25;
        }
        if n >= 33 {
            p.revives = b[20] & 7;
            p.sprint = b[20] & 8 != 0;
            p.custom.on = b[20] & 16 != 0;
            p.items_off = b[21];
            let st = u16::from_le_bytes([b[22], b[23]]);
            for i in 0..5 {
                p.structures[i] = ((st >> (i * 3)) & 7) as u8;
            }
            p.reading_room = b[24] as u32;
            let c = &mut p.custom;
            (c.color, c.speed_min, c.speed_max, c.passes, c.damage, c.warning, c.chance) = (b[25], b[26], b[27], b[28], b[29], b[30], b[31]);
            c.first_room = b[32] as u32;
        }
        Some(p.sanitized())
    }

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
        self.start_door = self.start_door.min(self.doors.saturating_sub(5));
        self.locks = self.locks.min(300);
        self.health = self.health.clamp(25, 400);
        self.speed = self.speed.clamp(60, 160);
        self.hide_time = self.hide_time.min(400);
        self.lumens = self.lumens.min(400);
        self.start_kit = self.start_kit.min(3);
        self.revives = self.revives.min(5);
        self.items_off &= 0b1_1111;
        for s in &mut self.structures {
            *s = (*s).min(4);
        }
        if self.reading_room != 0 {
            self.reading_room = self.reading_room.clamp(FIRST_ROOM + 2, 98);
        }
        let c = &mut self.custom;
        c.name = clean_name(&c.name);
        if c.name.is_empty() {
            c.name = "The Stranger".into();
        }
        c.color %= MONSTER_COLORS.len() as u8;
        c.speed_min = c.speed_min.clamp(6, 40);
        c.speed_max = c.speed_max.clamp(c.speed_min, 40);
        c.passes = c.passes.min(5);
        c.damage = c.damage.clamp(10, 100);
        c.warning = c.warning.clamp(10, 100);
        c.chance = c.chance.clamp(1, 100);
        c.first_room = c.first_room.clamp(FIRST_ROOM + 1, 99);
        self.levels.truncate(ENTITY_NAMES.len());
        for l in &mut self.levels {
            *l = (*l).min(4);
        }
        self.rooms.truncate(crate::mapfile::MAX_ROOMS);
        for r in &mut self.rooms {
            *r = r.sanitized();
        }
        if !self.rooms.is_empty() {
            // A map's last door: one per room, plus the final hall's.
            self.doors = FIRST_ROOM + self.rooms.len() as u32 + 1;
        }
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
            chases: self.chases,
            shuffle: self.shuffle_areas,
            locks: self.locks as f32 / 100.0,
            structures: self.structures.map(|l| LEVEL_RATE[l.min(4) as usize]),
            reading_hall: self.reading_room,
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
            // Overall rate times this monster's own level (Mode Creator).
            let k = k * LEVEL_RATE[self.level(i) as usize];
            t.chance = if self.entity_on(i) && k > 0.0 { (t.chance * k).min(0.95) } else { 0.0 };
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

const B32: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

fn base32(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in bytes {
        acc = (acc << 8) | *b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn unbase32(s: &str) -> Option<Vec<u8>> {
    let mut out = vec![];
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        // Easily confused letters read as the digits they look like.
        let c = match c {
            b'O' => b'0',
            b'I' => b'1',
            c => c,
        };
        let v = B32.iter().position(|x| *x == c)? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Some(out)
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
    if cfg!(any(target_os = "android", target_os = "ios")) {
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
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(EXTENSION) || x.eq_ignore_ascii_case(crate::mapfile::MAP_EXTENSION))).collect())
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

/// File name for a preset: maps (with rooms) are `.hallwaysmap` files.
pub fn file_name_for(p: &Preset) -> String {
    if p.rooms.is_empty() { file_name(&p.name) } else { format!("{}.{}", clean_name(&p.name), crate::mapfile::MAP_EXTENSION) }
}

/// Writes the preset as `<name>.hallways` in `dir`; returns the path.
pub fn save(dir: &Path, p: &Preset) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file_name_for(p));
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
    fn share_codes_round_trip() {
        let p0 = Preset { name: "Mine".into(), doors: 45, room_size: 130, brightness: 40, dark_rooms: 200, fake_doors: 150, stair_rooms: 50, loot: 75, entities: 125, areas: vec![Area::Grounds, Area::Atrium], ..Preset::default() };
        let mut p = p0;
        p.set_level(0, 4);
        p.set_level(2, 0);
        p.set_level(5, 1);
        let code = p.share_code();
        assert!(code.starts_with("HW-") && code.len() < 64, "{code}");
        let back = Preset::from_share_code(&code.to_lowercase()).unwrap();
        for f in PresetField::ALL {
            assert_eq!(f.get(&back), f.get(&p), "{f:?}");
        }
        assert_eq!(back.areas, p.areas);
        assert_eq!((0..6).map(|i| back.level(i)).collect::<Vec<_>>(), vec![4, 2, 0, 2, 2, 1]);
        let mut q = p.clone();
        q.start_door = 20;
        q.chases = false;
        q.shuffle_areas = false;
        q.start_kit = 3;
        q.health = 150;
        q.speed = 120;
        q.hide_time = 0;
        q.lumens = 200;
        q.locks = 50;
        let back = Preset::from_share_code(&q.share_code()).unwrap();
        assert_eq!((back.start_door, back.chases, back.shuffle_areas, back.start_kit, back.health, back.speed, back.hide_time, back.lumens, back.locks), (20, false, false, 3, 150, 120, 0, 200, 50));
        assert!(back.player_made());
        let mut r = q.clone();
        r.revives = 2;
        r.sprint = false;
        r.items_off = 0b10010;
        r.structures = [0, 1, 2, 3, 4];
        r.reading_room = 33;
        r.custom = CustomMonster { on: true, color: 4, speed_min: 10, speed_max: 30, passes: 2, damage: 40, warning: 25, chance: 60, first_room: 12, ..CustomMonster::default() };
        let back = Preset::from_share_code(&r.share_code()).unwrap();
        assert_eq!((back.revives, back.sprint, back.items_off, back.structures, back.reading_room), (2, false, 0b10010, [0, 1, 2, 3, 4], 33));
        assert_eq!(back.custom, r.custom);
        // A typo is caught by the checksum.
        let mut bad = code.clone();
        bad.replace_range(5..6, if &code[5..6] == "A" { "B" } else { "A" });
        assert!(Preset::from_share_code(&bad).is_none());
        assert!(!builtin()[0].player_made(), "the classic floor isn't player-made");
    }

    #[test]
    fn levels_shape_the_monsters_and_the_difficulty() {
        assert_eq!(builtin()[0].difficulty_name(), "HARD", "the classic floor is hard");
        let mut p = Preset::default();
        let easy = p.difficulty();
        for i in 0..6 {
            p.set_level(i, 4);
        }
        p.brightness = 30;
        assert!(p.difficulty() > easy + 15);
        let mut cfg = crate::data::EntityConfig::default();
        let base = cfg.surge.chance;
        p.apply_entities(&mut cfg);
        assert!(cfg.surge.chance > base);
        p.set_level(0, 0);
        let mut cfg = crate::data::EntityConfig::default();
        p.apply_entities(&mut cfg);
        assert_eq!(cfg.surge.chance, 0.0);
    }

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
