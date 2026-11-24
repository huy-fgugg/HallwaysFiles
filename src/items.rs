//! Items and the 3-slot inventory.
//!
//! * Flashlight: `F` toggles; battery drains while on. A second flashlight
//!   recharges the battery instead of taking a slot.
//! * Lockpick / Skeleton Key: used automatically on a key-locked door (E).
//! * Vitamins: short speed boost. Bandage: restores vitality.
//!
//! Select with 1-3 / mouse wheel / bumpers; use the selected item with Q,
//! right mouse, or gamepad Y. Picking something up with a full inventory
//! replaces the selected slot, so think about what you carry.

use bevy::{input::mouse::AccumulatedMouseScroll, prelude::*};

use crate::audio::{PlaySfx, Sfx};
use crate::data::{ItemConfig, ItemTuning};
use crate::player::PlayerCamera;
use crate::run::{RunState, Subtitle};
use crate::state::{gameplay_active, AppState};

pub const SLOTS: usize = 3;
/// Every run begins with these.
pub const STARTING_ITEMS: [ItemKind; 2] = [ItemKind::Flashlight, ItemKind::Bandage];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ItemKind {
    Flashlight,
    Lockpick,
    Vitamins,
    Bandage,
    SkeletonKey,
}

impl ItemKind {
    pub const ALL: [ItemKind; 5] = [ItemKind::Flashlight, ItemKind::Lockpick, ItemKind::Vitamins, ItemKind::Bandage, ItemKind::SkeletonKey];

    pub fn tuning_mut(self, cfg: &mut ItemConfig) -> &mut ItemTuning {
        match self {
            ItemKind::Flashlight => &mut cfg.flashlight,
            ItemKind::Lockpick => &mut cfg.lockpick,
            ItemKind::Vitamins => &mut cfg.vitamins,
            ItemKind::Bandage => &mut cfg.bandage,
            ItemKind::SkeletonKey => &mut cfg.skeleton_key,
        }
    }

    pub fn tuning(self, cfg: &ItemConfig) -> &ItemTuning {
        match self {
            ItemKind::Flashlight => &cfg.flashlight,
            ItemKind::Lockpick => &cfg.lockpick,
            ItemKind::Vitamins => &cfg.vitamins,
            ItemKind::Bandage => &cfg.bandage,
            ItemKind::SkeletonKey => &cfg.skeleton_key,
        }
    }

