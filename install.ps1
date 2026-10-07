<#
.SYNOPSIS
    Installs ShaderSweep for the current user. No administrator rights needed.

.DESCRIPTION
    Downloads a ShaderSweep release from GitHub, checks it against the
    SHA-256 hash published with that release, and installs it. If the hash
    does not match, nothing is installed.

    ShaderSweep itself asks for administrator rights when you run it,
    because some of the caches it clears belong to system accounts.

    Quick install:
        irm https://raw.githubusercontent.com/Kkthnx/ShaderSweep/main/install.ps1 | iex

    With options, run it as a script block:
        & ([scriptblock]::Create((irm https://raw.githubusercontent.com/Kkthnx/ShaderSweep/main/install.ps1))) -AddToPath

.PARAMETER Version
    A release tag such as 2.0.0, or "latest".

.PARAMETER InstallDir
    Where to install. Defaults to a per-user folder.

.PARAMETER NoShortcut
    Skip the Start menu shortcut.

.PARAMETER AddToPath
    Add the install folder to your user PATH so that "ShaderSweep" works in a
    terminal, for example "ShaderSweep --clean --preview".

.PARAMETER Uninstall
    Remove ShaderSweep, its shortcut, its PATH entry and its uninstall entry.

.PARAMETER RemoveData
    With -Uninstall, also remove %LOCALAPPDATA%\ShaderSweep, which holds the
    last-run report and the driver record.

.EXAMPLE
    .\install.ps1
    Installs the latest release.

.EXAMPLE
    .\install.ps1 -Version 2.0.0 -AddToPath
    Installs a specific release and puts it on your PATH.

.EXAMPLE
    .\install.ps1 -Uninstall
    Removes ShaderSweep.

.NOTES
    Author : Kkthnx
    License: MIT
    Project: https://github.com/Kkthnx/ShaderSweep
#>

# The parameters are read inside Invoke-Main, which the analyzer cannot see.
[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSReviewUnusedParameter", "")]
[CmdletBinding()]
param (
	[string]$Version = "latest",
	[string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\ShaderSweep"),
	[switch]$NoShortcut,
	[switch]$AddToPath,
	[switch]$Uninstall,
	[switch]$RemoveData
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$script:Repo = "Kkthnx/ShaderSweep"
$script:AppName = "ShaderSweep"
$script:UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ShaderSweep"

# Write-Host is deliberate. This is an interactive installer, and its output
# is meant for the person watching it.
function Write-Step {
	[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSAvoidUsingWriteHost", "")]
	param ([string]$Message, [string]$Color = "Gray")
	Write-Host $Message -ForegroundColor $Color
}

function Get-Release {
	param ([string]$Version)

	# Older Windows PowerShell does not offer TLS 1.2 by default.
	[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

	$tag = if ($Version -eq "latest") { "latest" } else { "tags/$Version" }
	$uri = "https://api.github.com/repos/$($script:Repo)/releases/$tag"
	Invoke-RestMethod -Uri $uri -Headers @{ "User-Agent" = "ShaderSweep-Installer" }
}

function Get-ExpectedHash {
	param ([string]$Uri)

	$text = (Invoke-RestMethod -Uri $Uri -Headers @{ "User-Agent" = "ShaderSweep-Installer" }) -as [string]
	# The file holds the hash, and may add a file name after it.
	$first = ($text.Trim() -split "\s+")[0]
	if ($first -notmatch "^[0-9A-Fa-f]{64}$") {
		throw "The published hash file is not a SHA-256 hash."
	}
	$first.ToUpperInvariant()
}

function Assert-FileHash {
	param ([string]$Path, [string]$Expected)

	$actual = (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToUpperInvariant()
	if ($actual -ne $Expected.ToUpperInvariant()) {
		throw "The download does not match its published SHA-256 hash, so it was not installed. Expected $Expected but got $actual."
	}
}

function Add-UserPath {
	[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSUseShouldProcessForStateChangingFunctions", "")]
	param ([string]$Dir)

	$current = [Environment]::GetEnvironmentVariable("Path", "User")
	$parts = @($current -split ";" | Where-Object { $_ })
	if ($parts -contains $Dir) { return $false }
	[Environment]::SetEnvironmentVariable("Path", (($parts + $Dir) -join ";"), "User")
	$true
}

function Remove-UserPath {
	[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSUseShouldProcessForStateChangingFunctions", "")]
	param ([string]$Dir)

	$current = [Environment]::GetEnvironmentVariable("Path", "User")
	$parts = @($current -split ";" | Where-Object { $_ -and ($_ -ne $Dir) })
	[Environment]::SetEnvironmentVariable("Path", ($parts -join ";"), "User")
}

function New-StartMenuShortcut {
	[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSUseShouldProcessForStateChangingFunctions", "")]
	param ([string]$Target)

	$folder = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
	$path = Join-Path $folder "ShaderSweep.lnk"
	$shell = New-Object -ComObject WScript.Shell
	$link = $shell.CreateShortcut($path)
	$link.TargetPath = $Target
	$link.WorkingDirectory = Split-Path $Target
	$link.Description = "Clear GPU, Windows and launcher caches"
	$link.Save()
	$path
}

function Stop-RunningInstance {
	[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSUseShouldProcessForStateChangingFunctions", "")]
	param ()

	$running = @(Get-Process -Name $script:AppName -ErrorAction SilentlyContinue)
	if ($running.Count -eq 0) { return }

	Write-Step "Closing the running copy of ShaderSweep..." "Yellow"
	try { $running | Stop-Process -Force -ErrorAction Stop }
	catch { throw "ShaderSweep is running with administrator rights and could not be closed. Close it and run this again." }
	Start-Sleep -Milliseconds 400
}

function Install-App {
	param ([string]$Version, [string]$Dir, [bool]$Shortcut, [bool]$Path)

	Write-Step "Finding the release..."
	$release = Get-Release -Version $Version
	$tag = $release.tag_name

	$exe = $release.assets | Where-Object { $_.name -eq "$($script:AppName)-$tag.exe" } | Select-Object -First 1
	$sum = $release.assets | Where-Object { $_.name -eq "$($script:AppName)-$tag.exe.sha256" } | Select-Object -First 1
	if (-not $exe -or -not $sum) {
		throw "Release $tag has no downloadable exe with a hash file."
	}

	$work = Join-Path ([IO.Path]::GetTempPath()) ("ShaderSweep-" + [Guid]::NewGuid().ToString("N"))
	New-Item -ItemType Directory -Path $work | Out-Null
	try {
		$file = Join-Path $work "ShaderSweep.exe"
		Write-Step "Downloading $($script:AppName) $tag..."
		Invoke-WebRequest -Uri $exe.browser_download_url -OutFile $file -UseBasicParsing -Headers @{ "User-Agent" = "ShaderSweep-Installer" }

		Write-Step "Checking the SHA-256 hash..."
		Assert-FileHash -Path $file -Expected (Get-ExpectedHash -Uri $sum.browser_download_url)

		Stop-RunningInstance
		New-Item -ItemType Directory -Path $Dir -Force | Out-Null
		$target = Join-Path $Dir "ShaderSweep.exe"
		Copy-Item -Path $file -Destination $target -Force
	}
	finally {
		Remove-Item -Path $work -Recurse -Force -ErrorAction SilentlyContinue
	}

	$notes = @()
	if ($Shortcut) { $notes += "Start menu shortcut: " + (New-StartMenuShortcut -Target $target) }
	if ($Path) {
		if (Add-UserPath -Dir $Dir) { $notes += "Added to your PATH. Open a new terminal to use it." }
	}

	# Lets Windows list it under Installed apps, and remove it from there.
	$remover = Join-Path $Dir "uninstall.ps1"
	@'
param ([switch]$RemoveData)
$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$cur = [Environment]::GetEnvironmentVariable("Path", "User")
$keep = @($cur -split ";" | Where-Object { $_ -and ($_ -ne $dir) })
[Environment]::SetEnvironmentVariable("Path", ($keep -join ";"), "User")
Remove-Item (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\ShaderSweep.lnk") -Force -ErrorAction SilentlyContinue
Remove-Item "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ShaderSweep" -Recurse -Force -ErrorAction SilentlyContinue
if ($RemoveData) { Remove-Item (Join-Path $env:LOCALAPPDATA "ShaderSweep") -Recurse -Force -ErrorAction SilentlyContinue }
Start-Process powershell -WindowStyle Hidden -ArgumentList "-NoProfile","-Command","Start-Sleep 2; Remove-Item -LiteralPath '$dir' -Recurse -Force"
'@ | Set-Content -Path $remover -Encoding UTF8

	New-Item -Path $script:UninstallKey -Force | Out-Null
	$props = @{
		DisplayName     = $script:AppName
		DisplayVersion  = $tag
		Publisher       = "Kkthnx"
		InstallLocation = $Dir
		DisplayIcon     = $target
		URLInfoAbout    = "https://github.com/$($script:Repo)"
		UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$remover`""
		NoModify        = 1
		NoRepair        = 1
	}
	foreach ($name in $props.Keys) {
		New-ItemProperty -Path $script:UninstallKey -Name $name -Value $props[$name] -Force | Out-Null
	}

	Write-Step ""
	Write-Step "Installed $($script:AppName) $tag" "Green"
	Write-Step "  $target"
	foreach ($note in $notes) { Write-Step "  $note" }
	Write-Step ""
	Write-Step "Run it from the Start menu, or open a terminal and type:"
	Write-Step "  ""$target"" --clean --preview" "Cyan"
}

function Uninstall-App {
	param ([string]$Dir, [bool]$Data)

	Stop-RunningInstance
	Remove-UserPath -Dir $Dir
	Remove-Item (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\ShaderSweep.lnk") -Force -ErrorAction SilentlyContinue
	Remove-Item $script:UninstallKey -Recurse -Force -ErrorAction SilentlyContinue
	if (Test-Path -LiteralPath $Dir) { Remove-Item -LiteralPath $Dir -Recurse -Force }
	if ($Data) {
		Remove-Item (Join-Path $env:LOCALAPPDATA "ShaderSweep") -Recurse -Force -ErrorAction SilentlyContinue
	}
	Write-Step "Removed $($script:AppName)." "Green"
}

function Invoke-Main {
	if ($Uninstall) {
		Uninstall-App -Dir $InstallDir -Data $RemoveData.IsPresent
	}
	else {
		Install-App -Version $Version -Dir $InstallDir -Shortcut (-not $NoShortcut) -Path $AddToPath.IsPresent
	}
}

# Dot sourcing the file loads the functions without installing anything,
# which is how the checks in this repository test them.
if ($MyInvocation.InvocationName -ne ".") {
	try { Invoke-Main }
	catch {
		Write-Step ""
		Write-Step "Install failed: $($_.Exception.Message)" "Red"
		exit 1
	}
}
