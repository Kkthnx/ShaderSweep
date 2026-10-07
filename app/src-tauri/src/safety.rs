//! The allow list. Nothing is deleted unless it passes `is_allowed`.
//!
//! The frontend only ever sends provider ids, and the backend rebuilds every
//! path itself, so this check is the single gate between a folder name and a
//! delete call.

use std::fs;
use std::path::{Component, Path};

use crate::fsutil::is_reparse;

/// Folder names that only ever hold regenerable shader or compute cache data.
const CACHE_NAMES: &[&str] = &[
    "DXCache",
    "GLCache",
    "ComputeCache",
    "OptixCache",
    "NV_Cache",
    "DX9Cache",
    "DxcCache",
    "OglCache",
    "VkCache",
    "D3DSCache",
];

/// Generic names that are only trusted under a specific parent folder.
const PAIRED_NAMES: &[(&str, &str)] = &[("ShaderCache", "Intel"), ("shadercache", "steamapps")];

fn name_of(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn same_path(a: &Path, b: &Path) -> bool {
    same(&a.to_string_lossy(), &b.to_string_lossy())
}

/// True when `path` is a folder this app is willing to empty.
pub fn is_allowed(path: &Path, system_drive: &Path) -> bool {
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

    let Some(leaf) = name_of(path) else {
        return false;
    };

    if CACHE_NAMES.iter().any(|n| same(n, &leaf)) {
        return true;
    }

    if let Some(parent_name) = path.parent().and_then(name_of) {
        if PAIRED_NAMES
            .iter()
            .any(|(n, p)| same(n, &leaf) && same(p, &parent_name))
        {
            return true;
        }
    }

    // Installer leftovers are matched as exact paths, never by name.
    same_path(path, &system_drive.join("AMD"))
        || same_path(path, &system_drive.join("NVIDIA").join("DisplayDriver"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive() -> &'static Path {
        Path::new(r"C:\")
    }

    #[test]
    fn accepts_known_cache_folders() {
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
        ] {
            assert!(is_allowed(Path::new(p), drive()), "{p}");
        }
    }

    #[test]
    fn refuses_everything_else() {
        for p in [
            r"C:\",
            r"C:\Windows",
            r"C:\Users",
            r"C:\Users\a",
            r"C:\Users\a\Documents",
            r"C:\Users\a\AppData\Local\NVIDIA",
            r"C:\Program Files (x86)\Steam\steamapps",
            r"C:\Program Files (x86)\Steam\steamapps\common",
            r"C:\NVIDIA",
            r"C:\Intel",
            r"D:\AMD",
            r"relative\DXCache",
        ] {
            assert!(!is_allowed(Path::new(p), drive()), "{p}");
        }
    }

    #[test]
    fn generic_names_need_the_right_parent() {
        assert!(!is_allowed(Path::new(r"C:\Games\Foo\ShaderCache"), drive()));
        assert!(!is_allowed(Path::new(r"C:\Games\Foo\shadercache"), drive()));
    }

    #[test]
    fn refuses_parent_traversal() {
        let sneaky = r"C:\Users\a\AppData\Local\NVIDIA\DXCache\..\..\..\..\Documents\DXCache\..";
        assert!(!is_allowed(Path::new(sneaky), drive()));
    }
}
