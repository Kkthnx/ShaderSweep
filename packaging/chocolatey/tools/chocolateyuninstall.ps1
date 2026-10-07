$ErrorActionPreference = 'Stop'

$shortcut = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\ShaderSweep.lnk'
if (Test-Path -LiteralPath $shortcut) {
    Remove-Item -LiteralPath $shortcut -Force
}

# The exe and its command are removed by Chocolatey. The app's own record in
# %LOCALAPPDATA%\ShaderSweep is the user's, so it is left alone.
