//! The shapes sent to the frontend.

use serde::Serialize;

use crate::drivers::Adapter;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderScan {
    pub id: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    pub note: Option<&'static str>,
    /// A warning for anything hard to undo, shown on the row itself.
    pub caution: Option<&'static str>,
    /// Plain words on what the folder is and what happens after it is cleared.
    pub about: &'static str,
    /// `shaders`, `launchers` or `housekeeping`.
    pub group: &'static str,
    /// Which GPU vendor's driver writes this cache, if any.
    pub vendor: Option<&'static str>,
    pub default_on: bool,
    pub found: bool,
    pub bytes: u64,
    pub files: u64,
    pub folders: Vec<Folder>,
    /// Programs that are running right now and will keep some files locked.
    pub running: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverChange {
    pub vendor: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub drive: String,
    pub free: u64,
    pub total: u64,
}

/// What became of the files queued for deletion at the last restart.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueueCheck {
    /// Windows has restarted since the files were queued.
    pub restarted: bool,
    pub removed: u32,
    pub removed_bytes: u64,
    /// Files still present. Before a restart that is expected. After one it
    /// means Windows could not remove them.
    pub remaining: u32,
    pub remaining_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub is_admin: bool,
    pub disk: Option<DiskInfo>,
    /// Files from an earlier clean that are still waiting for a restart.
    pub pending_restart: u32,
    pub queue_check: Option<QueueCheck>,
    pub adapters: Vec<Adapter>,
    pub providers: Vec<ProviderScan>,
    /// Seconds since the Unix epoch, or none if the caches were never cleaned.
    pub last_clean_at: Option<u64>,
    /// Bytes freed by every real clean so far.
    pub total_freed: u64,
    pub driver_changes: Vec<DriverChange>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderResult {
    pub id: String,
    pub freed: u64,
    pub removed_files: u32,
    pub queued_files: u32,
    pub queued_bytes: u64,
    pub failed_files: u32,
    pub failed_bytes: u64,
    /// Files left alone because they changed too recently to be safe to touch.
    pub skipped_recent: u32,
    /// Programs Windows says are holding files that could not be removed.
    pub holders: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub preview: bool,
    pub providers: Vec<ProviderResult>,
    pub freed: u64,
    pub queued_files: u32,
    pub queued_bytes: u64,
    pub failed_files: u32,
    /// The run was stopped by the user before it finished.
    pub cancelled: bool,
    /// Plain text summary, ready to copy or save.
    pub report: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub done: bool,
    /// Bytes freed so far in this row.
    pub freed: u64,
    /// Files removed so far in this row.
    pub files: u32,
    /// The file being removed right now.
    pub current: Option<String>,
}

/// One cache type finished being measured during a scan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStep {
    pub label: String,
    pub checked: u32,
    pub total: u32,
}
