//! Our own observation (feature) encoding.
//!
//! We deliberately do **not** use riichienv-core's `Observation::encode()`
//! (that tensor method is `python`-feature-gated and unavailable in a pure-Rust
//! build). Instead we design a compact, self-relative feature plane and encode
//! it from an [`EncInput`] that is built the same way for both training-data
//! extraction and live inference — so the model always sees identical inputs.
//!
//! ## Layout
//!
//! Output is a **channel-major** `f32` buffer of shape `[C, T]` flattened as
//! `buf[ch * T + tile]`, where `T` is the tile axis (34 for 4p, 27 for sanma)
//! and `C` = [`channels`]`(num_players)`. Everything is encoded in *relative*
//! seat order — index 0 is always the deciding player, then the seats to their
//! right (shimocha, toimen, kamicha).
//!
//! Channel groups (per-player groups have `N` planes = player count):
//! ```text
//!   self hand thresholds (>=1,>=2,>=3,>=4)   4
//!   self aka-5 in hand                        1
//!   self drawn tile (one-hot)                 1
//!   self tenpai waits (one-hot set)           1
//!   dora indicators (count)                   1
//!   dora tiles (count)                        1
//!   per-player discards (count)               N
//!   per-player meld tiles (count)             N
//!   per-player riichi (broadcast)             N
//!   per-player riichi-declare tile (one-hot)  N
//!   per-player score (broadcast, normalized)  N
//!   last discard (one-hot, call context)      1
//!   round wind (one-hot on honor tile)        1
//!   self seat wind (one-hot on honor tile)    1
//!   honba (broadcast)                         1
//!   riichi sticks (broadcast)                 1
//!   turn progress (broadcast)                 1
//!   self tenpai (broadcast)                   1
//!   self is-dealer (broadcast)                1
//!   kyoku index (broadcast)                   1
//!   self riichi-declared (broadcast)          1
//!   per-player kita count (broadcast, sanma)  N   (4p: 0 planes)
//! ```
//!
//! ## Feature sets
//!
//! The layout above is [`Features::V1`], what every bundled weight file was
//! trained on. [`Features::V2`] keeps it unchanged and **appends** planes for
//! what V1 cannot see — discard order, tedashi / tsumogiri, the tiles each
//! player can no longer ron, shanten and the effect of each discard:
//! ```text
//!   per-player tedashi discards (count)       N
//!   per-player discard recency (0.85^age)     N
//!   per-player latest discard (one-hot)       N
//!   per-player latest tedashi (one-hot)       N
//!   per-player passed tiles (one-hot set)     N   (see `timeline`)
//!   per-player meld count (broadcast)         N
//!   per-player placement (broadcast, 0=top)   N
//!   unseen copies (count)                     1
//!   self shanten <= 0, 1, 2, 3 (broadcast)    4
//!   self furiten (broadcast)                  1
//!   discard keeps best shanten (one-hot set)  1   (only with a discard to make)
//!   ukeire after that discard (/20)           1   (only with a discard to make)
//! ```
//! [`Features::V3`] keeps V2 and appends how far the game is from its end,
//! which V1's round wind and kyoku index cannot say on their own — East 3 is
//! the last round of an east-only game but the third of six in a hanchan:
//! ```text
//!   rounds left after this one (broadcast)    1   (/ the most a game has)
//!   this is the last round (broadcast)        1
//! ```
//! The game's planned length ([`GameLength`]) comes from outside the events:
//! the log's room tag when extracting, the platform's room settings live.
//!
//! A weight file's input width says which set it was trained on
//! ([`Features::of_channels`]), so old and new weights load side by side.

use riichienv_core::shanten::{calc_shanten_from_counts, calc_shanten_from_counts_3p};

use crate::tiles::{is_aka, next_dora_tile34, tile34, tile_dim, tile_index, AKA_TILE34};

/// Planes that describe the deciding player only, before the per-player groups:
/// hand thresholds (4) + aka + drawn tile + waits + dora indicators + dora tiles.
const SELF_PLANES: usize = 4 + 1 + 1 + 1 + 1 + 1; // = 9
/// Table-wide planes that follow the per-player groups: last discard, round wind,
/// seat wind, honba, riichi sticks, turn progress, self tenpai, self is-dealer,
/// kyoku index, self riichi-declared.
const GLOBAL_PLANES: usize = 10;
/// Per-player groups before the globals: discards, melds, riichi, riichi tile, score.
const PER_PLAYER_GROUPS: usize = 5;

