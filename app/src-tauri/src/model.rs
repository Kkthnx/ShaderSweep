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
    /// Plain words on what the folder is and what happens after it is cleared.
    pub about: &'static str,
    /// Which GPU vendor's driver writes this cache, if any.
    pub vendor: Option<&'static str>,
    pub default_on: bool,
    pub found: bool,
    pub bytes: u64,
    pub files: u64,
    pub folders: Vec<Folder>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub is_admin: bool,
    pub disk: Option<DiskInfo>,
    /// Files from an earlier clean that are still waiting for a restart.
    pub pending_restart: u32,
    pub adapters: Vec<Adapter>,
    pub providers: Vec<ProviderScan>,
    /// Seconds since the Unix epoch, or none if the caches were never cleaned.
    pub last_clean_at: Option<u64>,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub done: bool,
    pub freed: u64,
}
