//! Finds every Steam library so each one's shader cache can be cleared.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

fn install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(key) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\Valve\Steam")
    {
        if let Ok(path) = key.get_value::<String, _>("InstallPath") {
            dirs.push(PathBuf::from(path));
        }
    }
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"SOFTWARE\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("SteamPath") {
            // Steam writes this one with forward slashes.
            dirs.push(PathBuf::from(path.replace('/', "\\")));
        }
    }

    dirs
}

/// Pulls the `"path"` entries out of `libraryfolders.vdf`.
pub fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let mut quoted = line.split('"').skip(1).step_by(2);
            let key = quoted.next()?;
            let value = quoted.next()?;
            if key.eq_ignore_ascii_case("path") {
                Some(PathBuf::from(value.replace(r"\\", r"\")))
            } else {
                None
            }
        })
        .collect()
}

/// Every library root, deduplicated and confirmed to exist.
pub fn libraries() -> Vec<PathBuf> {
    let mut roots = install_dirs();

    for install in roots.clone() {
        let vdf = install.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(&vdf) {
            roots.extend(parse_library_paths(&text));
        }
    }

    let mut seen = HashSet::new();
    roots
        .into_iter()
        .filter(|p| Path::new(p).join("steamapps").is_dir())
        .filter(|p| seen.insert(p.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_library_paths() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"254449"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"label"		""
	}
}
"#;
        let paths = parse_library_paths(vdf);
        assert_eq!(
            paths,
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary"),
            ]
        );
    }

    #[test]
    fn ignores_other_keys() {
        assert!(parse_library_paths("\t\"label\"\t\t\"path\"\n").is_empty());
    }
}
