//! The Play screen: solo run, host or join an online room by code, seed
//! entry, the pre-run shop (spend banked Lumens on a starting item) and
//! optional modifiers. Everything here is saved in the [`Profile`].

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use super::focus::{spawn_button, Focusable, MenuAction, MenuActivated};
use crate::items::ItemKind;
use crate::net::{NetRequest, NetSession, NetStatus, RoomReady};
use crate::profile::{save_profile, shop_price, BuyResult, Modifier, Profile};
use crate::rng::RunSeed;
use crate::state::{AppState, Overlay, Screen};
use crate::theme::UiTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Name,
    Seed,
    Code,
    AdminCode,
    PresetName,
    PresetSeed,
}

impl FieldKind {
    fn label(self) -> &'static str {
        match self {
            FieldKind::Name => "YOUR NAME",
            FieldKind::Seed => "SEED",
            FieldKind::Code => "ROOM CODE",
            FieldKind::AdminCode => "CODE",
            FieldKind::PresetName => "PRESET NAME",
            FieldKind::PresetSeed => "SEED",
        }
    }

    fn max_len(self) -> usize {
        match self {
            FieldKind::Name => 16,
            FieldKind::Seed => 24,
            FieldKind::Code => 5,
            FieldKind::AdminCode => 5,
            FieldKind::PresetName => 24,
            FieldKind::PresetSeed => 24,
        }
    }

    fn accepts(self, c: char) -> bool {
        match self {
            FieldKind::Name => c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'),
            FieldKind::Seed => c.is_ascii_graphic() || c == ' ',
            FieldKind::Code => c.is_ascii_alphanumeric(),
            FieldKind::AdminCode => c.is_ascii_digit(),
            FieldKind::PresetName => c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'),
            FieldKind::PresetSeed => c.is_ascii_graphic() || c == ' ',
        }
    }
}

/// A one-line text box. Focus it (click or arrow keys) and type.
#[derive(Component, Debug)]
pub struct TextField {
    pub kind: FieldKind,
    pub value: String,
}

#[derive(Component)]
struct FieldText;

/// True while a text box has focus: menu navigation ignores letter keys.
#[derive(Resource, Debug, Default)]
pub struct Typing(pub bool);

#[derive(Component)]
struct BuyLabel(ItemKind);
#[derive(Component)]
struct ModLabel(Modifier);
#[derive(Component)]
struct BankText;
#[derive(Component)]
struct StatusText;

/// Which online action is waiting for the server.
#[derive(Resource, Debug, Default, PartialEq)]
enum Pending {
    #[default]
    None,
    Online,
}

pub struct PlayMenuPlugin;

impl Plugin for PlayMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typing>()
            .init_resource::<Pending>()
            .add_systems(OnEnter(Screen::Play), spawn_play)
            .add_systems(OnExit(Screen::Play), |mut t: ResMut<Typing>, mut p: ResMut<Pending>| {
                t.0 = false;
                *p = Pending::None;
            })
            .add_systems(Update, type_into_fields.run_if(|s: Option<Res<State<Screen>>>| s.is_some_and(|s| matches!(s.get(), Screen::Play | Screen::Admin | Screen::Custom))))
            .add_systems(OnExit(Screen::Admin), |mut t: ResMut<Typing>| t.0 = false)
            .add_systems(Update, (play_actions, refresh_labels, room_ready).chain().after(type_into_fields).run_if(in_state(Screen::Play)));
    }
}

pub fn field_row(parent: &mut ChildSpawnerCommands, theme: &UiTheme, kind: FieldKind, value: &str, order: i32) {
    parent
        .spawn((
            Node {
                width: percent(100),
                padding: UiRect::axes(px(14), px(9)),
                border: UiRect::all(px(theme.border)),
                border_radius: BorderRadius::all(px(theme.radius)),
                column_gap: px(10),
                ..default()
            },
            BackgroundColor(theme.bg),
            BorderColor::all(theme.text_dim),
            Focusable(order),
            TextField { kind, value: value.to_string() },
        ))
        .with_children(|r| {
            r.spawn((Text::new(kind.label()), theme.font(true, theme.size_small), TextColor(theme.text_dim)));
            r.spawn((Text::new(value.to_string()), theme.font(true, theme.size_small), TextColor(theme.text), FieldText));
        });
}

