//! Table-wide discard order, and the per-discard facts the game state does not
//! keep the same way on every path.
//!
//! Each seat's `discards` list is in order, but nothing says how the seats'
//! discards interleave — and the [`Features::V2`](crate::obs::Features) "passed
//! tiles" planes need exactly that: a tile discarded by anyone *after* a
//! player's riichi (or after their own last discard) is one that player can no
//! longer ron.
//!
//! riichienv-core also records tedashi and the riichi-declaring discard only
//! when the engine itself steps the game (the `arena`); a state driven by mjai
//! events (the extractor and live play) leaves both empty. The timeline holds
//! them for every path instead: from the events' `tsumogiri` / `reach`
//! ([`DiscardTimeline::note_mjai`]) where there are events, from the state
//! where it has them.
//!
//! [`DiscardTimeline::sync`] numbers each new discard as it appears. Callers
//! sync after **every** state mutation (each applied mjai event, each engine
//! step), so at most one seat has grown since the last sync and the numbering
//! is the real table order. A seat's list only grows within a round and is
//! cleared when the next one starts; any list shorter than last seen therefore
//! means a new round, and the timeline starts over.

use riichienv_core::replay::MjaiEvent;

/// What the timeline knows about one discard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiscardFlags {
    /// Cut from the hand rather than straight off the draw.
    pub tedashi: bool,
    /// The discard that declared riichi.
    pub riichi: bool,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    seq: u32,
    flags: DiscardFlags,
}

/// The table-wide order and flags of every discard of the round, per seat.
#[derive(Debug, Clone, Default)]
pub struct DiscardTimeline {
    entries: Vec<Vec<Entry>>,
    next: u32,
    /// From the events: the flags of each seat's coming discard.
    pending_tedashi: Vec<Option<bool>>,
    pending_riichi: Vec<bool>,
}

impl DiscardTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note what an mjai event says about the coming discard. Call before the
    /// event is applied (it is consumed by `apply_mjai_event`).
    pub fn note_mjai(&mut self, ev: &MjaiEvent) {
        match ev {
            MjaiEvent::Dahai {
                actor, tsumogiri, ..
            } => {
                grow(&mut self.pending_tedashi, *actor + 1);
                self.pending_tedashi[*actor] = Some(!*tsumogiri);
            }
            MjaiEvent::Reach { actor } => {
                grow(&mut self.pending_riichi, *actor + 1);
                self.pending_riichi[*actor] = true;
            }
            _ => {}
        }
    }

    /// Number the discards that appeared since the last sync. `lens[seat]` is
    /// the length of that seat's discard list now; `recorded(seat, index)` the
    /// flags the state itself holds for a discard, if it holds them.
    pub fn sync(
        &mut self,
        lens: &[usize],
        recorded: impl Fn(usize, usize) -> Option<DiscardFlags>,
    ) {
        if self.entries.len() != lens.len()
            || self.entries.iter().zip(lens).any(|(s, &n)| n < s.len())
        {
            self.entries = vec![Vec::new(); lens.len()];
            self.next = 0;
        }
        grow(&mut self.pending_tedashi, lens.len());
        grow(&mut self.pending_riichi, lens.len());
        for (seat, &n) in lens.iter().enumerate() {
            while self.entries[seat].len() < n {
                let index = self.entries[seat].len();
                let tedashi = self.pending_tedashi[seat].take();
                let riichi = std::mem::take(&mut self.pending_riichi[seat]);
                let flags = recorded(seat, index).unwrap_or(DiscardFlags {
                    tedashi: tedashi.unwrap_or(false),
                    riichi,
                });
                self.entries[seat].push(Entry {
                    seq: self.next,
                    flags,
                });
                self.next += 1;
            }
        }
    }

    /// Sequence number of `seat`'s `index`-th discard, if known.
    pub fn seq(&self, seat: usize, index: usize) -> Option<u32> {
        Some(self.entries.get(seat)?.get(index)?.seq)
    }

    /// Flags of `seat`'s `index`-th discard (all false if unknown).
    pub fn flags(&self, seat: usize, index: usize) -> DiscardFlags {
        self.entries
            .get(seat)
            .and_then(|s| s.get(index))
            .map(|e| e.flags)
            .unwrap_or_default()
    }

    /// For each seat: the tiles its opponents discarded after `seat`'s riichi
    /// declaration (if it declared) or else after its own latest discard (all
    /// of the round's when it has none yet) — tiles `seat` can no longer ron.
    ///
    /// `discards[seat]` are raw tile ids in order. Seats are absolute.
    pub fn passed_tiles(&self, discards: &[&[u8]]) -> Vec<Vec<u8>> {
        (0..discards.len())
            .map(|seat| {
                let riichi = (0..discards[seat].len()).find(|&i| self.flags(seat, i).riichi);
                let anchor = match riichi {
                    Some(i) => self.seq(seat, i),
                    None => discards[seat]
                        .len()
                        .checked_sub(1)
                        .and_then(|i| self.seq(seat, i)),
                };
                let mut out = Vec::new();
                for (other, tiles) in discards.iter().enumerate() {
                    if other == seat {
                        continue;
                    }
                    for (i, &tile) in tiles.iter().enumerate() {
                        let after = match (anchor, self.seq(other, i)) {
                            (None, _) => true,
                            (Some(a), Some(s)) => s > a,
                            // Unnumbered (never synced): order unknown, leave out.
                            (Some(_), None) => false,
                        };
                        if after {
                            out.push(tile);
                        }
                    }
                }
                out
            })
            .collect()
    }
}

