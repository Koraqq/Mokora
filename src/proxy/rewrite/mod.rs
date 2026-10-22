//! Outbound request rewriting.
//!
//! The mirror image of [`crate::inspector::annotate`]: annotators *read*
//! captured traffic, rewriters *change* it on the way past. Both are
//! per-vendor knowledge kept out of the generic capture path.
//!
//! Rewriting is a much bigger hammer than annotating and the bar is
//! correspondingly higher. Mokora's default posture is to forward traffic
//! byte-for-byte; anything here is a deliberate exception with a reason
//! that belongs in the module that implements it.
//!
//! ## Current rewriters
//!
//! - [`majsoul_cert`] — replaces the certificate report a Mahjong Soul
//!   standalone client sends about its gateway connections. Read that
//!   module before touching this one; the *why* lives there.
//!
//! ## Adding one
//!
//! Keep each rewriter in its own module, gated on the platform it applies
//! to, and give it a config switch. A rewriter should be able to decline:
//! if it cannot produce a correct result it must leave the request alone
//! rather than emit a half-changed one, which is usually more conspicuous
//! than the thing it was trying to fix.

pub mod majsoul_cert;
