//! Windows services the cleaner leans on that are not plain folders: which
//! programs are running, the Recycle Bin, and the Event Viewer logs.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_ITEMS, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::EventLog::{
    EvtClearLog, EvtClose, EvtNextChannelPath, EvtOpenChannelEnum,
};
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
    SHERB_NOSOUND, SHQUERYRBINFO,
};

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Lower cased names of every running program, such as `discord.exe`.
pub fn running_processes() -> HashSet<String> {
    let mut names = HashSet::new();

    // SAFETY: the snapshot handle is used only between its creation and the
    // matching `CloseHandle`, and `entry` is a correctly sized, zeroed record
    // whose `dwSize` is set before it is handed to the API.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return names;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            names.insert(String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase());
            more = Process32NextW(snapshot, &mut entry) != 0;
        }

        CloseHandle(snapshot);
    }

    names
}

/// Which of `wanted` are running right now. Matching ignores case.
pub fn running_among(running: &HashSet<String>, wanted: &[&str]) -> Vec<String> {
    wanted
        .iter()
        .filter(|name| running.contains(&name.to_ascii_lowercase()))
        .map(|name| name.to_string())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecycleBin {
    pub bytes: u64,
    pub items: u64,
}

/// Size of the Recycle Bin across every drive.
pub fn recycle_bin() -> RecycleBin {
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };
    // SAFETY: a null root means every drive, and `info` is a valid record
    // with `cbSize` set, as the API requires.
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if hr != 0 {
        return RecycleBin::default();
    }
    RecycleBin {
        bytes: info.i64Size.max(0) as u64,
        items: info.i64NumItems.max(0) as u64,
    }
}

/// Empties the Recycle Bin on every drive without Windows' own prompts. The
/// window asks first, and this cannot be undone.
pub fn empty_recycle_bin() -> bool {
    let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
    // SAFETY: null window and root mean "no owner, every drive".
    unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), flags) == 0 }
}

/// Logs that are never cleared. The Security log is the audit trail, and
/// clearing it is what an intruder does, so it raises event 1102.
const KEEP_LOGS: &[&str] = &["Security"];

pub fn is_protected_log(name: &str) -> bool {
    KEEP_LOGS.iter().any(|keep| keep.eq_ignore_ascii_case(name))
}

fn channel_names() -> Vec<String> {
    let mut names = Vec::new();

    // SAFETY: the enumeration handle is closed before returning, and every
    // buffer passed in outlives its call.
    unsafe {
        let channels = EvtOpenChannelEnum(0, 0);
        if channels == 0 {
            return names;
        }

        loop {
            let mut buffer = vec![0u16; 512];
            let mut used = 0u32;
            let ok = EvtNextChannelPath(
                channels,
                buffer.len() as u32,
                buffer.as_mut_ptr(),
                &mut used,
            );
            if ok == 0 {
                // ERROR_NO_MORE_ITEMS ends the list. Any other error, such as
                // a name longer than the buffer, also ends it safely.
                let _ = ERROR_NO_MORE_ITEMS;
                break;
            }
            let len = (used as usize).saturating_sub(1).min(buffer.len());
            names.push(String::from_utf16_lossy(&buffer[..len]));
        }

        EvtClose(channels);
    }

    names
}

/// Clears every Event Viewer log except the Security log. Returns how many
/// were cleared. Logs Windows will not let go of are skipped quietly.
pub fn clear_event_logs() -> u32 {
    let mut cleared = 0;
    for name in channel_names() {
        if is_protected_log(&name) {
            continue;
        }
        let channel = wide(Path::new(&name).as_os_str());
        // SAFETY: a null session is the local machine, a null target means
        // "do not export a copy first", and `channel` is nul terminated.
        let ok = unsafe { EvtClearLog(0, channel.as_ptr(), std::ptr::null(), 0) };
        if ok != 0 {
            cleared += 1;
        }
    }
    cleared
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_its_own_process() {
        let running = running_processes();
        assert!(!running.is_empty());
        // The test binary is named after the crate.
        assert!(running.iter().any(|p| p.starts_with("shadersweep_lib")));
        assert!(running.contains("explorer.exe") || running.contains("system"));
    }

    #[test]
    fn matches_running_programs_ignoring_case() {
        let running: HashSet<String> = ["discord.exe", "steam.exe"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            running_among(&running, &["Discord.exe", "DiscordPTB.exe"]),
            vec!["Discord.exe"]
        );
        assert!(running_among(&running, &["Battle.net.exe"]).is_empty());
    }

    #[test]
    fn the_security_log_is_always_kept() {
        assert!(is_protected_log("Security"));
        assert!(is_protected_log("security"));
        assert!(!is_protected_log("Application"));
        assert!(!is_protected_log("System"));
    }

    #[test]
    fn enumerates_event_logs_and_includes_the_security_one() {
        let names = channel_names();
        assert!(names.iter().any(|n| n == "Application"));
        assert!(names.iter().any(|n| n == "Security"));
    }

    #[test]
    fn the_recycle_bin_can_be_measured() {
        // Whatever it holds, asking must not fail.
        let _ = recycle_bin();
    }
}
