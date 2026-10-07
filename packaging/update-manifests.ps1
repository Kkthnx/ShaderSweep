<#
.SYNOPSIS
    Points the Scoop manifest and the Chocolatey package at a release.

.DESCRIPTION
    Run by the release workflow after a release is published, so the package
    files always name the newest exe and its SHA-256 hash. It can also be run
    by hand.

.PARAMETER Version
    The release tag, for example 2.1.0.

.PARAMETER Hash
    The SHA-256 hash of ShaderSweep-<Version>.exe.

.PARAMETER Root
    The repository root. Defaults to the folder above this script.
#>
[CmdletBinding()]
param (
	[Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
	[Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$Hash,
	[string]$Root = (Split-Path -Parent $PSScriptRoot)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$hash = $Hash.ToUpperInvariant()
$url = "https://github.com/Kkthnx/ShaderSweep/releases/download/$Version/ShaderSweep-$Version.exe"
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Set-Text {
	[Diagnostics.CodeAnalysis.SuppressMessageAttribute("PSUseShouldProcessForStateChangingFunctions", "")]
	param ([string]$Path, [string]$Text)
	[IO.File]::WriteAllText($Path, $Text, $utf8)
}

# Scoop. Edited as text so the file keeps its layout.
$scoop = Join-Path $Root "bucket\shadersweep.json"
$text = [IO.File]::ReadAllText($scoop)
$text = $text -replace '("version":\s*")[^"]+(")', "`${1}$Version`${2}"
$text = $text -replace '("url":\s*")https://github\.com/[^"]*/releases/download/\d+\.\d+\.\d+/ShaderSweep-\d+\.\d+\.\d+\.exe(#/ShaderSweep\.exe",\s*"hash":\s*")[0-9A-Fa-f]{64}(")', "`${1}$url`${2}$hash`${3}"
Set-Text $scoop $text

# Chocolatey.
$nuspec = Join-Path $Root "packaging\chocolatey\shadersweep.nuspec"
$text = [IO.File]::ReadAllText($nuspec)
$text = $text -replace '(<version>)[^<]+(</version>)', "`${1}$Version`${2}"
Set-Text $nuspec $text

$install = Join-Path $Root "packaging\chocolatey\tools\chocolateyinstall.ps1"
$text = [IO.File]::ReadAllText($install)
$text = $text -replace "(url64bit\s*=\s*')[^']+(')", "`${1}$url`${2}"
$text = $text -replace "(checksum64\s*=\s*')[0-9A-Fa-f]{64}(')", "`${1}$hash`${2}"
Set-Text $install $text

Write-Output "Package files now point at $Version."
