# Changelog

## 2.0.0

ShaderSweep, a small Windows app that replaces clicking through the script. The PowerShell script stays in `NvidiaShaderCleanup/` for automation and is unchanged.

### Added

- A single window that scans first and shows every cache with its size and exact folders before anything is deleted.
- AMD, Intel, Windows DirectX and Steam caches alongside NVIDIA, plus an opt-in row for driver installer leftovers in `C:\AMD` and `C:\NVIDIA\DisplayDriver`.
- Driver awareness. It records each GPU's driver version at the last clean and tells you when it changes, with honest wording about when cleaning is actually worth it and a note about NVIDIA's Auto Shader Compilation.
- Names the program holding any file it could not remove, using the Windows Restart Manager.
- Notices when files from an earlier clean are still waiting for a restart, and never queues the same file twice.
- Free disk space for the system drive, a plain explanation under every cache row, and a **Copy report** button.
- **Preview only** mode that measures and deletes nothing.
- Every user profile on the machine is covered, not just the current one, along with the system and service profiles.
- Headless mode (`--clean`, `--preview`, `--only`, `--installers`, `--no-restart-queue`) with exit codes, for chaining after a driver install.
- The result of each real clean is saved to `last-run.txt`, and the app shows how much it has freed in total.
- An **Open** button beside each folder, which only opens folders the app itself manages.
- A tip that NVIDIA caps its cache at 16 GB by default and where to change it.

### Changed

- It no longer stops any NVIDIA service or process. Testing showed stopping them made no difference to which files stayed locked, and the restart queue covers the rest, so the riskiest part of the script is gone.
- Freed space is counted per file actually deleted.

### Safety

- The interface only sends a row name. The backend rebuilds every path and checks it against a fixed allow list, refusing drive roots, `..` paths and junctions.
- Links inside a cache are removed without being followed, covered by a test.

### Project

- Rust backend with 54 tests, including full scan and clean runs against a made up machine, formatted with rustfmt and clean under clippy with warnings denied.
- CI builds and tests the app on every push, and a tagged release publishes the exe, a zip and a SHA-256 hash.

## 1.2.0

### Fixed

- Services are now stopped **before** the background processes. `nvcontainer.exe` is the host process for the NvContainer services, so killing it first made the service control manager restart the service immediately and re-lock the caches the tool was about to clear. `nvcontainer` has been removed from the process kill list for the same reason.
- Services that were running at the start are restarted even if they went down some other way, for example because their host process died.

### Added

- A handful of `.nvph` files are held open by the kernel mode display driver itself. No process or service can release them, so re-running the tool could never clear them. They are now queued for deletion on the next reboot with `MoveFileEx` and `MOVEFILE_DELAY_UNTIL_REBOOT`, the same mechanism Windows installers use. This is why NVIDIA's own instructions tell you to reboot.
- `-SkipRebootSchedule` leaves those files alone instead of queueing them.
- A folder whose only leftovers were queued no longer counts as a failure, so the tool stops telling you to close games that are not running.

## 1.1.0

### Added

- Cache folders are now discovered by name under the known NVIDIA roots instead of being hard coded, so new driver layouts such as `PerDriverVersion` are found without a code change.
- `%APPDATA%\NVIDIA\ComputeCache`, `OptixCache`, the `SysWOW64` system profile, and the `ServiceProfiles` accounts are now covered. These were being missed.
- `%__GL_SHADER_DISK_CACHE_PATH%\GLCache` is cleared when that NVIDIA variable has been used to move the OpenGL cache.
- `-AllUsers` clears every local user profile, not just the current one.
- `-IncludeD3DSCache` (on by default) to control whether the Windows DirectX cache is cleared.
- `-SkipServices` to run without touching any Windows service.
- `-LogPath` writes a full transcript of the run.
- `-WhatIf` and `-Confirm` support.
- Exit codes: `0` cleared, `1` partly cleared, `2` fatal error or declined elevation.
- A GitHub Actions workflow that runs PSScriptAnalyzer on every push.

### Fixed

- Stopped services are now restarted from a `finally` block, so a failure part way through can no longer leave the machine with the display service stopped.
- Services that were stopped as dependencies are recorded and started again in reverse order. Previously only the one named service came back.
- `NvContainerLocalSystem` and `NvContainerNetworkService` are handled, not just `NVDisplay.ContainerLocalSystem`.
- Space freed is measured as the size before minus the size after, so files that stayed locked are no longer counted as freed.
- The launcher passes its arguments through to the script, quotes paths that contain spaces, and returns the script's exit code.
- Elevation waits for the elevated run and returns its exit code instead of exiting immediately.

## 1.0.0

- First release.
