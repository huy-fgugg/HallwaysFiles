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
    /// Forward `data` to some players only (the host passing on a message
    /// that came to it directly, to players without a direct link).
    RelayTo { to: Vec<u8>, data: serde_json::Value },
    /// Pass a direct-link (peer-to-peer) setup message to one player.
    Signal { to: u8, data: serde_json::Value },
    /// What this player's device can do as the room's host (sent on joining
    /// and every few seconds). The server hands the host role to the best one.
    HostInfo(HostInfo),
    /// Choosing a new host by vote: this player's pick.
    Vote { id: u8 },
    /// Host only: everyone leaves the lift now.
    Start,
    Ping,
}

/// A player's device, as a possible host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    /// Runs the game well enough to host (a computer, not the Low-End
    /// Edition, enough cores, a steady frame rate).
    pub capable: bool,
    /// Can host at all (any computer; phones never host).
    pub eligible: bool,
    /// Higher is better (frame rate, cores, direct links working).
    pub score: u32,
}

impl Default for HostInfo {
    /// Older games never say: treat them as able to host, last choice.
    fn default() -> Self {
        Self { capable: true, eligible: true, score: 0 }
    }
}

/// One name on the host vote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u8,
    pub name: String,
    /// On a computer (can be voted in).
    pub eligible: bool,
    pub votes: u8,
}

/// Seconds the host vote stays open (it ends sooner once everyone voted).
pub const VOTE_SECS: u64 = 20;

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
    /// The host left and no computer here can take over on its own: vote.
    /// Sent again as votes come in.
    HostVote { candidates: Vec<Candidate>, seconds_left: u64 },
    /// The host left and nobody left can host (only phones): the room ends.
    NoHost,
    /// A direct-link setup message from `from`.
    Signal { from: u8, data: serde_json::Value },
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

/// Who takes over when the host leaves: the best player whose computer
/// runs the game well. None = nobody qualifies (then the room votes).
pub fn auto_host(players: &[(u8, HostInfo)]) -> Option<u8> {
    players.iter().filter(|(_, h)| h.capable && h.eligible).max_by(|a, b| a.1.score.cmp(&b.1.score).then(b.0.cmp(&a.0))).map(|(id, _)| *id)
}

/// The vote's result: the most votes among players on a computer (ties go
/// to the better computer, then the earlier player). None = only phones left.
pub fn vote_winner(players: &[(u8, HostInfo, u8)]) -> Option<u8> {
    players.iter().filter(|(_, h, _)| h.eligible).max_by(|a, b| a.2.cmp(&b.2).then(a.1.score.cmp(&b.1.score)).then(b.0.cmp(&a.0))).map(|(id, _, _)| *id)
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

    #[test]
    fn choosing_a_new_host() {
        let pc = |score| HostInfo { capable: true, eligible: true, score };
        let slow_pc = HostInfo { capable: false, eligible: true, score: 50 };
        let phone = HostInfo { capable: false, eligible: false, score: 0 };
        // The best capable computer takes over; phones and slow computers never do automatically.
        assert_eq!(auto_host(&[(2, phone), (3, pc(400)), (4, pc(900)), (5, slow_pc)]), Some(4));
        assert_eq!(auto_host(&[(2, phone), (5, slow_pc)]), None);
        // Then the vote: most votes among computers wins, phones can't win.
        assert_eq!(vote_winner(&[(2, phone, 3), (5, slow_pc, 1), (6, HostInfo { score: 10, ..slow_pc }, 0)]), Some(5));
        // Tie: the better computer.
        assert_eq!(vote_winner(&[(5, slow_pc, 1), (6, HostInfo { score: 90, ..slow_pc }, 1)]), Some(6));
        // Nobody voted: still a computer.
        assert_eq!(vote_winner(&[(2, phone, 0), (5, slow_pc, 0)]), Some(5));
        // Only phones: no host.
        assert_eq!(vote_winner(&[(2, phone, 2), (3, phone, 0)]), None);
        // Old games that never say are treated as able.
        let m: ClientMsg = serde_json::from_str(r#"{"t":"host_info","capable":false,"eligible":true,"score":7}"#).unwrap();
        assert_eq!(m, ClientMsg::HostInfo(HostInfo { capable: false, eligible: true, score: 7 }));
    }

}
