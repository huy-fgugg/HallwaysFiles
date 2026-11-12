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
use std::time::Duration;

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
use proto::{ClientMsg, PlayerInfo, ServerMsg, MAX_PLAYERS, PROTOCOL_VERSION};
use tokio::sync::mpsc;

/// Frames bigger than this are dropped (positions are ~200 bytes).
const MAX_FRAME: usize = 16 * 1024;
/// Clients ping every few seconds; silent sockets are closed.
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_ROOMS: usize = 2000;

struct Player {
    id: u8,
    name: String,
    tx: mpsc::UnboundedSender<String>,
}

struct Room {
    players: Vec<Player>,
    host_id: u8,
    seed: u64,
    started: bool,
    next_id: u8,
    config: Option<serde_json::Value>,
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
    fn host(&self, name: &str, seed: u64, config: Option<serde_json::Value>, tx: mpsc::UnboundedSender<String>) -> Result<(String, u8), String> {
        let mut map = self.0.lock().unwrap();
        if map.len() >= MAX_ROOMS {
            return Err("The server is full right now. Try again soon.".into());
        }
        let code = Self::new_code(&map);
        let player = Player { id: 1, name: clean_name(name), tx: tx.clone() };
        let info = vec![PlayerInfo { id: 1, name: player.name.clone() }];
        map.insert(code.clone(), Room { players: vec![player], host_id: 1, seed, started: false, next_id: 2, config: config.clone() });
        send(&tx, &ServerMsg::Welcome { code: code.clone(), id: 1, host_id: 1, seed, max: MAX_PLAYERS, players: info, config });
        Ok((code, 1))
    }

    fn join(&self, code: &str, name: &str, tx: mpsc::UnboundedSender<String>) -> Result<u8, String> {
        let code = proto::normalize_code(code);
        let mut map = self.0.lock().unwrap();
        let room = map.get_mut(&code).ok_or_else(|| format!("No room with code {code}. Check the letters and try again."))?;
        if room.started {
            return Err("That group has already left the lift.".into());
        }
        if room.players.len() >= MAX_PLAYERS as usize {
            return Err("That room is full (4 players).".into());
        }
        let id = room.next_id;
        room.next_id = room.next_id.wrapping_add(1).max(1);
        let player = PlayerInfo { id, name: clean_name(name) };
        room.broadcast(&ServerMsg::Joined { player: player.clone() }, None);
        room.players.push(Player { id, name: player.name.clone(), tx: tx.clone() });
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
        room.players.retain(|p| p.id != id);
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
            (ClientMsg::Host { version, .. } | ClientMsg::Join { version, .. }, None) if version != PROTOCOL_VERSION => {
                send(&tx, &ServerMsg::Error { msg: "Your game version doesn't match the server. Update HALLWAYS.".into() });
            }
            (ClientMsg::Host { name, seed, config, .. }, None) => match rooms.host(&name, seed, config, tx.clone()) {
                Ok(j) => joined = Some(j),
                Err(e) => send(&tx, &ServerMsg::Error { msg: e }),
            },
            (ClientMsg::Join { code, name, .. }, None) => match rooms.join(&code, &name, tx.clone()) {
                Ok(id) => joined = Some((proto::normalize_code(&code), id)),
                Err(e) => send(&tx, &ServerMsg::Error { msg: e }),
            },
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
        a.send(text(&ClientMsg::Host { name: "Ana".into(), seed: 42, version: PROTOCOL_VERSION, config: Some(serde_json::json!({"doors": 30})) })).await.unwrap();
        let ServerMsg::Welcome { code, id, host_id, seed, .. } = recv(&mut a).await else { panic!() };
        assert_eq!((id, host_id, seed), (1, 1, 42));
        assert!(proto::valid_code(&code));

        let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        b.send(text(&ClientMsg::Join { code: code.to_lowercase(), name: "<Bo>".into(), version: PROTOCOL_VERSION })).await.unwrap();
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
        c.send(text(&ClientMsg::Join { code: code.clone(), name: "Cy".into(), version: PROTOCOL_VERSION })).await.unwrap();
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
        a.send(text(&ClientMsg::Join { code: "ZZZZZ".into(), name: "x".into(), version: PROTOCOL_VERSION })).await.unwrap();
        assert!(matches!(recv(&mut a).await, ServerMsg::Error { .. }));
        a.send(text(&ClientMsg::Host { name: "x".into(), seed: 1, version: 999, config: None })).await.unwrap();
        assert!(matches!(recv(&mut a).await, ServerMsg::Error { .. }));
    }
}
