$ErrorActionPreference = 'Stop'

$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$exe = Join-Path $toolsDir 'ShaderSweep.exe'

# The download is checked against this SHA-256 before Chocolatey keeps it.
$packageArgs = @{
    packageName    = $env:ChocolateyPackageName
    fileFullPath   = $exe
    url64bit       = 'https://github.com/Kkthnx/ShaderSweep/releases/download/2.1.0/ShaderSweep-2.1.0.exe'
    checksum64     = '191E2194DED1E89A6A7ACC071D4A852B66D03E55C27586D54012F8F941267BBA'
    checksumType64 = 'sha256'
}
Get-ChocolateyWebFile @packageArgs

# Chocolatey makes a command for the exe by itself. This adds the Start menu entry.
$shortcut = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\ShaderSweep.lnk'
Install-ChocolateyShortcut -ShortcutFilePath $shortcut -TargetPath $exe -Description 'Clear GPU, Windows and launcher caches'
