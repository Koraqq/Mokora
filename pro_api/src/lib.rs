//! What Mokora and its paid module say to each other.
//!
//! The paid features (Yakuman's planner, for now) are not in Mokora's source:
//! they are a separate library, MokoraPro, that Mokora downloads once a key is
//! redeemed (see `src/pro/` in Mokora). The two meet only here:
//!
//! - three C functions the module exports ([`export_module!`] writes them):
//!   its [`ABI_VERSION`], a call (a method name and JSON in, JSON out), and
//!   freeing what a call returned;
//! - for each call, a [`Host`]: bytes Mokora lends for the call (a model's
//!   weights) and a way back into Mokora — the module calls Mokora's methods
//!   the same way (a name and JSON), e.g. to ask whether to stop;
//! - the data of each call, as plain JSON-able types.
//!
//! A module of another `ABI_VERSION` is not loaded; one is downloaded for this
//! Mokora instead.

use serde::{Deserialize, Serialize};

/// Changed whenever a call or its data changes in a way the other side would
/// misread. Mokora only loads a module that says the same number.
pub const ABI_VERSION: u32 = 2;

/// The methods a module answers.
pub mod method {
    /// [`crate::yakuman::PlanRequest`] → [`crate::yakuman::PlanResponse`].
    pub const YAKUMAN_RANK: &str = "yakuman.rank";
    /// [`crate::review::ReviewRequest`] (the model's weights lent as the
    /// call's bytes) → [`crate::review::ReviewRun`]. Asks the host
    /// [`crate::host::REVIEW_TICK`] between events.
    pub const REVIEW_RUN: &str = "review.run";
    /// `()` → [`crate::lobby::Layout`]: where full auto presses, and what the
    /// screen must show.
    pub const LOBBY_LAYOUT: &str = "lobby.layout";
}

/// The methods Mokora answers a module during a call.
pub mod host {
    /// `()` → `bool`: stop the review now (cancelled). Waits while it should
    /// pause, and paces the review.
    pub const REVIEW_TICK: &str = "review.tick";
}

/// What a call answers: its value, or why it failed.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply<T> {
    Ok(T),
    Err(String),
}

pub mod lobby;
pub mod review;

/// Yakuman: Mora's choices re-ordered towards a yakuman.
pub mod yakuman {
    use super::*;

    /// Our side of the table, as tile34 counts (34 entries).
    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    pub struct Hand {
        /// The concealed tiles, by tile34.
        pub counts: Vec<u8>,
        /// Triplet and kan melds: their tile34 and whether they are open (an
        /// ankan is not).
        pub melds: Vec<(u8, bool)>,
        /// Chi melds (yonma only), by their lowest tile34. Always open.
        pub runs: Vec<u8>,
    }

    /// What the rest of the table leaves us.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Table {
        /// Players at the table (3 or 4): which tiles exist at all.
        pub players: u8,
        /// Copies of each tile34 we have not seen (34 entries).
        pub unseen: Vec<u8>,
        /// Tiles left to draw in the live wall.
        pub wall_left: u32,
        /// How many of those we draw ourselves (rounded up).
        pub own_draws: u32,
    }

    /// What a choice does.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Kind {
        Discard,
        Kita,
        Ankan,
        Kakan,
        Pass,
        Pon,
        Chi,
        Daiminkan,
        Tsumo,
        Ron,
        Riichi,
        /// Anything else (ryuukyoku…): never the planner's.
        Other,
    }

    /// One legal choice, in the order the model ranked them.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Choice {
        pub kind: Kind,
        /// The tile it names (tile136), if any.
        pub tile: Option<u8>,
        /// The tiles it takes from the hand (tile136).
        pub consume: Vec<u8>,
        /// A win (tsumo or ron) that scores a yakuman, as Mokora scored it.
        pub yakuman_win: bool,
    }

    /// Order `choices` (every legal choice, best first by the model).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct PlanRequest {
        pub hand: Hand,
        pub table: Table,
        /// Our hand is declared riichi (or declaring): the model's business.
        pub riichi: bool,
        pub choices: Vec<Choice>,
    }

    /// The planner's order as `(index into choices, probability)`, best
    /// first; `None` when no yakuman is reachable and the model decides.
    pub type PlanResponse = Option<Vec<(usize, f32)>>;
}

/// What Mokora hands the module with a call: bytes lent for the call, and
/// its own methods to call back. `call` / `free` are null when it offers
/// none. Valid only during the call.
#[repr(C)]
pub struct Host {
    pub ctx: *mut std::ffi::c_void,
    pub call: Option<HostCallFn>,
    pub free: Option<HostFreeFn>,
    pub blob: *const u8,
    pub blob_len: usize,
}

/// Mokora's side of a call back: a method and JSON in, a [`Reply`] as JSON
/// out (freed with [`HostFreeFn`]).
pub type HostCallFn = unsafe extern "C" fn(
    ctx: *mut std::ffi::c_void,
    method: *const u8,
    method_len: usize,
    input: *const u8,
    input_len: usize,
    out_len: *mut usize,
) -> *mut u8;
pub type HostFreeFn = unsafe extern "C" fn(ctx: *mut std::ffi::c_void, ptr: *mut u8, len: usize);

