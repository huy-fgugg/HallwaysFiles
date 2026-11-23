//! Wire protocol between the game and the HALLWAYS room server.
//! Shared verbatim by the server crate (`server/src/main.rs` includes this
//! file), so both sides always agree. JSON text frames over a WebSocket.
//!
//! The server only understands rooms; gameplay payloads travel as opaque
//! JSON inside `Relay` and are interpreted by the game (`net::PeerMsg`).

use serde::{Deserialize, Serialize};

/// Bump when messages change incompatibly; the server rejects mismatches.
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_PLAYERS: u8 = 5;
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
    },
    /// Join a room by code.
    Join { code: String, name: String, version: u32 },
    /// Forward `data` to everyone else in the room.
    Relay { data: serde_json::Value },
    /// Host only: everyone leaves the lift now.
    Start,
    Ping,
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
        let m = ClientMsg::Join { code: "ABCDE".into(), name: "Ana".into(), version: PROTOCOL_VERSION };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.contains("\"t\":\"join\""));
        assert_eq!(serde_json::from_str::<ClientMsg>(&s).unwrap(), m);
        let r = ServerMsg::Relay { from: 2, data: serde_json::json!({"x": 1.5}) };
        assert_eq!(serde_json::from_str::<ServerMsg>(&serde_json::to_string(&r).unwrap()).unwrap(), r);
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