    /// Short HUD label.
    pub fn short(self) -> &'static str {
        match self {
            ItemKind::Flashlight => "LIGHT",
            ItemKind::Lockpick => "PICK",
            ItemKind::Vitamins => "VITA",
            ItemKind::Bandage => "BAND",
            ItemKind::SkeletonKey => "KEY",
        }
    }

    pub fn color(self) -> Color {
        match self {
            ItemKind::Flashlight => Color::srgb(0.98, 0.86, 0.5),
            ItemKind::Lockpick => Color::srgb(0.7, 0.75, 0.82),
            ItemKind::Vitamins => Color::srgb(0.55, 0.9, 0.45),
            ItemKind::Bandage => Color::srgb(0.95, 0.5, 0.48),
            ItemKind::SkeletonKey => Color::srgb(0.85, 0.68, 0.3),
        }
    }

    /// Picks an item by configured weights.
    pub fn roll(cfg: &ItemConfig, r: f32) -> ItemKind {
        let weights: Vec<f32> = ItemKind::ALL.iter().map(|k| k.tuning(cfg).weight.max(0.0)).collect();
        let total: f32 = weights.iter().sum();
        let mut x = r * total;
        for (k, w) in ItemKind::ALL.iter().zip(&weights) {
            if x < *w {
                return *k;
            }
            x -= w;
        }
        ItemKind::Bandage
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stack {
    pub kind: ItemKind,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddResult {
    Stacked,
    Placed(usize),
    Replaced(ItemKind),
    Recharged,
    StackFull,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct Inventory {
    pub slots: [Option<Stack>; SLOTS],
    pub selected: usize,
    pub flashlight_on: bool,
    /// Seconds of flashlight battery left.
    pub battery: f32,
    pub battery_max: f32,
}

impl Inventory {
    pub fn has(&self, kind: ItemKind) -> bool {
        self.slots.iter().flatten().any(|s| s.kind == kind)
    }

    pub fn add(&mut self, kind: ItemKind, cfg: &ItemConfig) -> AddResult {
        let t = kind.tuning(cfg);
        if kind == ItemKind::Flashlight {
            if self.has(ItemKind::Flashlight) {
                self.battery = t.power;
                return AddResult::Recharged;
            }
            self.battery = t.power;
            self.battery_max = t.power;
        }
        if let Some(s) = self.slots.iter_mut().flatten().find(|s| s.kind == kind) {
            if s.count < t.max_stack {
                s.count += 1;
                return AddResult::Stacked;
            }
            return AddResult::StackFull;
        }
        if let Some(i) = self.slots.iter().position(Option::is_none) {
            self.slots[i] = Some(Stack { kind, count: 1 });
            return AddResult::Placed(i);
        }
        let old = self.slots[self.selected].map(|s| s.kind).unwrap_or(kind);
        if old == ItemKind::Flashlight {
            self.flashlight_on = false;
        }
        self.slots[self.selected] = Some(Stack { kind, count: 1 });
        AddResult::Replaced(old)
    }

    /// Consumes one of `kind`; returns false if none.
    pub fn take(&mut self, kind: ItemKind) -> bool {
        for slot in self.slots.iter_mut() {
            if let Some(s) = slot
                && s.kind == kind
            {
                s.count -= 1;
                if s.count == 0 {
                    *slot = None;
                }
                return true;
            }
        }
        false
    }
}

/// Remaining seconds of the Vitamins speed boost.
#[derive(Resource, Debug, Default)]
pub struct SpeedBoost(pub f32);

#[derive(Component)]
struct Flashlight;

/// Point-light "pool" the torch casts where you're looking. Renders on every
/// GPU (including software renderers that skip spot lights).
#[derive(Component)]
struct FlashlightPool;

pub struct ItemsPlugin;

impl Plugin for ItemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inventory>()
            .init_resource::<SpeedBoost>()
            .add_systems(OnEnter(AppState::Playing), |mut c: Commands, mut profile: ResMut<crate::profile::Profile>, cfg: Res<ItemConfig>, active: Res<crate::presets::ActivePreset>| {
                // Items bought in the shop are handed over once, at the start of a run.
                let mut inv = Inventory::default();
                // Nobody starts empty-handed: a flashlight and a bandage, plus shop
                // items (a mode can start you with nothing, or more).
                let kit = active.0.as_ref().map_or(0, |p| p.start_kit);
                let items: &[ItemKind] = match kit {
                    1 => &[],
                    2 => &[ItemKind::Flashlight, ItemKind::Bandage, ItemKind::Bandage, ItemKind::Vitamins],
                    3 => &[ItemKind::Flashlight, ItemKind::Bandage, ItemKind::Vitamins, ItemKind::Lockpick, ItemKind::SkeletonKey],
                    _ => &STARTING_ITEMS,
                };
                for k in items {
                    inv.add(*k, &cfg);
                }
                if !profile.loadout.is_empty() {
                    for k in std::mem::take(&mut profile.loadout) {
                        inv.add(k, &cfg);
                    }
                    crate::profile::save_profile(&profile);
                }
                c.insert_resource(inv);
                c.insert_resource(SpeedBoost::default());
            })
            .add_systems(Update, (attach_flashlight, (select_and_use, drain).chain().run_if(gameplay_active), drive_flashlight).run_if(in_state(AppState::Playing)))
            .add_systems(PostUpdate, follow_camera.after(bevy::transform::TransformSystems::Propagate).run_if(in_state(AppState::Playing)));
    }
}

/// The flashlight is a standalone spot light that follows the camera each
/// frame (parenting it to the camera breaks light visibility propagation).
fn attach_flashlight(mut commands: Commands, cams: Query<Entity, Added<PlayerCamera>>) {
    for _ in &cams {
        commands.spawn((
            SpotLight {
                color: Color::srgb(1.0, 0.95, 0.85),
                intensity: 0.0,
                range: 25.0,
                radius: 0.02,
                inner_angle: 0.16,
                outer_angle: 0.38,
                // No shadow map: the beam must work on every GPU, and shadow
                // slots are reserved for the room lamps.
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::default(),
            Flashlight,
            DespawnOnExit(AppState::Playing),
            Name::new("Flashlight"),
        ));
        commands.spawn((
            PointLight { color: Color::srgb(1.0, 0.95, 0.85), intensity: 0.0, range: 7.0, radius: 0.05, shadow_maps_enabled: false, ..default() },
            Transform::default(),
            FlashlightPool,
            DespawnOnExit(AppState::Playing),
            Name::new("Flashlight pool"),
        ));
    }
}

fn follow_camera(
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    colliders: Query<&crate::physics::Collider>,
    mut light: Query<&mut Transform, (With<Flashlight>, Without<FlashlightPool>)>,
    mut pool: Query<&mut Transform, (With<FlashlightPool>, Without<Flashlight>)>,
) {
    let Ok(cam) = cam.single() else { return };
    let (_, rot, pos) = cam.to_scale_rotation_translation();
    for mut tf in &mut light {
        // Starts just past the first-person hands so it lights the room, not your own arms.
        tf.translation = pos + rot * Vec3::new(0.12, -0.12, -0.62);
        tf.rotation = rot;
    }
    // The pool sits ~2 m ahead (well past your hands), pulled back if a wall is closer.
    let fwd = rot * Vec3::NEG_Z;
    let mut dist: f32 = 2.1;
    for c in &colliders {
        if let Some(t) = c.0.ray_hit(pos, fwd, dist)
            && !c.0.contains(pos)
        {
            dist = dist.min((t - 0.25).max(0.2));
        }
    }
    for mut tf in &mut pool {
        tf.translation = pos + fwd * dist;
    }
}

#[allow(clippy::too_many_arguments)]
fn select_and_use(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    pads: Query<&Gamepad>,
    cfg: Res<ItemConfig>,
    mut inv: ResMut<Inventory>,
    mut boost: ResMut<SpeedBoost>,
    mut run: ResMut<RunState>,
    mut sfx: MessageWriter<PlaySfx>,
    mut subs: MessageWriter<Subtitle>,
    mut arms: MessageWriter<crate::viewmodel::ArmAnim>,
) {
    for (i, k) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3].into_iter().enumerate() {
        if keys.just_pressed(k) {
            inv.selected = i;
        }
    }
    let mut cycle = 0i32;
    if scroll.delta.y > 0.0 {
        cycle -= 1;
    } else if scroll.delta.y < 0.0 {
        cycle += 1;
    }
    for p in &pads {
        if p.just_pressed(GamepadButton::LeftTrigger2) || p.just_pressed(GamepadButton::DPadLeft) {
            cycle -= 1;
        }
        if p.just_pressed(GamepadButton::RightTrigger2) || p.just_pressed(GamepadButton::DPadRight) {
            cycle += 1;
        }
    }
    if cycle != 0 {
        inv.selected = (inv.selected as i32 + cycle).rem_euclid(SLOTS as i32) as usize;
    }
    // Flashlight toggle.
    let toggle = keys.just_pressed(KeyCode::KeyF) || pads.iter().any(|p| p.just_pressed(GamepadButton::DPadUp));
    if toggle {
        if inv.has(ItemKind::Flashlight) && inv.battery > 0.0 {
            inv.flashlight_on = !inv.flashlight_on;
            sfx.write(PlaySfx(Sfx::UiMove));
        } else if inv.has(ItemKind::Flashlight) {
            subs.write(Subtitle::info("The flashlight is dead."));
        } else {
            subs.write(Subtitle::info("You don't have a flashlight. Search the drawers."));
        }
    }
    // Use the selected item.
    let use_pressed = keys.just_pressed(KeyCode::KeyQ) || mouse.just_pressed(MouseButton::Right) || pads.iter().any(|p| p.just_pressed(GamepadButton::North));
    if !use_pressed {
        return;
    }
    let Some(stack) = inv.slots[inv.selected] else { return };
    if matches!(stack.kind, ItemKind::Flashlight | ItemKind::Vitamins) || (stack.kind == ItemKind::Bandage && run.vitality < 100.0) {
        arms.write(crate::viewmodel::ArmAnim(crate::viewmodel::Gesture::Use));
    }
    match stack.kind {
        ItemKind::Flashlight => {
            if inv.battery > 0.0 {
                inv.flashlight_on = !inv.flashlight_on;
            }
        }
        ItemKind::Vitamins => {
            inv.take(ItemKind::Vitamins);
            boost.0 = cfg.vitamins.power;
            sfx.write(PlaySfx(Sfx::Pickup));
            subs.write(Subtitle::info(format!("Vitamins: faster for {:.0} seconds.", cfg.vitamins.power)));
        }
        ItemKind::Bandage => {
            if run.vitality >= 100.0 {
                subs.write(Subtitle::info("You're not hurt."));
            } else {
                inv.take(ItemKind::Bandage);
                run.vitality = (run.vitality + cfg.bandage.power).min(100.0);
                sfx.write(PlaySfx(Sfx::Pickup));
                subs.write(Subtitle::info(format!("Bandaged up (+{:.0} vitality).", cfg.bandage.power)));
            }
        }
        ItemKind::Lockpick | ItemKind::SkeletonKey => {
            subs.write(Subtitle::info("Look at a locked door and press E to use it."));
        }
    }
}

fn drain(time: Res<Time>, mut inv: ResMut<Inventory>, mut boost: ResMut<SpeedBoost>, mut subs: MessageWriter<Subtitle>) {
    let dt = time.delta_secs();
    boost.0 = (boost.0 - dt).max(0.0);
    if inv.flashlight_on {
        inv.battery = (inv.battery - dt).max(0.0);
        if inv.battery <= 0.0 {
            inv.flashlight_on = false;
            subs.write(Subtitle::info("[Click] The flashlight dies."));
        }
    }
}

fn drive_flashlight(inv: Res<Inventory>, time: Res<Time>, mut lights: Query<(&mut SpotLight, &GlobalTransform), With<Flashlight>>, mut pools: Query<&mut PointLight, With<FlashlightPool>>, mut last: Local<f32>) {
    if std::env::var_os("HALLWAYS_DEBUG_LIGHT").is_some() && time.elapsed_secs() - *last > 1.0 {
        *last = time.elapsed_secs();
        for (l, g) in &lights {
            info!("flashlight dbg: on={} battery={:.0} intensity={:.0} pos={:?} fwd={:?}", inv.flashlight_on, inv.battery, l.intensity, g.translation(), g.forward());
        }
    }
    // Sputters when the battery is nearly flat.
    let sputter = if inv.flashlight_on && inv.battery < 12.0 && (time.elapsed_secs() * 13.0).sin() > 0.6 { 0.35 } else { 1.0 };
    for (mut l, _) in &mut lights {
        // Bevy spot lights spread their lumens like a point light, so a
        // believable beam needs a big number.
        l.intensity = if inv.flashlight_on { 900_000.0 * sputter } else { 0.0 };
    }
    for mut p in &mut pools {
        p.intensity = if inv.flashlight_on { 600_000.0 * sputter } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ItemConfig {
        ItemConfig::default()
    }

    #[test]
    fn stacking_and_limits() {
        let c = cfg();
        let mut inv = Inventory::default();
        assert_eq!(inv.add(ItemKind::Bandage, &c), AddResult::Placed(0));
        assert_eq!(inv.add(ItemKind::Bandage, &c), AddResult::Stacked);
        assert_eq!(inv.add(ItemKind::Bandage, &c), AddResult::Stacked);
        assert_eq!(inv.add(ItemKind::Bandage, &c), AddResult::StackFull);
        assert_eq!(inv.slots[0].unwrap().count, 3);
    }

    #[test]
    fn full_inventory_replaces_selected() {
        let c = cfg();
        let mut inv = Inventory::default();
        inv.add(ItemKind::Bandage, &c);
        inv.add(ItemKind::Lockpick, &c);
        inv.add(ItemKind::Vitamins, &c);
        inv.selected = 1;
        assert_eq!(inv.add(ItemKind::SkeletonKey, &c), AddResult::Replaced(ItemKind::Lockpick));
        assert!(inv.has(ItemKind::SkeletonKey));
        assert!(!inv.has(ItemKind::Lockpick));
    }

    #[test]
    fn second_flashlight_recharges() {
        let c = cfg();
        let mut inv = Inventory::default();
        inv.add(ItemKind::Flashlight, &c);
        inv.battery = 3.0;
        assert_eq!(inv.add(ItemKind::Flashlight, &c), AddResult::Recharged);
        assert_eq!(inv.battery, c.flashlight.power);
        assert_eq!(inv.slots.iter().flatten().count(), 1);
    }

    #[test]
    fn take_consumes_and_clears() {
        let c = cfg();
        let mut inv = Inventory::default();
        inv.add(ItemKind::Lockpick, &c);
        assert!(inv.take(ItemKind::Lockpick));
        assert!(!inv.take(ItemKind::Lockpick));
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn roll_covers_all_items() {
        let c = cfg();
        let mut seen = std::collections::HashSet::new();
        for i in 0..1000 {
            seen.insert(ItemKind::roll(&c, i as f32 / 1000.0));
        }
        assert_eq!(seen.len(), ItemKind::ALL.len());
    }
}
