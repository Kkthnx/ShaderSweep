$ErrorActionPreference = 'Stop'

$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$exe = Join-Path $toolsDir 'ShaderSweep.exe'

# The download is checked against this SHA-256 before Chocolatey keeps it.
$packageArgs = @{
    packageName    = $env:ChocolateyPackageName
    fileFullPath   = $exe
    url64bit       = 'https://github.com/Kkthnx/ShaderSweep/releases/download/2.3.1/ShaderSweep-2.3.1.exe'
    checksum64     = 'E76B7D3AF3728C95A60C131CD75F156425FF57A4AF88B6E454652CCFEE6F8D2D'
    checksumType64 = 'sha256'
}
Get-ChocolateyWebFile @packageArgs

# Chocolatey makes a command for the exe by itself. This adds the Start menu entry.
$shortcut = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\ShaderSweep.lnk'
Install-ChocolateyShortcut -ShortcutFilePath $shortcut -TargetPath $exe -Description 'Clear GPU, Windows and launcher caches'
