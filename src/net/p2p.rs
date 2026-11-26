//! Direct links between players' computers (WebRTC data channels), so a
//! room's traffic runs through the host's computer instead of the room
//! server. The server only introduces players (it passes the connection
//! offers along) and relays for anyone a direct link can't reach (some home
//! and mobile networks block them). Phones never host and never open direct
//! links: they always go through the server.
//!
//! Runs on its own thread with a Tokio runtime; the game talks to it through
//! channels ([`P2p::command`] / [`P2p::events`]).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use serde_json::Value;
use tokio::sync::mpsc;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceGatheringState, RTCIceServer, RTCPeerConnectionState,
    RTCSessionDescription, Registry, register_default_interceptors,
};

/// Public STUN servers: tell each computer its address as the internet sees it.
const STUN: [&str; 2] = ["stun:stun.l.google.com:19302", "stun:stun.cloudflare.com:3478"];

/// Game → link thread.
#[derive(Debug)]
pub enum Cmd {
    /// Open a link to `peer` (a guest calls its host).
    Call { peer: u8 },
    /// A signalling message from `from`, passed on by the room server.
    Signal { from: u8, data: Value },
    Send { peer: u8, text: String },
    Close { peer: u8 },
    CloseAll,
}

/// Link thread → game.
#[derive(Debug, Clone, PartialEq)]
pub enum Ev {
    /// Pass this to `to` through the room server.
    Signal { to: u8, data: Value },
    Open { peer: u8 },
    Closed { peer: u8 },
    Msg { peer: u8, text: String },
}

/// Handle to the link thread. Dropping it closes every link.
pub struct P2p {
    cmd: mpsc::UnboundedSender<Cmd>,
    ev: std::sync::Mutex<std_mpsc::Receiver<Ev>>,
}

impl P2p {
    pub fn start() -> Option<Self> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ev_tx, ev_rx) = std_mpsc::channel();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().thread_name("hallways-p2p").build().ok()?;
        std::thread::Builder::new()
            .name("hallways-p2p-main".into())
            .spawn(move || rt.block_on(run(cmd_rx, ev_tx)))
            .ok()?;
        Some(Self { cmd: cmd_tx, ev: std::sync::Mutex::new(ev_rx) })
    }

    pub fn command(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }

    /// Everything that happened since the last call.
    pub fn events(&self) -> Vec<Ev> {
        self.ev.lock().map(|r| r.try_iter().collect()).unwrap_or_default()
    }
}

impl Drop for P2p {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::CloseAll);
    }
}

struct Conn {
    pc: Arc<dyn PeerConnection>,
    dc: Option<Arc<dyn DataChannel>>,
    /// We made the call (sent the offer).
    caller: bool,
    /// Our offer/answer went out.
    sent: bool,
}

/// Longest wait for our addresses before sending the offer/answer anyway
/// (the "gathering complete" event doesn't always come).
const GATHER_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

fn gather_deadline(peer: u8, inner: &mpsc::UnboundedSender<Inner>) {
    let inner = inner.clone();
    tokio::spawn(async move {
        tokio::time::sleep(GATHER_WAIT).await;
        let _ = inner.send(Inner::Gathered { peer });
    });
}

/// Internal: link events from the WebRTC callbacks back to the main loop.
enum Inner {
    Gathered { peer: u8 },
    Channel { peer: u8, dc: Arc<dyn DataChannel> },
    State { peer: u8, failed: bool },
}

