//! HALLWAYS room server: hands out 5-letter room codes and relays messages
//! between the players in each room. Holds no game logic and stores nothing
//! on disk, so it runs happily on a free web-service plan (e.g. Render).
//!
//! Endpoints:
//! - `GET /`       plain-text status
//! - `GET /health` "ok" (for the host's health check)
//! - `GET /ws`     WebSocket; protocol in `src/net/proto.rs` of the game

#[path = "../../src/net/proto.rs"]
mod proto;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use proto::{AdminCmd, ClientMsg, PlayerInfo, RoomSummary, ServerMsg, MAX_PLAYERS, PROTOCOL_VERSION};
use tokio::sync::mpsc;

/// Frames bigger than this are dropped (positions are ~200 bytes).
const MAX_FRAME: usize = 16 * 1024;
/// Clients ping every 8 s; a socket silent for this long is closed.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ROOMS: usize = 2000;
/// A dropped player can rejoin (with the same token) for this long.
const REJOIN_WINDOW: Duration = Duration::from_secs(180);
/// Sent down a player's channel to make the writer close the socket.
const CLOSE: &str = "\u{0}close";

struct Player {
    id: u8,
    name: String,
    token: u64,
    tx: mpsc::UnboundedSender<String>,
}

struct Room {
    players: Vec<Player>,
    host_id: u8,
    seed: u64,
    started: bool,
    next_id: u8,
    config: Option<serde_json::Value>,
    created: Instant,
    /// Players whose connection dropped: (token, name, when).
    dropped: Vec<(u64, String, Instant)>,
}

impl Room {
    fn broadcast(&self, msg: &ServerMsg, except: Option<u8>) {
        let text = serde_json::to_string(msg).unwrap_or_default();
        for p in &self.players {
            if Some(p.id) != except {
                let _ = p.tx.send(text.clone());
            }
        }
    }
}

#[derive(Clone, Default)]
struct Rooms(Arc<Mutex<HashMap<String, Room>>>);

/// Tells a player why they're being removed, then closes their socket.
fn remove_player(room: &mut Room, id: u8, reason: &str) -> bool {
    let Some(i) = room.players.iter().position(|p| p.id == id) else { return false };
    let p = room.players.remove(i);
    send(&p.tx, &ServerMsg::Kicked { reason: reason.to_string() });
    let _ = p.tx.send(CLOSE.to_string());
    room.broadcast(&ServerMsg::Left { id }, None);
    if room.host_id == id
        && let Some(first) = room.players.first()
    {
        room.host_id = first.id;
        let host = room.host_id;
        room.broadcast(&ServerMsg::HostIs { id: host }, None);
    }
    true
}

/// The server's admin key (`HALLWAYS_ADMIN_KEY`); admin tools are off without one.
fn admin_key_ok(given: &str) -> Result<(), String> {
    let Some(key) = std::env::var("HALLWAYS_ADMIN_KEY").ok().filter(|k| k.len() >= 8) else {
        return Err("Server admin tools are off: set HALLWAYS_ADMIN_KEY (8+ characters) on the server.".into());
    };
    // Compare without stopping at the first difference.
    let (a, b) = (key.as_bytes(), given.as_bytes());
    let diff = a.len() ^ b.len() | a.iter().zip(b).fold(0usize, |d, (x, y)| d | usize::from(x ^ y));
    if diff == 0 { Ok(()) } else { Err("Wrong admin key.".into()) }
}

fn send(tx: &mpsc::UnboundedSender<String>, msg: &ServerMsg) {
    let _ = tx.send(serde_json::to_string(msg).unwrap_or_default());
}

fn clean_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(16).collect();
    let s = s.trim().to_string();
    if s.is_empty() { "Player".into() } else { s }
}

impl Rooms {
    fn new_code(map: &HashMap<String, Room>) -> String {
        loop {
            let code = proto::code_from_bytes(rand::random());
            if !map.contains_key(&code) {
                return code;
            }
        }
    }

    /// Creates a room; returns (code, player id).
    fn host(&self, name: &str, seed: u64, config: Option<serde_json::Value>, token: u64, tx: mpsc::UnboundedSender<String>) -> Result<(String, u8), String> {
        let mut map = self.0.lock().unwrap();
        if map.len() >= MAX_ROOMS {
            return Err("The server is full right now. Try again soon.".into());
        }
        let code = Self::new_code(&map);
        let player = Player { id: 1, name: clean_name(name), token, tx: tx.clone() };
        let info = vec![PlayerInfo { id: 1, name: player.name.clone() }];
        map.insert(code.clone(), Room { players: vec![player], host_id: 1, seed, started: false, next_id: 2, config: config.clone(), created: Instant::now(), dropped: vec![] });
        send(&tx, &ServerMsg::Welcome { code: code.clone(), id: 1, host_id: 1, seed, max: MAX_PLAYERS, players: info, config });
        Ok((code, 1))
    }

