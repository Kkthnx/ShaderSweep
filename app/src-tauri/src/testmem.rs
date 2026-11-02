//! Memory readings for the benchmark tests.

use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

pub struct Reading {
    pub peak_mb: f64,
    /// Memory this process has committed that is not shared. A leak shows here.
    pub private_mb: f64,
    /// Open handles. A handle that is never closed makes this climb.
    pub handles: u32,
}

pub fn read() -> Reading {
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    let mut handles = 0;
    unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut handles) };
    let mb = |b: usize| b as f64 / (1024.0 * 1024.0);
    Reading {
        peak_mb: mb(counters.PeakWorkingSetSize),
        private_mb: mb(counters.PagefileUsage),
        handles,
    }
}
