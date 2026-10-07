//! What the app remembers between runs: which driver each GPU had at the last
//! shader clean, how much it has freed, and which files it queued for the
//! next restart so it can check afterwards that Windows really removed them.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::drivers::Adapter;
use crate::model::QueueCheck;

/// A file queued for deletion at the next restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuedFile {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastClean {
    /// Seconds since the Unix epoch, at the last shader clean.
    pub at: u64,
    /// Adapter key to the driver version it had at the time.
    pub drivers: BTreeMap<String, String>,
    /// Bytes freed by every real clean so far.
    #[serde(default)]
    pub total_freed: u64,
    /// Files waiting for the next restart, not yet confirmed gone.
    #[serde(default)]
    pub queued: Vec<QueuedFile>,
    /// When the newest file was queued. A restart after this settles them.
    #[serde(default)]
    pub queued_at: u64,
}

/// What a finished run adds to the record.
pub struct RunSummary<'a> {
    /// Set when a shader cache was really cleared, so the driver versions
    /// and the "last cleaned" time move forward.
    pub shader_adapters: Option<&'a [Adapter]>,
    pub freed: u64,
    pub queued: Vec<QueuedFile>,
}

fn file() -> Option<PathBuf> {
    let base = env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("ShaderSweep").join("state.json"))
}

pub fn load() -> Option<LastClean> {
    let text = fs::read_to_string(file()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// The record that follows a run, carrying the totals and the queue forward.
pub fn next_record(previous: Option<&LastClean>, run: &RunSummary, now: u64) -> LastClean {
    let mut record = previous.cloned().unwrap_or_default();

    if let Some(adapters) = run.shader_adapters {
        record.at = now;
        record.drivers = adapters
            .iter()
            .map(|a| (a.key(), a.version.clone()))
            .collect();
    }
    record.total_freed = record.total_freed.saturating_add(run.freed);

    if !run.queued.is_empty() {
        // The same file queued twice is still one file.
        for file in &run.queued {
            if !record
                .queued
                .iter()
                .any(|q| q.path.eq_ignore_ascii_case(&file.path))
            {
                record.queued.push(file.clone());
            }
        }
        record.queued_at = now;
    }

    record
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn write(record: &LastClean) -> std::io::Result<()> {
    let Some(path) = file() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Through a temporary file, so a crash never leaves half a record.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(record)?)?;
    fs::rename(&tmp, &path)
}

pub fn save(run: &RunSummary) -> std::io::Result<()> {
    write(&next_record(load().as_ref(), run, now_secs()))
}

/// Forgets the queued files once their outcome has been reported.
pub fn clear_queue() -> std::io::Result<()> {
    let Some(mut record) = load() else {
        return Ok(());
    };
    if record.queued.is_empty() {
        return Ok(());
    }
    record.queued.clear();
    record.queued_at = 0;
    write(&record)
}

/// Keeps the text of the last real run beside the state file.
pub fn write_log(text: &str) -> std::io::Result<()> {
    let Some(path) = file() else {
        return Ok(());
    };
    fs::write(path.with_file_name("last-run.txt"), text)
}

/// Adapters whose driver differs from the last clean. Empty when nothing has
/// been cleaned yet, because there is nothing to compare against.
pub fn changed_since<'a>(last: Option<&LastClean>, now: &'a [Adapter]) -> Vec<&'a Adapter> {
    let Some(last) = last else {
        return Vec::new();
    };
    if last.drivers.is_empty() {
        return Vec::new();
    }
    now.iter()
        .filter(|a| last.drivers.get(&a.key()) != Some(&a.version))
        .collect()
}

