//! Shared state between the Riichi City bridge, the autoplay manager, and
//! the proxy's client→server relay: injected frames travel through the
//! broadcast channel; the rest is observation state (windows, settlements)
//! the bridge writes and autoplay reads.
//!
//! Broadcast rather than mpsc because every client→server flow subscribes
//! and only the gameplay flow should carry gameplay frames — the
//! `gameplay` flag plus the bridge's `in_game` gate enforce that in the
//! relay.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

pub type SharedInjectBus = Arc<InjectBus>;

/// One frame to transmit. `gameplay` frames are gated on `in_game`.
#[derive(Debug, Clone)]
pub struct InjectFrame {
    pub gameplay: bool,
    pub bytes: Vec<u8>,
}

pub struct InjectBus {
    tx: broadcast::Sender<InjectFrame>,
    in_game: Arc<AtomicBool>,
    /// Count of `rsp_game_action` responses and the latest code (0 = ok).
    /// The count moving after a send proves the server processed the
    /// frame — the injection counterpart of the Majsoul input watch.
    rsp_seen: AtomicU64,
    last_rsp_code: AtomicI64,
    /// Whether a decision window for us is currently open, set by the
    /// bridge on `cmd_send_current_action`/`cmd_send_other_action` and
    /// cleared when any action broadcast / settlement / room transition
    /// arrives. State, not a timestamp: acting before the window opens
    /// is rejected (`rsp code 1`), and manager-side clock comparisons
    /// against window-open times drift once plans queue up.
    window_open: AtomicBool,
    window_opened_at: std::sync::Mutex<Option<std::time::Instant>>,
    /// (han, yaku count) of the last settlement; (0, 0) on a draw. The
    /// round-advance reading beat scales with it — bigger hands render
    /// longer.
    settlement: std::sync::Mutex<(u32, u32)>,
    /// Highest `message_index` seen on the client's uplink; `send`
    /// re-stamps injected frames past it so the connection's request
    /// counter never rewinds.
    up_index: AtomicU32,
}

impl Default for InjectBus {
    fn default() -> Self {
        Self::new()
    }
}

impl InjectBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            tx,
            in_game: Arc::new(AtomicBool::new(false)),
            rsp_seen: AtomicU64::new(0),
            last_rsp_code: AtomicI64::new(0),
            up_index: AtomicU32::new(0),
            window_open: AtomicBool::new(false),
            window_opened_at: std::sync::Mutex::new(None),
            settlement: std::sync::Mutex::new((0, 0)),
        }
    }

    pub fn note_rsp(&self, code: i64) {
        self.rsp_seen.fetch_add(1, Ordering::Relaxed);
        self.last_rsp_code.store(code, Ordering::Relaxed);
    }

    pub fn rsp_ticket(&self) -> u64 {
        self.rsp_seen.load(Ordering::Relaxed)
    }

    pub fn rsp_since(&self, ticket: u64) -> bool {
        self.rsp_seen.load(Ordering::Relaxed) > ticket
    }

    pub fn note_up_index(&self, idx: u32) {
        self.up_index.fetch_max(idx, Ordering::Relaxed);
    }

    pub fn note_window(&self) {
        *self
            .window_opened_at
            .lock()
            .expect("window_opened_at poisoned") = Some(std::time::Instant::now());
        self.window_open.store(true, Ordering::Relaxed);
    }

    pub fn window_opened_at(&self) -> Option<std::time::Instant> {
        *self
            .window_opened_at
            .lock()
            .expect("window_opened_at poisoned")
    }

    pub fn note_window_closed(&self) {
        self.window_open.store(false, Ordering::Relaxed);
    }

    pub fn window_is_open(&self) -> bool {
        self.window_open.load(Ordering::Relaxed)
    }

    pub fn note_settlement(&self, han: u32, yakus: u32) {
        *self.settlement.lock().expect("settlement poisoned") = (han, yakus);
    }

    pub fn settlement(&self) -> (u32, u32) {
        *self.settlement.lock().expect("settlement poisoned")
    }

    pub fn last_rsp_code(&self) -> i64 {
        self.last_rsp_code.load(Ordering::Relaxed)
    }

    /// Queue a wire frame for the game server. `false` when no relay is
    /// subscribed (capture not running). Re-stamps the frame's
    /// `message_index` past the client's live uplink counter — the
    /// builder encodes offline and cannot know it.
    pub fn send(&self, mut frame: InjectFrame) -> bool {
        stamp_message_index(&mut frame.bytes, &self.up_index);
        match self.tx.send(frame) {
            Ok(_) => true,
            Err(broadcast::error::SendError(_)) => false,
        }
    }

    /// Receiver side for the proxy's client→server WS relay loops.
    pub fn subscribe(&self) -> broadcast::Receiver<InjectFrame> {
        self.tx.subscribe()
    }

    /// Maintained by the bridge: `cmd_enter_room` … `cmd_room_end`.
    pub fn set_in_game(&self, v: bool) {
        self.in_game.store(v, Ordering::Relaxed);
    }

    pub fn in_game(&self) -> bool {
        self.in_game.load(Ordering::Relaxed)
    }
}

