//! Empties a folder and reports what really happened.
//!
//! Freed space is counted per file that was actually deleted, never taken from
//! the size before the run, so a file that stays locked is never reported as
//! freed.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::fsutil::{has_dir_attribute, is_reparse, Eligible, Verdict};
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
    /// The run was stopped before it reached the end of this folder.
    pub cancelled: bool,
    /// Files left alone because they changed too recently.
    pub skipped_recent: u32,
    /// Every file queued for the next restart, with its size, so a later
    /// launch can check that Windows really removed it.
    pub queued_paths: Vec<(PathBuf, u64)>,
}

impl Outcome {
    pub fn add(&mut self, other: Outcome) {
        self.freed += other.freed;
        self.removed_files += other.removed_files;
        self.queued_files += other.queued_files;
        self.queued_bytes += other.queued_bytes;
        self.failed_files += other.failed_files;
        self.failed_bytes += other.failed_bytes;
        self.cancelled |= other.cancelled;
        self.skipped_recent += other.skipped_recent;
        self.queued_paths.extend(other.queued_paths);
        let room = SAMPLE_LIMIT.saturating_sub(self.failed_samples.len());
        self.failed_samples
            .extend(other.failed_samples.into_iter().take(room));
    }
}

/// How a folder should be cleared.
pub struct Options<'a> {
    /// Queue files that stay locked for deletion at the next restart.
    pub queue_locked: bool,
    /// Files Windows already has queued, so none is queued twice.
    pub pending: &'a HashSet<String>,
    /// Checked between files. Setting it stops the run cleanly.
    pub cancel: &'a AtomicBool,
    /// Which files may go. Age keeps a clean away from files running apps are
    /// still using, and a name filter keeps it off the rest of a mixed folder.
    pub eligible: Eligible,
}

impl<'a> Options<'a> {
    pub fn new(queue_locked: bool, pending: &'a HashSet<String>, cancel: &'a AtomicBool) -> Self {
        Self {
            queue_locked,
            pending,
            cancel,
            eligible: Eligible::default(),
        }
    }
}

