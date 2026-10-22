mod binary;
mod flow;
mod session;
mod stream;

pub use binary::BinaryLogger;
pub use flow::FlowLogger;
pub use session::{LogTarget, Session};
pub use stream::LogStreamHandle;

use anyhow::Result;
use std::path::Path;

/// Initialise the logging subsystem and return an active `Session`.
///
/// Creates `<log_root>/<YYYYMMDD-HHMMSS>/` as the session directory and
/// installs a tracing subscriber with three kinds of layers:
///
/// - stderr console (env-controlled level, defaults to `default_level`)
/// - combined `all.log` (severity-filtered by `all_level`, accepts the same
///   syntax as `RUST_LOG` / `EnvFilter`)
/// - one `<target.name>.log` per entry in `targets`, filtered by tracing
///   target prefix (e.g. `mokora::proxy` matches the proxy module tree)
///
/// The returned `Session` must be kept alive for the lifetime of the app —
/// dropping it flushes pending writes and tears down the file appenders.
pub fn init(
    log_root: &Path,
    default_level: &str,
    all_level: &str,
    targets: &[LogTarget],
) -> Result<Session> {
    Session::init(log_root, default_level, all_level, targets)
}

/// Send every panic to the log as well as wherever it went before.
///
/// Mokora runs as a windowed app with no console, so a panic's default
/// message to stderr is lost. A panic in a background task ends that task
/// silently while the rest of the app carries on — the bot or autoplay
/// simply stops — and without this there is nothing in `all.log` to say
/// why. Call once, after [`init`].
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let thread = std::thread::current();
        tracing::error!(
            target: "mokora::panic",
            "panic in thread '{}' at {location}: {}\n{}",
            thread.name().unwrap_or("<unnamed>"),
            payload_text(info.payload()),
            std::backtrace::Backtrace::force_capture()
        );
        previous(info);
    }));
}

/// The message a panic carried: `panic!` with a literal gives a `&str`, with
/// formatting a `String`; anything else is opaque.
fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(non-text panic payload)".to_string())
}

#[cfg(test)]
mod tests {
    use super::payload_text;

    #[test]
    fn panic_payloads_are_read_as_text() {
        let literal = std::panic::catch_unwind(|| panic!("plain")).unwrap_err();
        assert_eq!(payload_text(literal.as_ref()), "plain");
        let n = 3;
        let formatted = std::panic::catch_unwind(|| panic!("seat {n}")).unwrap_err();
        assert_eq!(payload_text(formatted.as_ref()), "seat 3");
        let other = std::panic::catch_unwind(|| std::panic::panic_any(7u8)).unwrap_err();
        assert_eq!(payload_text(other.as_ref()), "(non-text panic payload)");
    }
}
