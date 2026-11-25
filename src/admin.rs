//! Admin panel (F8 in a run, or Pause → Admin Panel). Locked behind a code.
//! Once unlocked for the session it can spawn items for yourself, and kill,
//! revive or jumpscare any player in the room (including yourself offline).
//!
//! This is a party/testing tool for friends playing together: the code ships
//! inside the game, so it is not real security.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use serde::{Deserialize, Serialize};

use crate::audio::{PlaySfx, Sfx};
use crate::items::{Inventory, ItemKind};
use crate::net::{NetSession, PeerEvent, PeerMsg, SendPeer};
use crate::run::{CameraShake, RunState, Subtitle};
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;
use crate::ui::focus::{spawn_button, InitialFocus, MenuAction, MenuActivated};
use crate::ui::play_menu::{field_row, FieldKind, TextField};

pub const ADMIN_CODE: &str = "58470";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminAction {
    Kill,
    Revive,
    Jumpscare,
}

/// Unlocked for the rest of the session once the right code is entered.
#[derive(Resource, Debug, Default)]
pub struct AdminUnlocked(pub bool);

/// Something an admin did to this player.
#[derive(Message, Debug, Clone)]
pub struct AdminEffect {
    pub action: AdminAction,
    pub by: String,
}

/// Seconds left on the jumpscare overlay.
#[derive(Resource, Debug, Default)]
struct Jumpscare(f32);

#[derive(Component)]
struct ScareOverlay;
#[derive(Component)]
struct ScareFace;
#[derive(Component)]
struct AdminMessage;

const SCARE_TIME: f32 = 1.3;

pub struct AdminPlugin;

impl Plugin for AdminPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AdminUnlocked>()
            .init_resource::<Jumpscare>()
            .add_message::<AdminEffect>()
            .add_systems(OnEnter(Screen::Admin), spawn_panel)
            .add_systems(OnEnter(AppState::Playing), spawn_scare_overlay)
            .add_systems(
                Update,
                (
                    open_panel.run_if(in_state(Screen::Hud)),
                    (submit_code, panel_actions, close_on_escape).chain().run_if(in_state(Screen::Admin)),
                    receive_admin,
                    apply_effects,
                    animate_scare,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

fn open_panel(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<Overlay>>) {
    if keys.just_pressed(KeyCode::F8) {
        next.set(Overlay::Admin);
    }
}

fn close_on_escape(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<Overlay>>) {
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::F8) {
        next.set(Overlay::None);
    }
}

/// (id, name) of everyone the panel can target; id 0 = you when offline.
fn targets(session: &NetSession, me: &str) -> Vec<(u8, String)> {
    let me = crate::profile::clean_name(me);
    if !session.online() {
        return vec![(0, format!("{me} (you)"))];
    }
    let mut v: Vec<(u8, String)> =
        session.players.iter().map(|p| (p.id, if p.id == session.my_id { format!("{} (you)", p.name) } else { p.name.clone() })).collect();
    v.sort_by_key(|(id, _)| *id);
    v
}

/// Root of the admin panel (rebuilt in place when it unlocks).
#[derive(Component)]
struct AdminRoot;

fn spawn_panel(mut commands: Commands, theme: Res<UiTheme>, unlocked: Res<AdminUnlocked>, session: Res<NetSession>, profile: Res<crate::profile::Profile>) {
    build_panel(&mut commands, &theme, unlocked.0, &session, &profile.name);
}

fn build_panel(commands: &mut Commands, theme: &UiTheme, unlocked: bool, session: &NetSession, me: &str) {
    let t = theme.clone();
    commands
        .spawn((t.screen_root(false), GlobalZIndex(30), DespawnOnExit(Screen::Admin), AdminRoot, Name::new("Admin panel")))
        .with_children(|root| {
            root.spawn(t.heading_text("ADMIN PANEL"));
            // Never squash the panel (it scrolls if it really is too tall).
            root.spawn(t.panel(560.0)).entry::<Node>().and_modify(|mut n| n.flex_shrink = 0.0).entity().with_children(|p| {
                if !unlocked {
                    p.spawn(t.dim_text("Enter the admin code, then press Enter or Unlock."));
                    // The code box is selected straight away: just type.
                    let field = field_row(p, &t, FieldKind::AdminCode, "", 0);
                    p.commands().entity(field).insert(InitialFocus);
                    spawn_button(p, &t, "Unlock", MenuAction::AdminUnlock, 1, false);
                    p.spawn((Text::new(""), t.font(false, t.size_small), TextColor(t.danger), AdminMessage));
                    spawn_button(p, &t, "Close", MenuAction::CloseAdmin, 2, false);
                    return;
                }
                p.spawn((Text::new("SPAWN ITEM (FOR YOU)"), t.font(true, t.size_small), TextColor(t.interact)));
                // Fixed rows of three (a wrapping row is measured as one line,
                // which squashed the panel and hid the buttons below it).
                let items: Vec<ItemKind> = ItemKind::ALL.into_iter().collect();
                for (r, chunk) in items.chunks(3).enumerate() {
                    p.spawn(Node { column_gap: px(8), ..default() }).with_children(|row| {
                        for (j, k) in chunk.iter().copied().enumerate() {
                            let i = r * 3 + j;
                            let id = spawn_button(row, &t, item_label(k), MenuAction::AdminSpawn(k), i as i32, i == 0);
                            row.commands().entity(id).entry::<Node>().and_modify(|mut n| {
                                n.width = px(170);
                                n.min_width = px(170);
                            });
                        }
                    });
                }
                p.spawn((Text::new("PLAYERS"), t.font(true, t.size_small), TextColor(t.interact), Node { margin: UiRect::top(px(10)), ..default() }));
                for (row_i, (id, name)) in targets(session, me).into_iter().enumerate() {
                    p.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                        row.spawn((Text::new(name), t.font(true, t.size_small), TextColor(t.text), Node { width: px(170), ..default() }));
                        let base = 10 + row_i as i32 * 3;
                        for (j, (label, action)) in
                            [("Kill", AdminAction::Kill), ("Revive", AdminAction::Revive), ("Scare", AdminAction::Jumpscare)].into_iter().enumerate()
                        {
                            let b = spawn_button(row, &t, label, MenuAction::Admin(id, action), base + j as i32, false);
                            row.commands().entity(b).entry::<Node>().and_modify(|mut n| {
                                n.width = px(120);
                                n.min_width = px(120);
                            });
                        }
                    });
                }
                p.spawn((Text::new(""), t.font(false, t.size_small), TextColor(t.text_dim), AdminMessage));
                let close = spawn_button(p, &t, "Close", MenuAction::CloseAdmin, 100, false);
                p.commands().entity(close).entry::<Node>().and_modify(|mut n| {
                    n.min_height = px(46);
                    n.flex_shrink = 0.0;
                });
            });
        });
}

