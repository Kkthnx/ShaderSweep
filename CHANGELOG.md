# Changelog

## 2.1.1

### Fixed

- Clicking a row near the bottom of the list, such as Discord, could scroll the whole window out of view and leave it looking blank. The invisible checkbox behind each row was not anchored to its row, so focusing it made the browser scroll the entire page to reach it. It now sits inside its own row, and a guard puts the page straight back if anything ever scrolls it. Checked by focusing every row in a long list and confirming the page never moves.

## 2.1.0

### Added

- **Live progress.** While it cleans, the window shows the space freed counting up, a progress bar with a percentage, files removed, elapsed time, a streaming feed of the file being deleted, and a live total on each row. The scan shows which cache it is checking.
- **Cancel.** Stops a clean between files. What is already gone stays gone, and a stopped run is never recorded as a finished clean.
- **Game launcher caches.** Discord (including PTB and Canary), Epic Games Launcher, Battle.net, the Steam client web cache, the EA app, Ubisoft Connect, GOG Galaxy and the World of Warcraft `Cache` folder. Each is matched by the tail of its path, so logins, settings, `WTF`, `Interface` and the games themselves are never in scope.
- **Running app detection.** A row for a program that is open right now starts unticked, says which app to close, and is skipped by default in headless mode. Launcher files are never queued for a restart.
- **Windows housekeeping**, with the risky rows off by default: temporary files (only files a day old), the thumbnail cache, Delivery Optimization files, Windows Update downloads, Prefetch, error reports and crash dumps, GPU crash dumps, the Recycle Bin and the Event Viewer logs. The Security log is never cleared.
- **Restart now.** Asks you to save your work, then starts a 60 second countdown you can cancel. It uses the Windows restart API with force turned off, so an app with unsaved changes makes Windows stop and ask. It deliberately avoids `shutdown /r /t`, which Microsoft documents as implying `/f`.
- **Restart verification.** The files queued for deletion are remembered, and on the next launch the app tells you how many Windows removed and how many were still there.
- **Install from a terminal.** A PowerShell installer that checks the SHA-256 hash and needs no administrator rights, a Scoop manifest, and a Chocolatey package. The release workflow keeps the manifests pointing at the newest release.
- Links to Kkthnx's GitHub and website in the app and the README, opened from a fixed list in the backend.
- `--include` for headless mode, to add rows that start off.

### Changed

- Rows are grouped into shader caches, launchers and housekeeping. Rows with nothing to clear in the last two are hidden.
- The size shown for a row is only what a clean can really remove, so skipped recent files are not promised.
- The driver update notice now says that drivers ignore shaders built for an older version, so cleaning mainly reclaims space and helps when something is wrong.

### Safety

- Allow list rules are now the tail of a path, so a bare `Cache` folder is never trusted on its own.
- The Explorer folder is only ever cleared through a filter that names the thumbnail database files.
- Tests for the allow list now name the folders beside each cache that must never match.

### Project

- Rust backend with 99 tests, including complete scan and clean runs against a made up machine.

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
