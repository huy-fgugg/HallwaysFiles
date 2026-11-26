//! Who runs the room. The host's computer carries the room's traffic over
//! direct links (peer-to-peer); everyone else connects to it, and the room
//! server only introduces players and relays for anyone a direct link can't
//! reach. Phones never host and always use the server.
//!
//! When the host leaves, every game pauses on a RECONNECTING screen while the
//! server hands the room to the best remaining computer (one that runs the
//! game well: see [`host_info`]). If none qualifies, the players vote among
//! the computers left; if only phones are left, the room ends.

use std::collections::HashSet;

use bevy::prelude::*;
use serde_json::Value;

use super::proto::{Candidate, ClientMsg, HostInfo};
use super::{to_text, Link, NetSession, PeerMsg};

/// Host change in progress (drives the RECONNECTING screen and the pause).
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub enum HostChange {
    #[default]
    None,
    /// The host is gone or unreachable: waiting for the new host and for the
    /// links to it. `t` = seconds so far.
    Choosing { t: f32 },
    /// Nobody can take over on their own: pick a host.
    Voting { candidates: Vec<Candidate>, seconds_left: u64, my_vote: Option<u8> },
    /// Only phones left: the room is over (back to the menu after a moment).
    NoHost { t: f32 },
}

impl HostChange {
    pub fn active(&self) -> bool {
        *self != HostChange::None
    }
}

/// Seconds the "no new host" screen shows before going back to the menu.
pub const NO_HOST_SECS: f32 = 5.0;
/// Longest wait for direct links after a new host is chosen; after that the
/// game carries on through the server for anyone not linked yet.
const LINK_WAIT: f32 = 8.0;
/// A call (offer, answer, connection checks) gets this long before trying again.
const CALL_RETRY: f32 = 15.0;

/// This device's frame rate, smoothed (measured during runs).
#[derive(Resource, Debug, Default)]
pub struct HostPerf {
    pub fps: f32,
    samples: u32,
}

/// What this device can do as host (pure, for tests).
pub fn host_info(phone: bool, low_end: bool, cores: usize, fps: Option<f32>, direct_ok: bool) -> HostInfo {
    if phone {
        return HostInfo { capable: false, eligible: false, score: 0 };
    }
    let fps = fps.unwrap_or(60.0).min(144.0);
    let capable = !low_end && cores >= 4 && fps >= 30.0;
    let score = fps as u32 * 5 + cores.min(32) as u32 * 10 + if direct_ok { 200 } else { 0 } + if low_end { 0 } else { 100 };
    HostInfo { capable, eligible: true, score }
}

