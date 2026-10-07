//! Reads the list of files Windows has queued for deletion at the next boot,
//! so the app can say so and never queues the same file twice.

use std::collections::HashSet;
use std::path::PathBuf;

use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

/// `\??\C:\x`, `*1\??\C:\x` and `!\??\C:\x` all name `c:\x`.
pub fn normalize(raw: &str) -> String {
    let mut text = raw.trim_start_matches('!');
    if let Some(rest) = text.strip_prefix('*') {
        text = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let text = text.strip_prefix(r"\??\").unwrap_or(text);
    text.to_ascii_lowercase()
}

/// Every path in the queue, lower cased and without the `\??\` prefix.
pub fn load() -> HashSet<String> {
    let queue: Vec<String> = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager")
        .and_then(|key| key.get_value("PendingFileRenameOperations"))
        .unwrap_or_default();

    queue
        .iter()
        .filter(|entry| !entry.is_empty())
        .map(|entry| normalize(entry))
        .filter(|entry| entry.len() > 3)
        .collect()
}

/// How many queued paths sit inside one of `folders`.
pub fn count_inside(queue: &HashSet<String>, folders: &[PathBuf]) -> u32 {
    let roots: Vec<String> = folders
        .iter()
        .map(|f| format!("{}\\", f.to_string_lossy().to_ascii_lowercase()))
        .collect();

    queue
        .iter()
        .filter(|entry| roots.iter().any(|root| entry.starts_with(root.as_str())))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_known_prefixes() {
        assert_eq!(normalize(r"\??\C:\A\B.nvph"), r"c:\a\b.nvph");
        assert_eq!(normalize(r"*1\??\C:\A\B.nvph"), r"c:\a\b.nvph");
        assert_eq!(normalize(r"!\??\C:\A\B.nvph"), r"c:\a\b.nvph");
        assert_eq!(normalize(r"C:\plain"), r"c:\plain");
    }

    #[test]
    fn counts_only_paths_inside_the_given_folders() {
        let queue: HashSet<String> = [
            r"c:\users\a\appdata\local\nvidia\dxcache\one.nvph",
            r"c:\users\a\appdata\local\nvidia\dxcache\sub\two.nvph",
            r"c:\windows\system32\other.dll",
            r"c:\users\a\appdata\local\nvidia\dxcache2\three.nvph",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let folders = vec![PathBuf::from(r"C:\Users\A\AppData\Local\NVIDIA\DXCache")];
        assert_eq!(count_inside(&queue, &folders), 2);
    }
}
