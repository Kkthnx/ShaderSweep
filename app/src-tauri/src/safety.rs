//! The allow list. Nothing is deleted unless it passes `is_allowed`.
//!
//! The frontend only ever sends provider ids, and the backend rebuilds every
//! path itself, so this check is the single gate between a folder name and a
//! delete call.
//!
//! A bare folder name such as `Cache` is far too common to trust on its own,
//! so each rule is the *tail* of a path. `discord*\Cache` allows Discord's
//! cache and nothing else called `Cache`, and `Local Storage` beside it, where
//! the login lives, matches no rule at all.

use std::fs;
use std::path::{Component, Path};

use crate::fsutil::is_reparse;

/// Path tails that only ever hold regenerable data. Components are matched
/// from the end, ignoring case, and `*` matches any run of characters.
const RULES: &[&[&str]] = &[
    // GPU driver shader and compute caches.
    &["DXCache"],
    &["GLCache"],
    &["ComputeCache"],
    &["OptixCache"],
    &["NV_Cache"],
    &["DX9Cache"],
    &["DxcCache"],
    &["OglCache"],
    &["VkCache"],
    &["D3DSCache"],
    &["Intel", "ShaderCache"],
    &["steamapps", "shadercache"],
    // Game launcher caches. Chromium based, so these are disk caches that the
    // app rebuilds. Logins, settings and installed games live elsewhere.
    &["discord*", "Cache"],
    &["discord*", "Code Cache"],
    &["discord*", "GPUCache"],
    &["discord*", "DawnCache"],
    &["discord*", "DawnGraphiteCache"],
    &["discord*", "DawnWebGPUCache"],
    &["EpicGamesLauncher", "Saved", "webcache*"],
    &["Battle.net", "Cache"],
    &["Battle.net", "BrowserCaches"],
    &["Steam", "htmlcache"],
    &["EA Desktop", "Cache"],
    &["Ubisoft Game Launcher", "cache"],
    &["GOG.com", "Galaxy", "webcache"],
    // Windows housekeeping. The Explorer folder also holds settings, so a
    // provider may only clear it through a filter that names the files.
    &["AppData", "Local", "Temp"],
    &["AppData", "Local", "CrashDumps"],
    &["DeliveryOptimization", "Cache"],
    &["Microsoft", "Windows", "WER", "ReportArchive"],
    &["Microsoft", "Windows", "WER", "ReportQueue"],
    &["Microsoft", "Windows", "Explorer"],
    // Blizzard's own troubleshooting says to remove a game's Cache folder.
    // Data (the game itself), WTF (settings) and Interface (add-ons) are not
    // on this list and never will be.
    &["World of Warcraft", "_*_", "Cache"],
];

/// `*` is the only wildcard, and it matches any run of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 1 {
        return pattern == text;
    }

    let mut rest = text.as_str();
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            let Some(after) = rest.strip_prefix(part) else {
                return false;
            };
            rest = after;
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            let Some(at) = rest.find(part) else {
                return false;
            };
            rest = &rest[at + part.len()..];
        }
    }
    true
}