fn item_label(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Flashlight => "Flashlight",
        ItemKind::Lockpick => "Lockpick",
        ItemKind::Vitamins => "Vitamins",
        ItemKind::Bandage => "Bandage",
        ItemKind::SkeletonKey => "Skeleton Key",
        ItemKind::Battery => "Battery",
        ItemKind::MedKit => "Med Kit",
        ItemKind::LuckyCharm => "Lucky Charm",
    }
}

#[allow(clippy::too_many_arguments)]
/// Enter in the code box unlocks (no need to move to the button).
fn submit_code(keys: Res<ButtonInput<KeyCode>>, focus: Res<bevy::input_focus::InputFocus>, fields: Query<&TextField>, mut out: MessageWriter<MenuActivated>) {
    if !keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        return;
    }
    if focus.get().and_then(|e| fields.get(e).ok()).is_some_and(|f| f.kind == FieldKind::AdminCode) {
        out.write(MenuActivated(MenuAction::AdminUnlock));
    }
}

#[allow(clippy::too_many_arguments)]
fn panel_actions(
    mut commands: Commands,
    theme: Res<UiTheme>,
    roots: Query<Entity, With<AdminRoot>>,
    mut reader: MessageReader<MenuActivated>,
    mut unlocked: ResMut<AdminUnlocked>,
    mut next: ResMut<NextState<Overlay>>,
    fields: Query<&TextField>,
    mut msg: Query<&mut Text, With<AdminMessage>>,
    mut inv: ResMut<Inventory>,
    cfg: Res<crate::data::ItemConfig>,
    session: Res<NetSession>,
    profile: Res<crate::profile::Profile>,
    mut out: MessageWriter<SendPeer>,
    mut effects: MessageWriter<AdminEffect>,
) {
    let mut say = |s: String| {
        for mut t in &mut msg {
            t.0 = s.clone();
        }
    };
    for MenuActivated(a) in reader.read() {
        match *a {
            MenuAction::CloseAdmin => next.set(Overlay::None),
            MenuAction::AdminUnlock => {
                let code = fields.iter().find(|f| f.kind == FieldKind::AdminCode).map(|f| f.value.clone()).unwrap_or_default();
                if code == ADMIN_CODE {
                    unlocked.0 = true;
                    // Rebuild the panel in place with the unlocked controls.
                    for e in &roots {
                        commands.entity(e).try_despawn();
                    }
                    build_panel(&mut commands, &theme, true, &session, &profile.name);
                } else {
                    say("Wrong code.".into());
                }
            }
            MenuAction::AdminSpawn(k) if unlocked.0 => {
                let _ = inv.add(k, &cfg);
                say(format!("Spawned: {}.", item_label(k)));
            }
            MenuAction::Admin(target, action) if unlocked.0 => {
                let me = !session.online() || target == session.my_id;
                if me {
                    effects.write(AdminEffect { action, by: "You".into() });
                } else {
                    out.write(SendPeer(PeerMsg::Admin { target, action }));
                }
                let who = if me { "yourself".to_string() } else { session.name_of(target) };
                say(match action {
                    AdminAction::Kill => format!("Killed {who}."),
                    AdminAction::Revive => format!("Revived {who}."),
                    AdminAction::Jumpscare => format!("Jumpscared {who}."),
                });
                // Close the panel when targeting yourself so you can see it happen.
                if me {
                    next.set(Overlay::None);
                }
                let _ = &profile;
            }
            _ => {}
        }
    }
}

