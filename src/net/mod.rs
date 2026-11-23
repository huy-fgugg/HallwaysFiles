//! Online rooms (co-op): connects to the HALLWAYS room server, hosts or joins
//! a room by its 5-letter code, and shares each player's position, pose,
//! held items and gestures plus opened doors. Everyone generates the same
//! floor from the host's seed; entities run on each player's own machine.
//!
//! The socket lives on a background thread (blocking tungstenite) and talks
//! to the game through channels, so a slow connection never stalls a frame.

pub mod proto;

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::items::ItemKind;
use crate::viewmodel::Gesture;
use proto::{ClientMsg, PlayerInfo, ServerMsg, PROTOCOL_VERSION};

/// Used when neither `HALLWAYS_SERVER` nor an `assets/server.txt` override says otherwise.
pub const DEFAULT_SERVER: &str = "wss://hallways-server.onrender.com/ws";

/// Server address: env var, then the first non-comment line of `assets/server.txt`.
pub fn server_url() -> String {
    if let Ok(u) = std::env::var("HALLWAYS_SERVER") {
        if !u.trim().is_empty() {
            return u.trim().to_string();
        }
    }
    let path = crate::music::asset_root().join("server.txt");
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')) {
            return line.to_string();
        }
    }
    DEFAULT_SERVER.to_string()
}

/// One player's replicated state (~15 times a second).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PeerState {
    pub p: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub crouch: bool,
    pub speed: f32,
    pub phase: f32,
    pub gesture: Gesture,
    pub gt: f32,
    pub right: Option<ItemKind>,
    pub left: Option<ItemKind>,
    pub light: bool,
    pub hiding: bool,
    pub alive: bool,
    pub room: u32,
    /// The player's look (older clients don't send one).
    #[serde(default)]
    pub skin: Option<crate::body::Skin>,
}

/// Gameplay messages carried inside `Relay` frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum PeerMsg {
    State(PeerState),
    /// A door was opened (so it opens for everyone).
    Door { number: u32 },
    /// Short notice shown as a subtitle ("Ana reached Door 050").
    Notice { text: String },
    /// Admin panel action aimed at one player.
    Admin { target: u8, action: crate::admin::AdminAction },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum NetStatus {
    #[default]
    Offline,
    Connecting,
    InRoom,
    Error(String),
}

/// What the rest of the game sees about the online room.
#[derive(Resource, Debug, Default)]
pub struct NetSession {
    pub status: NetStatus,
    pub code: String,
    pub my_id: u8,
    pub host_id: u8,
    pub seed: u64,
    pub max: u8,
    pub players: Vec<PlayerInfo>,
    /// The host started the descent.
    pub started: bool,
}

impl NetSession {
    pub fn online(&self) -> bool {
        self.status == NetStatus::InRoom
    }

    pub fn is_host(&self) -> bool {
        !self.online() || self.my_id == self.host_id
    }

    pub fn name_of(&self, id: u8) -> String {
        self.players.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_else(|| format!("Player {id}"))
    }
}

/// Other players' latest states.
#[derive(Resource, Debug, Default)]
pub struct Peers(pub HashMap<u8, PeerState>);

/// Requests from menus.
#[derive(Message, Debug, Clone)]
pub enum NetRequest {
    Host { name: String, seed: u64, preset: Option<crate::presets::Preset> },
    Join { name: String, code: String },
    Leave,
}

/// Fired once when the room is ready (the seed is known): menus start the run.
#[derive(Message, Debug, Clone)]
pub struct RoomReady {
    pub seed: u64,
    /// The host's custom floor (None = classic).
    pub preset: Option<crate::presets::Preset>,
}

/// Fired when the host starts the descent.
#[derive(Message, Debug, Clone, Copy)]
pub struct DescentStarted;

/// Gameplay → network.
#[derive(Message, Debug, Clone)]
pub struct SendPeer(pub PeerMsg);

/// Network → gameplay (other players' messages).
#[derive(Message, Debug, Clone)]
pub struct PeerEvent {
    pub from: u8,
    pub msg: PeerMsg,
}

enum Incoming {
    Msg(ServerMsg),
    Failed(String),
}

