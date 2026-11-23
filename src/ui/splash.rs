//! Studio splash: "CREATED BY DONUT STUDIOS" with the studio logo, shown
//! first when the game starts. Any key, click, tap or button skips it.

use bevy::prelude::*;

use crate::theme::UiTheme;
use crate::ui::menus::MenuClock;

const FADE_IN: f32 = 0.6;
const HOLD: f32 = 2.4;
const FADE_OUT: f32 = 0.7;

#[derive(Component)]
struct Splash {
    t: f32,
}

#[derive(Component)]
struct SplashPart(Color);

pub struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        // Scripted QA runs and the trailer recorder start straight at the menu.
        let skip = ["HALLWAYS_QA", "HALLWAYS_BOT", "HALLWAYS_WARP", "HALLWAYS_TRAILER", "HALLWAYS_CAPTURE", "HALLWAYS_FAKE_TOUCH", "HALLWAYS_NO_SPLASH"].iter().any(|v| std::env::var_os(v).is_some());
        if skip {
            return;
        }
        app.add_systems(Startup, spawn_splash).add_systems(PreUpdate, (skip_splash.after(bevy::input::InputSystems), animate_splash).chain());
    }
}

/// Alpha of the splash contents at time `t` (0 → 1 → 0).
pub fn splash_alpha(t: f32) -> f32 {
    if t < FADE_IN {
        t / FADE_IN
    } else if t < FADE_IN + HOLD {
        1.0
    } else {
        (1.0 - (t - FADE_IN - HOLD) / FADE_OUT).max(0.0)
    }
}

fn spawn_splash(mut commands: Commands, theme: Res<UiTheme>, assets: Res<AssetServer>) {
    let t = theme.clone();
    let white = Color::srgb(0.93, 0.91, 0.95);
    let pink = Color::srgb(1.0, 0.45, 0.85);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: vh(2.5),
                ..default()
            },
            BackgroundColor(Color::srgb(0.03, 0.02, 0.05)),
            GlobalZIndex(300),
            Splash { t: 0.0 },
            Name::new("Studio splash"),
        ))
        .with_children(|root| {
            root.spawn(Node { column_gap: px(16), ..default() }).with_children(|line| {
                line.spawn((Text::new("CREATED BY"), t.font(true, 40.0), TextColor(white.with_alpha(0.0)), SplashPart(white)));
                line.spawn((Text::new("DONUT STUDIOS!"), t.font(true, 40.0), TextColor(pink.with_alpha(0.0)), SplashPart(pink)));
            });
            root.spawn((
                ImageNode { image: assets.load("branding/donut_studios.png"), color: Color::WHITE.with_alpha(0.0), ..default() },
                Node { height: vh(64), aspect_ratio: Some(614.0 / 768.0), ..default() },
                SplashPart(Color::WHITE),
            ));
        });
}

fn skip_splash(
    mut splash: Query<&mut Splash>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    pads: Query<&Gamepad>,
) {
    let Ok(mut s) = splash.single_mut() else { return };
    // Ignore the first moment so a held key from launching doesn't skip it.
    if s.t < 0.3 || s.t >= FADE_IN + HOLD {
        return;
    }
    let pressed = keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some() || touches.any_just_pressed() || pads.iter().any(|p| p.get_just_pressed().next().is_some());
    if pressed {
        s.t = FADE_IN + HOLD;
        // The press that skips the splash doesn't also press a menu button.
        keys.reset_all();
        mouse.reset_all();
    }
}

#[allow(clippy::type_complexity)]
fn animate_splash(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut splash: Query<(Entity, &mut Splash, &mut BackgroundColor)>,
    mut texts: Query<(&SplashPart, &mut TextColor)>,
    mut images: Query<(&SplashPart, &mut ImageNode)>,
    mut menu: ResMut<MenuClock>,
) {
    let Ok((e, mut s, mut bg)) = splash.single_mut() else { return };
    // The first frames can be long while everything loads; don't skip ahead.
    s.t += time.delta_secs().min(1.0 / 20.0);
    let a = splash_alpha(s.t);
    for (part, mut c) in &mut texts {
        c.0 = part.0.with_alpha(a);
    }
    for (part, mut img) in &mut images {
        img.color = part.0.with_alpha(a);
    }
    // The menu's own intro waits until the splash is fading away.
    if s.t < FADE_IN + HOLD {
        menu.0 = 0.0;
    } else {
        let k = ((s.t - FADE_IN - HOLD) / FADE_OUT).clamp(0.0, 1.0);
        bg.0 = bg.0.with_alpha(1.0 - k);
    }
    if s.t >= FADE_IN + HOLD + FADE_OUT {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_fades_in_holds_and_out() {
        assert_eq!(splash_alpha(0.0), 0.0);
        assert_eq!(splash_alpha(FADE_IN + 1.0), 1.0);
        assert_eq!(splash_alpha(FADE_IN + HOLD + FADE_OUT + 0.1), 0.0);
    }

    #[test]
    fn logo_is_shipped() {
        assert!(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/branding/donut_studios.png")).exists());
    }
}
