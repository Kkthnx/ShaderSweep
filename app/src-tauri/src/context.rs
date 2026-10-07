//! Everything the providers need to know about this machine, gathered once.

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::steam;

#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Every profile with an AppData tree: real users plus the system and
    /// service accounts, whose profiles hold the caches the driver service
    /// writes.
    pub profiles: Vec<PathBuf>,
    pub system_drive: PathBuf,
    /// Usually `C:\Windows`.
    pub windows_dir: PathBuf,
    pub program_data: PathBuf,
    /// `Program Files` and `Program Files (x86)`.
    pub program_files: Vec<PathBuf>,
    pub steam_libraries: Vec<PathBuf>,
    /// Folders that hold the World of Warcraft game folders such as
    /// `_retail_`.
    pub wow_roots: Vec<PathBuf>,
    /// Folder moved with NVIDIA's `__GL_SHADER_DISK_CACHE_PATH` variable.
    pub gl_override: Option<PathBuf>,
}

impl Context {
    pub fn detect() -> Self {
        let system_drive = env::var("SystemDrive")
            .map(|d| PathBuf::from(format!("{d}\\")))
            .unwrap_or_else(|_| PathBuf::from(r"C:\"));

        let program_files = dedupe(
            ["ProgramFiles", "ProgramFiles(x86)"]
                .into_iter()
                .filter_map(|name| env::var_os(name).map(PathBuf::from))
                .collect(),
        );

        Self {
            profiles: profile_roots(),
            wow_roots: wow_roots(&program_files),
            program_files,
            system_drive,
            windows_dir: env::var_os("SystemRoot")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows")),
            program_data: env::var_os("ProgramData")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")),
            steam_libraries: steam::libraries(),
            gl_override: env::var_os("__GL_SHADER_DISK_CACHE_PATH").map(PathBuf::from),
        }
    }
}

fn dedupe(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(p.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

fn expand_env(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn profile_roots() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();

    if let Ok(list) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList")
    {
        for name in list.enum_keys().flatten() {
            let Ok(key) = list.open_subkey(&name) else {
                continue;
            };
            if let Ok(raw) = key.get_value::<String, _>("ProfileImagePath") {
                found.push(PathBuf::from(expand_env(&raw)));
            }
        }
    }

    // The registry lists the 64 bit system profile. The 32 bit one lives
    // beside it and holds caches written by 32 bit driver components.
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    found.push(Path::new(&system_root).join(r"System32\config\systemprofile"));
    found.push(Path::new(&system_root).join(r"SysWOW64\config\systemprofile"));

    if let Ok(user) = env::var("USERPROFILE") {
        found.push(PathBuf::from(user));
    }

    dedupe(found)
        .into_iter()
        .filter(|p| p.join("AppData").is_dir())
        .collect()
}

/// Where World of Warcraft is installed. Battle.net lets people pick any
/// folder, so the registry is asked first, then the usual places on every
/// drive.
fn wow_roots(program_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();

    for key in [
        r"SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
        r"SOFTWARE\Blizzard Entertainment\World of Warcraft",
    ] {
        if let Ok(subkey) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(key) {
            if let Ok(install) = subkey.get_value::<String, _>("InstallPath") {
                // The value points at a flavour such as `...\_retail_\`.
                if let Some(parent) = Path::new(install.trim_end_matches('\\')).parent() {
                    found.push(parent.to_path_buf());
                }
            }
        }
    }

    for pf in program_files {
        found.push(pf.join("World of Warcraft"));
    }
    for letter in b'C'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        if !Path::new(&drive).exists() {
            continue;
        }
        found.push(Path::new(&drive).join("World of Warcraft"));
        found.push(Path::new(&drive).join("Games").join("World of Warcraft"));
        found.push(
            Path::new(&drive)
                .join("Program Files (x86)")
                .join("World of Warcraft"),
        );
    }

    dedupe(found).into_iter().filter(|p| p.is_dir()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_known_variables() {
        env::set_var("SHADERSWEEP_TEST", "value");
        assert_eq!(expand_env(r"%SHADERSWEEP_TEST%\x"), r"value\x");
    }

    #[test]
    fn leaves_unknown_variables_alone() {
        assert_eq!(expand_env("%NOPE_NOT_SET_12345%"), "%NOPE_NOT_SET_12345%");
        assert_eq!(expand_env("100% sure"), "100% sure");
    }

    #[test]
    fn detects_at_least_the_current_profile() {
        let ctx = Context::detect();
        assert!(!ctx.profiles.is_empty());
        assert!(ctx.windows_dir.is_dir());
        assert!(!ctx.program_files.is_empty());
    }

    #[test]
    fn dedupe_ignores_case() {
        let list = dedupe(vec![PathBuf::from(r"C:\A"), PathBuf::from(r"c:\a")]);
        assert_eq!(list.len(), 1);
    }
}
