<#
.SYNOPSIS
    Clears the NVIDIA and Windows DirectX/OpenGL shader caches.

.DESCRIPTION
    NVIDIA Shader Cache Cleanup Utility.

    Run this after installing a new GPU driver, or when games start
    crashing on launch, stuttering, flickering, or showing visual
    artifacts. Stale or corrupted shader caches are a common cause.

    What it does:
      1. Stops the NVIDIA background apps (NVIDIA App, overlay, Share,
         Broadcast, helpers) so they release their file locks.
      2. Temporarily stops the NVIDIA container services, along with
         anything that depends on them, so the caches those services
         hold open can be cleared. They are always restarted, even if
         the cleanup fails part way through.
      3. Finds every known shader cache folder and deletes its
         contents, keeping the folder so the driver refills it in
         place.
      4. Reports what was freed, per folder and in total.

    The driver and Windows rebuild the caches the next time each game
    is launched. Folders that do not exist are skipped, so the script
    is safe to run on any Windows system with or without an NVIDIA
    card.

.PARAMETER DryRun
    Preview mode. Reports what would be cleared and how much space it
    would free, without stopping any process or service and without
    deleting anything.

.PARAMETER AllUsers
    Also clear the caches belonging to every other local user profile,
    not just the current one. Requires administrator rights.

.PARAMETER IncludeD3DSCache
    Clear the Windows DirectX shader cache as well as the NVIDIA ones.
    On by default. Use -IncludeD3DSCache:$false to leave it alone.

.PARAMETER SkipServices
    Do not touch any Windows service. The cleanup still runs, but
    caches held open by the driver service are likely to be skipped or
    only partly cleared.

.PARAMETER SkipRebootSchedule
    Do not queue driver held files for deletion on the next reboot.
    Those files are then left in place and reported as still in use.

.PARAMETER NoPause
    Do not wait for a key press before exiting. Useful for automation
    or for chaining the script after a driver install.

.PARAMETER LogPath
    Write a full transcript of the run to this file. The folder is
    created if it does not exist.

.EXAMPLE
    .\NvidiaShaderCleanup.ps1
    Runs a full cleanup, prompting for admin rights if needed.

.EXAMPLE
    .\NvidiaShaderCleanup.ps1 -DryRun
    Shows what would be cleared without changing anything.

.EXAMPLE
    .\NvidiaShaderCleanup.ps1 -AllUsers -NoPause -LogPath .\cleanup.log
    Clears every local profile, writes a transcript, exits on its own.

.OUTPUTS
    Exit code 0 when every cache folder was cleared, 1 when one or
    more folders could not be fully cleared, 2 on a fatal error.

.NOTES
    Author : Kkthnx
    License: MIT
    Project: https://github.com/Kkthnx/ShaderSweep
    Requires: Windows 10 or 11, Windows PowerShell 5.1 or PowerShell 7+,
              administrator rights.
#>

