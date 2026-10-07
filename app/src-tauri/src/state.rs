//! Remembers which driver each GPU had the last time the caches were cleared.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::drivers::Adapter;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastClean {
    /// Seconds since the Unix epoch.
    pub at: u64,
    /// Adapter key to the driver version it had at the time.
    pub drivers: BTreeMap<String, String>,
    /// Bytes freed by every real clean so far.
    #[serde(default)]
    pub total_freed: u64,
}

fn file() -> Option<PathBuf> {
    let base = env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("ShaderSweep").join("state.json"))
}

pub fn load() -> Option<LastClean> {
    let text = fs::read_to_string(file()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// The record that follows a clean, carrying the running total forward.
pub fn next_record(
    previous: Option<&LastClean>,
    adapters: &[Adapter],
    freed: u64,
    now: u64,
) -> LastClean {
    LastClean {
        at: now,
        drivers: adapters
            .iter()
            .map(|a| (a.key(), a.version.clone()))
            .collect(),
        total_freed: previous.map_or(0, |p| p.total_freed).saturating_add(freed),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Writes through a temporary file so a crash never leaves half a record.
pub fn save(adapters: &[Adapter], freed: u64) -> std::io::Result<()> {
    let Some(path) = file() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    let record = next_record(load().as_ref(), adapters, freed, now_secs());

    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&record)?)?;
    fs::rename(&tmp, &path)
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
    now.iter()
        .filter(|a| last.drivers.get(&a.key()) != Some(&a.version))
        .collect()
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

    #[test]
    fn no_record_means_no_change() {
        let now = [adapter("RTX 5070", "616.92")];
        assert!(changed_since(None, &now).is_empty());
    }

    #[test]
    fn flags_a_new_driver_version() {
        let last = LastClean {
            at: 1,
            drivers: BTreeMap::from([("nvidia:RTX 5070".to_string(), "610.10".to_string())]),
            total_freed: 0,
        };
        let now = [adapter("RTX 5070", "616.92")];
        assert_eq!(changed_since(Some(&last), &now).len(), 1);
    }

    #[test]
    fn same_version_is_not_a_change() {
        let last = LastClean {
            at: 1,
            drivers: BTreeMap::from([("nvidia:RTX 5070".to_string(), "616.92".to_string())]),
            total_freed: 0,
        };
        let now = [adapter("RTX 5070", "616.92")];
        assert!(changed_since(Some(&last), &now).is_empty());
    }

    #[test]
    fn a_gpu_missing_from_the_record_counts_as_changed() {
        let last = LastClean::default();
        let now = [adapter("RTX 5070", "616.92")];
        assert_eq!(changed_since(Some(&last), &now).len(), 1);
    }

    #[test]
    fn the_total_carries_forward_and_never_overflows() {
        let now = [adapter("RTX 5070", "616.92")];
        let first = next_record(None, &now, 100, 10);
        assert_eq!(first.total_freed, 100);
        assert_eq!(first.at, 10);

        let second = next_record(Some(&first), &now, 50, 20);
        assert_eq!(second.total_freed, 150);

        let capped = next_record(Some(&second), &now, u64::MAX, 30);
        assert_eq!(capped.total_freed, u64::MAX);
    }

    #[test]
    fn old_state_files_without_a_total_still_load() {
        let text = r#"{"at": 5, "drivers": {"nvidia:RTX": "1.0"}}"#;
        let parsed: LastClean = serde_json::from_str(text).unwrap();
        assert_eq!(parsed.total_freed, 0);
    }
}
