//! This computer's id as the license server sees it.
//!
//! The operating system's own install id (Windows `MachineGuid`, Linux
//! `/etc/machine-id`), hashed with a Mokora-specific prefix: stable across
//! Mokora reinstalls on the same system, and says nothing about the hardware.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Hex SHA-256 of `mokora-license:<os machine id>`.
pub fn id() -> Result<String> {
    let raw = machine_uid::get()
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("read the system's machine id")?;
    Ok(hash(raw.trim()))
}

fn hash(raw: &str) -> String {
    let digest = Sha256::digest(format!("mokora-license:{raw}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server takes 64 lowercase hex digits.
    #[test]
    fn the_id_is_hex_sha256() {
        let h = hash("1234-abcd");
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(h, hash("1234-abcd"));
        assert_ne!(h, hash("1234-abce"));
    }

    #[test]
    fn this_computer_has_one() {
        assert_eq!(id().unwrap().len(), 64);
    }
}
