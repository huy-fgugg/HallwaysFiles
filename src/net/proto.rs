//! Wire protocol between the game and the HALLWAYS room server.
//! Shared verbatim by the server crate (`server/src/main.rs` includes this
//! file), so both sides always agree. JSON text frames over a WebSocket.
//!
//! The server only understands rooms; gameplay payloads travel as opaque
//! JSON inside `Relay` and are interpreted by the game (`net::PeerMsg`).

use serde::{Deserialize, Serialize};

/// Bump when messages change incompatibly; the server rejects mismatches.
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_PLAYERS: u8 = 4;
pub const CODE_LEN: usize = 5;
/// No 0/O or 1/I so codes are easy to read out loud.
pub const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Create a room. The server answers with `Welcome` (including the code).
    Host {
        name: String,
        seed: u64,
        version: u32,
        /// Custom floor preset, passed through to everyone who joins.
        #[serde(default)]
        config: Option<serde_json::Value>,
        /// Random number this game picked, so it can rejoin after a dropped connection.
        #[serde(default)]
        token: u64,
    },
    /// Join a room by code.
    Join {
        code: String,
        name: String,
        version: u32,
        #[serde(default)]
        token: u64,
    },
    /// Come back to a room after the connection dropped (allowed even after
    /// the group has left the lift, for a couple of minutes, with the same token).
    /// If the server restarted and lost the room, the first player back
    /// recreates it from `seed`, `started` and `config`, and the rest rejoin it.
    Rejoin {
        code: String,
        name: String,
        version: u32,
        token: u64,
        #[serde(default)]
        seed: u64,
        #[serde(default)]
        started: bool,
        #[serde(default)]
        config: Option<serde_json::Value>,
    },
    /// Room host only: remove a player from the room.
    Kick { id: u8, #[serde(default)] reason: String },
    /// Server admin tools; `key` must match the server's `HALLWAYS_ADMIN_KEY`.
    Admin { key: String, cmd: AdminCmd },
    /// Forward `data` to everyone else in the room.
    Relay { data: serde_json::Value },
    /// Host only: everyone leaves the lift now.
    Start,
    Ping,
}

/// What a server admin can do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "c", rename_all = "snake_case")]
pub enum AdminCmd {
    /// List every room on the server.
    Rooms,
    /// Message shown to everyone (or to one room, by code).
    Announce { text: String, #[serde(default)] code: Option<String> },
    /// Close a room: everyone in it is told why and disconnected.
    CloseRoom { code: String, #[serde(default)] reason: String },
    /// Remove one player from a room.
    KickPlayer { code: String, id: u8, #[serde(default)] reason: String },
}

/// One room, as the admin tools list it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomSummary {
    pub code: String,
    pub players: Vec<PlayerInfo>,
    pub host_id: u8,
    pub started: bool,
    /// Seconds since the room was made.
    pub age: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: u8,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome {
        code: String,
        id: u8,
        host_id: u8,
        seed: u64,
        max: u8,
        players: Vec<PlayerInfo>,
        #[serde(default)]
        config: Option<serde_json::Value>,
    },
    Joined { player: PlayerInfo },
    Left { id: u8 },
    /// The host left; this player now hosts.
    HostIs { id: u8 },
    Relay { from: u8, data: serde_json::Value },
    Start,
    Pong,
    Error { msg: String },
    /// You were removed from the room (kicked, or the room was closed).
    Kicked { reason: String },
    /// An admin's message for everyone.
    Announcement { text: String },
    /// Admin tools: the server's rooms.
    AdminRooms { rooms: Vec<RoomSummary> },
    /// Admin tools: done (with a short result).
    AdminOk { msg: String },
}

/// Room names are unique ignoring case and surrounding spaces.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Normalises a typed room code (case, spaces, dashes).
pub fn normalize_code(input: &str) -> String {
    input.chars().filter(|c| !c.is_whitespace() && *c != '-').map(|c| c.to_ascii_uppercase()).take(CODE_LEN).collect()
}

pub fn valid_code(code: &str) -> bool {
    code.len() == CODE_LEN && code.bytes().all(|b| CODE_ALPHABET.contains(&b))
}

/// Builds a room code from random bytes.
pub fn code_from_bytes(bytes: [u8; CODE_LEN]) -> String {
    bytes.iter().map(|b| CODE_ALPHABET[*b as usize % CODE_ALPHABET.len()] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_as_tagged_json() {
        let m = ClientMsg::Join { code: "ABCDE".into(), name: "Ana".into(), version: PROTOCOL_VERSION, token: 7 };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"t\":\"join\""));
        assert_eq!(serde_json::from_str::<ClientMsg>(&s).unwrap(), m);
        let r = ServerMsg::Relay { from: 2, data: serde_json::json!({"x": 1.5}) };
        assert_eq!(serde_json::from_str::<ServerMsg>(&serde_json::to_string(&r).unwrap()).unwrap(), r);
    }

    #[test]
    fn older_games_still_parse() {
        // A 1.1.4 game sends Join without a token.
        let m: ClientMsg = serde_json::from_str(r#"{"t":"join","code":"ABCDE","name":"Ana","version":1}"#).unwrap();
        assert!(matches!(m, ClientMsg::Join { token: 0, .. }));
        let a = ClientMsg::Admin { key: "k".into(), cmd: AdminCmd::Announce { text: "hi".into(), code: None } };
        assert_eq!(serde_json::from_str::<ClientMsg>(&serde_json::to_string(&a).unwrap()).unwrap(), a);
        assert!(same_name(" ana ", "ANA"));
        assert!(!same_name("Ana", "Anna"));
    }

    #[test]
    fn codes() {
        let c = code_from_bytes([0, 31, 32, 200, 7]);
        assert!(valid_code(&c), "{c}");
        assert_eq!(normalize_code(" abc-de "), "ABCDE");
        assert!(!valid_code("ABC"));
        assert!(!valid_code("ABCD0"));
        assert!(!valid_code(&normalize_code("abcdo")), "O is never issued");
    }
}