fn spawn_play(mut commands: Commands, theme: Res<UiTheme>, profile: Res<Profile>) {
    let t = theme.clone();
    commands
        .spawn((t.screen_root(false), GlobalZIndex(20), DespawnOnExit(Screen::Play), Name::new("Play menu")))
        .with_children(|root| {
            root.spawn(t.heading_text("PLAY"));
            root.spawn(Node { column_gap: px(24), align_items: AlignItems::FlexStart, ..default() }).with_children(|cols| {
                // ---- Left: who / where ----
                cols.spawn(t.panel(430.0)).with_children(|p| {
                    field_row(p, &t, FieldKind::Name, &profile.name, 0);
                    p.spawn(t.dim_text("Click the box and type. Other players see this name over your head."));
                    field_row(p, &t, FieldKind::Seed, &profile.seed_text, 1);
                    p.spawn(t.dim_text("Leave the seed empty for a new floor every run."));
                    // First visit (default name): the name box gets the cursor instead.
                    spawn_button(p, &t, "Solo Run", MenuAction::PlaySolo, 2, profile.name != "Player");
                    spawn_button(p, &t, "Custom Floors", MenuAction::OpenCustom, 3, false);
                    p.spawn((Text::new("ONLINE (UP TO 4 PLAYERS)"), t.font(true, t.size_small), TextColor(t.interact), Node { margin: UiRect::top(px(10)), ..default() }));
                    spawn_button(p, &t, "Host Room", MenuAction::HostRoom, 4, false);
                    field_row(p, &t, FieldKind::Code, "", 5);
                    spawn_button(p, &t, "Join Room", MenuAction::JoinRoom, 6, false);
                    p.spawn((Text::new(""), t.font(false, t.size_small), TextColor(t.text_dim), StatusText, Node { max_width: px(400), ..default() }));
                    spawn_button(p, &t, "Back", MenuAction::ClosePlay, 7, false);
                });
                // ---- Right: shop and modifiers ----
                cols.spawn(t.panel(470.0)).with_children(|p| {
                    p.spawn((Text::new(""), t.font(true, t.size_body), TextColor(t.interact), BankText));
                    p.spawn(t.dim_text("Buy up to 3 items to start your next run with."));
                    for (i, kind) in ItemKind::ALL.into_iter().enumerate() {
                        let e = spawn_button(p, &t, "", MenuAction::Buy(kind), 10 + i as i32, false);
                        p.commands().entity(e).insert(BuyLabel(kind));
                    }
                    spawn_button(p, &t, "Refund Last", MenuAction::Refund, 16, false);
                    p.spawn((Text::new("MODIFIERS (MORE LUMENS)"), t.font(true, t.size_small), TextColor(t.interact), Node { margin: UiRect::top(px(10)), ..default() }));
                    for (i, m) in Modifier::ALL.into_iter().enumerate() {
                        let e = spawn_button(p, &t, "", MenuAction::ToggleModifier(m), 20 + i as i32, false);
                        p.commands().entity(e).insert(ModLabel(m));
                    }
                });
            });
        });
}