/// Rewrite a WPacket's `message_index` (header bytes 8..12, big-endian) to
/// the next value past `counter`, bumping it. Non-WPacket bytes pass
/// through untouched.
fn stamp_message_index(bytes: &mut [u8], counter: &AtomicU32) {
    if bytes.len() < 15 || bytes[4..8] != [0x00, 0x0f, 0x00, 0x01] {
        return;
    }
    let idx = counter.fetch_add(1, Ordering::Relaxed) + 1;
    bytes[8..12].copy_from_slice(&idx.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_without_subscribers_reports_false() {
        let bus = InjectBus::new();
        assert!(!bus.send(InjectFrame {
            gameplay: true,
            bytes: vec![1, 2, 3],
        }));
    }

    #[tokio::test]
    async fn subscribed_round_trip_and_in_game_flag() {
        let bus = InjectBus::new();
        let mut rx = bus.subscribe();
        let frame = InjectFrame {
            gameplay: true,
            bytes: vec![9],
        };
        assert!(bus.send(frame.clone()));
        assert_eq!(rx.recv().await.unwrap().bytes, frame.bytes);

        assert!(!bus.in_game());
        bus.set_in_game(true);
        assert!(bus.in_game());
    }

    #[test]
    fn window_state_opens_and_closes() {
        let bus = InjectBus::new();
        assert!(
            !bus.window_is_open(),
            "no window before the server offers one"
        );
        bus.note_window();
        assert!(bus.window_is_open());
        bus.note_window_closed();
        assert!(!bus.window_is_open(), "resolved by an action broadcast");
    }

    #[test]
    fn rsp_tickets_track_responses() {
        let bus = InjectBus::new();
        let t = bus.rsp_ticket();
        assert!(!bus.rsp_since(t), "nothing arrived yet");
        bus.note_rsp(0);
        bus.note_rsp(0);
        assert!(bus.rsp_since(t));
        assert_eq!(bus.rsp_ticket(), 2);
        assert_eq!(bus.last_rsp_code(), 0);

        bus.note_rsp(4001);
        assert_eq!(bus.last_rsp_code(), 4001);
    }

    /// Injected frames must be indexed past whatever the client has already
    /// sent, and must never reuse an index themselves.
    #[tokio::test]
    async fn injected_frames_are_indexed_past_the_client() {
        let bus = InjectBus::new();
        let mut rx = bus.subscribe();
        bus.note_up_index(40); // the client just sent #40

        let frame = crate::bridge::riichi_city::build::user_prepare();
        assert!(bus.send(InjectFrame {
            gameplay: true,
            bytes: frame
        }));
        let sent = rx.recv().await.unwrap();
        let be = |b: &[u8]| u32::from_be_bytes(b[8..12].try_into().unwrap());
        assert_eq!(be(&sent.bytes), 41, "past the client's 40");

        // The client advances underneath us; our next frame clears that too.
        bus.note_up_index(100);
        let mut again = sent.bytes.clone();
        again[8..12].copy_from_slice(&0u32.to_be_bytes()); // stale index
        assert!(bus.send(InjectFrame {
            gameplay: true,
            bytes: again
        }));
        let sent2 = rx.recv().await.unwrap();
        assert_eq!(be(&sent2.bytes), 101);
    }
}