    /// Joins a room (`rejoin`: coming back after a dropped connection, which
    /// is allowed after the start if the token matches a recent drop).
    fn join(&self, code: &str, name: &str, token: u64, rejoin: bool, tx: mpsc::UnboundedSender<String>) -> Result<u8, String> {
        let code = proto::normalize_code(code);
        let mut map = self.0.lock().unwrap();
        let room = map.get_mut(&code).ok_or_else(|| format!("No room with code {code}. Check the letters and try again."))?;
        room.dropped.retain(|d| d.2.elapsed() < REJOIN_WINDOW);
        let returning = rejoin && token != 0 && room.dropped.iter().any(|d| d.0 == token);
        if rejoin && !returning {
            return Err("Couldn't get back into the room (it waited too long, or it's gone).".into());
        }
        if room.started && !returning {
            return Err("That group has already left the lift.".into());
        }
        if room.players.len() >= MAX_PLAYERS as usize {
            return Err(format!("That room is full ({MAX_PLAYERS} players)."));
        }
        let name = clean_name(name);
        if room.players.iter().any(|p| proto::same_name(&p.name, &name)) {
            return Err(format!("Someone in that room is already called {name}. Change your name and try again."));
        }
        room.dropped.retain(|d| d.0 != token || token == 0);
        let id = room.next_id;
        room.next_id = room.next_id.wrapping_add(1).max(1);
        let player = PlayerInfo { id, name };
        room.broadcast(&ServerMsg::Joined { player: player.clone() }, None);
        room.players.push(Player { id, name: player.name.clone(), token, tx: tx.clone() });
        let players = room.players.iter().map(|p| PlayerInfo { id: p.id, name: p.name.clone() }).collect();
        send(&tx, &ServerMsg::Welcome { code, id, host_id: room.host_id, seed: room.seed, max: MAX_PLAYERS, players, config: room.config.clone() });
        Ok(id)
    }

    fn with_room(&self, code: &str, f: impl FnOnce(&mut Room)) {
        if let Some(room) = self.0.lock().unwrap().get_mut(code) {
            f(room);
        }
    }

    fn leave(&self, code: &str, id: u8) {
        let mut map = self.0.lock().unwrap();
        let Some(room) = map.get_mut(code) else { return };
        // Already removed (kicked): nothing to tell anyone.
        let Some(i) = room.players.iter().position(|p| p.id == id) else { return };
        let p = room.players.remove(i);
        // Remembered so they can come back if their connection only dropped.
        if p.token != 0 {
            room.dropped.push((p.token, p.name.clone(), Instant::now()));
        }
        if room.players.is_empty() {
            map.remove(code);
            return;
        }
        room.broadcast(&ServerMsg::Left { id }, None);
        if room.host_id == id {
            room.host_id = room.players[0].id;
            let host = room.host_id;
            room.broadcast(&ServerMsg::HostIs { id: host }, None);
        }
    }

    /// Room host removes a player.
    fn kick(&self, code: &str, by: u8, id: u8, reason: &str) -> Result<String, String> {
        let mut map = self.0.lock().unwrap();
        let room = map.get_mut(code).ok_or("No such room.")?;
        if room.host_id != by {
            return Err("Only the room's host can remove players.".into());
        }
        if id == by {
            return Err("You can't remove yourself.".into());
        }
        let name = room.players.iter().find(|p| p.id == id).map(|p| p.name.clone()).ok_or("That player isn't in the room.")?;
        let reason = if reason.trim().is_empty() { "The host removed you from the room.".to_string() } else { reason.chars().take(120).collect() };
        remove_player(room, id, &reason);
        Ok(format!("Removed {name}."))
    }