# Write-Host is deliberate. This is an interactive console tool whose
# whole job is coloured progress output, not a pipeline cmdlet.
[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSAvoidUsingWriteHost", "")]
# IncludeD3DSCache defaults to on because clearing the Windows cache
# alongside the NVIDIA ones is what fixes most artifact reports.
[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSAvoidDefaultValueSwitchParameter", "")]
[CmdletBinding(SupportsShouldProcess, ConfirmImpact = "Medium")]
param (
	[switch]$DryRun,
	[switch]$AllUsers,
	[switch]$IncludeD3DSCache = $true,
	[switch]$SkipServices,
	[switch]$SkipRebootSchedule,
	[switch]$NoPause,
	[string]$LogPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Exit codes
$EXIT_OK = 0
$EXIT_PARTIAL = 1
$EXIT_FATAL = 2

# -----------------------------
# Self elevate
# -----------------------------
# Only a safety net for people who run the .ps1 directly. The .bat
# launcher elevates first, so this normally does nothing.
function Test-Administrator {
	$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
	$principal = New-Object Security.Principal.WindowsPrincipal($identity)
	return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-Administrator)) {
	Write-Host "Administrator rights are required. Requesting elevation..." -ForegroundColor Yellow

	# Relaunch under the same PowerShell edition the user started us in.
	$hostExe = (Get-Process -Id $PID).Path
	if (-not $hostExe) { $hostExe = "powershell.exe" }

	$forwarded = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $PSCommandPath)
	if ($DryRun) { $forwarded += "-DryRun" }
	if ($AllUsers) { $forwarded += "-AllUsers" }
	if (-not $IncludeD3DSCache) { $forwarded += "-IncludeD3DSCache:`$false" }
	if ($SkipServices) { $forwarded += "-SkipServices" }
	if ($SkipRebootSchedule) { $forwarded += "-SkipRebootSchedule" }
	if ($NoPause) { $forwarded += "-NoPause" }
	if ($LogPath) { $forwarded += @("-LogPath", $LogPath) }

	try {
		$elevated = Start-Process -FilePath $hostExe -Verb RunAs -ArgumentList $forwarded -PassThru -Wait
		exit $elevated.ExitCode
	}
	catch {
		Write-Host "Elevation was declined. Nothing was changed." -ForegroundColor Yellow
		exit $EXIT_FATAL
	}
}

# -----------------------------
# Transcript
# -----------------------------
$transcriptStarted = $false
if ($LogPath) {
	try {
		$logDir = Split-Path -Path $LogPath -Parent
		if ($logDir -and -not (Test-Path -LiteralPath $logDir)) {
			New-Item -ItemType Directory -Path $logDir -Force | Out-Null
		}
		Start-Transcript -Path $LogPath -Force | Out-Null
		$transcriptStarted = $true
	}
	catch {
		Write-Host "Could not start the log at $LogPath. Continuing without one." -ForegroundColor Yellow
	}
}

# -----------------------------
# Helpers
# -----------------------------
# Some cache files are held open by the kernel mode display driver
# itself, not by any process or service we can stop. Those cannot be
# deleted while Windows is running with the driver loaded, which is why
# NVIDIA's own instructions tell you to reboot. MoveFileEx with
# MOVEFILE_DELAY_UNTIL_REBOOT is the supported way to queue them, the
# same mechanism Windows installers use.
$script:CanScheduleDelete = $false
try {
	Add-Type -Namespace NvShaderCleanup -Name Native -MemberDefinition @"
[System.Runtime.InteropServices.DllImport("kernel32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode, SetLastError = true)]
[return: System.Runtime.InteropServices.MarshalAs(System.Runtime.InteropServices.UnmanagedType.Bool)]
public static extern bool MoveFileEx(string lpExistingFileName, string lpNewFileName, int dwFlags);
"@ -ErrorAction Stop
	$script:CanScheduleDelete = $true
}
catch {
	Write-Verbose "MoveFileEx is unavailable, locked files cannot be queued for reboot."
}

function Register-DeleteOnReboot {
	param ([string]$Path)

	if (-not $script:CanScheduleDelete) { return $false }

	# MOVEFILE_DELAY_UNTIL_REBOOT. A null destination means delete.
	# It has to be [NullString]::Value, because PowerShell marshals a
	# plain $null string argument as an empty string, and the call then
	# fails with ERROR_PATH_NOT_FOUND.
	try { return [NvShaderCleanup.Native]::MoveFileEx($Path, [NullString]::Value, 4) }
	catch { return $false }
}

function Format-Size {
	param ([long]$Bytes)

	if ($Bytes -ge 1GB) { return ("{0:N2} GB" -f ($Bytes / 1GB)) }
	if ($Bytes -ge 1MB) { return ("{0:N2} MB" -f ($Bytes / 1MB)) }
	if ($Bytes -ge 1KB) { return ("{0:N2} KB" -f ($Bytes / 1KB)) }
	return "$Bytes B"
}

function Get-FolderSize {
	param ([string]$Path)

	if (-not (Test-Path -LiteralPath $Path)) { return [long]0 }

	$sum = [long]0
	Get-ChildItem -LiteralPath $Path -Force -Recurse -File -ErrorAction SilentlyContinue |
		ForEach-Object { $sum += $_.Length }
	return $sum
}

# -----------------------------
# Header
# -----------------------------
function Write-Header {
	Write-Host ""
	Write-Host "=================================================" -ForegroundColor DarkGreen
	Write-Host " NVIDIA Shader Cache Cleanup Utility" -ForegroundColor Green
	Write-Host " Written by Kkthnx" -ForegroundColor Green
	Write-Host "=================================================" -ForegroundColor DarkGreen
	if ($DryRun) {
		Write-Host " DRY RUN, nothing will be deleted" -ForegroundColor Yellow
		Write-Host "=================================================" -ForegroundColor DarkGreen
	}
	Write-Host ""
}

Write-Header

# -----------------------------
# Work out which profiles to clean
# -----------------------------
# A profile root is any folder that has an AppData tree under it. The
# system profile is included because the driver service caches live
# there, and it is the reason this tool needs admin rights.
function Get-ProfileRoot {
	$roots = New-Object System.Collections.Generic.List[string]

	if ($env:USERPROFILE) { $roots.Add($env:USERPROFILE) }

	$systemRoot = $env:SystemRoot
	if (-not $systemRoot) { $systemRoot = "C:\Windows" }

	$roots.Add((Join-Path $systemRoot "System32\config\systemprofile"))
	$roots.Add((Join-Path $systemRoot "SysWOW64\config\systemprofile"))
	$roots.Add((Join-Path $systemRoot "ServiceProfiles\LocalService"))
	$roots.Add((Join-Path $systemRoot "ServiceProfiles\NetworkService"))

	if ($AllUsers -and $env:USERPROFILE) {
		$usersDir = Split-Path -Path $env:USERPROFILE -Parent
		if ($usersDir -and (Test-Path -LiteralPath $usersDir)) {
			Get-ChildItem -LiteralPath $usersDir -Directory -Force -ErrorAction SilentlyContinue |
				Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "AppData") } |
				ForEach-Object { $roots.Add($_.FullName) }
		}
	}

	return $roots | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ }
}

