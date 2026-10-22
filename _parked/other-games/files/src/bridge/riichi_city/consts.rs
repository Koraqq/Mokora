//! Riichi City tile encoding → mjai tile string conversion.
//!
//! Riichi City encodes each tile as a hex byte `0x[SUIT][RANK]`:
//! - pinzu `0x01..=0x09` → `1p..9p`
//! - souzu `0x11..=0x19` → `1s..9s`
//! - manzu `0x21..=0x29` → `1m..9m`
//! - honors `0x31/0x41/0x51/0x61/0x71/0x81/0x91` → `E/S/W/N/P/F/C`
//! - red fives `0x105/0x115/0x125` → `5pr/5sr/5mr`
//! - `0x00` (hidden / unknown) → `?`
//!
//! Direct port of the original Mokora v2 `CARD2MJAI` table
//! (`mitm/bridge/riichi_city/consts.py`). Unmapped codes fall back to `?`
//! so malformed traffic cannot panic the bridge.

/// Convert a Riichi City tile code to its mjai string (owned).
pub fn card_to_mjai(code: u32) -> String {
    card_to_mjai_str(code).to_string()
}

/// Borrowed form of [`card_to_mjai`].
pub fn card_to_mjai_str(code: u32) -> &'static str {
    match code {
        0x00 => "?",
        // pinzu
        0x01 => "1p",
        0x02 => "2p",
        0x03 => "3p",
        0x04 => "4p",
        0x05 => "5p",
        0x06 => "6p",
        0x07 => "7p",
        0x08 => "8p",
        0x09 => "9p",
        // souzu
        0x11 => "1s",
        0x12 => "2s",
        0x13 => "3s",
        0x14 => "4s",
        0x15 => "5s",
        0x16 => "6s",
        0x17 => "7s",
        0x18 => "8s",
        0x19 => "9s",
        // manzu
        0x21 => "1m",
        0x22 => "2m",
        0x23 => "3m",
        0x24 => "4m",
        0x25 => "5m",
        0x26 => "6m",
        0x27 => "7m",
        0x28 => "8m",
        0x29 => "9m",
        // honors
        0x31 => "E",
        0x41 => "S",
        0x51 => "W",
        0x61 => "N",
        0x71 => "P",
        0x81 => "F",
        0x91 => "C",
        // red fives
        0x105 => "5pr",
        0x115 => "5sr",
        0x125 => "5mr",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suits_map_in_riichi_city_order() {
        // Pinzu is the low nibble-0 block, souzu nibble-1, manzu nibble-2.
        assert_eq!(card_to_mjai(0x01), "1p");
        assert_eq!(card_to_mjai(0x09), "9p");
        assert_eq!(card_to_mjai(0x11), "1s");
        assert_eq!(card_to_mjai(0x19), "9s");
        assert_eq!(card_to_mjai(0x21), "1m");
        assert_eq!(card_to_mjai(0x29), "9m");
    }

    #[test]
    fn honors() {
        assert_eq!(card_to_mjai(0x31), "E");
        assert_eq!(card_to_mjai(0x41), "S");
        assert_eq!(card_to_mjai(0x51), "W");
        assert_eq!(card_to_mjai(0x61), "N");
        assert_eq!(card_to_mjai(0x71), "P");
        assert_eq!(card_to_mjai(0x81), "F");
        assert_eq!(card_to_mjai(0x91), "C");
    }

    #[test]
    fn red_fives() {
        assert_eq!(card_to_mjai(0x105), "5pr");
        assert_eq!(card_to_mjai(0x115), "5sr");
        assert_eq!(card_to_mjai(0x125), "5mr");
        // Plain fives stay plain.
        assert_eq!(card_to_mjai(0x05), "5p");
        assert_eq!(card_to_mjai(0x15), "5s");
        assert_eq!(card_to_mjai(0x25), "5m");
    }

    #[test]
    fn hidden_and_unknown() {
        assert_eq!(card_to_mjai(0x00), "?");
        assert_eq!(card_to_mjai(0xFFFF), "?");
    }
}