fn receive_admin(session: Res<NetSession>, mut events: MessageReader<PeerEvent>, mut effects: MessageWriter<AdminEffect>) {
    for ev in events.read() {
        if let PeerMsg::Admin { target, action } = &ev.msg {
            if *target == session.my_id {
                effects.write(AdminEffect { action: *action, by: session.name_of(ev.from) });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_effects(
    mut effects: MessageReader<AdminEffect>,
    mut run: ResMut<RunState>,
    overlay: Res<State<Overlay>>,
    mut next: ResMut<NextState<Overlay>>,
    mut death: ResMut<crate::hollow::DeathCause>,
    mut chase: ResMut<crate::hollow::ChaseState>,
    mut scare: ResMut<Jumpscare>,
    mut sfx: MessageWriter<PlaySfx>,
    mut shake: ResMut<CameraShake>,
    mut subs: MessageWriter<Subtitle>,
    settings: Res<crate::settings::Settings>,
) {
    for e in effects.read() {
        match e.action {
            AdminAction::Kill => {
                if *overlay.get() != Overlay::Dead {
                    run.vitality = 0.0;
                    *death = crate::hollow::DeathCause {
                        title: "AN ADMIN ENDED YOUR RUN".into(),
                        cause: format!("{} used the admin panel on you.", e.by),
                        lesson: "Not your fault. Retry, or wait for someone to revive you.".into(),
                    };
                    next.set(Overlay::Dead);
                }
            }
            AdminAction::Revive => {
                *chase = crate::hollow::ChaseState::default();
                run.vitality = 100.0;
                if *overlay.get() == Overlay::Dead {
                    next.set(Overlay::None);
                }
                subs.write(Subtitle::info(format!("{} revived you.", e.by)));
            }
            AdminAction::Jumpscare => {
                scare.0 = SCARE_TIME;
                sfx.write(PlaySfx(Sfx::Stinger));
                if !settings.limit_loud_sounds {
                    sfx.write(PlaySfx(Sfx::SurgeRoar));
                }
                shake.add(1.0);
            }
        }
    }
}

/// A full-screen face: an original cracked porcelain mask with hollow eyes.
fn scare_image() -> Image {
    const N: usize = 256;
    let mut px = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let u = (x as f32 / N as f32) * 2.0 - 1.0;
            let v = (y as f32 / N as f32) * 2.0 - 1.0;
            let face = (u / 0.72).powi(2) + ((v + 0.05) / 0.92).powi(2) < 1.0;
            let eye = |cx: f32| ((u - cx) / 0.2).powi(2) + ((v + 0.18) / 0.27).powi(2) < 1.0;
            let pupil = |cx: f32| ((u - cx) / 0.035).powi(2) + ((v + 0.14) / 0.035).powi(2) < 1.0;
            let mouth = (u / 0.42).powi(2) + ((v - 0.45) / 0.2).powi(2) < 1.0;
            let tooth = mouth && v < 0.42 && ((u * 14.0).fract().abs() < 0.45) && v > 0.3 - (u * 14.0).fract().abs() * 0.1;
            let crack = ((u * 9.0 + v * 3.0).sin() * 0.5 - (v * 7.0 - u * 2.0).cos() * 0.4).abs() < 0.03 && v < 0.3;
            let mut c = [0u8, 0, 0, 0];
            if face {
                let shade = (1.0 - (u * u + v * v) * 0.35).clamp(0.4, 1.0);
                c = [(232.0 * shade) as u8, (226.0 * shade) as u8, (214.0 * shade) as u8, 255];
                if crack {
                    c = [60, 52, 48, 255];
                }
                if eye(-0.3) || eye(0.3) {
                    c = [6, 4, 4, 255];
                }
                if pupil(-0.3) || pupil(0.3) {
                    c = [250, 245, 235, 255];
                }
                if mouth {
                    c = if tooth { [210, 200, 180, 255] } else { [10, 4, 4, 255] };
                }
            }
            let i = (y * N + x) * 4;
            px[i..i + 4].copy_from_slice(&c);
        }
    }
    Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

fn spawn_scare_overlay(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let face = images.add(scare_image());
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::NONE),
            GlobalZIndex(90),
            Visibility::Hidden,
            Pickable::IGNORE,
            crate::theme::PassThrough,
            ScareOverlay,
            DespawnOnExit(AppState::Playing),
        ))
        .with_child((ImageNode::new(face), Node { height: percent(100), aspect_ratio: Some(1.0), ..default() }, ScareFace));
}