/// Number of feature channels for the given player count.
pub fn channels(num_players: u8) -> usize {
    let n = num_players as usize;
    let kita_groups = if num_players == 3 { 1 } else { 0 };
    SELF_PLANES + GLOBAL_PLANES + (PER_PLAYER_GROUPS + kita_groups) * n
}

/// Per-player groups V2 appends: tedashi, recency, latest discard, latest
/// tedashi, passed tiles, meld count, placement.
const V2_PER_PLAYER_GROUPS: usize = 7;
/// Self/table planes V2 appends: unseen copies, shanten x4, furiten,
/// best-shanten discards, ukeire after discard.
const V2_SELF_PLANES: usize = 8;
/// Decay per discard of the recency plane: the latest is 1.0, the one before
/// 0.85, and so on.
const RECENCY: f32 = 0.85;
/// tile34 of North, the sanma kita tile.
const NORTH34: usize = 30;

/// Planes V3 appends: rounds left after this one, last round.
const V3_PLANES: usize = 2;

/// Which feature planes to encode; see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Features {
    #[default]
    V1,
    V2,
    V3,
}

/// How long a game is planned to be: east rounds only, or east and south.
/// An extension round (a game going on past its planned end) counts as the
/// last round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameLength {
    East,
    /// Hanchan — what nearly every training log is, so also the default
    /// when the length is unknown.
    #[default]
    EastSouth,
}

impl GameLength {
    /// Rounds in a game of this length (one per seat per wind).
    pub fn rounds(self, num_players: u8) -> u8 {
        match self {
            GameLength::East => num_players,
            GameLength::EastSouth => 2 * num_players,
        }
    }

    /// Rounds after the one at `round_wind` / `kyoku_index` (within its wind).
    pub fn rounds_left(self, num_players: u8, round_wind: u8, kyoku_index: u8) -> u8 {
        let played = round_wind
            .saturating_mul(num_players)
            .saturating_add(kyoku_index);
        self.rounds(num_players)
            .saturating_sub(played.saturating_add(1))
    }

    /// Tenhou's room tag: bit 0x08 of the hex game type (`…gm-00b9-…`) is the
    /// south half. `None` when `name` carries no such tag.
    pub fn of_tenhou_log(name: &str) -> Option<Self> {
        let tag = name.split("gm-").nth(1)?.get(..4)?;
        let kind = u16::from_str_radix(tag, 16).ok()?;
        Some(if kind & 0x08 != 0 {
            GameLength::EastSouth
        } else {
            GameLength::East
        })
    }
}

impl Features {
    /// Number of feature channels of this set for the given player count.
    pub fn channels(self, num_players: u8) -> usize {
        let n = num_players as usize;
        match self {
            Features::V1 => channels(num_players),
            Features::V2 => channels(num_players) + V2_PER_PLAYER_GROUPS * n + V2_SELF_PLANES,
            Features::V3 => Features::V2.channels(num_players) + V3_PLANES,
        }
    }

    /// The set a model with `channels` input channels was trained on.
    pub fn of_channels(num_players: u8, channels: usize) -> Option<Self> {
        [Features::V1, Features::V2, Features::V3]
            .into_iter()
            .find(|f| f.channels(num_players) == channels)
    }

    /// `v1` / `v2` / `v3`, as the extractor's command line spells it.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "v1" => Some(Features::V1),
            "v2" => Some(Features::V2),
            "v3" => Some(Features::V3),
            _ => None,
        }
    }

    /// Does this set include the V2 planes (and so need their inputs)?
    pub fn has_v2(self) -> bool {
        self != Features::V1
    }
}

/// Channel index of the "last discard" one-hot plane — the first of the global
/// planes, i.e. straight after the per-player groups. `encode` `debug_assert`s
/// its cursor against this, so the two can't drift apart.
pub fn last_discard_channel(num_players: u8) -> usize {
    SELF_PLANES + PER_PLAYER_GROUPS * num_players as usize
}

