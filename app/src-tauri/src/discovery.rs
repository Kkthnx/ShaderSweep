//! Finds cache folders by name, so a new driver layout such as
//! `PerDriverVersion` is picked up without a code change.

use std::fs;
use std::path::{Path, PathBuf};

use crate::fsutil::is_reparse;

fn leaf_matches(path: &Path, names: &[&str]) -> bool {
    path.file_name()
        .map(|n| {
            names
                .iter()
                .any(|w| n.to_string_lossy().eq_ignore_ascii_case(w))
        })
        .unwrap_or(false)
}

fn walk(dir: &Path, names: &[&str], depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_dir() || is_reparse(&meta) {
            continue;
        }
        let path = entry.path();
        if leaf_matches(&path, names) {
            out.push(path);
        } else {
            walk(&path, names, depth - 1, out);
        }
    }
}

/// Folders called one of `names` at most `max_depth` levels under each root.
/// A root that is itself a match is returned as is.
pub fn find_named(roots: &[PathBuf], names: &[&str], max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();

    for root in roots {
        let Ok(meta) = fs::symlink_metadata(root) else {
            continue;
        };
        if !meta.is_dir() || is_reparse(&meta) {
            continue;
        }
        if leaf_matches(root, names) {
            out.push(root.clone());
        } else {
            walk(root, names, max_depth, &mut out);
        }
    }

    drop_nested(out)
}

/// Removes duplicates and any folder that sits inside another one on the list,
/// so a parent is never cleared twice.
pub fn drop_nested(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort_by_key(|p| p.to_string_lossy().to_ascii_lowercase());
    paths.dedup_by_key(|p| p.to_string_lossy().to_ascii_lowercase());

    let mut kept: Vec<PathBuf> = Vec::new();
    for path in paths {
        let lower = path.to_string_lossy().to_ascii_lowercase();
        let nested = kept.iter().any(|k| {
            let prefix = format!("{}\\", k.to_string_lossy().to_ascii_lowercase());
            lower.starts_with(&prefix)
        });
        if !nested {
            kept.push(path);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_nested_match_and_ignores_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NVIDIA");
        fs::create_dir_all(root.join("PerDriverVersion").join("DXCache")).unwrap();
        fs::create_dir_all(root.join("GLCache")).unwrap();
        fs::create_dir_all(root.join("Other").join("Keep")).unwrap();

        let found = find_named(std::slice::from_ref(&root), &["DXCache", "GLCache"], 3);
        let names: Vec<_> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert_eq!(found.len(), 2);
        assert!(names.contains(&"DXCache".to_string()));
        assert!(names.contains(&"GLCache".to_string()));
    }

    #[test]
    fn respects_the_depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("R");
        fs::create_dir_all(root.join("a").join("b").join("c").join("DXCache")).unwrap();

        assert!(find_named(std::slice::from_ref(&root), &["DXCache"], 3).is_empty());
        assert_eq!(find_named(&[root], &["DXCache"], 4).len(), 1);
    }

    #[test]
    fn root_that_matches_is_returned() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("NV_Cache");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(
            find_named(std::slice::from_ref(&root), &["NV_Cache"], 3),
            vec![root]
        );
    }

    #[test]
    fn drops_nested_and_duplicate_paths() {
        let kept = drop_nested(vec![
            PathBuf::from(r"C:\x\DXCache\inner\DXCache"),
            PathBuf::from(r"C:\x\DXCache"),
            PathBuf::from(r"c:\x\dxcache"),
            PathBuf::from(r"C:\y\GLCache"),
        ]);
        assert_eq!(kept.len(), 2);
    }
}