/// Handle to the socket thread.
#[derive(Resource)]
struct Link {
    out: Sender<String>,
    inc: std::sync::Mutex<Receiver<Incoming>>,
}

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        app.init_resource::<NetSession>()
            .init_resource::<Peers>()
            .add_message::<NetRequest>()
            .add_message::<RoomReady>()
            .add_message::<DescentStarted>()
            .add_message::<SendPeer>()
            .add_message::<PeerEvent>()
            .add_message::<RequestStart>()
            .add_systems(Update, (auto_online, handle_requests, pump, send_outgoing, send_start).chain())
            .add_systems(OnEnter(crate::state::AppState::MainMenu), |mut req: MessageWriter<NetRequest>, s: Res<NetSession>| {
                if s.status != NetStatus::Offline && !matches!(s.status, NetStatus::Error(_)) {
                    req.write(NetRequest::Leave);
                }
            });
    }
}

/// QA hook: `HALLWAYS_AUTO_ONLINE=host|join` with `HALLWAYS_AUTO_CODEFILE=path`
/// hosts a room (writing its code to the file) or joins the code in the file,
/// then starts the run. Used to test two game windows against a local server.
fn auto_online(
    mut req: MessageWriter<NetRequest>,
    mut ready: MessageReader<RoomReady>,
    session: Res<NetSession>,
    mut seed: ResMut<crate::rng::RunSeed>,
    mut next_app: ResMut<NextState<crate::state::AppState>>,
    mut lobby: Option<ResMut<crate::lobby::LiftLobby>>,
    mut start: MessageWriter<RequestStart>,
    time: Res<Time<Real>>,
    mut step: Local<(u8, f32)>,
    mut player: Query<(&mut crate::player::Player, &mut Transform)>,
) {
    let Ok(mode) = std::env::var("HALLWAYS_AUTO_ONLINE") else { return };
    let file = std::env::var("HALLWAYS_AUTO_CODEFILE").unwrap_or_else(|_| "room_code.txt".into());
    step.1 += time.delta_secs();
    match step.0 {
        0 if step.1 > 3.0 => {
            if mode == "host" {
                req.write(NetRequest::Host { name: "Host".into(), seed: 4242, preset: None });
                step.0 = 1;
            } else if let Ok(code) = std::fs::read_to_string(&file) {
                req.write(NetRequest::Join { name: "Guest".into(), code: code.trim().to_string() });
                step.0 = 1;
            }
        }
        1 => {
            if let Some(r) = ready.read().last() {
                if mode == "host" {
                    let _ = std::fs::write(&file, &session.code);
                }
                *seed = crate::rng::RunSeed(r.seed);
                next_app.set(crate::state::AppState::Playing);
                step.0 = 2;
                step.1 = 0.0;
            }
        }
        // Host starts once the guest is in.
        2 if mode == "host" && session.players.len() >= 2 && step.1 > 2.0 => {
            if lobby.as_mut().is_some_and(|l| l.waiting) {
                start.write(RequestStart);
            }
            step.0 = 3;
            step.1 = 0.0;
        }
        // Host walks out of the lift and turns around so the guest can see them.
        3 if mode == "host" && step.1 > 25.0 => {
            if let Ok((mut p, mut tf)) = player.single_mut() {
                tf.translation.z -= 2.4;
                p.yaw = std::f32::consts::PI;
                p.pitch = -0.1;
            }
            step.0 = 4;
        }
        _ => {}
    }
}

fn to_text(m: &ClientMsg) -> String {
    serde_json::to_string(m).unwrap_or_default()
}