# Folder names that hold regenerable shader or compute cache data.
# Anything not on this list is never touched.
$cacheFolderNames = @(
	"DXCache",
	"GLCache",
	"ComputeCache",
	"OptixCache",
	"NV_Cache"
)

# Roots under a profile that NVIDIA has used across driver generations.
# Local holds DXCache and GLCache, LocalLow holds the newer
# PerDriverVersion tree, Roaming holds ComputeCache, and NVIDIA
# Corporation holds the legacy NV_Cache.
$profileRelativeRoots = @(
	"AppData\Local\NVIDIA",
	"AppData\Local\NVIDIA Corporation",
	"AppData\LocalLow\NVIDIA",
	"AppData\LocalLow\NVIDIA Corporation",
	"AppData\Roaming\NVIDIA",
	"AppData\Roaming\NVIDIA Corporation",
	"AppData\Local\Temp\NVIDIA Corporation"
)

# Finds cache folders by name instead of hard coding full paths, so
# new driver layouts such as PerDriverVersion are picked up on their
# own. The depth limit keeps the search off the rest of the disk.
function Get-CachePath {
	$found = New-Object System.Collections.Generic.List[string]

	$searchRoots = New-Object System.Collections.Generic.List[string]

	foreach ($profileRoot in Get-ProfileRoot) {
		foreach ($relative in $profileRelativeRoots) {
			$searchRoots.Add((Join-Path $profileRoot $relative))
		}
	}

	if ($env:ProgramData) {
		$searchRoots.Add((Join-Path $env:ProgramData "NVIDIA Corporation"))
		$searchRoots.Add((Join-Path $env:ProgramData "NVIDIA"))
	}

	foreach ($root in ($searchRoots | Select-Object -Unique)) {
		if (-not (Test-Path -LiteralPath $root)) { continue }

		# The root itself can be a cache folder, for example NV_Cache.
		if ($cacheFolderNames -contains (Split-Path -Path $root -Leaf)) {
			$found.Add($root)
			continue
		}

		Get-ChildItem -LiteralPath $root -Directory -Recurse -Depth 3 -Force -ErrorAction SilentlyContinue |
			Where-Object { $cacheFolderNames -contains $_.Name } |
			ForEach-Object { $found.Add($_.FullName) }
	}

	# The OpenGL cache can be moved off its default location with this
	# documented NVIDIA variable, so follow it when it is set.
	if ($env:__GL_SHADER_DISK_CACHE_PATH) {
		$glOverride = Join-Path $env:__GL_SHADER_DISK_CACHE_PATH "GLCache"
		if (Test-Path -LiteralPath $glOverride) { $found.Add($glOverride) }
	}

	if ($IncludeD3DSCache) {
		foreach ($profileRoot in Get-ProfileRoot) {
			$found.Add((Join-Path $profileRoot "AppData\Local\D3DSCache"))
			$found.Add((Join-Path $profileRoot "AppData\Local\Microsoft\D3DSCache"))
		}
	}

	# Drop nested duplicates. If a parent is already on the list there
	# is no point clearing a child of it a second time.
	$unique = $found | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ } | Sort-Object

	$result = New-Object System.Collections.Generic.List[string]
	foreach ($path in $unique) {
		$isNested = $false
		foreach ($kept in $result) {
			if ($path.StartsWith(($kept.TrimEnd("\") + "\"), [System.StringComparison]::OrdinalIgnoreCase)) {
				$isNested = $true
				break
			}
		}
		if (-not $isNested) { $result.Add($path) }
	}

	return $result
}

# -----------------------------
# Services
# -----------------------------
# These hold open handles on the system profile caches. Stopping a
# container service also stops whatever depends on it, so those
# dependents are recorded and started again in reverse order.
#
# Order matters. These must go down before the processes are touched,
# because nvcontainer.exe is the host process for the NvContainer
# services. Killing it first makes the service control manager restart
# the service straight away, which re-locks the caches we are about to
# clear.
$serviceNames = @(
	"NVDisplay.ContainerLocalSystem",
	"NvContainerLocalSystem",
	"NvContainerNetworkService"
)

function Stop-NvidiaService {
	[CmdletBinding(SupportsShouldProcess)]
	param ()

	# Restart order is the reverse of this list, so a host service has
	# to sit after its own dependents.
	$stopped = New-Object System.Collections.Generic.List[string]

	# Snapshot taken before anything is touched, so the restart pass can
	# also put back a service that went down some other way, for example
	# because its host process died.
	$wasRunning = New-Object System.Collections.Generic.List[string]
	foreach ($name in $serviceNames) {
		$running = Get-Service -Name $name -ErrorAction SilentlyContinue
		if ($running -and $running.Status -eq "Running") { $wasRunning.Add($name) }
	}

	foreach ($name in $serviceNames) {
		$service = Get-Service -Name $name -ErrorAction SilentlyContinue
		if (-not $service) { continue }
		if ($service.Status -ne "Running") { continue }

		$dependents = @(
			Get-Service -Name $name -DependentServices -ErrorAction SilentlyContinue |
				Where-Object { $_.Status -eq "Running" } |
				ForEach-Object { $_.Name }
		)

		if (-not $PSCmdlet.ShouldProcess($name, "Stop service")) { continue }

		Write-Host ("Stopping service: {0}" -f $service.DisplayName)
		try {
			Stop-Service -Name $name -Force -ErrorAction Stop

			# Dependents come back first, then the service itself.
			foreach ($dependent in $dependents) {
				if (-not $stopped.Contains($dependent)) { $stopped.Add($dependent) }
			}
			if (-not $stopped.Contains($name)) { $stopped.Add($name) }
		}
		catch {
			Write-Host ("  -> Could not stop it. Caches it holds open may be skipped.") -ForegroundColor Yellow
		}
	}

	# Anything that was running at the start but never made it onto the
	# restart list goes on the end, which means it is restarted first.
	foreach ($name in $wasRunning) {
		if (-not $stopped.Contains($name)) { $stopped.Add($name) }
	}

	return $stopped
}

function Start-NvidiaService {
	[CmdletBinding(SupportsShouldProcess)]
	param ([System.Collections.Generic.List[string]]$Names)

	if (-not $Names -or $Names.Count -eq 0) { return }

	Write-Host ""
	Write-Host "Restarting services..."

	# Reverse order so a container service is back before its dependents.
	for ($i = $Names.Count - 1; $i -ge 0; $i--) {
		$name = $Names[$i]
		$service = Get-Service -Name $name -ErrorAction SilentlyContinue
		if (-not $service) { continue }
		if ($service.Status -eq "Running") { continue }
		if ($service.StartType -eq "Disabled") {
			Write-Host ("  -> {0} is disabled, left alone" -f $name) -ForegroundColor DarkGray
			continue
		}

		if (-not $PSCmdlet.ShouldProcess($name, "Start service")) { continue }

		try {
			Start-Service -Name $name -ErrorAction Stop
			Write-Host ("  -> Started: {0}" -f $name) -ForegroundColor Green
		}
		catch {
			Write-Host ("  -> Could not start {0}. It will come back on the next reboot." -f $name) -ForegroundColor Yellow
		}
	}
}

# -----------------------------
# Processes
# -----------------------------
# Not critical and they relaunch on their own. Stopping them releases
# the handles they keep on the per user caches.
#
# nvcontainer.exe is deliberately absent. It is the host process for
# the NvContainer services, so killing it makes the service control
# manager restart the service and re-lock the caches. Those go down
# through Stop-NvidiaService instead, which is why services are
# stopped before processes.
$processNames = @(
	"NVIDIA app",
	"NVIDIA Share",
	"NVIDIA Web Helper",
	"NVIDIA Overlay",
	"NVIDIA Broadcast",
	"NVIDIA Broadcast UI",
	"nvsphelper64",
	"NvOAWrapperCache",
	"NvTelemetryContainer"
)

function Stop-NvidiaProcess {
	[CmdletBinding(SupportsShouldProcess)]
	param ()

	Write-Host "Stopping NVIDIA background processes..."

	$any = $false
	foreach ($name in $processNames) {
		$procs = @(Get-Process -Name $name -ErrorAction SilentlyContinue)
		if ($procs.Count -eq 0) { continue }
		if (-not $PSCmdlet.ShouldProcess($name, "Stop process")) { continue }

		try {
			$procs | Stop-Process -Force -ErrorAction Stop
			Write-Host ("  -> Stopped: {0}" -f $name)
			$any = $true
		}
		catch {
			Write-Host ("  -> Could not stop: {0}" -f $name) -ForegroundColor Yellow
		}
	}

	if (-not $any) { Write-Host "  -> Nothing was running" -ForegroundColor DarkGray }
}

# -----------------------------
# Cleanup
# -----------------------------
# Deletes the contents of a cache folder but keeps the folder itself,
# so the driver refills it in place. Space freed is measured as the
# difference between the size before and after, which means files that
# stayed locked are never counted as freed.
function Clear-ShaderCache {
	[CmdletBinding(SupportsShouldProcess)]
	param (
		[Parameter(Mandatory)][string]$Path,
		[switch]$Preview
	)

	$before = Get-FolderSize -Path $Path

	if ($Preview) {
		Write-Host ("[WOULD CLEAR] {0}" -f $Path)
		Write-Host ("  -> {0}" -f (Format-Size $before)) -ForegroundColor Yellow
		return [PSCustomObject]@{ Path = $Path; Freed = $before; Complete = $true; Queued = 0 }
	}

	if (-not $PSCmdlet.ShouldProcess($Path, "Delete shader cache contents")) {
		return [PSCustomObject]@{ Path = $Path; Freed = [long]0; Complete = $true; Queued = 0 }
	}

	Write-Host ("[CLEAR] {0}" -f $Path)

	Get-ChildItem -LiteralPath $Path -Force -ErrorAction SilentlyContinue |
		Remove-Item -Force -Recurse -ErrorAction SilentlyContinue

	$after = Get-FolderSize -Path $Path
	$freed = $before - $after
	if ($freed -lt 0) { $freed = [long]0 }

	$leftovers = @(Get-ChildItem -LiteralPath $Path -Force -File -Recurse -ErrorAction SilentlyContinue)
	$queued = 0

	# Whatever survived is held by the driver. Queue it for the next
	# reboot rather than telling the user to try again, which would
	# never work while the driver is loaded.
	if ($leftovers.Count -gt 0 -and -not $SkipRebootSchedule) {
		foreach ($leftover in $leftovers) {
			if (Register-DeleteOnReboot -Path $leftover.FullName) { $queued++ }
		}
	}

	$stuck = $leftovers.Count - $queued
	$complete = ($stuck -eq 0)

	if ($leftovers.Count -eq 0) {
		Write-Host ("  -> Cleared, {0} freed" -f (Format-Size $freed)) -ForegroundColor Green
	}
	elseif ($complete) {
		Write-Host ("  -> Cleared, {0} freed, {1} driver held file(s) queued for the next reboot" -f (Format-Size $freed), $queued) -ForegroundColor Green
	}
	else {
		Write-Host ("  -> Partly cleared, {0} freed, {1} file(s) still in use" -f (Format-Size $freed), $stuck) -ForegroundColor Yellow
	}

	return [PSCustomObject]@{ Path = $Path; Freed = $freed; Complete = $complete; Queued = $queued }
}

# -----------------------------
# Run
# -----------------------------
$stoppedServices = New-Object System.Collections.Generic.List[string]
$exitCode = $EXIT_OK

try {
	$cachePaths = @(Get-CachePath)

	if ($cachePaths.Count -eq 0) {
		Write-Host "No shader cache folders were found on this system." -ForegroundColor Yellow
		Write-Host "Either the NVIDIA driver is not installed, or the caches are already empty."
	}
	else {
		if (-not $DryRun) {
			# Services first. See the note above $processNames for why
			# the other order re-locks everything.
			if (-not $SkipServices) {
				$stoppedServices = Stop-NvidiaService
				Write-Host ""
			}

			Stop-NvidiaProcess

			# Windows needs a moment to release the handles.
			Start-Sleep -Seconds 2
		}

		Write-Host ""
		Write-Host ("Found {0} cache folder(s)." -f $cachePaths.Count)
		Write-Host ""

		[long]$totalFreed = 0
		$incomplete = 0
		$totalQueued = 0

		foreach ($path in $cachePaths) {
			$result = Clear-ShaderCache -Path $path -Preview:$DryRun
			$totalFreed += $result.Freed
			$totalQueued += $result.Queued
			if (-not $result.Complete) { $incomplete++ }
		}

		if ($incomplete -gt 0) { $exitCode = $EXIT_PARTIAL }
	}
}
catch {
	Write-Host ""
	Write-Host ("Something went wrong: {0}" -f $_.Exception.Message) -ForegroundColor Red
	$exitCode = $EXIT_FATAL
}
finally {
	# Runs even on a fatal error, so the machine is never left with the
	# display service stopped.
	Start-NvidiaService -Names $stoppedServices
}

# -----------------------------
# Summary
# -----------------------------
if (-not (Test-Path Variable:totalFreed)) { [long]$totalFreed = 0 }
if (-not (Test-Path Variable:incomplete)) { $incomplete = 0 }
if (-not (Test-Path Variable:totalQueued)) { $totalQueued = 0 }

Write-Host ""
Write-Host "=================================================" -ForegroundColor DarkGreen
if ($exitCode -eq $EXIT_FATAL) {
	Write-Host " Cleanup Failed" -ForegroundColor Red
}
elseif ($DryRun) {
	Write-Host " Dry Run Complete" -ForegroundColor Green
	Write-Host (" Space that would be freed: {0}" -f (Format-Size $totalFreed))
}
else {
	Write-Host " Cleanup Complete" -ForegroundColor Green
	Write-Host (" Space freed: {0}" -f (Format-Size $totalFreed))
	if ($totalQueued -gt 0) {
		Write-Host (" {0} file(s) are held by the display driver and were" -f $totalQueued) -ForegroundColor Cyan
		Write-Host " queued for deletion on your next reboot." -ForegroundColor Cyan
	}
	if ($incomplete -gt 0) {
		Write-Host (" {0} folder(s) were only partly cleared" -f $incomplete) -ForegroundColor Yellow
		Write-Host " Close your games and the NVIDIA App, then run it again." -ForegroundColor Yellow
	}
}
Write-Host "=================================================" -ForegroundColor DarkGreen
Write-Host ""

if (-not $DryRun -and $exitCode -ne $EXIT_FATAL) {
	Write-Host "NOTE:" -ForegroundColor Cyan
	Write-Host "Games may stutter or load slowly the first time after this"
	Write-Host "cleanup while their shaders are rebuilt. That is normal and"
	Write-Host "happens once per game."
	Write-Host ""
}

if ($transcriptStarted) {
	try { Stop-Transcript | Out-Null }
	catch { Write-Verbose "The transcript was already closed." }
}

if (-not $NoPause) {
	Write-Host "Press Enter to exit..."
	Read-Host | Out-Null
}

exit $exitCode