/// Per-player, self-relative view fed to the encoder.
#[derive(Debug, Clone, Default)]
pub struct SeatFeat {
    /// Raw tile ids (0..=135) discarded by this seat, in order.
    pub discards: Vec<u8>,
    /// Raw tile ids that make up this seat's called melds (flattened).
    pub meld_tiles: Vec<u8>,
    pub riichi_declared: bool,
    /// Raw tile id of this seat's riichi-declaration discard, if any.
    pub riichi_tile: Option<u8>,
    pub score: i32,
    /// Number of kita (nukidora) set aside (sanma only).
    pub kita_count: u8,
    // --- V2 only (left empty for V1) ---
    /// Per discard, whether it came from the hand (tedashi) rather than
    /// straight off the draw; parallel to `discards`.
    pub tedashi: Vec<bool>,
    /// Raw tile ids this seat claimed from discards (chi/pon/daiminkan). They
    /// are in this seat's `meld_tiles` **and** still in the discarder's
    /// `discards`, so the unseen count takes them out once.
    pub called_tiles: Vec<u8>,
    pub meld_count: u8,
    /// Raw tile ids this seat can no longer ron: discarded by others after its
    /// riichi, or after its own latest discard (see `timeline`).
    pub passed: Vec<u8>,
}

/// Everything the encoder needs, already resolved to relative seat order
/// (index 0 = deciding player). Built by adapters in [`crate::adapt`].
#[derive(Debug, Clone)]
pub struct EncInput {
    pub num_players: u8,
    /// Deciding player's concealed hand (raw tile ids).
    pub hand: Vec<u8>,
    /// Deciding player's just-drawn tile, if any.
    pub drawn_tile: Option<u8>,
    /// Deciding player's tenpai waits as tile34 indices.
    pub waits: Vec<u8>,
    pub is_tenpai: bool,
    /// Dora *indicator* raw tile ids.
    pub dora_indicators: Vec<u8>,
    /// Per-seat features, relative order (index 0 = self).
    pub seats: Vec<SeatFeat>,
    /// Most-recent discard on the table (raw tile id) — call context.
    pub last_discard: Option<u8>,
    /// Round wind index (0=E,1=S,2=W,3=N).
    pub round_wind: u8,
    /// Deciding player's seat wind index (0=E dealer .. 3=N).
    pub seat_wind: u8,
    pub honba: u8,
    pub riichi_sticks: u32,
    pub turn_count: u32,
    pub is_dealer: bool,
    pub kyoku_index: u8,
    /// Deciding player's own riichi-declared flag.
    pub self_riichi: bool,
    /// Deciding player is furiten (V2 only).
    pub self_furiten: bool,
    /// Rounds the game has left after this one (V3 only; see [`GameLength`]).
    pub rounds_left: u8,
}

impl EncInput {
    /// Encode into a channel-major `[C, T]` f32 buffer with the V1 planes.
    pub fn encode(&self) -> Vec<f32> {
        self.encode_as(Features::V1)
    }

    /// Encode with the planes of `features`.
    pub fn encode_as(&self, features: Features) -> Vec<f32> {
        let mut buf = self.encode_v1();
        if features.has_v2() {
            self.append_v2(&mut buf);
        }
        if features == Features::V3 {
            self.append_v3(&mut buf);
        }
        buf
    }

