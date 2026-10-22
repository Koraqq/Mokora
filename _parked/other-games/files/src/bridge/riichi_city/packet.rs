//! Riichi City WebSocket packet (WPacket) framing.
//!
//! Riichi City frames its WebSocket messages with a fixed 15-byte header
//! followed by an optional UTF-8 JSON body. All multi-byte header fields are
//! **big-endian** on the wire (the C struct in the riichishitty reverse-
//! engineering notes documents field layout only — the working Mokora v2 bridge
//! and the captured `CMDAuth` example both decode big-endian):
//!
//! ```text
//! [0..4]  packet_size    u32  total bytes incl. header
//! [4..6]  header_size    u16  always 15
//! [6..8]  version        u16  always 1
//! [8..12] message_index  u32  request/response correlation counter
//! [12..14] cmd           u16  binary command enum (e.g. CMDAuth = 1)
//! [14]    has_body       u8   0 or 1
//! [15..]  json_payload   UTF-8 JSON (present when packet_size > 15)
//! ```
//!
//! Validation mirrors the v2 bridge: bytes `[4..8]` must equal
//! `00 0f 00 01` (header_size = 15, version = 1). One WebSocket message
//! normally carries exactly one WPacket; [`parse_frame`] additionally tolerates
//! several concatenated packets in a single frame (a safe superset of v2, which
//! handled only one).

use serde_json::Value as JsonValue;

/// Binary command: the client's WebSocket auth handshake. Carries our `uid`.
pub const CMD_AUTH: u16 = 1;

/// The fixed `header_size (15) || version (1)` magic, big-endian.
const MAGIC: [u8; 4] = [0x00, 0x0f, 0x00, 0x01];

/// Minimum packet length: the 15-byte header with an empty body.
const HEADER_LEN: usize = 15;

/// One decoded Riichi City packet.
#[derive(Debug, Clone)]
pub struct WPacket {
    /// Sequence/correlation counter echoed between request and response.
    pub message_index: u32,
    /// Binary command enum value.
    pub cmd: u16,
    /// JSON body. An empty body decodes to `Value::Null`.
    pub body: JsonValue,
}

impl WPacket {
    /// Inspector-facing label: the JSON `"cmd"` discriminator when present
    /// (gameplay messages), else a synthetic name from the binary command.
    pub fn method_label(&self) -> String {
        if let Some(cmd) = self.body.get("cmd").and_then(JsonValue::as_str) {
            cmd.to_string()
        } else if self.cmd == CMD_AUTH {
            "auth".to_string()
        } else {
            format!("cmd#{}", self.cmd)
        }
    }

    /// Decode a single WPacket from the front of `buf`.
    ///
    /// Returns the packet plus the number of bytes it consumed
    /// (`packet_size`), or `None` if the buffer is too short, the magic bytes
    /// are wrong, or the body is not valid JSON.
    pub fn parse_one(buf: &[u8]) -> Option<(WPacket, usize)> {
        if buf.len() < HEADER_LEN {
            return None;
        }
        let packet_size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        if packet_size < HEADER_LEN || packet_size > buf.len() {
            // Incomplete or nonsensical length — caller logs / drops.
            return None;
        }
        if buf[4..8] != MAGIC {
            return None;
        }
        let message_index = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let cmd = u16::from_be_bytes([buf[12], buf[13]]);
        // buf[14] is has_body (0/1); we infer presence from packet_size instead,
        // which is robust to clients that always set the flag.
        let body = if packet_size > HEADER_LEN {
            serde_json::from_slice(&buf[HEADER_LEN..packet_size]).ok()?
        } else {
            JsonValue::Null
        };
        Some((
            WPacket {
                message_index,
                cmd,
                body,
            },
            packet_size,
        ))
    }