fn names(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn ends_with_rule(parts: &[String], rule: &[&str]) -> bool {
    parts.len() >= rule.len()
        && parts[parts.len() - rule.len()..]
            .iter()
            .zip(rule)
            .all(|(part, pattern)| glob_match(pattern, part))
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

/// True when `path` is a folder this app is willing to empty.
pub fn is_allowed(path: &Path, system_drive: &Path, windows_dir: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    // A drive root has two components, so this also refuses `C:\`.
    if path.components().count() < 3 {
        return false;
    }
    // A cache folder that has been swapped for a junction could point
    // anywhere, so it is never trusted.
    if let Ok(meta) = fs::symlink_metadata(path) {
        if is_reparse(&meta) {
            return false;
        }
    }

    let parts = names(path);
    if RULES.iter().any(|rule| ends_with_rule(&parts, rule)) {
        return true;
    }

    // Installer leftovers and crash dumps are matched as exact paths.
    same_path(path, &system_drive.join("AMD"))
        || same_path(path, &system_drive.join("NVIDIA").join("DisplayDriver"))
        || same_path(path, &windows_dir.join("LiveKernelReports"))
        || same_path(path, &windows_dir.join("Temp"))
        || same_path(path, &windows_dir.join("Prefetch"))
        || same_path(
            path,
            &windows_dir.join("SoftwareDistribution").join("Download"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive() -> &'static Path {
        Path::new(r"C:\")
    }

    fn windows() -> &'static Path {
        Path::new(r"C:\Windows")
    }

    fn allowed(p: &str) -> bool {
        is_allowed(Path::new(p), drive(), windows())
    }

    #[test]
    fn wildcards_match_runs_and_nothing_else() {
        assert!(glob_match("discord*", "discord"));
        assert!(glob_match("discord*", "DiscordPTB"));
        assert!(glob_match("webcache*", "webcache_4430"));
        assert!(glob_match("_*_", "_retail_"));
        assert!(glob_match("_*_", "_classic_era_"));
        assert!(!glob_match("_*_", "retail"));
        assert!(!glob_match("_*_", "_retail"));
        assert!(!glob_match("discord*", "mydiscord"));
        assert!(glob_match("Cache", "cache"));
        assert!(!glob_match("Cache", "Code Cache"));
    }

    #[test]
    fn accepts_gpu_and_windows_caches() {
        for p in [
            r"C:\Users\a\AppData\Local\NVIDIA\DXCache",
            r"C:\Users\a\AppData\Local\nvidia\dxcache",
            r"C:\Users\a\AppData\LocalLow\NVIDIA\PerDriverVersion\GLCache",
            r"C:\Users\a\AppData\Roaming\NVIDIA\ComputeCache",
            r"C:\Users\a\AppData\Local\AMD\DxcCache",
            r"C:\Users\a\AppData\Local\D3DSCache",
            r"C:\Users\a\AppData\LocalLow\Intel\ShaderCache",
            r"C:\Program Files (x86)\Steam\steamapps\shadercache",
            r"C:\AMD",
            r"C:\NVIDIA\DisplayDriver",
            r"C:\Windows\LiveKernelReports",
        ] {
            assert!(allowed(p), "{p}");
        }
    }

    #[test]
    fn accepts_windows_housekeeping_folders() {
        for p in [
            r"C:\Windows\Temp",
            r"C:\Windows\Prefetch",
            r"C:\Windows\SoftwareDistribution\Download",
            r"C:\Users\a\AppData\Local\Temp",
            r"C:\Users\a\AppData\Local\CrashDumps",
            r"C:\Users\a\AppData\Local\Microsoft\Windows\Explorer",
            r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive",
            r"C:\Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
        ] {
            assert!(allowed(p), "{p}");
        }
    }

    #[test]
    fn accepts_launcher_caches_in_their_known_places() {
        for p in [
            r"C:\Users\a\AppData\Roaming\discord\Cache",
            r"C:\Users\a\AppData\Roaming\discord\Code Cache",
            r"C:\Users\a\AppData\Roaming\discord\GPUCache",
            r"C:\Users\a\AppData\Roaming\discordptb\DawnGraphiteCache",
            r"C:\Users\a\AppData\Local\EpicGamesLauncher\Saved\webcache",
            r"C:\Users\a\AppData\Local\EpicGamesLauncher\Saved\webcache_4430",
            r"C:\Users\a\AppData\Local\Battle.net\Cache",
            r"C:\Users\a\AppData\Local\Battle.net\BrowserCaches",
            r"C:\Users\a\AppData\Local\Steam\htmlcache",
            r"C:\Users\a\AppData\Local\Electronic Arts\EA Desktop\Cache",
            r"C:\Users\a\AppData\Local\Ubisoft Game Launcher\cache",
            r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\cache",
            r"C:\ProgramData\GOG.com\Galaxy\webcache",
            r"C:\Program Files (x86)\World of Warcraft\_retail_\Cache",
            r"C:\Program Files (x86)\World of Warcraft\_classic_era_\Cache",
        ] {
            assert!(allowed(p), "{p}");
        }
    }

    #[test]
    fn never_allows_logins_settings_saves_or_the_games_themselves() {
        for p in [
            r"C:\Users\a\AppData\Roaming\discord",
            r"C:\Users\a\AppData\Roaming\discord\Local Storage",
            r"C:\Users\a\AppData\Roaming\discord\IndexedDB",
            r"C:\Users\a\AppData\Roaming\discord\Network",
            r"C:\Users\a\AppData\Roaming\discord\Session Storage",
            r"C:\Users\a\AppData\Local\EpicGamesLauncher\Saved",
            r"C:\Users\a\AppData\Local\EpicGamesLauncher\Saved\Config",
            r"C:\Users\a\AppData\Local\EpicGamesLauncher\Saved\Crashes",
            r"C:\Users\a\AppData\Local\Battle.net\Account",
            r"C:\Users\a\AppData\Local\Battle.net",
            r"C:\ProgramData\Battle.net\Agent",
            r"C:\Users\a\AppData\Local\Steam",
            r"C:\Program Files (x86)\Steam\steamapps\common",
            r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games",
            r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\savegames",
            r"C:\Program Files (x86)\World of Warcraft\Data",
            r"C:\Program Files (x86)\World of Warcraft\_retail_\WTF",
            r"C:\Program Files (x86)\World of Warcraft\_retail_\Interface",
            r"C:\Program Files (x86)\World of Warcraft\_retail_",
            r"C:\Program Files (x86)\World of Warcraft\Cache",
            r"C:\Users\a\AppData\Local\GOG.com\Galaxy",
            r"C:\Users\a\AppData\Local",
            r"C:\Users\a\AppData\Local\Microsoft",
            r"C:\Users\a\AppData\Local\Microsoft\Windows",
            r"C:\Windows\SoftwareDistribution",
            r"C:\Windows\System32\winevt\Logs",
            r"C:\ProgramData\Microsoft\Windows\WER",
            r"C:\ProgramData\Microsoft\Windows",
        ] {
            assert!(!allowed(p), "{p}");
        }
    }

    #[test]
    fn a_bare_cache_folder_is_never_enough() {
        for p in [
            r"C:\Games\Foo\Cache",
            r"C:\Games\Foo\cache",
            r"C:\Users\a\Documents\Cache",
            r"C:\Games\Foo\ShaderCache",
            r"C:\Games\Foo\shadercache",
            r"C:\Users\a\AppData\Roaming\Code\Cache",
            r"C:\Users\a\AppData\Roaming\Slack\Cache",
        ] {
            assert!(!allowed(p), "{p}");
        }
    }

    #[test]
    fn refuses_system_locations_and_odd_paths() {
        for p in [
            r"C:\",
            r"C:\Windows",
            r"C:\Windows\System32",
            r"C:\Windows\Temp\sub",
            r"C:\Windows\LiveKernelReports\WATCHDOG",
            r"C:\Users",
            r"C:\Users\a",
            r"C:\Users\a\Documents",
            r"C:\Users\a\AppData\Local\NVIDIA",
            r"C:\NVIDIA",
            r"C:\Intel",
            r"D:\AMD",
            r"relative\DXCache",
        ] {
            assert!(!allowed(p), "{p}");
        }
    }

    #[test]
    fn refuses_parent_traversal() {
        let sneaky = r"C:\Users\a\AppData\Local\NVIDIA\DXCache\..\..\..\..\Documents\DXCache\..";
        assert!(!allowed(sneaky));
        assert!(!allowed(
            r"C:\Users\a\AppData\Roaming\discord\Cache\..\Local Storage"
        ));
    }
}
