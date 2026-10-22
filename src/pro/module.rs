//! Loading the module's library and calling it (`pro_api`'s three
//! functions). Loaded once and kept for the rest of the run.

use anyhow::{bail, Context, Result};
use pro_api::{AbiFn, CallFn, FreeFn, Host, Reply};
use serde::{de::DeserializeOwned, Serialize};
use std::ffi::c_void;
use std::path::Path;
use tracing::warn;

/// The loaded module.
pub struct Module {
    // Kept so the functions below stay valid; never unloaded.
    _lib: libloading::Library,
    call: CallFn,
    free: FreeFn,
    version: String,
}

/// Mokora's side of a module's call back: a method name and its JSON in, the
/// answer's JSON out (or why not).
pub type OnHost<'a> = dyn FnMut(&str, &[u8]) -> Result<Vec<u8>, String> + 'a;

impl Module {
    /// Load the library at `path` (already verified) and check it speaks this
    /// Mokora's `pro_api`.
    pub fn load(path: &Path, version: String) -> Result<Self> {
        // SAFETY: the file's signature was checked against the module key
        // before this; its initialisers are ours.
        let lib = unsafe { libloading::Library::new(path) }
            .with_context(|| format!("load {}", path.display()))?;
        // SAFETY: the symbols are declared with these types by
        // `pro_api::export_module!`.
        let (abi, call, free) = unsafe {
            let abi: AbiFn = *lib.get::<AbiFn>(pro_api::symbols::ABI).context("no ABI function")?;
            let call: CallFn = *lib.get::<CallFn>(pro_api::symbols::CALL).context("no call function")?;
            let free: FreeFn = *lib.get::<FreeFn>(pro_api::symbols::FREE).context("no free function")?;
            (abi, call, free)
        };
        // SAFETY: takes nothing, returns a number.
        let got = unsafe { abi() };
        if got != pro_api::ABI_VERSION {
            bail!("the module speaks ABI {got}, this Mokora {}", pro_api::ABI_VERSION);
        }
        Ok(Self {
            _lib: lib,
            call,
            free,
            version,
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Call `method` with `req`; its answer.
    pub fn call<Req: Serialize, Resp: DeserializeOwned>(&self, method: &str, req: &Req) -> Result<Resp> {
        self.call_with(method, req, &[], &mut |m, _| Err(format!("{m}: not offered here")))
    }

    /// Call `method` with `req`, lending `blob` for the call and answering
    /// the module's calls back with `on_host`.
    pub fn call_with<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        method: &str,
        req: &Req,
        blob: &[u8],
        on_host: &mut OnHost<'_>,
    ) -> Result<Resp> {
        let input = serde_json::to_vec(req)?;
        // The context the trampolines get back: the closure, behind one more
        // pointer (a `dyn` pointer is two words).
        let mut on_host: &mut OnHost<'_> = on_host;
        let host = Host {
            ctx: &mut on_host as *mut &mut OnHost<'_> as *mut c_void,
            call: Some(host_call),
            free: Some(host_free),
            blob: blob.as_ptr(),
            blob_len: blob.len(),
        };
        let mut len = 0usize;
        // SAFETY: the buffers and `host` are live for the call; the answer is
        // read once and handed straight back to the module to free.
        let bytes = unsafe {
            let ptr = (self.call)(method.as_ptr(), method.len(), input.as_ptr(), input.len(), &host, &mut len);
            if ptr.is_null() {
                bail!("{method}: the module answered nothing");
            }
            let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
            (self.free)(ptr, len);
            bytes
        };
        match serde_json::from_slice::<Reply<Resp>>(&bytes).with_context(|| format!("{method}: unreadable answer"))? {
            Reply::Ok(v) => Ok(v),
            Reply::Err(e) => bail!("{method}: {e}"),
        }
    }
}

/// A module calling back into Mokora: run the closure behind `ctx`, never
/// letting a panic across, and answer a [`Reply`] as JSON.
unsafe extern "C" fn host_call(
    ctx: *mut c_void,
    method: *const u8,
    method_len: usize,
    input: *const u8,
    input_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    // SAFETY: `ctx` is the `&mut OnHost` `call_with` built, alive for the call.
    let on_host = &mut *(ctx as *mut &mut OnHost<'_>);
    let method = std::slice::from_raw_parts(method, method_len);
    let input = std::slice::from_raw_parts(input, input_len);
    let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let method = std::str::from_utf8(method).map_err(|e| e.to_string())?;
        on_host(method, input)
    }));
    let reply: Reply<serde_json::Value> = match reply {
        Ok(Ok(bytes)) => match serde_json::from_slice(&bytes) {
            Ok(v) => Reply::Ok(v),
            Err(e) => Reply::Err(e.to_string()),
        },
        Ok(Err(e)) => Reply::Err(e),
        Err(_) => Reply::Err("Mokora panicked answering the module".into()),
    };
    let bytes = serde_json::to_vec(&reply)
        .unwrap_or_else(|_| br#"{"err":"unserialisable reply"}"#.to_vec())
        .into_boxed_slice();
    *out_len = bytes.len();
    Box::into_raw(bytes) as *mut u8
}

unsafe extern "C" fn host_free(_ctx: *mut c_void, ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
    }
}

/// Yakuman's planner, asked through the module. A failed call leaves the
/// decision to the model (and says so once).
pub(super) struct ModulePlanner;

impl native_bot::yakuman::Planner for ModulePlanner {
    fn plan(&self, req: &pro_api::yakuman::PlanRequest) -> pro_api::yakuman::PlanResponse {
        let module = super::loaded()?;
        match module.call(pro_api::method::YAKUMAN_RANK, req) {
            Ok(order) => order,
            Err(e) => {
                static SAID: std::sync::Once = std::sync::Once::new();
                SAID.call_once(|| warn!("yakuman planner: {e:#}"));
                None
            }
        }
    }
}