/// This device, now. `HALLWAYS_HOST_INFO="capable,eligible,score"` overrides (QA).
pub fn my_host_info(perf: &HostPerf, direct_ok: bool) -> HostInfo {
    if let Ok(v) = std::env::var("HALLWAYS_HOST_INFO") {
        let n: Vec<u32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if n.len() == 3 {
            return HostInfo { capable: n[0] != 0, eligible: n[1] != 0, score: n[2] };
        }
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let fps = (perf.samples > 120).then_some(perf.fps);
    host_info(cfg!(target_os = "android"), crate::lowend::enabled(), cores, fps, direct_ok)
}

/// Direct links to other players (computers only).
#[derive(Resource, Default)]
pub struct Direct {
    #[cfg(not(target_os = "android"))]
    link: Option<super::p2p::P2p>,
    /// Players with an open direct link.
    pub open: HashSet<u8>,
    /// The host we last called, and seconds until we may call again.
    calling: Option<(u8, f32)>,
    /// The host id these links were made for.
    for_host: u8,
}

impl Direct {
    pub fn send(&self, peer: u8, text: String) -> bool {
        #[cfg(not(target_os = "android"))]
        if self.open.contains(&peer) {
            if let Some(l) = &self.link {
                l.command(super::p2p::Cmd::Send { peer, text });
                return true;
            }
        }
        let _ = (peer, text);
        false
    }

    pub fn signal(&self, from: u8, data: Value) {
        #[cfg(not(target_os = "android"))]
        if let Some(l) = &self.link {
            l.command(super::p2p::Cmd::Signal { from, data });
            return;
        }
        #[allow(unreachable_code)]
        let _ = (from, data);
    }

    fn close_all(&mut self) {
        #[cfg(not(target_os = "android"))]
        if let Some(l) = &self.link {
            l.command(super::p2p::Cmd::CloseAll);
        }
        self.open.clear();
        self.calling = None;
    }
}

/// Someone else's message as it arrives (direct, or relayed by the server).
/// Wrapped forms: `{"f": id, "m": msg}` over a direct link from the host,
/// `{"fwd": id, "m": msg}` through the server from the host.
pub fn unwrap(from: u8, data: Value, host: u8) -> (u8, Value) {
    if from == host {
        for key in ["f", "fwd"] {
            if let Some(id) = data.get(key).and_then(Value::as_u64) {
                return (id as u8, data.get("m").cloned().unwrap_or(Value::Null));
            }
        }
    }
    if let Some(m) = data.get("m") {
        if data.get("k").is_none() {
            return (from, m.clone());
        }
    }
    (from, data)
}

/// Sends one of our messages to the room: through the host's direct links
/// where they're open, through the server otherwise.
pub(super) fn send_room(link: &Link, direct: &Direct, session: &NetSession, data: Value) {
    if session.is_host() {
        let mut relay = vec![];
        for p in session.players.iter().filter(|p| p.id != session.my_id) {
            if !direct.send(p.id, serde_json::json!({ "m": data }).to_string()) {
                relay.push(p.id);
            }
        }
        if relay.is_empty() {
            return;
        }
        // No direct links at all: a plain relay (understood by any server).
        let msg = if direct.open.is_empty() { ClientMsg::Relay { data } } else { ClientMsg::RelayTo { to: relay, data } };
        let _ = link.out.send(to_text(&msg));
    } else if !direct.send(session.host_id, serde_json::json!({ "m": data }).to_string()) {
        let _ = link.out.send(to_text(&ClientMsg::Relay { data }));
    }
}

/// The host passes on a message that came to it directly from `from`.
pub(super) fn forward(link: &Link, direct: &Direct, session: &NetSession, from: u8, data: &Value) {
    let mut relay = vec![];
    for p in session.players.iter().filter(|p| p.id != session.my_id && p.id != from) {
        if !direct.send(p.id, serde_json::json!({ "f": from, "m": data }).to_string()) {
            relay.push(p.id);
        }
    }
    if !relay.is_empty() {
        let _ = link.out.send(to_text(&ClientMsg::RelayTo { to: relay, data: serde_json::json!({ "fwd": from, "m": data }) }));
    }
}

/// A message that arrived over a direct link, ready to deliver.
#[derive(Message, Debug, Clone)]
pub struct DirectIn {
    pub from: u8,
    pub msg: PeerMsg,
}

/// Opens/closes direct links as the host changes, and reads them.
#[allow(clippy::too_many_arguments)]
pub(super) fn direct_links(
    mut direct: ResMut<Direct>,
    session: Res<NetSession>,
    link: Option<Res<Link>>,
    time: Res<Time<Real>>,
    mut change: ResMut<HostChange>,
    mut inbox: MessageWriter<DirectIn>,
) {
    #[cfg(target_os = "android")]
    {
        let _ = (&mut direct, &session, &link, &time, &mut change, &mut inbox);
    }
    #[cfg(not(target_os = "android"))]
    {
        use super::p2p::{Cmd, Ev, P2p};
        if std::env::var_os("HALLWAYS_NO_P2P").is_some() {
            return;
        }
        let Some(link) = link else {
            if direct.link.is_some() {
                *direct = Direct::default();
            }
            return;
        };
        if !session.online() {
            return;
        }
        if direct.link.is_none() {
            direct.link = P2p::start();
        }
        if direct.link.is_none() {
            return;
        }
        // The host changed: links were for the old one.
        if direct.for_host != session.host_id {
            direct.close_all();
            direct.for_host = session.host_id;
        }
        let events = direct.link.as_ref().map(P2p::events).unwrap_or_default();
        for e in events {
            match e {
                Ev::Signal { to, data } => {
                    let _ = link.out.send(to_text(&ClientMsg::Signal { to, data }));
                }
                Ev::Open { peer } => {
                    if direct.open.insert(peer) {
                        info!("direct link to player {peer} open");
                    }
                }
                Ev::Closed { peer } => {
                    let was_open = direct.open.remove(&peer);
                    // Lost the direct link to the host mid-run: pause until it's back
                    // (or the server names a new host).
                    if was_open && peer == session.host_id && !session.is_host() && session.started && !change.active() {
                        *change = HostChange::Choosing { t: 0.0 };
                    }
                }
                Ev::Msg { peer, text } => {
                    let Ok(data) = serde_json::from_str::<Value>(&text) else { continue };
                    let Some(m) = data.get("m").cloned() else { continue };
                    if session.is_host() {
                        // From a guest: pass it on to everyone else, then take it ourselves.
                        forward(&link, &direct, &session, peer, &m);
                        if let Ok(msg) = serde_json::from_value::<PeerMsg>(m) {
                            inbox.write(DirectIn { from: peer, msg });
                        }
                    } else {
                        let from = data.get("f").and_then(Value::as_u64).map(|f| f as u8).unwrap_or(peer);
                        if let Ok(msg) = serde_json::from_value::<PeerMsg>(m) {
                            inbox.write(DirectIn { from, msg });
                        }
                    }
                }
            }
        }
        // A guest calls its host (and calls again if the link fails).
        let host = session.host_id;
        if !session.is_host() && host != 0 && !direct.open.contains(&host) {
            let dt = time.delta_secs();
            let due = match direct.calling {
                Some((h, wait)) if h == host => wait - dt <= 0.0,
                _ => true,
            };
            if let Some((_, wait)) = direct.calling.as_mut() {
                *wait -= dt;
            }
            if due {
                direct.calling = Some((host, CALL_RETRY));
                info!("calling host (player {host}) for a direct link");
                if let Some(l) = direct.link.as_ref() {
                    l.command(Cmd::Call { peer: host });
                }
            }
        }
    }
}

/// Tells the server what this device can do as host, now and every 10 s.
pub(super) fn report_host_info(link: Option<Res<Link>>, session: Res<NetSession>, perf: Res<HostPerf>, direct: Res<Direct>, time: Res<Time<Real>>, mut since: Local<Option<f32>>) {
    let Some(link) = link else {
        *since = None;
        return;
    };
    if !session.online() {
        *since = None;
        return;
    }
    let t = since.get_or_insert(f32::MAX);
    *t += time.delta_secs();
    if *t < 10.0 {
        return;
    }
    *t = 0.0;
    let info = my_host_info(&perf, !direct.open.is_empty());
    let _ = link.out.send(to_text(&ClientMsg::HostInfo(info)));
}

/// Frame-rate meter (during runs only: menus are easy on any device).
pub fn measure(time: Res<Time<Real>>, mut perf: ResMut<HostPerf>, state: Res<State<crate::state::AppState>>) {
    if *state.get() != crate::state::AppState::Playing {
        return;
    }
    let dt = time.delta_secs();
    if dt <= 0.0 || dt > 1.0 {
        return;
    }
    let fps = 1.0 / dt;
    perf.fps = if perf.samples == 0 { fps } else { perf.fps + (fps - perf.fps) * 0.02 };
    perf.samples = perf.samples.saturating_add(1);
}

/// Ends the RECONNECTING pause once the new host is reachable.
#[allow(clippy::too_many_arguments)]
pub fn settle(
    mut change: ResMut<HostChange>,
    session: Res<NetSession>,
    direct: Res<Direct>,
    time: Res<Time<Real>>,
    mut subs: MessageWriter<crate::run::Subtitle>,
    mut req: MessageWriter<super::NetRequest>,
    mut next_app: ResMut<NextState<crate::state::AppState>>,
) {
    let dt = time.delta_secs();
    match &mut *change {
        HostChange::None | HostChange::Voting { .. } => {}
        HostChange::Choosing { t } => {
            *t += dt;
            let t = *t;
            if !session.online() {
                // Our own connection is being re-established (the banner says so).
                if t > 30.0 {
                    *change = HostChange::None;
                }
                return;
            }
            if session.host_id == 0 {
                return;
            }
            let phone = cfg!(target_os = "android") || std::env::var_os("HALLWAYS_NO_P2P").is_some();
            let ready = if session.is_host() {
                // Wait a moment for the others to link up to us.
                t >= 1.5 && (direct.open.len() + 1 >= session.players.len() || t >= LINK_WAIT || phone)
            } else if phone {
                t >= 2.0
            } else {
                (direct.open.contains(&session.host_id) && t >= 1.0) || t >= LINK_WAIT
            };
            if ready {
                *change = HostChange::None;
                let msg = if session.is_host() { "You are the new host.".to_string() } else { format!("{} is the new host.", session.name_of(session.host_id)) };
                subs.write(crate::run::Subtitle::info(msg));
            }
        }
        HostChange::NoHost { t } => {
            *t += dt;
            if *t >= NO_HOST_SECS {
                *change = HostChange::None;
                req.write(super::NetRequest::Leave);
                next_app.set(crate::state::AppState::MainMenu);
            }
        }
    }
}

/// Shows the RECONNECTING screen (which pauses the run) while a host change is on.
pub fn drive_overlay(change: Res<HostChange>, app: Res<State<crate::state::AppState>>, overlay: Res<State<crate::state::Overlay>>, mut next: ResMut<NextState<crate::state::Overlay>>) {
    use crate::state::Overlay;
    if *app.get() != crate::state::AppState::Playing {
        return;
    }
    let cur = *overlay.get();
    if change.active() {
        // Dead players watch their spectator screen; it doesn't need pausing.
        if !matches!(cur, Overlay::Reconnecting | Overlay::Dead | Overlay::Summary) {
            next.set(Overlay::Reconnecting);
        }
    } else if cur == Overlay::Reconnecting {
        next.set(Overlay::None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_devices_can_host() {
        // Phones never host.
        assert_eq!(host_info(true, false, 8, Some(120.0), true), HostInfo { capable: false, eligible: false, score: 0 });
        // A good computer can.
        let good = host_info(false, false, 8, Some(90.0), true);
        assert!(good.capable && good.eligible);
        // The Low-End Edition, too few cores or a low frame rate: only by vote.
        for weak in [host_info(false, true, 8, Some(90.0), true), host_info(false, false, 2, Some(90.0), true), host_info(false, false, 8, Some(20.0), true)] {
            assert!(!weak.capable && weak.eligible, "{weak:?}");
            assert!(weak.score < good.score);
        }
        // Faster is better.
        assert!(host_info(false, false, 8, Some(140.0), false).score > host_info(false, false, 8, Some(45.0), false).score);
    }

    #[test]
    fn forwarded_messages_keep_their_sender() {
        let m = serde_json::json!({"k": "chat", "text": "hi"});
        // Straight from a guest (plain relay).
        assert_eq!(unwrap(3, m.clone(), 1), (3, m.clone()));
        // The host passing on guest 3's message through the server.
        assert_eq!(unwrap(1, serde_json::json!({"fwd": 3, "m": m}), 1), (3, m.clone()));
        // Only the host may say who a message is from.
        assert_eq!(unwrap(2, serde_json::json!({"fwd": 3, "m": m}), 1), (2, m.clone()));
    }
}