    /// Server admin tools.
    fn admin(&self, cmd: AdminCmd) -> ServerMsg {
        let mut map = self.0.lock().unwrap();
        match cmd {
            AdminCmd::Rooms => {
                let mut rooms: Vec<RoomSummary> = map
                    .iter()
                    .map(|(code, r)| RoomSummary { code: code.clone(), players: r.players.iter().map(|p| PlayerInfo { id: p.id, name: p.name.clone() }).collect(), host_id: r.host_id, started: r.started, age: r.created.elapsed().as_secs() })
                    .collect();
                rooms.sort_by(|a, b| a.code.cmp(&b.code));
                ServerMsg::AdminRooms { rooms }
            }
            AdminCmd::Announce { text, code } => {
                let text: String = text.chars().filter(|c| !c.is_control()).take(200).collect();
                if text.trim().is_empty() {
                    return ServerMsg::Error { msg: "Type a message first.".into() };
                }
                let msg = ServerMsg::Announcement { text };
                let mut n = 0;
                for (c, r) in map.iter() {
                    if code.as_ref().is_none_or(|want| proto::normalize_code(want) == *c) {
                        r.broadcast(&msg, None);
                        n += r.players.len();
                    }
                }
                ServerMsg::AdminOk { msg: format!("Announcement sent to {n} player(s).") }
            }
            AdminCmd::CloseRoom { code, reason } => {
                let code = proto::normalize_code(&code);
                let Some(mut room) = map.remove(&code) else { return ServerMsg::Error { msg: format!("No room {code}.") } };
                let reason = if reason.trim().is_empty() { "An admin closed this room.".to_string() } else { reason };
                for p in room.players.drain(..) {
                    send(&p.tx, &ServerMsg::Kicked { reason: reason.clone() });
                    let _ = p.tx.send(CLOSE.to_string());
                }
                ServerMsg::AdminOk { msg: format!("Closed room {code}.") }
            }
            AdminCmd::KickPlayer { code, id, reason } => {
                let code = proto::normalize_code(&code);
                let Some(room) = map.get_mut(&code) else { return ServerMsg::Error { msg: format!("No room {code}.") } };
                let reason = if reason.trim().is_empty() { "An admin removed you from the room.".to_string() } else { reason };
                if remove_player(room, id, &reason) {
                    if room.players.is_empty() {
                        map.remove(&code);
                    }
                    ServerMsg::AdminOk { msg: format!("Removed player {id} from {code}.") }
                } else {
                    ServerMsg::Error { msg: "That player isn't in the room.".into() }
                }
            }
        }
    }

    fn stats(&self) -> (usize, usize) {
        let map = self.0.lock().unwrap();
        (map.len(), map.values().map(|r| r.players.len()).sum())
    }
}

async fn index(State(rooms): State<Rooms>) -> impl IntoResponse {
    let (r, p) = rooms.stats();
    format!("HALLWAYS room server (protocol {PROTOCOL_VERSION})\nrooms: {r}\nplayers: {p}\n")
}

async fn ws(ws: WebSocketUpgrade, State(rooms): State<Rooms>) -> impl IntoResponse {
    ws.max_message_size(MAX_FRAME).on_upgrade(move |socket| handle(socket, rooms))
}

async fn handle(socket: WebSocket, rooms: Rooms) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if text == CLOSE {
                let _ = sink.send(Message::Close(None)).await;
                break;
            }
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut joined: Option<(String, u8)> = None;
    loop {
        let next = tokio::time::timeout(IDLE_TIMEOUT, stream.next()).await;
        let Ok(Some(Ok(msg))) = next else { break };
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(msg) = serde_json::from_str::<ClientMsg>(&text) else {
            send(&tx, &ServerMsg::Error { msg: "Bad message.".into() });
            continue;
        };
        match (msg, &joined) {
            (ClientMsg::Ping, _) => send(&tx, &ServerMsg::Pong),
            (ClientMsg::Admin { key, cmd }, _) => match admin_key_ok(&key) {
                Ok(()) => send(&tx, &rooms.admin(cmd)),
                Err(e) => send(&tx, &ServerMsg::Error { msg: e }),
            },
            (ClientMsg::Host { version, .. } | ClientMsg::Join { version, .. } | ClientMsg::Rejoin { version, .. }, None) if version != PROTOCOL_VERSION => {
                send(&tx, &ServerMsg::Error { msg: "Your game version doesn't match the server. Update HALLWAYS.".into() });
            }
            (ClientMsg::Host { name, seed, config, token, .. }, None) => match rooms.host(&name, seed, config, token, tx.clone()) {
                Ok(j) => joined = Some(j),
                Err(e) => send(&tx, &ServerMsg::Error { msg: e }),
            },
            (ClientMsg::Join { code, name, token, .. }, None) => match rooms.join(&code, &name, token, false, tx.clone()) {
                Ok(id) => joined = Some((proto::normalize_code(&code), id)),
                Err(e) => send(&tx, &ServerMsg::Error { msg: e }),
            },
            (ClientMsg::Rejoin { code, name, token, .. }, None) => match rooms.join(&code, &name, token, true, tx.clone()) {
                Ok(id) => joined = Some((proto::normalize_code(&code), id)),
                Err(e) => send(&tx, &ServerMsg::Error { msg: e }),
            },
            (ClientMsg::Kick { id: target, reason }, Some((code, id))) => {
                let msg = match rooms.kick(code, *id, target, &reason) {
                    Ok(m) => ServerMsg::AdminOk { msg: m },
                    Err(e) => ServerMsg::Error { msg: e },
                };
                send(&tx, &msg);
            }
            (ClientMsg::Relay { data }, Some((code, id))) => {
                let id = *id;
                rooms.with_room(code, |room| room.broadcast(&ServerMsg::Relay { from: id, data }, Some(id)));
            }
            (ClientMsg::Start, Some((code, id))) => {
                let id = *id;
                rooms.with_room(code, |room| {
                    if room.host_id == id && !room.started {
                        room.started = true;
                        room.broadcast(&ServerMsg::Start, None);
                    }
                });
            }
            _ => send(&tx, &ServerMsg::Error { msg: "Not allowed right now.".into() }),
        }
    }
    if let Some((code, id)) = joined {
        rooms.leave(&code, id);
    }
    writer.abort();
}