    fn encode_v1(&self) -> Vec<f32> {
        let np = self.num_players;
        let t = tile_dim(np);
        let c = channels(np);
        let n = np as usize;
        let mut buf = vec![0.0f32; c * t];

        // Cursor over channels; helpers close over `buf`, `t`.
        let mut ch = 0usize;
        macro_rules! set {
            ($channel:expr, $tile:expr, $val:expr) => {
                buf[$channel * t + $tile] = $val;
            };
        }
        // Fill an entire plane with a constant (broadcast scalar).
        macro_rules! broadcast {
            ($channel:expr, $val:expr) => {{
                let base = $channel * t;
                for i in 0..t {
                    buf[base + i] = $val;
                }
            }};
        }

        // --- self hand: 4 threshold planes + aka ---
        let mut hand_counts = vec![0u8; t];
        let mut aka_here = [false; 3];
        for &tid in &self.hand {
            if let Some(idx) = tile_index(tid, np) {
                hand_counts[idx] += 1;
            }
            if is_aka(tid) {
                for (k, &a34) in AKA_TILE34.iter().enumerate() {
                    if crate::tiles::tile34(tid) == a34 {
                        aka_here[k] = true;
                    }
                }
            }
        }
        for thresh in 1..=4u8 {
            for idx in 0..t {
                if hand_counts[idx] >= thresh {
                    set!(ch, idx, 1.0);
                }
            }
            ch += 1;
        }
        // aka plane
        for (k, &a34) in AKA_TILE34.iter().enumerate() {
            if aka_here[k] {
                if let Some(idx) = tile_index((a34 as u8) * 4, np) {
                    set!(ch, idx, 1.0);
                }
            }
        }
        ch += 1;

        // --- self drawn tile (one-hot) ---
        if let Some(tid) = self.drawn_tile {
            if let Some(idx) = tile_index(tid, np) {
                set!(ch, idx, 1.0);
            }
        }
        ch += 1;

        // --- self waits (tile34 -> compact) ---
        for &w34 in &self.waits {
            if let Some(idx) = tile_index(w34 * 4, np) {
                set!(ch, idx, 1.0);
            }
        }
        ch += 1;

        // --- dora indicators (count) ---
        for &tid in &self.dora_indicators {
            if let Some(idx) = tile_index(tid, np) {
                buf[ch * t + idx] += 0.25;
            }
        }
        ch += 1;

        // --- dora tiles (count) ---
        for &tid in &self.dora_indicators {
            let dora34 = next_dora_tile34(crate::tiles::tile34(tid) as u8, np);
            if let Some(idx) = tile_index(dora34 * 4, np) {
                buf[ch * t + idx] += 0.25;
            }
        }
        ch += 1;

        // --- per-player discards (count, normalized /4) ---
        for seat in &self.seats {
            for &tid in &seat.discards {
                if let Some(idx) = tile_index(tid, np) {
                    buf[ch * t + idx] += 0.25;
                }
            }
            ch += 1;
        }
        // Pad if fewer seats than n (defensive; adapters always fill n).
        ch += n.saturating_sub(self.seats.len());

        // --- per-player meld tiles (count, normalized /4) ---
        for seat in &self.seats {
            for &tid in &seat.meld_tiles {
                if let Some(idx) = tile_index(tid, np) {
                    buf[ch * t + idx] += 0.25;
                }
            }
            ch += 1;
        }
        ch += n.saturating_sub(self.seats.len());

        // --- per-player riichi (broadcast) ---
        for seat in &self.seats {
            if seat.riichi_declared {
                broadcast!(ch, 1.0);
            }
            ch += 1;
        }
        ch += n.saturating_sub(self.seats.len());

        // --- per-player riichi-declare tile (one-hot) ---
        for seat in &self.seats {
            if let Some(tid) = seat.riichi_tile {
                if let Some(idx) = tile_index(tid, np) {
                    set!(ch, idx, 1.0);
                }
            }
            ch += 1;
        }
        ch += n.saturating_sub(self.seats.len());

        // --- per-player score (broadcast, normalized, non-negative) ---
        // Kept in [0, 4] so the extractor can store obs as u8 without a sign.
        for seat in &self.seats {
            let v = (seat.score as f32 / 50_000.0).clamp(0.0, 4.0);
            broadcast!(ch, v);
            ch += 1;
        }
        ch += n.saturating_sub(self.seats.len());

        // --- last discard (one-hot) ---
        debug_assert_eq!(
            ch,
            last_discard_channel(np),
            "last-discard plane moved; update `last_discard_channel`"
        );
        if let Some(tid) = self.last_discard {
            if let Some(idx) = tile_index(tid, np) {
                set!(ch, idx, 1.0);
            }
        }
        ch += 1;

        // --- round wind (one-hot on honor position) ---
        // wind i lives at tile34 27+i; encode via tile id (27+i)*4.
        if let Some(idx) = tile_index((27 + self.round_wind.min(3)) as u8 * 4, np) {
            set!(ch, idx, 1.0);
        }
        ch += 1;

        // --- self seat wind (one-hot on honor position) ---
        if let Some(idx) = tile_index((27 + self.seat_wind.min(3)) as u8 * 4, np) {
            set!(ch, idx, 1.0);
        }
        ch += 1;

        // --- honba / sticks / turn / tenpai / dealer / kyoku / self-riichi (broadcast) ---
        broadcast!(ch, (self.honba as f32 / 4.0).min(2.0));
        ch += 1;
        broadcast!(ch, (self.riichi_sticks as f32 / 4.0).min(2.0));
        ch += 1;
        broadcast!(ch, (self.turn_count as f32 / 70.0).min(1.0));
        ch += 1;
        broadcast!(ch, if self.is_tenpai { 1.0 } else { 0.0 });
        ch += 1;
        broadcast!(ch, if self.is_dealer { 1.0 } else { 0.0 });
        ch += 1;
        broadcast!(ch, (self.kyoku_index as f32 / 8.0).min(1.0));
        ch += 1;
        broadcast!(ch, if self.self_riichi { 1.0 } else { 0.0 });
        ch += 1;

        // --- per-player kita count (broadcast, sanma only) ---
        if np == 3 {
            for seat in &self.seats {
                broadcast!(ch, (seat.kita_count as f32 / 4.0).min(1.0));
                ch += 1;
            }
            ch += n.saturating_sub(self.seats.len());
        }

        debug_assert_eq!(ch, c, "channel cursor must match declared channel count");
        buf
    }
}