    /// Serialize into a full wire frame: 15-byte big-endian header + JSON
    /// body. The inverse of [`WPacket::parse_one`] for frames we build
    /// ourselves (autoplay injection).
    pub fn encode(message_index: u32, cmd: u16, body: &serde_json::Value) -> Vec<u8> {
        let json = serde_json::to_vec(body).unwrap_or_default();
        let packet_size = (HEADER_LEN + json.len()) as u32;
        let mut v = Vec::with_capacity(packet_size as usize);
        v.extend_from_slice(&packet_size.to_be_bytes());
        v.extend_from_slice(&MAGIC);
        v.extend_from_slice(&message_index.to_be_bytes());
        v.extend_from_slice(&cmd.to_be_bytes());
        v.push(if json.is_empty() { 0 } else { 1 });
        v.extend_from_slice(&json);
        v
    }

    /// [`WPacket::encode`] with the next process-wide message index and the
    /// given binary command. The message index is a placeholder: the inject
    /// bus re-stamps it past the client's live counter before transmission
    /// (see `autoplay::inject`).
    pub fn encode_request(bin_cmd: u16, body: &serde_json::Value) -> Vec<u8> {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let idx = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        WPacket::encode(idx, bin_cmd, body)
    }

    /// Decode every WPacket in a WebSocket frame. Stops at the first packet
    /// that fails to decode (returning whatever decoded cleanly before it).
    pub fn parse_frame(buf: &[u8]) -> Vec<WPacket> {
        let mut out = Vec::new();
        let mut off = 0;
        while off < buf.len() {
            match WPacket::parse_one(&buf[off..]) {
                Some((pkt, consumed)) if consumed > 0 => {
                    out.push(pkt);
                    off += consumed;
                }
                _ => break,
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a WPacket on the wire: 15-byte big-endian header + JSON body.
    fn frame(message_index: u32, cmd: u16, json: &[u8]) -> Vec<u8> {
        let packet_size = (HEADER_LEN + json.len()) as u32;
        let mut v = Vec::new();
        v.extend_from_slice(&packet_size.to_be_bytes());
        v.extend_from_slice(&MAGIC);
        v.extend_from_slice(&message_index.to_be_bytes());
        v.extend_from_slice(&cmd.to_be_bytes());
        v.push(if json.is_empty() { 0 } else { 1 });
        v.extend_from_slice(json);
        v
    }

    #[test]
    fn decodes_auth_handshake() {
        // Mirrors the riichishitty README CMDAuth example (fake uid).
        let json = br#"{"platform":"pc","uid":"123456789","lang":"en","sid":"deadbeef"}"#;
        let buf = frame(14, CMD_AUTH, json);
        let (pkt, consumed) = WPacket::parse_one(&buf).expect("auth decodes");
        assert_eq!(consumed, buf.len());
        assert_eq!(pkt.message_index, 14);
        assert_eq!(pkt.cmd, CMD_AUTH);
        assert_eq!(pkt.body["uid"], "123456789");
        assert_eq!(pkt.method_label(), "auth");
    }

    #[test]
    fn empty_body_is_null() {
        let buf = frame(1, 0, b"");
        let (pkt, _) = WPacket::parse_one(&buf).expect("empty body decodes");
        assert!(pkt.body.is_null());
    }

    #[test]
    fn gameplay_cmd_label_comes_from_json() {
        let json = br#"{"cmd":"cmd_room_end","data":{}}"#;
        let buf = frame(7, 18, json);
        let (pkt, _) = WPacket::parse_one(&buf).unwrap();
        assert_eq!(pkt.method_label(), "cmd_room_end");
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = frame(1, CMD_AUTH, b"{}");
        buf[5] = 0xFF; // corrupt header_size
        assert!(WPacket::parse_one(&buf).is_none());
    }

    #[test]
    fn rejects_incomplete() {
        let buf = frame(1, CMD_AUTH, b"{\"uid\":\"1\"}");
        // Truncate so packet_size > available bytes.
        assert!(WPacket::parse_one(&buf[..buf.len() - 3]).is_none());
    }

    #[test]
    fn parses_concatenated_packets() {
        let mut buf = frame(1, 0, br#"{"cmd":"a"}"#);
        buf.extend(frame(2, 0, br#"{"cmd":"b"}"#));
        let pkts = WPacket::parse_frame(&buf);
        assert_eq!(pkts.len(), 2);
        assert_eq!(pkts[0].method_label(), "a");
        assert_eq!(pkts[1].method_label(), "b");
    }
}
