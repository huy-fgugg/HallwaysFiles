//! The RECONNECTING screen while a room changes host: "Choosing new host...",
//! the host vote (when no computer can take over on its own), or
//! "No new host found :(" when only phones are left.

use bevy::prelude::*;

use super::focus::{spawn_button, MenuAction, MenuActivated};
use crate::net::host::{HostChange, NO_HOST_SECS};
use crate::net::{NetRequest, NetSession};
use crate::state::Screen;
use crate::theme::UiTheme;

pub struct ReconnectPlugin;

impl Plugin for ReconnectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (draw, countdown, vote_clicks).chain().run_if(in_state(Screen::Reconnecting)));
    }
}

#[derive(Component)]
struct ReconnectRoot;

/// "Back to the menu in N..." (updated in place, not rebuilt).
#[derive(Component)]
struct Countdown;

/// What the screen shows (rebuilt only when this changes).
fn signature(change: &HostChange, my_id: u8) -> String {
    match change {
        HostChange::None => String::new(),
        HostChange::Choosing { .. } => "choosing".into(),
        HostChange::Voting { candidates, seconds_left, my_vote } => format!("vote {my_id} {candidates:?} {seconds_left} {my_vote:?}"),
        HostChange::NoHost { .. } => "nohost".into(),
    }
}

fn draw(
    mut commands: Commands,
    theme: Res<UiTheme>,
    change: Res<HostChange>,
    session: Res<NetSession>,
    roots: Query<Entity, With<ReconnectRoot>>,
    mut last: Local<String>,
) {
    let sig = signature(&change, session.my_id);
    if sig == *last && !roots.is_empty() {
        return;
    }
    *last = sig;
    for e in &roots {
        commands.entity(e).despawn();
    }
    let t = theme.clone();
    commands
        .spawn((t.screen_root(true), GlobalZIndex(30), DespawnOnExit(Screen::Reconnecting), ReconnectRoot, Name::new("Reconnecting")))
        .with_children(|root| {
            root.spawn(t.panel(520.0)).with_children(|panel| {
                let centred = || Node { align_self: AlignSelf::Center, ..default() };
                match &*change {
                    HostChange::None | HostChange::Choosing { .. } => {
                        panel.spawn((t.heading_text("RECONNECTING"), centred()));
                        panel.spawn((t.body_text("Choosing new host..."), centred()));
                        panel.spawn((t.dim_text("The game is paused for everyone until the new host is ready."), centred(), TextLayout::justify(Justify::Center)));
                    }
                    HostChange::NoHost { t: secs } => {
                        panel.spawn((t.heading_text("NO NEW HOST FOUND :("), centred()));
                        panel.spawn((t.body_text("Nobody left can host the game (phones can't host)."), centred(), TextLayout::justify(Justify::Center)));
                        panel.spawn((t.dim_text(back_text(*secs)), centred(), Countdown));
                    }
                    HostChange::Voting { candidates, seconds_left, my_vote } => {
                        panel.spawn((t.heading_text("NO NEW HOST FOUND :("), centred()));
                        panel.spawn((t.body_text("No computer here can host on its own. Vote for who should host:"), centred(), TextLayout::justify(Justify::Center)));
                        let mut first = true;
                        for (i, c) in candidates.iter().enumerate() {
                            let you = if c.id == session.my_id { " (you)" } else { "" };
                            let votes = if c.votes == 1 { "1 vote".to_string() } else { format!("{} votes", c.votes) };
                            if c.eligible {
                                let picked = if *my_vote == Some(c.id) { "> " } else { "" };
                                spawn_button(panel, &t, &format!("{picked}{}{you}  -  {votes}", c.name), MenuAction::VoteHost(c.id), i as i32, first);
                                first = false;
                            } else {
                                panel.spawn((t.dim_text(format!("{}{you}  -  phone, can't host", c.name)), Node { align_self: AlignSelf::Center, margin: UiRect::vertical(px(6)), ..default() }));
                            }
                        }
                        let status = if my_vote.is_some() { "Vote cast: waiting for the others." } else { "Tap a name to vote." };
                        panel.spawn((t.dim_text(format!("{status}  {seconds_left} s left.")), centred()));
                    }
                }
            });
        });
}

fn back_text(t: f32) -> String {
    format!("Back to the menu in {}...", (NO_HOST_SECS - t).ceil().max(0.0) as i32)
}

fn countdown(change: Res<HostChange>, mut q: Query<&mut Text, With<Countdown>>) {
    let HostChange::NoHost { t } = *change else { return };
    let want = back_text(t);
    for mut text in &mut q {
        if text.0 != want {
            text.0 = want.clone();
        }
    }
}

fn vote_clicks(mut reader: MessageReader<MenuActivated>, mut req: MessageWriter<NetRequest>) {
    for MenuActivated(action) in reader.read() {
        if let MenuAction::VoteHost(id) = action {
            req.write(NetRequest::Vote { id: *id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::proto::Candidate;

    #[test]
    fn the_screen_redraws_only_when_it_changes() {
        // The "choosing" clock ticks every frame but the screen stays put.
        assert_eq!(signature(&HostChange::Choosing { t: 0.1 }, 1), signature(&HostChange::Choosing { t: 3.0 }, 1));
        let v = |votes| HostChange::Voting { candidates: vec![Candidate { id: 2, name: "Ana".into(), eligible: true, votes }], seconds_left: 9, my_vote: None };
        assert_ne!(signature(&v(0), 1), signature(&v(1), 1));
    }
}
