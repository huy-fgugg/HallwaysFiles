//! After a crash, the next launch shows what went wrong (the error from
//! `crash.log`), so players on phones, where the settings folder is hidden,
//! can screenshot it and report it. Tap, click or press any key to close.

use bevy::prelude::*;

use crate::settings::config_dir;
use crate::state::AppState;
use crate::theme::UiTheme;

#[derive(Component)]
struct CrashPanel;

pub struct CrashReportPlugin;

impl Plugin for CrashReportPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), show_once).add_systems(Update, close_panel);
    }
}

/// The useful part of a crash log: the header and the panic message, without
/// the long backtrace.
pub fn summary(log: &str) -> String {
    let head: Vec<&str> = log.lines().take_while(|l| !l.starts_with("Backtrace")).filter(|l| !l.trim().is_empty()).take(8).collect();
    let mut s = head.join("\n");
    if s.len() > 700 {
        s.truncate(700);
        s.push_str("...");
    }
    s
}

fn show_once(mut commands: Commands, theme: Res<UiTheme>, mut shown: Local<bool>) {
    if *shown {
        return;
    }
    *shown = true;
    let dir = config_dir();
    let path = dir.join("crash.log");
    let Ok(log) = std::fs::read_to_string(&path) else { return };
    // Keep it as the last crash, but don't show it again.
    let _ = std::fs::rename(&path, dir.join("crash_last.log"));
    let t = theme.clone();
    commands
        .spawn((t.screen_root(true), GlobalZIndex(200), CrashPanel, Name::new("Crash report")))
        .with_children(|root| {
            root.spawn(t.heading_text("THE GAME CRASHED LAST TIME"));
            root.spawn(t.panel(560.0)).with_children(|p| {
                p.spawn(t.dim_text("Sorry about that. Please screenshot this and send it to the developer:"));
                p.spawn((Text::new(summary(&log)), t.font(false, t.size_small), TextColor(t.text), Node { max_width: px(900), ..default() }));
                p.spawn(t.dim_text("Tip: lower Graphics quality in Settings if it keeps happening."));
                p.spawn((Text::new("TAP OR PRESS ANY KEY TO CONTINUE"), t.font(true, t.size_small), TextColor(t.interact)));
            });
        });
}

fn close_panel(
    mut commands: Commands,
    panels: Query<Entity, With<CrashPanel>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    pads: Query<&Gamepad>,
) {
    if panels.is_empty() {
        return;
    }
    let pressed = keys.get_just_pressed().next().is_some()
        || mouse.just_pressed(MouseButton::Left)
        || touches.any_just_pressed()
        || pads.iter().any(|p| p.get_just_pressed().next().is_some());
    if pressed {
        for e in &panels {
            commands.entity(e).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_drops_the_backtrace() {
        let log = "HALLWAYS 1.0.5 crashed (unix time 1)\npanicked at src/x.rs:1:2:\nboom\n\nBacktrace:\n  0: foo\n  1: bar\n";
        let s = summary(log);
        assert!(s.contains("boom") && s.contains("1.0.5"));
        assert!(!s.contains("foo"));
    }
}
