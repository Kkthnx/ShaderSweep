$ErrorActionPreference = 'Stop'

$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$exe = Join-Path $toolsDir 'ShaderSweep.exe'

# The download is checked against this SHA-256 before Chocolatey keeps it.
$packageArgs = @{
    packageName    = $env:ChocolateyPackageName
    fileFullPath   = $exe
    url64bit       = 'https://github.com/Kkthnx/ShaderSweep/releases/download/2.0.0/ShaderSweep-2.0.0.exe'
    checksum64     = '54E30AEF01392D84A7D620E1D5557C19C5FF85DCE2A8AC99354EF82F63267CD4'
    checksumType64 = 'sha256'
}
Get-ChocolateyWebFile @packageArgs

# Chocolatey makes a command for the exe by itself. This adds the Start menu entry.
$shortcut = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\ShaderSweep.lnk'
Install-ChocolateyShortcut -ShortcutFilePath $shortcut -TargetPath $exe -Description 'Clear GPU, Windows and launcher caches'