impl EncInput {
    /// Append the V2 planes to a V1 buffer (channel-major, so appending planes
    /// is appending to the buffer).
    fn append_v2(&self, buf: &mut Vec<f32>) {
        let np = self.num_players;
        let t = tile_dim(np);
        let n = np as usize;
        let base = buf.len();
        debug_assert_eq!(base, channels(np) * t);
        buf.resize(Features::V2.channels(np) * t, 0.0);
        let mut ch = base / t;
        let idx = |tid: u8| tile_index(tid, np);
        macro_rules! broadcast {
            ($channel:expr, $val:expr) => {{
                let start = $channel * t;
                for v in &mut buf[start..start + t] {
                    *v = $val;
                }
            }};
        }

        // --- per-player tedashi discards (count) ---
        for k in 0..n {
            if let Some(seat) = self.seats.get(k) {
                for (i, &tid) in seat.discards.iter().enumerate() {
                    if seat.tedashi.get(i).copied().unwrap_or(false) {
                        if let Some(x) = idx(tid) {
                            buf[ch * t + x] += 0.25;
                        }
                    }
                }
            }
            ch += 1;
        }

        // --- per-player discard recency ---
        for k in 0..n {
            if let Some(seat) = self.seats.get(k) {
                let len = seat.discards.len();
                for (i, &tid) in seat.discards.iter().enumerate() {
                    if let Some(x) = idx(tid) {
                        let v = RECENCY.powi((len - 1 - i) as i32);
                        let cell = &mut buf[ch * t + x];
                        *cell = cell.max(v);
                    }
                }
            }
            ch += 1;
        }

        // --- per-player latest discard / latest tedashi (one-hot) ---
        for k in 0..n {
            if let Some(&tid) = self.seats.get(k).and_then(|s| s.discards.last()) {
                if let Some(x) = idx(tid) {
                    buf[ch * t + x] = 1.0;
                }
            }
            ch += 1;
        }
        for k in 0..n {
            if let Some(seat) = self.seats.get(k) {
                let last = (0..seat.discards.len())
                    .rev()
                    .find(|&i| seat.tedashi.get(i).copied().unwrap_or(false));
                if let Some(x) = last.and_then(|i| idx(seat.discards[i])) {
                    buf[ch * t + x] = 1.0;
                }
            }
            ch += 1;
        }

        // --- per-player passed tiles ---
        for k in 0..n {
            if let Some(seat) = self.seats.get(k) {
                for &tid in &seat.passed {
                    if let Some(x) = idx(tid) {
                        buf[ch * t + x] = 1.0;
                    }
                }
            }
            ch += 1;
        }

        // --- per-player meld count (broadcast) ---
        for k in 0..n {
            let melds = self.seats.get(k).map_or(0, |s| s.meld_count);
            broadcast!(ch, (melds as f32 / 4.0).min(1.0));
            ch += 1;
        }

        // --- per-player placement (broadcast; 0 = top, 1 = last) ---
        // Ties go to the seat nearer the dealer, as on both platforms.
        let wind = |k: usize| (self.seat_wind as usize + k) % n;
        let score = |k: usize| self.seats.get(k).map_or(0, |s| s.score);
        for k in 0..n {
            let above = (0..n)
                .filter(|&j| {
                    j != k && (score(j) > score(k) || (score(j) == score(k) && wind(j) < wind(k)))
                })
                .count();
            broadcast!(ch, above as f32 / (n - 1) as f32);
            ch += 1;
        }

        // --- unseen copies ---
        let unseen = self.unseen34();
        for (t34, &left) in unseen.iter().enumerate() {
            if let Some(x) = tile_index((t34 * 4) as u8, np) {
                buf[ch * t + x] = left as f32 / 4.0;
            }
        }
        ch += 1;

        // --- self shanten thresholds ---
        let mut counts = [0u8; 34];
        for &tid in &self.hand {
            counts[tile34(tid)] += 1;
        }
        let shanten = shanten_of(&counts, np);
        for thresh in 0..=3i8 {
            if shanten <= thresh {
                broadcast!(ch, 1.0);
            }
            ch += 1;
        }

        // --- self furiten ---
        if self.self_furiten {
            broadcast!(ch, 1.0);
        }
        ch += 1;

        // --- discard candidates: keeps best shanten / ukeire after ---
        if self.hand.len() % 3 == 2 {
            let mut after = Vec::new();
            for t34 in 0..34 {
                if counts[t34] == 0 {
                    continue;
                }
                counts[t34] -= 1;
                let s = shanten_of(&counts, np);
                let ukeire = ukeire(&mut counts, s, &unseen, np);
                counts[t34] += 1;
                after.push((t34, s, ukeire));
            }
            let best = after.iter().map(|&(_, s, _)| s).min().unwrap_or(i8::MAX);
            for &(t34, s, ukeire) in &after {
                if let Some(x) = tile_index((t34 * 4) as u8, np) {
                    if s == best {
                        buf[ch * t + x] = 1.0;
                    }
                    buf[(ch + 1) * t + x] = (ukeire as f32 / 20.0).min(4.0);
                }
            }
        }
        ch += 2;

        debug_assert_eq!(
            ch * t,
            buf.len(),
            "V2 cursor must match the V2 channel count"
        );
    }