struct Handler {
    peer: u8,
    inner: mpsc::UnboundedSender<Inner>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        bevy::log::debug!("p2p: peer {} gathering {state:?}", self.peer);
        if state == RTCIceGatheringState::Complete {
            let _ = self.inner.send(Inner::Gathered { peer: self.peer });
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        bevy::log::debug!("p2p: peer {} connection {state:?}", self.peer);
        if matches!(state, RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed | RTCPeerConnectionState::Disconnected) {
            let _ = self.inner.send(Inner::State { peer: self.peer, failed: true });
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let _ = self.inner.send(Inner::Channel { peer: self.peer, dc });
    }
}

async fn new_conn(peer: u8, inner: &mpsc::UnboundedSender<Inner>) -> Option<Arc<dyn PeerConnection>> {
    let mut media = MediaEngine::default();
    media.register_default_codecs().ok()?;
    let registry = register_default_interceptors(Registry::new(), &mut media).ok()?;
    let config = RTCConfigurationBuilder::new().with_ice_servers(vec![RTCIceServer { urls: STUN.iter().map(|s| s.to_string()).collect(), ..Default::default() }]).build();
    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_handler(Arc::new(Handler { peer, inner: inner.clone() }))
        .with_runtime(webrtc::runtime::default_runtime()?)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await
        .map_err(|e| bevy::log::warn!("p2p: couldn't make a link: {e}"))
        .ok()?;
    Some(Arc::new(pc))
}

/// Reads a channel until it closes, forwarding messages to the game.
fn watch(peer: u8, dc: Arc<dyn DataChannel>, ev: std_mpsc::Sender<Ev>, inner: mpsc::UnboundedSender<Inner>) {
    tokio::spawn(async move {
        while let Some(e) = dc.poll().await {
            match e {
                DataChannelEvent::OnOpen => {
                    let _ = ev.send(Ev::Open { peer });
                }
                DataChannelEvent::OnMessage(m) => {
                    if let Ok(text) = String::from_utf8(m.data.to_vec()) {
                        let _ = ev.send(Ev::Msg { peer, text });
                    }
                }
                DataChannelEvent::OnClose => break,
                _ => {}
            }
        }
        let _ = inner.send(Inner::State { peer, failed: true });
    });
}

async fn run(mut cmds: mpsc::UnboundedReceiver<Cmd>, ev: std_mpsc::Sender<Ev>) {
    let (inner_tx, mut inner) = mpsc::unbounded_channel::<Inner>();
    let mut conns: HashMap<u8, Conn> = HashMap::new();
    loop {
        tokio::select! {
            c = cmds.recv() => {
                let Some(c) = c else { break };
                match c {
                    Cmd::Call { peer } => {
                        if let Some(old) = conns.remove(&peer) {
                            let _ = old.pc.close().await;
                        }
                        let Some(pc) = new_conn(peer, &inner_tx).await else {
                            let _ = ev.send(Ev::Closed { peer });
                            continue;
                        };
                        // The caller makes the channel; the other side gets it in on_data_channel.
                        let dc = pc.create_data_channel("game", None).await.ok();
                        let ok = async {
                            let offer = pc.create_offer(None).await.ok()?;
                            pc.set_local_description(offer).await.ok()
                        }.await;
                        if ok.is_none() || dc.is_none() {
                            let _ = ev.send(Ev::Closed { peer });
                            continue;
                        }
                        if let Some(dc) = dc.clone() {
                            watch(peer, dc, ev.clone(), inner_tx.clone());
                        }
                        conns.insert(peer, Conn { pc, dc, caller: true, sent: false });
                        gather_deadline(peer, &inner_tx);
                        // The offer goes out once its addresses are gathered (Inner::Gathered).
                    }
                    Cmd::Signal { from, data } => {
                        let desc = match serde_json::from_value::<RTCSessionDescription>(data["sdp"].clone()) {
                            Ok(d) => d,
                            Err(e) => {
                                bevy::log::warn!("p2p: bad description: {e}");
                                continue;
                            }
                        };
                        let is_offer = data["kind"] == "offer";
                        if is_offer {
                            // Someone is calling us (we host): answer.
                            if let Some(old) = conns.remove(&from) {
                                let _ = old.pc.close().await;
                            }
                            let Some(pc) = new_conn(from, &inner_tx).await else { continue };
                            let ok = async {
                                pc.set_remote_description(desc).await.map_err(|e| bevy::log::warn!("p2p: remote offer: {e}")).ok()?;
                                let answer = pc.create_answer(None).await.map_err(|e| bevy::log::warn!("p2p: answer: {e}")).ok()?;
                                pc.set_local_description(answer).await.map_err(|e| bevy::log::warn!("p2p: local answer: {e}")).ok()
                            }.await;
                            if ok.is_some() {
                                conns.insert(from, Conn { pc, dc: None, caller: false, sent: false });
                                gather_deadline(from, &inner_tx);
                            }
                        } else if let Some(c) = conns.get(&from) {
                            if c.pc.set_remote_description(desc).await.is_err() {
                                let _ = ev.send(Ev::Closed { peer: from });
                            }
                        }
                    }
                    Cmd::Send { peer, text } => {
                        if let Some(dc) = conns.get(&peer).and_then(|c| c.dc.clone()) {
                            let _ = dc.send_text(text.as_str()).await;
                        }
                    }
                    Cmd::Close { peer } => {
                        if let Some(c) = conns.remove(&peer) {
                            let _ = c.pc.close().await;
                        }
                    }
                    Cmd::CloseAll => {
                        for (_, c) in conns.drain() {
                            let _ = c.pc.close().await;
                        }
                    }
                }
            }
            i = inner.recv() => {
                let Some(i) = i else { break };
                match i {
                    Inner::Gathered { peer } => {
                        if let Some(c) = conns.get_mut(&peer) {
                            if c.sent {
                                continue;
                            }
                            c.sent = true;
                            if let Some(desc) = c.pc.local_description().await {
                                let kind = if c.caller { "offer" } else { "answer" };
                                if let Ok(sdp) = serde_json::to_value(&desc) {
                                    let _ = ev.send(Ev::Signal { to: peer, data: serde_json::json!({ "kind": kind, "sdp": sdp }) });
                                }
                            }
                        }
                    }
                    Inner::Channel { peer, dc } => {
                        if let Some(c) = conns.get_mut(&peer) {
                            c.dc = Some(dc.clone());
                            // It may already be open by the time we get it.
                            if matches!(dc.ready_state().await, Ok(webrtc::data_channel::RTCDataChannelState::Open)) {
                                let _ = ev.send(Ev::Open { peer });
                            }
                            watch(peer, dc, ev.clone(), inner_tx.clone());
                        }
                    }
                    Inner::State { peer, failed } => {
                        if failed && conns.remove(&peer).is_some() {
                            let _ = ev.send(Ev::Closed { peer });
                        }
                    }
                }
            }
        }
    }
    for (_, c) in conns.drain() {
        let _ = c.pc.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Two links in one process, signalling each other directly (loopback).
    #[test]
    #[ignore = "opens real network sockets"]
    fn two_computers_link_up_and_talk() {
        let _ = bevy::log::tracing_subscriber::fmt().with_env_filter("warn,hallways=debug").try_init();
        let a = P2p::start().unwrap();
        let b = P2p::start().unwrap();
        // a is player 1 (host), b is player 2 calling it.
        b.command(Cmd::Call { peer: 1 });
        let start = Instant::now();
        let (mut a_open, mut b_open, mut got) = (false, false, false);
        while start.elapsed() < Duration::from_secs(20) && !got {
            for e in a.events() {
                eprintln!("a: {e:?}");
                match e {
                    Ev::Signal { data, .. } => b.command(Cmd::Signal { from: 1, data }),
                    Ev::Open { .. } => a_open = true,
                    Ev::Msg { text, .. } => got = text == "hello host",
                    _ => {}
                }
            }
            for e in b.events() {
                eprintln!("b: {e:?}");
                match e {
                    Ev::Signal { data, .. } => a.command(Cmd::Signal { from: 2, data }),
                    Ev::Open { .. } => {
                        b_open = true;
                        b.command(Cmd::Send { peer: 1, text: "hello host".into() });
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(a_open && b_open && got, "a_open {a_open} b_open {b_open} got {got}");
    }
}