/// Works out what happened to the queued files. `boot_secs` is when Windows
/// last started, and `exists` says whether a path is still on disk.
///
/// After a restart, a file that is gone was removed by Windows and one that
/// is still there was not. Before a restart nothing has been decided yet.
pub fn check_queue(
    record: &LastClean,
    boot_secs: u64,
    exists: impl Fn(&str) -> bool,
) -> Option<QueueCheck> {
    if record.queued.is_empty() {
        return None;
    }

    let restarted = boot_secs > record.queued_at;
    let mut check = QueueCheck {
        restarted,
        removed: 0,
        removed_bytes: 0,
        remaining: 0,
        remaining_bytes: 0,
    };
    for file in &record.queued {
        if exists(&file.path) {
            check.remaining += 1;
            check.remaining_bytes += file.bytes;
        } else {
            check.removed += 1;
            check.removed_bytes += file.bytes;
        }
    }
    Some(check)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, version: &str) -> Adapter {
        Adapter {
            vendor: "nvidia".into(),
            name: name.into(),
            version: version.into(),
            date: "2026-09-04".into(),
        }
    }

    fn queued(path: &str, bytes: u64) -> QueuedFile {
        QueuedFile {
            path: path.into(),
            bytes,
        }
    }

    fn record(drivers: &[(&str, &str)]) -> LastClean {
        LastClean {
            at: 1,
            drivers: drivers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..LastClean::default()
        }
    }

    #[test]
    fn no_record_means_no_change() {
        let now = [adapter("RTX 5070", "616.92")];
        assert!(changed_since(None, &now).is_empty());
    }

    #[test]
    fn flags_a_new_driver_version() {
        let last = record(&[("nvidia:RTX 5070", "610.10")]);
        let now = [adapter("RTX 5070", "616.92")];
        assert_eq!(changed_since(Some(&last), &now).len(), 1);
    }

    #[test]
    fn same_version_is_not_a_change() {
        let last = record(&[("nvidia:RTX 5070", "616.92")]);
        let now = [adapter("RTX 5070", "616.92")];
        assert!(changed_since(Some(&last), &now).is_empty());
    }

    #[test]
    fn a_gpu_missing_from_a_real_record_counts_as_changed() {
        let last = record(&[("nvidia:GTX 1080", "500.00")]);
        let now = [adapter("RTX 5070", "616.92")];
        assert_eq!(changed_since(Some(&last), &now).len(), 1);
    }

    #[test]
    fn a_record_with_no_drivers_yet_flags_nothing() {
        // Only housekeeping has been cleaned so far, so there is no driver to
        // compare against.
        let last = LastClean {
            total_freed: 5,
            ..LastClean::default()
        };
        let now = [adapter("RTX 5070", "616.92")];
        assert!(changed_since(Some(&last), &now).is_empty());
    }

    #[test]
    fn the_total_carries_forward_and_never_overflows() {
        let now = [adapter("RTX 5070", "616.92")];
        let run = |freed| RunSummary {
            shader_adapters: Some(&now),
            freed,
            queued: vec![],
        };

        let first = next_record(None, &run(100), 10);
        assert_eq!((first.total_freed, first.at), (100, 10));

        let second = next_record(Some(&first), &run(50), 20);
        assert_eq!(second.total_freed, 150);

        let capped = next_record(Some(&second), &run(u64::MAX), 30);
        assert_eq!(capped.total_freed, u64::MAX);
    }

    #[test]
    fn a_housekeeping_only_run_keeps_the_driver_record() {
        let now = [adapter("RTX 5070", "616.92")];
        let first = next_record(
            None,
            &RunSummary {
                shader_adapters: Some(&now),
                freed: 10,
                queued: vec![],
            },
            100,
        );
        let after = next_record(
            Some(&first),
            &RunSummary {
                shader_adapters: None,
                freed: 5,
                queued: vec![],
            },
            200,
        );

        assert_eq!(after.at, 100, "the last shader clean did not move");
        assert_eq!(after.drivers, first.drivers);
        assert_eq!(after.total_freed, 15);
    }

    #[test]
    fn queued_files_accumulate_without_duplicates() {
        let none: [Adapter; 0] = [];
        let a = next_record(
            None,
            &RunSummary {
                shader_adapters: Some(&none),
                freed: 0,
                queued: vec![queued(r"C:\x\one.nvph", 10), queued(r"C:\x\two.nvph", 20)],
            },
            100,
        );
        let b = next_record(
            Some(&a),
            &RunSummary {
                shader_adapters: None,
                freed: 0,
                queued: vec![queued(r"c:\X\ONE.nvph", 10), queued(r"C:\x\three.nvph", 5)],
            },
            200,
        );

        assert_eq!(b.queued.len(), 3);
        assert_eq!(b.queued_at, 200, "a restart must come after the newest");
    }

    #[test]
    fn old_state_files_without_the_newer_fields_still_load() {
        let text = r#"{"at": 5, "drivers": {"nvidia:RTX": "1.0"}}"#;
        let parsed: LastClean = serde_json::from_str(text).unwrap();
        assert_eq!(parsed.total_freed, 0);
        assert!(parsed.queued.is_empty());
    }

    // ---- checking the queue after a restart ----

    fn waiting() -> LastClean {
        LastClean {
            queued: vec![
                queued(r"C:\a.nvph", 100),
                queued(r"C:\b.nvph", 200),
                queued(r"C:\c.nvph", 300),
            ],
            queued_at: 1_000,
            ..LastClean::default()
        }
    }

    #[test]
    fn nothing_queued_means_nothing_to_report() {
        assert_eq!(check_queue(&LastClean::default(), 5_000, |_| false), None);
    }

    #[test]
    fn before_a_restart_the_files_are_simply_still_waiting() {
        // Windows last started before the files were queued.
        let check = check_queue(&waiting(), 500, |_| true).unwrap();
        assert!(!check.restarted);
        assert_eq!(check.remaining, 3);
        assert_eq!(check.removed, 0);
    }

    #[test]
    fn after_a_restart_gone_files_were_removed() {
        let check = check_queue(&waiting(), 2_000, |_| false).unwrap();
        assert!(check.restarted);
        assert_eq!((check.removed, check.removed_bytes), (3, 600));
        assert_eq!(check.remaining, 0);
    }

    #[test]
    fn after_a_restart_a_file_still_there_means_windows_could_not_remove_it() {
        let check = check_queue(&waiting(), 2_000, |p| p.ends_with("b.nvph")).unwrap();
        assert!(check.restarted);
        assert_eq!((check.removed, check.removed_bytes), (2, 400));
        assert_eq!((check.remaining, check.remaining_bytes), (1, 200));
    }

    #[test]
    fn a_restart_exactly_at_queue_time_does_not_count() {
        let check = check_queue(&waiting(), 1_000, |_| true).unwrap();
        assert!(!check.restarted);
    }
}