/// Reported after every file, so the window can show work happening.
pub struct Tick<'a> {
    /// Bytes freed so far in this folder.
    pub freed: u64,
    /// Files removed so far in this folder.
    pub files: u32,
    pub current: &'a Path,
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
pub fn clear_contents(root: &Path, options: &Options, on_tick: &mut dyn FnMut(&Tick)) -> Outcome {
    let mut out = Outcome::default();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut locked: Vec<(PathBuf, u64)> = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if options.cancel.load(Ordering::Relaxed) {
                out.cancelled = true;
                break 'walk;
            }

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
                if options.eligible.enters_folders() {
                    dirs.push(path.clone());
                    stack.push(path);
                }
                continue;
            }

            match options
                .eligible
                .judge(&entry.file_name().to_string_lossy(), &meta)
            {
                Verdict::Keep => continue,
                Verdict::TooRecent => {
                    out.skipped_recent += 1;
                    continue;
                }
                Verdict::Remove => {}
            }

            match remove_file(&path) {
                Ok(()) => {
                    out.freed += meta.len();
                    out.removed_files += 1;
                }
                Err(_) => locked.push((path.clone(), meta.len())),
            }
            on_tick(&Tick {
                freed: out.freed,
                files: out.removed_files,
                current: &path,
            });
        }
    }

    // Deepest first, so a folder is empty by the time it is removed.
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for dir in dirs {
        let _ = fs::remove_dir(dir);
    }

    // A stopped run leaves the locked files alone instead of queueing them.
    if out.cancelled {
        return out;
    }

    for (path, len) in locked {
        let already = options
            .pending
            .contains(&pending::normalize(&path.to_string_lossy()));
        if options.queue_locked && (already || native::delete_on_reboot(&path)) {
            out.queued_files += 1;
            out.queued_bytes += len;
            out.queued_paths.push((path, len));
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
    use std::time::{Duration, SystemTime};

    fn file(path: &Path, len: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::File::create(path)
            .unwrap()
            .write_all(&vec![1u8; len])
            .unwrap();
    }

    /// Clears a folder with no restart queue, no cancel and no progress.
    fn quiet(root: &Path) -> Outcome {
        run(root, false, &AtomicBool::new(false), &mut |_| {})
    }

    fn run(
        root: &Path,
        queue_locked: bool,
        cancel: &AtomicBool,
        on_tick: &mut dyn FnMut(&Tick),
    ) -> Outcome {
        let pending = HashSet::new();
        let options = Options::new(queue_locked, &pending, cancel);
        clear_contents(root, &options, on_tick)
    }

    fn run_with(root: &Path, tweak: impl FnOnce(&mut Options)) -> Outcome {
        let pending = HashSet::new();
        let cancel = AtomicBool::new(false);
        let mut options = Options::new(false, &pending, &cancel);
        tweak(&mut options);
        clear_contents(root, &options, &mut |_| {})
    }

    fn age(path: &Path, seconds_ago: u64) {
        let when = SystemTime::now() - Duration::from_secs(seconds_ago);
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    #[test]
    fn recent_files_are_left_alone_when_an_age_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Temp");
        let old = root.join("old.tmp");
        let fresh = root.join("fresh.tmp");
        file(&old, 100);
        file(&fresh, 40);
        age(&old, 3 * 86_400);

        let out = run_with(&root, |o| {
            o.eligible.min_age = Some(Duration::from_secs(86_400))
        });

        assert_eq!(out.freed, 100);
        assert_eq!(out.skipped_recent, 1);
        assert!(!old.exists());
        assert!(fresh.exists(), "a file in use right now must survive");
    }

    #[test]
    fn a_filter_removes_only_named_files_and_never_enters_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Explorer");
        file(&root.join("thumbcache_256.db"), 10);
        file(&root.join("thumbcache_idx.db"), 5);
        file(&root.join("settings.dat"), 3);
        file(&root.join("sub").join("thumbcache_1.db"), 7);

        let out = run_with(&root, |o| {
            o.eligible.file_filter =
                Some(|name| name.starts_with("thumbcache_") && name.ends_with(".db"));
        });

        assert_eq!(out.freed, 15);
        assert!(root.join("settings.dat").exists());
        assert!(
            root.join("sub").join("thumbcache_1.db").exists(),
            "subfolders are not entered"
        );
    }

    #[test]
    fn empties_the_folder_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("DXCache");
        file(&root.join("a.bin"), 100);
        file(&root.join("sub").join("deep").join("b.bin"), 50);

        let out = quiet(&root);

        assert_eq!(out.freed, 150);
        assert_eq!(out.removed_files, 2);
        assert_eq!(out.failed_files, 0);
        assert!(!out.cancelled);
        assert!(root.is_dir());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    #[test]
    fn reports_progress_after_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("DXCache");
        for n in 0..5 {
            file(&root.join(format!("f{n}.bin")), 10);
        }

        let mut seen: Vec<(u64, u32)> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        run(&root, false, &AtomicBool::new(false), &mut |t| {
            seen.push((t.freed, t.files));
            names.push(
                t.current
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        });

        assert_eq!(seen.len(), 5);
        // Running totals only ever go up, and end at the full amount.
        assert!(seen.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        assert_eq!(seen.last(), Some(&(50, 5)));
        assert!(names.iter().all(|n| n.ends_with(".bin")));
    }

    #[test]
    fn a_cancel_stops_the_run_and_keeps_what_is_left() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("DXCache");
        for n in 0..10 {
            file(&root.join(format!("f{n}.bin")), 10);
        }

        let cancel = AtomicBool::new(false);
        let out = run(&root, false, &cancel, &mut |t| {
            if t.files == 3 {
                cancel.store(true, Ordering::Relaxed);
            }
        });

        assert!(out.cancelled);
        assert_eq!(out.removed_files, 3);
        assert_eq!(out.freed, 30);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 7);
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

        let out = quiet(&root);

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
        let out = quiet(&root);

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

        let out = run(&root, true, &AtomicBool::new(false), &mut |_| {});

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

        quiet(&root);

        assert!(
            outside.join("keep.txt").exists(),
            "the junction target was deleted"
        );
        assert!(!link.exists());
    }

    #[test]
    fn missing_folder_is_a_no_op() {
        let out = quiet(Path::new(r"Z:\no\such\folder"));
        assert_eq!(out, Outcome::default());
    }
}
