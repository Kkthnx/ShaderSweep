//! The Win32 calls the app needs.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::ERROR_MORE_DATA;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT,
};
use windows_sys::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
    RM_PROCESS_INFO,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::UI::Shell::IsUserAnAdmin;

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Queues a file the display driver holds open for deletion at the next boot.
///
/// A few `.nvph` index files are mapped by the kernel mode driver itself, so
/// no process or service can release them while Windows is running. This is
/// the same mechanism installers use, and it needs administrator rights
/// because the queue lives in HKLM.
///
/// The destination is a real null pointer. Passing an empty string instead
/// fails with ERROR_PATH_NOT_FOUND.
pub fn delete_on_reboot(path: &Path) -> bool {
    let from = wide(path.as_os_str());
    // SAFETY: `from` is a valid, nul terminated UTF-16 buffer that outlives
    // the call, and a null destination is the documented way to ask for a
    // delete.
    unsafe { MoveFileExW(from.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) != 0 }
}

/// When Windows last started, in seconds since the Unix epoch. A restart
/// resets the uptime counter, so this moves forward after one.
pub fn boot_time_secs() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // SAFETY: takes no arguments and only reads the uptime counter.
    let uptime = unsafe { GetTickCount64() } / 1000;
    now.saturating_sub(uptime)
}

pub fn is_admin() -> bool {
    // SAFETY: takes no arguments and only reads the current token.
    unsafe { IsUserAnAdmin() != 0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disk {
    pub free: u64,
    pub total: u64,
}

/// Free and total space of the drive that holds `path`.
pub fn disk_space(path: &Path) -> Option<Disk> {
    let dir = wide(path.as_os_str());
    let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
    // SAFETY: `dir` is nul terminated and the three out pointers are valid
    // for the duration of the call.
    let ok = unsafe { GetDiskFreeSpaceExW(dir.as_ptr(), &mut free, &mut total, &mut total_free) };
    (ok != 0).then_some(Disk { free, total })
}

/// Names of the programs that currently hold any of `paths` open.
///
/// Uses the Windows Restart Manager, the same service installers use to find
/// out which apps to close. A file held by the kernel mode driver reports
/// nothing here, which is itself a useful answer.
pub fn holders(paths: &[PathBuf]) -> Vec<String> {
    if paths.is_empty() {
        return Vec::new();
    }

    let mut names: Vec<String> = Vec::new();
    let mut session = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];

    // SAFETY: every pointer handed to the Restart Manager points at a buffer
    // that lives until the matching `RmEndSession`, and the session handle is
    // only used between a successful start and that end.
    unsafe {
        if RmStartSession(&mut session, 0, key.as_mut_ptr()) != 0 {
            return names;
        }

        let files: Vec<Vec<u16>> = paths.iter().take(32).map(|p| wide(p.as_os_str())).collect();
        let pointers: Vec<*const u16> = files.iter().map(|f| f.as_ptr()).collect();

        if RmRegisterResources(
            session,
            pointers.len() as u32,
            pointers.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        ) == 0
        {
            let (mut needed, mut count, mut reasons) = (0u32, 0u32, 0u32);
            let first = RmGetList(
                session,
                &mut needed,
                &mut count,
                std::ptr::null_mut(),
                &mut reasons,
            );

            if (first == 0 || first == ERROR_MORE_DATA) && needed > 0 {
                let mut info: Vec<RM_PROCESS_INFO> = vec![std::mem::zeroed(); needed as usize];
                count = needed;
                if RmGetList(
                    session,
                    &mut needed,
                    &mut count,
                    info.as_mut_ptr(),
                    &mut reasons,
                ) == 0
                {
                    for item in info.iter().take(count as usize) {
                        let len = item
                            .strAppName
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(item.strAppName.len());
                        let name = String::from_utf16_lossy(&item.strAppName[..len]);
                        if !name.is_empty() && !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
        }

        RmEndSession(session);
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    fn the_boot_time_is_in_the_past_and_not_absurd() {
        let boot = boot_time_secs();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(boot < now, "Windows started before now");
        assert!(now - boot < 3 * 365 * 86_400, "uptime of years is a bug");
    }

    #[test]
    fn reports_free_space_for_the_system_drive() {
        let disk = disk_space(Path::new(r"C:\")).expect("C: should answer");
        assert!(disk.total > 0 && disk.free <= disk.total);
    }

    #[test]
    fn names_the_program_that_holds_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("held.bin");
        fs::write(&path, b"x").unwrap();
        let _guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();

        let names = holders(&[path]);
        assert!(!names.is_empty(), "the test process holds this file");
    }

    #[test]
    fn a_free_file_has_no_holders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("free.bin");
        fs::write(&path, b"x").unwrap();
        assert!(holders(&[path]).is_empty());
        assert!(holders(&[]).is_empty());
    }
}