    /// Append the V3 planes to a V2 buffer.
    fn append_v3(&self, buf: &mut Vec<f32>) {
        let np = self.num_players;
        let t = tile_dim(np);
        debug_assert_eq!(buf.len(), Features::V2.channels(np) * t);
        // Normalised by the most rounds a game has left (hanchan East 1).
        let most = (GameLength::EastSouth.rounds(np) - 1) as f32;
        buf.extend(std::iter::repeat_n(
            (self.rounds_left as f32 / most).min(1.0),
            t,
        ));
        let last = if self.rounds_left == 0 { 1.0 } else { 0.0 };
        buf.extend(std::iter::repeat_n(last, t));
        debug_assert_eq!(buf.len(), Features::V3.channels(np) * t);
    }

    /// Copies of each tile34 the deciding player has not seen: not in the
    /// hand, any discard pond, any meld, the dora indicators or the kita.
    fn unseen34(&self) -> [u8; 34] {
        let mut seen = [0u8; 34];
        let mut see = |tid: u8| seen[tile34(tid)] += 1;
        self.hand.iter().for_each(|&x| see(x));
        self.dora_indicators.iter().for_each(|&x| see(x));
        for seat in &self.seats {
            seat.discards.iter().for_each(|&x| see(x));
            seat.meld_tiles.iter().for_each(|&x| see(x));
        }
        for seat in &self.seats {
            for &tid in &seat.called_tiles {
                seen[tile34(tid)] = seen[tile34(tid)].saturating_sub(1);
            }
            seen[NORTH34] += seat.kita_count;
        }
        std::array::from_fn(|i| {
            // Sanma has no 2m-8m.
            let exists = self.num_players == 4 || !(1..=7).contains(&i);
            if exists {
                4u8.saturating_sub(seen[i])
            } else {
                0
            }
        })
    }
}

/// Shanten of a concealed hand given as tile34 counts.
fn shanten_of(counts: &[u8; 34], num_players: u8) -> i8 {
    let len: u8 = counts.iter().sum();
    if num_players == 3 {
        calc_shanten_from_counts_3p(counts, len / 3)
    } else {
        calc_shanten_from_counts(counts, len / 3)
    }
}

