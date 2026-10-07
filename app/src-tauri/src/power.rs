//! Restarting the PC the careful way.
//!
//! `shutdown /r /t 30` looks like the obvious route, but Microsoft documents
//! that a timeout above zero implies `/f`, which closes running apps without
//! warning and loses unsaved work. The API call below asks for no force: apps
//! with unsaved changes make Windows show its own prompt, and the user keeps
//! control. The countdown it shows can be cancelled until it ends.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Shutdown::{
    AbortSystemShutdownW, InitiateSystemShutdownExW, SHTDN_REASON_FLAG_PLANNED,
    SHTDN_REASON_MAJOR_APPLICATION, SHTDN_REASON_MINOR_MAINTENANCE,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Shortest and longest countdown the window may ask for, in seconds.
pub const MIN_DELAY: u32 = 15;
pub const MAX_DELAY: u32 = 300;

/// A planned application maintenance restart, so the event log says why.
const REASON: u32 =
    SHTDN_REASON_MAJOR_APPLICATION | SHTDN_REASON_MINOR_MAINTENANCE | SHTDN_REASON_FLAG_PLANNED;

pub const MESSAGE: &str =
    "ShaderSweep is restarting Windows to finish removing files the display driver was holding open.";

pub fn clamp_delay(seconds: u32) -> u32 {
    seconds.clamp(MIN_DELAY, MAX_DELAY)
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Turns on the shutdown privilege. Administrators hold it, but it starts
/// disabled in the token.
fn enable_shutdown_privilege() -> Result<(), String> {
    // SAFETY: `token` is only used between a successful `OpenProcessToken`
    // and the `CloseHandle` that follows, and `privileges` is a fully
    // initialised record that outlives the call.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return Err("Could not read the process token.".into());
        }

        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        if LookupPrivilegeValueW(std::ptr::null(), SE_SHUTDOWN_NAME, &mut luid) == 0 {
            CloseHandle(token);
            return Err("Could not find the shutdown privilege.".into());
        }

        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let adjusted = AdjustTokenPrivileges(
            token,
            0,
            &privileges,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        // AdjustTokenPrivileges can succeed without granting everything, so
        // the last error is the real answer.
        let granted = adjusted != 0 && GetLastError() == 0;
        CloseHandle(token);

        if granted {
            Ok(())
        } else {
            Err("Windows did not allow this app to restart the PC.".into())
        }
    }
}

/// Starts a restart countdown. Running apps are never forced closed.
pub fn request_restart(delay_seconds: u32) -> Result<(), String> {
    enable_shutdown_privilege()?;

    let message = wide(MESSAGE);
    // SAFETY: `message` is nul terminated and outlives the call. A null
    // machine name means this computer.
    let ok = unsafe {
        InitiateSystemShutdownExW(
            std::ptr::null(),
            message.as_ptr(),
            clamp_delay(delay_seconds),
            0, // do not force apps with unsaved changes closed
            1, // restart rather than power off
            REASON,
        )
    };

    if ok != 0 {
        return Ok(());
    }
    // SAFETY: reads the thread's last error and nothing else.
    match unsafe { GetLastError() } {
        1115 => Err("Windows is already shutting down.".into()),
        code => Err(format!("Windows refused the restart (error {code}).")),
    }
}

/// Cancels a restart countdown that has not finished.
pub fn cancel_restart() -> Result<(), String> {
    enable_shutdown_privilege()?;
    // SAFETY: a null machine name means this computer.
    if unsafe { AbortSystemShutdownW(std::ptr::null()) } != 0 {
        Ok(())
    } else {
        Err("There is no restart to cancel, or it has already begun.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_countdown_stays_within_sane_limits() {
        assert_eq!(clamp_delay(0), MIN_DELAY);
        assert_eq!(clamp_delay(60), 60);
        assert_eq!(clamp_delay(100_000), MAX_DELAY);
    }

    #[test]
    fn the_reason_is_a_planned_application_restart() {
        assert_ne!(REASON & SHTDN_REASON_FLAG_PLANNED, 0);
        assert_eq!(REASON & 0x00ff_0000, SHTDN_REASON_MAJOR_APPLICATION);
    }

    /// Starts a ten minute countdown and cancels it straight away. It shows
    /// Windows' restart dialog for a moment, so it only runs on request:
    /// cargo test -- --ignored restart_can_be_scheduled_and_cancelled
    #[test]
    #[ignore]
    fn restart_can_be_scheduled_and_cancelled() {
        request_restart(MAX_DELAY).expect("the restart should be accepted");
        cancel_restart().expect("the countdown should cancel");
    }
}