fn type_into_fields(
    mut focus: ResMut<InputFocus>,
    mut typing: ResMut<Typing>,
    mut keys: MessageReader<KeyboardInput>,
    mut fields: Query<(Entity, &mut TextField, &mut BorderColor, &Children)>,
    mut texts: Query<&mut Text, With<FieldText>>,
    theme: Res<UiTheme>,
    mut focus_once: Local<bool>,
    mut editor: Option<ResMut<crate::ui::custom_menu::CustomEditor>>,
    time: Res<Time<Real>>,
    mut profile: ResMut<Profile>,
) {
    // First visit with the default name: put the cursor in the name box.
    if !*focus_once {
        *focus_once = true;
        if profile.name == "Player" {
            if let Some((e, ..)) = fields.iter().find(|(_, f, ..)| f.kind == FieldKind::Name) {
                *focus = InputFocus::from_entity(e);
                // Start the default name blank so typing replaces it.
                if let Ok((_, mut f, ..)) = fields.get_mut(e) {
                    f.value.clear();
                }
            }
        }
    }
    let focused = focus.get();
    typing.0 = focused.is_some_and(|f| fields.contains(f));
    let events: Vec<KeyboardInput> = keys.read().cloned().collect();
    for (e, mut field, mut border, children) in &mut fields {
        let active = Some(e) == focused;
        *border = BorderColor::all(if active { theme.interact } else { theme.text_dim });
        if active {
            for ev in events.iter().filter(|ev| ev.state.is_pressed()) {
                match &ev.logical_key {
                    Key::Backspace => {
                        field.value.pop();
                    }
                    Key::Character(s) => {
                        for c in s.chars() {
                            if field.kind.accepts(c) && field.value.chars().count() < field.kind.max_len() {
                                let ch = if field.kind == FieldKind::Code { c.to_ascii_uppercase() } else { c };
                                field.value.push(ch);
                            }
                        }
                    }
                    Key::Space if field.kind != FieldKind::Code && field.value.len() < field.kind.max_len() => field.value.push(' '),
                    _ => {}
                }
            }
            match field.kind {
                // An emptied box keeps the old name until something is typed.
                FieldKind::Name if !field.value.trim().is_empty() => profile.name = field.value.clone(),
                FieldKind::Name => {}
                FieldKind::Seed => profile.seed_text = field.value.clone(),
                FieldKind::Code | FieldKind::AdminCode => {}
                FieldKind::PresetName => {
                    if let Some(ed) = editor.as_mut() {
                        ed.edit.name = field.value.clone();
                    }
                }
                FieldKind::PresetSeed => {
                    if let Some(ed) = editor.as_mut() {
                        ed.edit.seed = field.value.clone();
                    }
                }
            }
        }
        let caret = if active && (time.elapsed_secs() * 2.0) as i32 % 2 == 0 { "_" } else { " " };
        let shown = if field.value.is_empty() && !active {
            match field.kind {
                FieldKind::Seed => "random".to_string(),
                FieldKind::Code | FieldKind::AdminCode => "-----".to_string(),
                FieldKind::PresetSeed => "random".to_string(),
                FieldKind::Name | FieldKind::PresetName => String::new(),
            }
        } else {
            let v = if field.kind == FieldKind::AdminCode { "*".repeat(field.value.len()) } else { field.value.clone() };
            format!("{v}{caret}")
        };
        for c in children.iter() {
            if let Ok(mut t) = texts.get_mut(c) {
                if t.0 != shown {
                    t.0 = shown.clone();
                }
            }
        }
    }
}

fn item_name(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Flashlight => "Flashlight",
        ItemKind::Lockpick => "Lockpick",
        ItemKind::Vitamins => "Vitamins",
        ItemKind::Bandage => "Bandage",
        ItemKind::SkeletonKey => "Skeleton Key",
    }
}

/// A fresh seed from the clock (used when the seed box is empty).
pub fn random_seed() -> u64 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
    let mut r = crate::rng::GameRng::new(nanos ^ 0x9E37_79B9_7F4A_7C15);
    r.next_u64()
}

fn chosen_seed(profile: &Profile) -> u64 {
    if profile.seed_text.trim().is_empty() { random_seed() } else { RunSeed::from_text(&profile.seed_text).0 }
}