/// Unseen copies of every draw that would lower `shanten` (the hand's current
/// shanten). `counts` is restored before returning.
fn ukeire(counts: &mut [u8; 34], shanten: i8, unseen: &[u8; 34], num_players: u8) -> u32 {
    let mut total = 0u32;
    for t34 in 0..34 {
        if unseen[t34] == 0 || counts[t34] >= 4 {
            continue;
        }
        counts[t34] += 1;
        if shanten_of(counts, num_players) < shanten {
            total += unseen[t34] as u32;
        }
        counts[t34] -= 1;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(np: u8) -> EncInput {
        let seats = (0..np)
            .map(|i| SeatFeat {
                discards: vec![0, 4, 4],
                meld_tiles: vec![],
                riichi_declared: i == 1,
                riichi_tile: if i == 1 { Some(36) } else { None },
                score: 25_000,
                kita_count: if np == 3 { i } else { 0 },
                ..Default::default()
            })
            .collect();
        EncInput {
            num_players: np,
            hand: vec![0, 1, 2, 16, 52, 88, 108, 108, 108, 120, 124, 128, 132],
            drawn_tile: Some(132),
            waits: vec![9, 18],
            is_tenpai: true,
            dora_indicators: vec![0, 108],
            seats,
            last_discard: Some(4),
            round_wind: 0,
            seat_wind: 1,
            honba: 2,
            riichi_sticks: 1,
            turn_count: 7,
            is_dealer: false,
            kyoku_index: 0,
            self_riichi: false,
            self_furiten: false,
            rounds_left: 5,
        }
    }

    #[test]
    fn encode_shape_4p() {
        let inp = sample(4);
        let buf = inp.encode();
        assert_eq!(buf.len(), channels(4) * 34);
    }

    #[test]
    fn encode_shape_3p() {
        let inp = sample(3);
        let buf = inp.encode();
        assert_eq!(buf.len(), channels(3) * 27);
    }

    #[test]
    fn self_relative_hand_present() {
        // Hand contains 1m (tile34 0); the >=1 plane (channel 0) must mark it.
        let inp = sample(4);
        let buf = inp.encode();
        // Channel 0 (hand >= 1), tile34 0 (1m).
        assert_eq!(buf[0], 1.0, "1m present in >=1 hand plane");
        // aka plane is channel 4: 5m/5p/5s red present -> positions 4,13,22
        let aka_ch = 4;
        assert_eq!(buf[aka_ch * 34 + 4], 1.0);
        assert_eq!(buf[aka_ch * 34 + 13], 1.0);
        assert_eq!(buf[aka_ch * 34 + 22], 1.0);
    }

    #[test]
    fn v2_appends_to_v1_in_both_modes() {
        for np in [3u8, 4] {
            let inp = sample(np);
            let v1 = inp.encode();
            let v2 = inp.encode_as(Features::V2);
            assert_eq!(v2.len(), Features::V2.channels(np) * tile_dim(np));
            assert_eq!(
                &v2[..v1.len()],
                &v1[..],
                "V2 keeps the V1 planes as they are"
            );
        }
        assert_eq!(Features::V2.channels(3), 66);
        assert_eq!(Features::V2.channels(4), 75);
    }

    #[test]
    fn v3_appends_rounds_left_and_last_round() {
        for np in [3u8, 4] {
            let mut inp = sample(np);
            let t = tile_dim(np);
            let v2 = inp.encode_as(Features::V2);
            let v3 = inp.encode_as(Features::V3);
            assert_eq!(v3.len(), Features::V3.channels(np) * t);
            assert_eq!(
                &v3[..v2.len()],
                &v2[..],
                "V3 keeps the V2 planes as they are"
            );
            let plane =
                |buf: &[f32], k: usize| buf[v2.len() + k * t..v2.len() + (k + 1) * t].to_vec();
            inp.rounds_left = 0;
            let last = inp.encode_as(Features::V3);
            assert!(plane(&last, 0).iter().all(|v| *v == 0.0));
            assert!(plane(&last, 1).iter().all(|v| *v == 1.0), "the last round");
            inp.rounds_left = GameLength::EastSouth.rounds(np) - 1;
            let first = inp.encode_as(Features::V3);
            assert!(plane(&first, 0).iter().all(|v| *v == 1.0), "hanchan East 1");
            assert!(plane(&first, 1).iter().all(|v| *v == 0.0));
        }
        assert_eq!(Features::V3.channels(3), 68);
    }

    /// East 3 is the last round of a sanma east-only game, the third of six in
    /// a hanchan — which South 3 then ends, as East 3 would an east game.
    #[test]
    fn rounds_left_follows_the_game_length() {
        assert_eq!(GameLength::East.rounds_left(3, 0, 2), 0);
        assert_eq!(GameLength::EastSouth.rounds_left(3, 0, 2), 3);
        assert_eq!(GameLength::EastSouth.rounds_left(3, 1, 2), 0);
        assert_eq!(GameLength::East.rounds_left(3, 0, 0), 2);
        assert_eq!(GameLength::EastSouth.rounds_left(4, 0, 0), 7);
        // An extension round past the planned end is the last round too.
        assert_eq!(GameLength::East.rounds_left(3, 1, 0), 0);
        assert_eq!(GameLength::EastSouth.rounds_left(4, 2, 1), 0);
    }

    #[test]
    fn tenhou_room_tag_names_the_length() {
        let of = GameLength::of_tenhou_log;
        assert_eq!(
            of("2026010100gm-00b9-0000-0363f52c.json.gz"),
            Some(GameLength::EastSouth)
        );
        assert_eq!(
            of("2026010100gm-00b1-0000-0363f52c.json.gz"),
            Some(GameLength::East)
        );
        assert_eq!(
            of("2026010100gm-00f1-0000-0363f52c.json.gz"),
            Some(GameLength::East)
        );
        assert_eq!(of("game.json.gz"), None);
    }

    #[test]
    fn input_width_names_the_feature_set() {
        for np in [3u8, 4] {
            for f in [Features::V1, Features::V2, Features::V3] {
                assert_eq!(Features::of_channels(np, f.channels(np)), Some(f));
            }
        }
        assert_eq!(Features::of_channels(3, 39), None);
    }

    #[test]
    fn discard_planes_find_the_tenpai_discard_and_its_ukeire() {
        // Sanma: 1m1m 123p 456p 789p 12s + N. Only discarding N keeps the
        // best shanten (tenpai on 3s); its ukeire is the four unseen 3s.
        let mut inp = sample(3);
        inp.hand = vec![0, 1, 36, 40, 44, 48, 52, 56, 60, 64, 68, 72, 76, 120];
        inp.seats.iter_mut().for_each(|s| {
            s.discards.clear();
            s.meld_tiles.clear();
            s.kita_count = 0;
        });
        inp.dora_indicators.clear();
        let buf = inp.encode_as(Features::V2);
        let t = tile_dim(3);
        let total = Features::V2.channels(3);
        let (best, uke) = (total - 2, total - 1);
        let north = tile_index(120, 3).unwrap();
        let marked: Vec<usize> = (0..t).filter(|&x| buf[best * t + x] == 1.0).collect();
        assert_eq!(marked, vec![north]);
        assert!((buf[uke * t + north] - 4.0 / 20.0).abs() < 1e-6);
        // Shanten after the draw is 0 (tenpai once N goes): every threshold set.
        let shanten0 = total - 7;
        assert_eq!(buf[shanten0 * t], 1.0);
    }

    #[test]
    fn called_tiles_are_seen_once() {
        // A 5p discarded by seat 1 and ponned by seat 2 is in both lists.
        let mut inp = sample(3);
        inp.hand.clear();
        inp.dora_indicators.clear();
        for s in &mut inp.seats {
            s.discards.clear();
            s.meld_tiles.clear();
            s.kita_count = 0;
        }
        inp.seats[1].discards = vec![52];
        inp.seats[2].meld_tiles = vec![52, 53, 54];
        inp.seats[2].called_tiles = vec![52];
        assert_eq!(inp.unseen34()[13], 1, "three 5p seen, one left");
        assert_eq!(inp.unseen34()[4], 0, "5m does not exist in sanma");
    }

    #[test]
    fn channels_match_encode_cursor() {
        // The debug_assert in encode() guards drift; run it for both modes.
        let _ = sample(4).encode();
        let _ = sample(3).encode();
        let _ = sample(4).encode_as(Features::V2);
        let _ = sample(3).encode_as(Features::V2);
        let _ = sample(4).encode_as(Features::V3);
        let _ = sample(3).encode_as(Features::V3);
    }
}