fn grow<T: Default + Clone>(v: &mut Vec<T>, len: usize) {
    if v.len() < len {
        v.resize(len, T::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: usize, _: usize) -> Option<DiscardFlags> {
        None
    }

    #[test]
    fn numbers_discards_in_table_order() {
        let mut t = DiscardTimeline::new();
        for lens in [[0, 0, 0], [1, 0, 0], [1, 1, 0], [1, 1, 1], [2, 1, 1]] {
            t.sync(&lens, none);
        }
        assert_eq!(t.seq(0, 0), Some(0));
        assert_eq!(t.seq(1, 0), Some(1));
        assert_eq!(t.seq(2, 0), Some(2));
        assert_eq!(t.seq(0, 1), Some(3));
    }

    #[test]
    fn a_shorter_list_starts_a_new_round() {
        let mut t = DiscardTimeline::new();
        t.sync(&[2, 2, 1], none);
        t.sync(&[0, 0, 0], none);
        t.sync(&[0, 1, 0], none);
        assert_eq!(t.seq(1, 0), Some(0));
        assert_eq!(t.seq(0, 0), None);
    }

    #[test]
    fn flags_come_from_the_events_or_the_state() {
        let mut t = DiscardTimeline::new();
        t.note_mjai(&MjaiEvent::Reach { actor: 1 });
        t.note_mjai(&MjaiEvent::Dahai {
            actor: 1,
            pai: "5p".into(),
            tsumogiri: false,
        });
        t.sync(&[0, 1], none);
        assert_eq!(
            t.flags(1, 0),
            DiscardFlags {
                tedashi: true,
                riichi: true
            }
        );
        // The state's own record wins where it has one.
        t.sync(&[1, 1], |seat, _| {
            (seat == 0).then_some(DiscardFlags::default())
        });
        assert_eq!(t.flags(0, 0), DiscardFlags::default());
        // A riichi is consumed by the discard it declared on.
        t.note_mjai(&MjaiEvent::Dahai {
            actor: 1,
            pai: "6p".into(),
            tsumogiri: true,
        });
        t.sync(&[1, 2], none);
        assert_eq!(t.flags(1, 1), DiscardFlags::default());
    }

    #[test]
    fn passed_tiles_follow_riichi_or_the_last_discard() {
        // Order: s0 a, s1 b, s2 c, s0 d (riichi), s1 e, s2 f.
        let mut t = DiscardTimeline::new();
        for lens in [[1, 0, 0], [1, 1, 0], [1, 1, 1]] {
            t.sync(&lens, none);
        }
        t.note_mjai(&MjaiEvent::Reach { actor: 0 });
        for lens in [[2, 1, 1], [2, 2, 1], [2, 2, 2]] {
            t.sync(&lens, none);
        }
        let d0: &[u8] = &[10, 13];
        let d1: &[u8] = &[11, 14];
        let d2: &[u8] = &[12, 15];
        let passed = t.passed_tiles(&[d0, d1, d2]);
        // Seat 0 declared on its 2nd discard: only e and f came after.
        assert_eq!(passed[0], vec![14, 15]);
        // Seat 1's last discard was e: only f came after.
        assert_eq!(passed[1], vec![15]);
        // Seat 2 discarded last: nothing has passed since.
        assert!(passed[2].is_empty());
    }

    #[test]
    fn before_a_first_discard_everything_has_passed() {
        let mut t = DiscardTimeline::new();
        t.sync(&[1, 0, 0], none);
        let d0: &[u8] = &[10];
        let passed = t.passed_tiles(&[d0, &[], &[]]);
        assert_eq!(passed[1], vec![10]);
        assert_eq!(passed[2], vec![10]);
    }
}