#[allow(clippy::too_many_arguments)]
fn play_actions(
    mut reader: MessageReader<MenuActivated>,
    mut profile: ResMut<Profile>,
    mut seed: ResMut<RunSeed>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
    mut net: MessageWriter<NetRequest>,
    mut pending: ResMut<Pending>,
    fields: Query<&TextField>,
    mut status: Query<&mut Text, With<StatusText>>,
    mut preset: ResMut<crate::presets::ActivePreset>,
) {
    let mut say = |s: String| {
        for mut t in &mut status {
            t.0 = s.clone();
        }
    };
    for MenuActivated(action) in reader.read() {
        match *action {
            MenuAction::ClosePlay => {
                save_profile(&profile);
                next_overlay.set(Overlay::None);
            }
            MenuAction::PlaySolo => {
                profile.name = crate::profile::clean_name(&profile.name);
                save_profile(&profile);
                *seed = RunSeed(chosen_seed(&profile));
                preset.0 = None;
                next_overlay.set(Overlay::None);
                next_app.set(AppState::Playing);
            }
            MenuAction::OpenCustom => next_overlay.set(Overlay::Custom),
            MenuAction::HostRoom => {
                save_profile(&profile);
                preset.0 = None;
                net.write(NetRequest::Host { name: crate::profile::clean_name(&profile.name), seed: chosen_seed(&profile), preset: None });
                *pending = Pending::Online;
                say("Creating a room... (a sleeping free server can take up to a minute to wake up)".into());
            }
            MenuAction::JoinRoom => {
                let code = fields.iter().find(|f| f.kind == FieldKind::Code).map(|f| f.value.clone()).unwrap_or_default();
                save_profile(&profile);
                net.write(NetRequest::Join { name: crate::profile::clean_name(&profile.name), code });
                *pending = Pending::Online;
                say("Joining...".into());
            }
            MenuAction::Buy(kind) => match profile.buy(kind) {
                BuyResult::Bought => {
                    save_profile(&profile);
                    say(format!("Bought: {}. It will be in your pockets when the run starts.", item_name(kind)));
                }
                BuyResult::TooExpensive => say(format!("Not enough Lumens (costs {}). Lumens you collect are banked when a run ends.", shop_price(kind))),
                BuyResult::LoadoutFull => say("You can only start with 3 items.".into()),
            },
            MenuAction::Refund => {
                if profile.refund_last() {
                    save_profile(&profile);
                    say("Refunded.".into());
                }
            }
            MenuAction::ToggleModifier(m) => {
                profile.toggle(m);
                save_profile(&profile);
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn refresh_labels(
    profile: Res<Profile>,
    session: Res<NetSession>,
    pending: Res<Pending>,
    buys: Query<(&BuyLabel, &Children)>,
    mods: Query<(&ModLabel, &Children)>,
    mut texts: ParamSet<(Query<&mut Text>, Query<&mut Text, With<BankText>>, Query<&mut Text, With<StatusText>>)>,
) {
    let loadout: Vec<&str> = profile.loadout.iter().map(|k| item_name(*k)).collect();
    let bank = format!(
        "LUMENS BANKED: {}    START WITH: {}",
        profile.lumens,
        if loadout.is_empty() { "nothing".into() } else { loadout.join(", ") }
    );
    for mut t in &mut texts.p1() {
        if t.0 != bank {
            t.0 = bank.clone();
        }
    }
    let set = |e: Entity, s: String, q: &mut Query<&mut Text>| {
        if let Ok(mut t) = q.get_mut(e) {
            if t.0 != s {
                t.0 = s;
            }
        }
    };
    let mut q = texts.p0();
    for (b, children) in &buys {
        if let Some(c) = children.first() {
            set(*c, format!("{}  -  {} LUMENS", item_name(b.0), shop_price(b.0)), &mut q);
        }
    }
    for (m, children) in &mods {
        if let Some(c) = children.first() {
            let on = if profile.has(m.0) { "[ON] " } else { "[OFF]" };
            set(*c, format!("{on} {}: {}  (+{:.0}%)", m.0.name().to_uppercase(), m.0.blurb(), m.0.bonus() * 100.0), &mut q);
        }
    }
    if *pending == Pending::Online {
        if let NetStatus::Error(e) = &session.status {
            for mut t in &mut texts.p2() {
                if t.0 != *e {
                    t.0 = e.clone();
                }
            }
        }
    }
}

pub fn room_ready(
    mut ready: MessageReader<RoomReady>,
    mut preset: ResMut<crate::presets::ActivePreset>,
    mut seed: ResMut<RunSeed>,
    mut next_app: ResMut<NextState<AppState>>,
    mut next_overlay: ResMut<NextState<Overlay>>,
) {
    if let Some(r) = ready.read().last() {
        *seed = RunSeed(r.seed);
        preset.0 = r.preset.clone();
        next_overlay.set(Overlay::None);
        next_app.set(AppState::Playing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_filter_input() {
        assert!(FieldKind::Code.accepts('a'));
        assert!(!FieldKind::Code.accepts('-'));
        assert!(!FieldKind::Name.accepts('<'));
        assert!(FieldKind::Seed.accepts('!'));
        assert_eq!(FieldKind::Code.max_len(), crate::net::proto::CODE_LEN);
    }

    #[test]
    fn seeds() {
        let p = Profile { seed_text: "basement".into(), ..default() };
        assert_eq!(chosen_seed(&p), RunSeed::from_text("basement").0);
        let empty = Profile::default();
        // Random seeds differ between calls (clock based).
        let a = chosen_seed(&empty);
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_ne!(a, chosen_seed(&empty));
    }
}