fn handle_requests(mut commands: Commands, mut reqs: MessageReader<NetRequest>, mut session: ResMut<NetSession>, mut peers: ResMut<Peers>) {
    for r in reqs.read() {
        match r {
            NetRequest::Leave => {
                commands.remove_resource::<Link>();
                *session = NetSession::default();
                peers.0.clear();
            }
            NetRequest::Host { name, seed, preset } => {
                let config = preset.as_ref().and_then(|p| serde_json::to_value(p).ok());
                let first = to_text(&ClientMsg::Host { name: name.clone(), seed: *seed, version: PROTOCOL_VERSION, config });
                commands.insert_resource(connect(server_url(), first));
                *session = NetSession { status: NetStatus::Connecting, ..default() };
                peers.0.clear();
            }
            NetRequest::Join { name, code } => {
                let code = proto::normalize_code(code);
                if !proto::valid_code(&code) {
                    session.status = NetStatus::Error("Room codes are 5 letters/numbers, like K7QXM.".into());
                    continue;
                }
                let first = to_text(&ClientMsg::Join { code, name: name.clone(), version: PROTOCOL_VERSION });
                commands.insert_resource(connect(server_url(), first));
                *session = NetSession { status: NetStatus::Connecting, ..default() };
                peers.0.clear();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn pump(
    mut commands: Commands,
    link: Option<Res<Link>>,
    mut session: ResMut<NetSession>,
    mut peers: ResMut<Peers>,
    mut ready: MessageWriter<RoomReady>,
    mut started: MessageWriter<DescentStarted>,
    mut events: MessageWriter<PeerEvent>,
    mut subs: MessageWriter<crate::run::Subtitle>,
) {
    let Some(link) = link else { return };
    let Ok(inc) = link.inc.lock() else { return };
    loop {
        match inc.try_recv() {
            Ok(Incoming::Msg(m)) => match m {
                ServerMsg::Welcome { code, id, host_id, seed, max, players, config } => {
                    *session = NetSession { status: NetStatus::InRoom, code, my_id: id, host_id, seed, max, players, started: false };
                    let preset = config.and_then(|c| serde_json::from_value::<crate::presets::Preset>(c).ok()).map(crate::presets::Preset::sanitized);
                    ready.write(RoomReady { seed, preset });
                }
                ServerMsg::Joined { player } => {
                    subs.write(crate::run::Subtitle::info(format!("{} joined the lift.", player.name)));
                    session.players.retain(|p| p.id != player.id);
                    session.players.push(player);
                }
                ServerMsg::Left { id } => {
                    subs.write(crate::run::Subtitle::info(format!("{} left.", session.name_of(id))));
                    session.players.retain(|p| p.id != id);
                    peers.0.remove(&id);
                }
                ServerMsg::HostIs { id } => {
                    session.host_id = id;
                    if id == session.my_id {
                        subs.write(crate::run::Subtitle::info("You are now the host."));
                    }
                }
                ServerMsg::Relay { from, data } => {
                    if let Ok(msg) = serde_json::from_value::<PeerMsg>(data) {
                        if let PeerMsg::State(s) = &msg {
                            peers.0.insert(from, *s);
                        }
                        events.write(PeerEvent { from, msg });
                    }
                }
                ServerMsg::Start => {
                    session.started = true;
                    started.write(DescentStarted);
                }
                ServerMsg::Pong => {}
                ServerMsg::Error { msg } => {
                    if session.online() {
                        subs.write(crate::run::Subtitle::danger(msg));
                    } else {
                        session.status = NetStatus::Error(msg);
                        commands.remove_resource::<Link>();
                        return;
                    }
                }
            },
            Ok(Incoming::Failed(e)) => {
                let was_online = session.online();
                session.status = NetStatus::Error(e.clone());
                peers.0.clear();
                if was_online {
                    subs.write(crate::run::Subtitle::danger(format!("Disconnected from the room: {e}")));
                }
                commands.remove_resource::<Link>();
                return;
            }
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                commands.remove_resource::<Link>();
                return;
            }
        }
    }
}

fn send_outgoing(link: Option<Res<Link>>, session: Res<NetSession>, mut out: MessageReader<SendPeer>) {
    let Some(link) = link else {
        out.clear();
        return;
    };
    for SendPeer(msg) in out.read() {
        if !session.online() {
            continue;
        }
        if let Ok(data) = serde_json::to_value(msg) {
            let _ = link.out.send(to_text(&ClientMsg::Relay { data }));
        }
    }
}

/// Host only: ask the server to start the descent for everyone.
#[derive(Message, Debug, Clone, Copy)]
pub struct RequestStart;

fn send_start(link: Option<Res<Link>>, mut r: MessageReader<RequestStart>) {
    if r.read().count() > 0 {
        if let Some(l) = link {
            let _ = l.out.send(to_text(&ClientMsg::Start));
        }
    }
}

/// Opens the socket thread. The first message (Host/Join) is sent once connected.
fn connect(url: String, first: String) -> Link {
    let (out_tx, out_rx) = mpsc::channel::<String>();
    let (inc_tx, inc_rx) = mpsc::channel::<Incoming>();
    std::thread::Builder::new()
        .name("hallways-net".into())
        .spawn(move || {
            if let Err(e) = run_socket(&url, first, out_rx, &inc_tx) {
                let _ = inc_tx.send(Incoming::Failed(e));
            }
        })
        .expect("spawn network thread");
    Link { out: out_tx, inc: std::sync::Mutex::new(inc_rx) }
}

fn friendly(e: impl std::fmt::Display) -> String {
    let s = e.to_string();
    if s.contains("refused") || s.contains("resolve") || s.contains("timed out") || s.contains("No such host") {
        "Couldn't reach the room server. Check your internet connection (a free server can take up to a minute to wake up, so try again).".into()
    } else {
        s
    }
}

fn run_socket(url: &str, first: String, out: Receiver<String>, inc: &Sender<Incoming>) -> Result<(), String> {
    use tungstenite::{Message, client::IntoClientRequest};
    let request = url.into_client_request().map_err(|e| format!("Bad server address {url}: {e}"))?;
    let uri = request.uri().clone();
    let host = uri.host().ok_or("Bad server address")?.to_string();
    let tls = uri.scheme_str() == Some("wss");
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port)).map_err(friendly)?.next().ok_or("Couldn't resolve the server address")?;
    // Free hosts sleep when idle and take a while to wake: allow a long connect.
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(75)).map_err(friendly)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(75))).ok();
    let raw = stream.try_clone().map_err(|e| e.to_string())?;
    let (mut ws, _) = tungstenite::client_tls(request, stream).map_err(friendly)?;
    // After the handshake, poll: short read timeouts so we can also write.
    raw.set_read_timeout(Some(Duration::from_millis(10))).ok();
    ws.send(Message::text(first)).map_err(|e| e.to_string())?;
    let mut last_ping = Instant::now();
    loop {
        // Drain outgoing.
        loop {
            match out.try_recv() {
                Ok(text) => ws.send(Message::text(text)).map_err(|e| e.to_string())?,
                Err(TryRecvError::Empty) => break,
                // The game dropped the link (left the room).
                Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return Ok(());
                }
            }
        }
        if last_ping.elapsed() > Duration::from_secs(8) {
            ws.send(Message::text(to_text(&ClientMsg::Ping))).map_err(|e| e.to_string())?;
            last_ping = Instant::now();
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Ok(m) = serde_json::from_str::<ServerMsg>(&t) {
                    if inc.send(Incoming::Msg(m)).is_err() {
                        return Ok(());
                    }
                }
            }
            Ok(Message::Close(_)) => return Err("The server closed the connection.".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                let _ = ws.flush();
            }
            Err(e) => return Err(friendly(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_messages_round_trip() {
        let s = PeerState {
            p: [1.0, 0.0, -3.0],
            yaw: 0.5,
            pitch: -0.1,
            crouch: true,
            speed: 2.0,
            phase: 3.0,
            gesture: Gesture::Reach,
            gt: 0.1,
            right: Some(ItemKind::Lockpick),
            left: None,
            light: false,
            hiding: false,
            alive: true,
            room: 7,
        };
        for m in [PeerMsg::State(s), PeerMsg::Door { number: 12 }, PeerMsg::Notice { text: "hi".into() }] {
            let v = serde_json::to_value(&m).unwrap();
            assert_eq!(serde_json::from_value::<PeerMsg>(v).unwrap(), m);
        }
    }

    #[test]
    fn session_host_rules() {
        let mut s = NetSession::default();
        assert!(s.is_host(), "offline play is always the host");
        s.status = NetStatus::InRoom;
        s.my_id = 2;
        s.host_id = 1;
        assert!(!s.is_host());
        s.players.push(PlayerInfo { id: 1, name: "Ana".into() });
        assert_eq!(s.name_of(1), "Ana");
        assert_eq!(s.name_of(9), "Player 9");
    }
}