/// The module's view of [`Host`] during a call.
pub struct HostRef<'a> {
    host: &'a Host,
}

impl<'a> HostRef<'a> {
    /// # Safety
    /// `host` is the one Mokora passed to this call, still in it.
    pub unsafe fn new(host: &'a Host) -> Self {
        Self { host }
    }

    /// The bytes lent for this call (empty when none).
    pub fn blob(&self) -> &'a [u8] {
        if self.host.blob.is_null() || self.host.blob_len == 0 {
            return &[];
        }
        // SAFETY: Mokora keeps them alive and unchanged for the call.
        unsafe { std::slice::from_raw_parts(self.host.blob, self.host.blob_len) }
    }

    /// Call Mokora's `method` with `req`.
    pub fn call<Req: Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        req: &Req,
    ) -> Result<Resp, String> {
        let (Some(call), Some(free)) = (self.host.call, self.host.free) else {
            return Err(format!("{method}: Mokora offers no calls here"));
        };
        let input = serde_json::to_vec(req).map_err(|e| e.to_string())?;
        let mut len = 0usize;
        // SAFETY: as `Host` says; the answer is read once and handed back.
        let bytes = unsafe {
            let ptr = call(self.host.ctx, method.as_ptr(), method.len(), input.as_ptr(), input.len(), &mut len);
            if ptr.is_null() {
                return Err(format!("{method}: no answer"));
            }
            let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
            free(self.host.ctx, ptr, len);
            bytes
        };
        match serde_json::from_slice::<Reply<Resp>>(&bytes).map_err(|e| e.to_string())? {
            Reply::Ok(v) => Ok(v),
            Reply::Err(e) => Err(e),
        }
    }
}

/// The module's side of the three functions, around `dispatch(method, input
/// JSON, host) -> Result<output JSON, error>`. A panic inside is caught and
/// answered as an error, never let across into Mokora.
#[macro_export]
macro_rules! export_module {
    ($dispatch:path) => {
        #[no_mangle]
        pub extern "C" fn mokora_pro_abi() -> u32 {
            $crate::ABI_VERSION
        }

        /// # Safety
        /// `method` and `input` point at `*_len` readable bytes; `host` at a
        /// [`Host`] valid for the call; `out_len` is writable. The returned
        /// buffer goes back through `mokora_pro_free`.
        #[no_mangle]
        pub unsafe extern "C" fn mokora_pro_call(
            method: *const u8,
            method_len: usize,
            input: *const u8,
            input_len: usize,
            host: *const $crate::Host,
            out_len: *mut usize,
        ) -> *mut u8 {
            let method = std::slice::from_raw_parts(method, method_len);
            let input = std::slice::from_raw_parts(input, input_len);
            let reply = std::panic::catch_unwind(|| {
                let method = std::str::from_utf8(method).map_err(|e| e.to_string())?;
                let none = $crate::Host {
                    ctx: std::ptr::null_mut(),
                    call: None,
                    free: None,
                    blob: std::ptr::null(),
                    blob_len: 0,
                };
                let host = $crate::HostRef::new(if host.is_null() { &none } else { &*host });
                $dispatch(method, input, &host)
            });
            let reply: $crate::Reply<serde_json::Value> = match reply {
                Ok(Ok(bytes)) => match serde_json::from_slice(&bytes) {
                    Ok(v) => $crate::Reply::Ok(v),
                    Err(e) => $crate::Reply::Err(e.to_string()),
                },
                Ok(Err(e)) => $crate::Reply::Err(e),
                Err(_) => $crate::Reply::Err("the module panicked".into()),
            };
            let bytes = serde_json::to_vec(&reply)
                .unwrap_or_else(|_| br#"{"err":"unserialisable reply"}"#.to_vec())
                .into_boxed_slice();
            *out_len = bytes.len();
            Box::into_raw(bytes) as *mut u8
        }

        /// # Safety
        /// `ptr`/`len` are exactly what one `mokora_pro_call` returned.
        #[no_mangle]
        pub unsafe extern "C" fn mokora_pro_free(ptr: *mut u8, len: usize) {
            if !ptr.is_null() {
                drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
            }
        }
    };
}

/// The names the module's functions are exported under.
pub mod symbols {
    pub const ABI: &[u8] = b"mokora_pro_abi\0";
    pub const CALL: &[u8] = b"mokora_pro_call\0";
    pub const FREE: &[u8] = b"mokora_pro_free\0";
}

/// The signatures of those functions, for the loader.
pub type AbiFn = unsafe extern "C" fn() -> u32;
pub type CallFn =
    unsafe extern "C" fn(*const u8, usize, *const u8, usize, *const Host, *mut usize) -> *mut u8;
pub type FreeFn = unsafe extern "C" fn(*mut u8, usize);

/// The platform names modules are built and served for.
pub fn platform() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else {
        "unsupported"
    }
}

/// The module's file name on this platform.
pub fn file_name() -> &'static str {
    if cfg!(windows) {
        "mokora_pro.dll"
    } else {
        "libmokora_pro.so"
    }
}

/// The trusted comment a module's signature carries: which platform and ABI
/// it was built for, so a module of another one is never taken for this.
pub fn trusted_comment(platform: &str, version: &str) -> String {
    format!("mokora_pro {platform} abi{ABI_VERSION} {version}")
}