fn app(rooms: Rooms) -> Router {
    Router::new().route("/", get(index)).route("/health", get(|| async { "ok" })).route("/ws", get(ws)).with_state(rooms)
}

#[tokio::main]
async fn main() {
    // Render (and most hosts) tell the service which port to use via $PORT.
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind port");
    println!("HALLWAYS room server listening on {addr}");
    axum::serve(listener, app(Rooms::default())).await.expect("server");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::Message as TMsg;

    async fn start() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app(Rooms::default())).await.unwrap() });
        format!("ws://{addr}/ws")
    }

    async fn recv(ws: &mut (impl StreamExt<Item = Result<TMsg, tokio_tungstenite::tungstenite::Error>> + Unpin)) -> ServerMsg {
        loop {
            let m = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
            if let TMsg::Text(t) = m {
                return serde_json::from_str(&t).unwrap();
            }
        }
    }

    fn text(m: &ClientMsg) -> TMsg {
        TMsg::Text(serde_json::to_string(m).unwrap().into())
    }

    #[tokio::test]
    async fn host_join_relay_start_leave() {
        let url = start().await;
        let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        a.send(text(&ClientMsg::Host { name: "Ana".into(), seed: 42, version: PROTOCOL_VERSION, config: Some(serde_json::json!({"doors": 30})), token: 11 })).await.unwrap();
        let ServerMsg::Welcome { code, id, host_id, seed, .. } = recv(&mut a).await else { panic!() };
        assert_eq!((id, host_id, seed), (1, 1, 42));
        assert!(proto::valid_code(&code));

        let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        b.send(text(&ClientMsg::Join { code: code.to_lowercase(), name: "<Bo>".into(), version: PROTOCOL_VERSION, token: 22 })).await.unwrap();
        let ServerMsg::Welcome { id: bid, seed, players, config, .. } = recv(&mut b).await else { panic!() };
        assert_eq!((bid, seed, players.len()), (2, 42, 2));
        assert_eq!(config, Some(serde_json::json!({"doors": 30})), "guests get the host's preset");
        assert_eq!(players[1].name, "Bo");
        assert!(matches!(recv(&mut a).await, ServerMsg::Joined { player } if player.id == 2));

        b.send(text(&ClientMsg::Relay { data: serde_json::json!({"hello": 1}) })).await.unwrap();
        assert!(matches!(recv(&mut a).await, ServerMsg::Relay { from: 2, .. }));

        // Only the host can start.
        b.send(text(&ClientMsg::Start)).await.unwrap();
        a.send(text(&ClientMsg::Start)).await.unwrap();
        assert_eq!(recv(&mut a).await, ServerMsg::Start);
        assert_eq!(recv(&mut b).await, ServerMsg::Start);

        // Late joiners are turned away once the group has left.
        let (mut c, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        c.send(text(&ClientMsg::Join { code: code.clone(), name: "Cy".into(), version: PROTOCOL_VERSION, token: 33 })).await.unwrap();
        assert!(matches!(recv(&mut c).await, ServerMsg::Error { .. }));

        // Host leaves: Bo is promoted.
        a.close(None).await.unwrap();
        assert_eq!(recv(&mut b).await, ServerMsg::Left { id: 1 });
        assert_eq!(recv(&mut b).await, ServerMsg::HostIs { id: 2 });
    }

    #[tokio::test]
    async fn bad_code_and_version() {
        let url = start().await;
        let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        a.send(text(&ClientMsg::Join { code: "ZZZZZ".into(), name: "x".into(), version: PROTOCOL_VERSION, token: 0 })).await.unwrap();
        assert!(matches!(recv(&mut a).await, ServerMsg::Error { .. }));
        a.send(text(&ClientMsg::Host { name: "x".into(), seed: 1, version: 999, config: None, token: 0 })).await.unwrap();
        assert!(matches!(recv(&mut a).await, ServerMsg::Error { .. }));
    }

    #[tokio::test]
    async fn names_kicks_rejoin_and_admin() {
        let url = start().await;
        let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        a.send(text(&ClientMsg::Host { name: "Ana".into(), seed: 1, version: PROTOCOL_VERSION, config: None, token: 101 })).await.unwrap();
        let ServerMsg::Welcome { code, .. } = recv(&mut a).await else { panic!() };

        // Same name (any case) can't join.
        let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        b.send(text(&ClientMsg::Join { code: code.clone(), name: " ANA ".into(), version: PROTOCOL_VERSION, token: 202 })).await.unwrap();
        assert!(matches!(recv(&mut b).await, ServerMsg::Error { msg } if msg.contains("already called")));
        b.send(text(&ClientMsg::Join { code: code.clone(), name: "Bo".into(), version: PROTOCOL_VERSION, token: 202 })).await.unwrap();
        let ServerMsg::Welcome { id: bo, .. } = recv(&mut b).await else { panic!() };
        assert!(matches!(recv(&mut a).await, ServerMsg::Joined { .. }));

        // The group leaves; Bo's connection drops; Bo rejoins with the token.
        a.send(text(&ClientMsg::Start)).await.unwrap();
        assert_eq!(recv(&mut a).await, ServerMsg::Start);
        assert_eq!(recv(&mut b).await, ServerMsg::Start);
        drop(b);
        assert_eq!(recv(&mut a).await, ServerMsg::Left { id: bo });
        let (mut b2, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        b2.send(text(&ClientMsg::Rejoin { code: code.clone(), name: "Bo".into(), version: PROTOCOL_VERSION, token: 999 })).await.unwrap();
        assert!(matches!(recv(&mut b2).await, ServerMsg::Error { .. }), "wrong token");
        b2.send(text(&ClientMsg::Rejoin { code: code.clone(), name: "Bo".into(), version: PROTOCOL_VERSION, token: 202 })).await.unwrap();
        let ServerMsg::Welcome { id: bo2, .. } = recv(&mut b2).await else { panic!() };
        assert!(matches!(recv(&mut a).await, ServerMsg::Joined { player } if player.name == "Bo"));

        // Only the host kicks; the kicked player is told why, then disconnected.
        b2.send(text(&ClientMsg::Kick { id: 1, reason: String::new() })).await.unwrap();
        assert!(matches!(recv(&mut b2).await, ServerMsg::Error { .. }));
        a.send(text(&ClientMsg::Kick { id: bo2, reason: "bye".into() })).await.unwrap();
        assert_eq!(recv(&mut b2).await, ServerMsg::Kicked { reason: "bye".into() });
        assert!(matches!(recv(&mut a).await, ServerMsg::Left { .. }));
        assert!(matches!(recv(&mut a).await, ServerMsg::AdminOk { .. }));

        // Admin tools need the server's key.
        let (mut admin, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        admin.send(text(&ClientMsg::Admin { key: "wrong-key-123".into(), cmd: AdminCmd::Rooms })).await.unwrap();
        assert!(matches!(recv(&mut admin).await, ServerMsg::Error { .. }));
        // SAFETY: tests in this crate don't read the variable concurrently except here.
        unsafe { std::env::set_var("HALLWAYS_ADMIN_KEY", "test-admin-key") };
        admin.send(text(&ClientMsg::Admin { key: "test-admin-key".into(), cmd: AdminCmd::Rooms })).await.unwrap();
        let ServerMsg::AdminRooms { rooms } = recv(&mut admin).await else { panic!() };
        assert!(rooms.iter().any(|r| r.code == code && r.players.len() == 1));
        admin.send(text(&ClientMsg::Admin { key: "test-admin-key".into(), cmd: AdminCmd::Announce { text: "Server restart soon".into(), code: None } })).await.unwrap();
        assert_eq!(recv(&mut a).await, ServerMsg::Announcement { text: "Server restart soon".into() });
        assert!(matches!(recv(&mut admin).await, ServerMsg::AdminOk { .. }));
        admin.send(text(&ClientMsg::Admin { key: "test-admin-key".into(), cmd: AdminCmd::CloseRoom { code: code.clone(), reason: String::new() } })).await.unwrap();
        assert!(matches!(recv(&mut a).await, ServerMsg::Kicked { .. }));
        assert!(matches!(recv(&mut admin).await, ServerMsg::AdminOk { .. }));
    }
}
