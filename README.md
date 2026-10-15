# ShaderSweep

[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6)](https://github.com/Kkthnx/ShaderSweep)
[![Release](https://img.shields.io/github/v/release/Kkthnx/ShaderSweep?color=38bdf8)](https://github.com/Kkthnx/ShaderSweep/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-38bdf8)](LICENSE)

Clear NVIDIA, AMD, Intel, Windows and Steam shader caches, plus game launcher and Windows clutter, in one click. It also watches your GPU driver version and tells you when it changes, so you know when a clean might be worth it.

## What it does

- Scans first. Every cache shows its size and exact folders before anything is deleted.
- Clears shader caches for NVIDIA, AMD, Intel, Windows DirectX, and Steam, plus Chromium-style caches for Discord, Epic, Battle.net, Steam, the EA app, Ubisoft Connect, GOG Galaxy, and World of Warcraft.
- Remembers each GPU's driver version from your last clean and tells you when it changes.
- Counts freed space from files it really deleted. Locked files are queued for the next restart, and it reports whether they were removed.
- Runs headless with `--clean`, so you can chain it after a driver install or a scheduled task. No installer; the app asks for admin rights when you run it.

## Get it

Run this in PowerShell (it checks the download against its published SHA-256 hash first):

```powershell
irm https://raw.githubusercontent.com/Kkthnx/ShaderSweep/main/install.ps1 | iex
```

Or grab the `ShaderSweep-<version>.zip` or single `.exe` from the [Releases page](https://github.com/Kkthnx/ShaderSweep/releases/latest). The exe is not code signed yet, so Windows SmartScreen may warn you the first time. The source is all here, and CI builds the release from it.

## Quick start

1. Run ShaderSweep. It asks for administrator rights when it starts.
2. Scan to see every cache it found, with sizes and exact folders.
3. Untick anything you want to keep. Running apps are unticked for you.
4. Press Clean. If files are waiting for a restart, it asks before rebooting.

## Support the work

Free forever. If it saved you time, chip in:

- [Ko-fi](https://ko-fi.com/kkthnx)
- [Buy Me a Coffee](https://buymeacoffee.com/kkthnx)
- [PayPal](https://www.paypal.com/paypalme/kkthnxtv)

Every bit goes back into building more.

## Community

- Glitch Haven Discord: https://discord.gg/FaVGhTYmck
- Website: https://kkthnx.com

## License

MIT. See [LICENSE](LICENSE).
