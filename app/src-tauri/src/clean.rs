//! Empties a folder and reports what really happened.
//!
//! Freed space is counted per file that was actually deleted, never taken from
//! the size before the run, so a file that stays locked is never reported as
//! freed.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::fsutil::{has_dir_attribute, is_reparse};
use crate::native;
use crate::pending;

/// How many stuck files are kept to ask Windows who is holding them.
const SAMPLE_LIMIT: usize = 24;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub freed: u64,
    pub removed_files: u32,
    pub queued_files: u32,
    pub queued_bytes: u64,
    pub failed_files: u32,
    pub failed_bytes: u64,
    /// A few of the files that could not be removed.
    pub failed_samples: Vec<PathBuf>,
}

impl Outcome {
    pub fn add(&mut self, other: Outcome) {
        self.freed += other.freed;
        self.removed_files += other.removed_files;
        self.queued_files += other.queued_files;
        self.queued_bytes += other.queued_bytes;
        self.failed_files += other.failed_files;
        self.failed_bytes += other.failed_bytes;
        let room = SAMPLE_LIMIT.saturating_sub(self.failed_samples.len());
        self.failed_samples
            .extend(other.failed_samples.into_iter().take(room));
    }
}

fn remove_file(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(first) => {
            // Cache files are sometimes read only. Clear that and try once more.
            if let Ok(meta) = fs::metadata(path) {
                let mut perms = meta.permissions();
                if perms.readonly() {
                    #[allow(clippy::permissions_set_readonly_false)]
                    perms.set_readonly(false);
                    let _ = fs::set_permissions(path, perms);
                    return fs::remove_file(path);
                }
            }
            Err(first)
        }
    }
}

/// Deletes everything inside `root` and keeps `root` itself, so the driver
/// refills the same folder. Links inside are removed without being followed.
///
/// Files that stay locked are queued for the next restart when `queue_locked`
/// is set. Anything already in `pending` counts as queued without being added
/// a second time.
pub fn clear_contents(root: &Path, queue_locked: bool, pending: &HashSet<String>) -> Outcome {
    let mut out = Outcome::default();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut locked: Vec<(PathBuf, u64)> = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };

            if is_reparse(&meta) {
                // Remove the link itself and leave whatever it points at alone.
                let _ = if has_dir_attribute(&meta) {
                    fs::remove_dir(&path)
                } else {
                    fs::remove_file(&path)
                };
                continue;
            }

            if meta.is_dir() {
                dirs.push(path.clone());
                stack.push(path);
                continue;
            }

            match remove_file(&path) {
                Ok(()) => {
                    out.freed += meta.len();
                    out.removed_files += 1;
                }
                Err(_) => locked.push((path, meta.len())),
            }
        }
    }

    // Deepest first, so a folder is empty by the time it is removed.
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for dir in dirs {
        let _ = fs::remove_dir(dir);
    }

    for (path, len) in locked {
        let already = pending.contains(&pending::normalize(&path.to_string_lossy()));
        if queue_locked && (already || native::delete_on_reboot(&path)) {
            out.queued_files += 1;
            out.queued_bytes += len;
        } else {
            out.failed_files += 1;
            out.failed_bytes += len;
            if out.failed_samples.len() < SAMPLE_LIMIT {
                out.failed_samples.push(path);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;

    fn file(path: &Path, len: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::File::create(path)
            .unwrap()
            .write_all(&vec![1u8; len])
            .unwrap();
    }

    #[test]
    fn empties_the_folder_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("DXCache");
        file(&root.join("a.bin"), 100);
        file(&root.join("sub").join("deep").join("b.bin"), 50);

        let out = clear_contents(&root, false, &HashSet::new());

        assert_eq!(out.freed, 150);
        assert_eq!(out.removed_files, 2);
        assert_eq!(out.failed_files, 0);
        assert!(root.is_dir());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    #[test]
    fn removes_read_only_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("GLCache");
        let ro = root.join("ro.bin");
        file(&ro, 10);
        let mut perms = fs::metadata(&ro).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&ro, perms).unwrap();

        let out = clear_contents(&root, false, &HashSet::new());

        assert_eq!(out.freed, 10);
        assert_eq!(out.failed_files, 0);
    }

    #[test]
    fn an_open_file_is_reported_as_failed_not_freed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("VkCache");
        let held = root.join("held.bin");
        file(&held, 40);
        file(&root.join("free.bin"), 5);

        // Rust opens files with share-delete by default, which would let the
        // delete through. A share mode of zero is what a driver holding the
        // file looks like.
        let _guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&held)
            .unwrap();
        let out = clear_contents(&root, false, &HashSet::new());

        assert_eq!(out.freed, 5);
        assert_eq!(out.failed_files, 1);
        assert_eq!(out.failed_bytes, 40);
        assert!(held.exists());
    }

    /// Writes a real entry into the system's pending delete queue, so it only
    /// runs on request: cargo test -- --ignored
    #[test]
    #[ignore]
    fn a_held_file_is_queued_for_the_next_reboot() {
        if !native::is_admin() {
            eprintln!("skipped: queueing a delete needs administrator rights");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("DXCache");
        let held = root.join("held-by-driver.nvph");
        file(&held, 33);
        let _guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&held)
            .unwrap();

        let out = clear_contents(&root, true, &HashSet::new());

        assert_eq!(out.queued_files, 1);
        assert_eq!(out.queued_bytes, 33);
        assert_eq!(out.failed_files, 0);

        // The queue is a REG_MULTI_SZ under Session Manager.
        let queue: Vec<String> = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager")
            .unwrap()
            .get_value("PendingFileRenameOperations")
            .unwrap();
        // Windows stores the expanded path, not the 8.3 name a temp folder
        // may be reported under, so match on the unique file name.
        let wanted = "held-by-driver.nvph";
        assert!(
            queue
                .iter()
                .any(|entry| entry.to_ascii_lowercase().contains(wanted)),
            "{wanted} is not in the pending queue"
        );
    }

    #[test]
    fn never_follows_a_junction_out_of_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("precious");
        file(&outside.join("keep.txt"), 7);

        let root = dir.path().join("DXCache");
        fs::create_dir_all(&root).unwrap();
        let link = root.join("link");

        // mklink /J needs no special privilege.
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap()
            .status
            .success();
        assert!(made, "could not create the test junction");

        clear_contents(&root, false, &HashSet::new());

        assert!(
            outside.join("keep.txt").exists(),
            "the junction target was deleted"
        );
        assert!(!link.exists());
    }

    #[test]
    fn missing_folder_is_a_no_op() {
        let out = clear_contents(Path::new(r"Z:\no\such\folder"), true, &HashSet::new());
        assert_eq!(out, Outcome::default());
    }
}
