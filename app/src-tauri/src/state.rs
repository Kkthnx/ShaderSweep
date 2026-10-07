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
}

fn file() -> Option<PathBuf> {
    let base = env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("ShaderSweep").join("state.json"))
}

pub fn load() -> Option<LastClean> {
    let text = fs::read_to_string(file()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes through a temporary file so a crash never leaves half a record.
pub fn save(adapters: &[Adapter]) -> std::io::Result<()> {
    let Some(path) = file() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    let record = LastClean {
        at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        drivers: adapters
            .iter()
            .map(|a| (a.key(), a.version.clone()))
            .collect(),
    };

    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&record)?)?;
    fs::rename(&tmp, &path)
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
        };
        let now = [adapter("RTX 5070", "616.92")];
        assert_eq!(changed_since(Some(&last), &now).len(), 1);
    }

    #[test]
    fn same_version_is_not_a_change() {
        let last = LastClean {
            at: 1,
            drivers: BTreeMap::from([("nvidia:RTX 5070".to_string(), "616.92".to_string())]),
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
}