fn animate_scare(
    time: Res<Time<Real>>,
    mut scare: ResMut<Jumpscare>,
    settings: Res<crate::settings::Settings>,
    mut overlay: Query<(&mut Visibility, &mut BackgroundColor), With<ScareOverlay>>,
    mut face: Query<&mut Node, With<ScareFace>>,
) {
    let active = scare.0 > 0.0;
    scare.0 = (scare.0 - time.delta_secs()).max(0.0);
    let k = 1.0 - scare.0 / SCARE_TIME;
    for (mut v, mut bg) in &mut overlay {
        *v = if active { Visibility::Inherited } else { Visibility::Hidden };
        // Red-black flicker unless Reduce Flashing is on.
        let flick = if settings.reduce_flashing { 0.85 } else if (k * 30.0) as i32 % 2 == 0 { 0.95 } else { 0.7 };
        bg.0 = Color::srgba(0.25 * (1.0 - k), 0.0, 0.0, flick);
    }
    for mut n in &mut face {
        // Lunges toward the camera.
        n.height = percent(70.0 + 60.0 * k.min(0.4) / 0.4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_and_targets() {
        assert_eq!(ADMIN_CODE, "58470");
        let mut s = NetSession::default();
        assert_eq!(targets(&s, "Ana"), vec![(0, "Ana (you)".to_string())]);
        s.status = crate::net::NetStatus::InRoom;
        s.my_id = 2;
        s.players = vec![crate::net::proto::PlayerInfo { id: 2, name: "Ana".into() }, crate::net::proto::PlayerInfo { id: 1, name: "Bo".into() }];
        assert_eq!(targets(&s, "Ana"), vec![(1, "Bo".to_string()), (2, "Ana (you)".to_string())]);
    }

    #[test]
    fn admin_messages_round_trip() {
        let m = PeerMsg::Admin { target: 3, action: AdminAction::Jumpscare };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(serde_json::from_value::<PeerMsg>(v).unwrap(), m);
    }

    #[test]
    fn scare_face_has_eyes() {
        let img = scare_image();
        let data = img.data.unwrap();
        let dark = data.chunks(4).filter(|p| p[3] == 255 && p[0] < 20).count();
        assert!(dark > 2000);
    }
}
