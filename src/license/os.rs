//! A short name of the operating system, shown to the admin
//! ("Windows 11 Home", "Ubuntu 24.04.1 LTS").

/// The system's name, or `None` when it cannot be read.
pub fn name() -> Option<String> {
    imp()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(windows)]
fn imp() -> Option<String> {
    sysinfo::System::long_os_version()
}

#[cfg(target_os = "linux")]
fn imp() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    pretty_name(&text)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn imp() -> Option<String> {
    None
}

/// `PRETTY_NAME` of an os-release file.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn pretty_name(os_release: &str) -> Option<String> {
    os_release.lines().find_map(|line| {
        let value = line.strip_prefix("PRETTY_NAME=")?;
        Some(value.trim_matches('"').to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_pretty_name() {
        let text = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(pretty_name(text).as_deref(), Some("Ubuntu 24.04.1 LTS"));
        assert_eq!(pretty_name("ID=x"), None);
    }

    #[test]
    fn this_system_has_a_name() {
        assert!(name().is_some());
    }
}
