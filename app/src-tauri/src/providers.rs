//! What the app knows how to clean. Each provider turns the machine context
//! into a list of folders, and `safety` decides whether each one may be
//! touched. Adding something means adding one entry here and, if it is a new
//! kind of folder, one rule in `safety`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::context::Context;
use crate::discovery::{drop_nested, find_named};
use crate::fsutil::Eligible;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Empty one or more folders.
    Folders,
    /// The Recycle Bin, through the Shell.
    RecycleBin,
    /// The Event Viewer logs, through the Event Log service.
    EventLogs,
}

pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    /// `shaders`, `launchers` or `housekeeping`. The window groups rows by it.
    pub group: &'static str,
    pub kind: Kind,
    /// Matches `Adapter::vendor`, so a driver change can point at its row.
    pub vendor: Option<&'static str>,
    pub default_on: bool,
    /// True for shader caches, which is what a "last cleaned" record tracks.
    pub is_shader_cache: bool,
    /// Programs whose running means some files will be locked.
    pub processes: &'static [&'static str],
    /// May files that stay locked be queued for the next restart? Right for
    /// driver caches, wrong for a launcher's cache.
    pub queue_ok: bool,
    /// Leave files changed within this many hours alone.
    pub min_age_hours: Option<u64>,
    /// Remove only files whose name passes. Folders are not entered.
    pub file_filter: Option<fn(&str) -> bool>,
    pub note: Option<&'static str>,
    /// A plain warning shown on the row itself, for anything hard to undo.
    pub caution: Option<&'static str>,
    pub about: &'static str,
    pub resolve: fn(&Context) -> Vec<PathBuf>,
}

impl Provider {
    pub fn eligible(&self) -> Eligible {
        Eligible {
            min_age: self.min_age_hours.map(|h| Duration::from_secs(h * 3600)),
            file_filter: self.file_filter,
        }
    }
}

// ---- helpers ---------------------------------------------------------------

fn under_profiles(ctx: &Context, relative: &[&str]) -> Vec<PathBuf> {
    ctx.profiles
        .iter()
        .flat_map(|p| relative.iter().map(move |r| p.join(r)))
        .collect()
}

fn existing(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|p| p.is_dir()).collect()
}

fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ---- shader caches ---------------------------------------------------------

fn nvidia(ctx: &Context) -> Vec<PathBuf> {
    let mut roots = under_profiles(
        ctx,
        &[
            r"AppData\Local\NVIDIA",
            r"AppData\Local\NVIDIA Corporation",
            r"AppData\LocalLow\NVIDIA",
            r"AppData\LocalLow\NVIDIA Corporation",
            r"AppData\Roaming\NVIDIA",
            r"AppData\Roaming\NVIDIA Corporation",
            r"AppData\Local\Temp\NVIDIA Corporation",
        ],
    );
    roots.push(ctx.program_data.join("NVIDIA Corporation"));
    roots.push(ctx.program_data.join("NVIDIA"));

    let mut found = find_named(
        &roots,
        &[
            "DXCache",
            "GLCache",
            "ComputeCache",
            "OptixCache",
            "NV_Cache",
        ],
        3,
    );

    if let Some(moved) = &ctx.gl_override {
        let gl = moved.join("GLCache");
        if gl.is_dir() {
            found.push(gl);
        }
    }

    drop_nested(found)
}

fn amd(ctx: &Context) -> Vec<PathBuf> {
    let roots = under_profiles(ctx, &[r"AppData\Local\AMD"]);
    find_named(
        &roots,
        &[
            "DX9Cache", "DxCache", "DxcCache", "OglCache", "GLCache", "VkCache",
        ],
        1,
    )
}

fn intel(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[r"AppData\LocalLow\Intel\ShaderCache"],
    ))
}

fn windows_d3d(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(ctx, &[r"AppData\Local\D3DSCache"]))
}

fn steam_shaders(ctx: &Context) -> Vec<PathBuf> {
    existing(
        ctx.steam_libraries
            .iter()
            .map(|lib| lib.join("steamapps").join("shadercache"))
            .collect(),
    )
}

// ---- game launchers --------------------------------------------------------

const DISCORD_APPS: &[&str] = &[
    "discord",
    "discordptb",
    "discordcanary",
    "discorddevelopment",
];

fn discord(ctx: &Context) -> Vec<PathBuf> {
    let roots = existing(
        ctx.profiles
            .iter()
            .flat_map(|p| {
                DISCORD_APPS
                    .iter()
                    .map(move |app| p.join("AppData").join("Roaming").join(app))
            })
            .collect(),
    );
    find_named(
        &roots,
        &[
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
        ],
        1,
    )
}

/// Epic names its caches `webcache`, `webcache_4147`, `webcache_4430` and so
/// on, one per embedded browser version.
fn epic(ctx: &Context) -> Vec<PathBuf> {
    under_profiles(ctx, &[r"AppData\Local\EpicGamesLauncher\Saved"])
        .iter()
        .flat_map(|saved| child_dirs(saved))
        .filter(|dir| name_of(dir).to_ascii_lowercase().starts_with("webcache"))
        .collect()
}

fn battlenet(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[
            r"AppData\Local\Battle.net\Cache",
            r"AppData\Local\Battle.net\BrowserCaches",
        ],
    ))
}

fn steam_client(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(ctx, &[r"AppData\Local\Steam\htmlcache"]))
}

fn ea(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[r"AppData\Local\Electronic Arts\EA Desktop\Cache"],
    ))
}

fn ubisoft(ctx: &Context) -> Vec<PathBuf> {
    let mut paths = under_profiles(ctx, &[r"AppData\Local\Ubisoft Game Launcher\cache"]);
    paths.extend(ctx.program_files.iter().map(|pf| {
        pf.join("Ubisoft")
            .join("Ubisoft Game Launcher")
            .join("cache")
    }));
    existing(paths)
}

fn gog(ctx: &Context) -> Vec<PathBuf> {
    let mut paths = under_profiles(ctx, &[r"AppData\Local\GOG.com\Galaxy\webcache"]);
    paths.push(
        ctx.program_data
            .join("GOG.com")
            .join("Galaxy")
            .join("webcache"),
    );
    existing(paths)
}

/// Each game folder such as `_retail_` or `_classic_era_` keeps a `Cache`
/// folder. Blizzard's own troubleshooting says to remove it. The `Data`,
/// `WTF` and `Interface` folders beside it are never touched.
fn wow(ctx: &Context) -> Vec<PathBuf> {
    ctx.wow_roots
        .iter()
        .flat_map(|root| child_dirs(root))
        .filter(|dir| {
            let name = name_of(dir);
            name.len() > 2 && name.starts_with('_') && name.ends_with('_')
        })
        .map(|dir| dir.join("Cache"))
        .filter(|cache| cache.is_dir())
        .collect()
}

// ---- Windows housekeeping --------------------------------------------------

fn temp_files(ctx: &Context) -> Vec<PathBuf> {
    let mut paths = under_profiles(ctx, &[r"AppData\Local\Temp"]);
    paths.push(ctx.windows_dir.join("Temp"));
    existing(paths)
}

fn is_thumbnail_database(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("thumbcache_") && name.ends_with(".db")
}

fn thumbnails(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[r"AppData\Local\Microsoft\Windows\Explorer"],
    ))
}

fn delivery_optimization(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[r"AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"],
    ))
}

fn windows_update(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![ctx
        .windows_dir
        .join("SoftwareDistribution")
        .join("Download")])
}

fn prefetch(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![ctx.windows_dir.join("Prefetch")])
}

fn error_reports(ctx: &Context) -> Vec<PathBuf> {
    let mut paths = under_profiles(ctx, &[r"AppData\Local\CrashDumps"]);
    let wer = ctx
        .program_data
        .join("Microsoft")
        .join("Windows")
        .join("WER");
    paths.push(wer.join("ReportArchive"));
    paths.push(wer.join("ReportQueue"));
    existing(paths)
}

fn gpu_dumps(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![ctx.windows_dir.join("LiveKernelReports")])
}

fn installers(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![
        ctx.system_drive.join("AMD"),
        ctx.system_drive.join("NVIDIA").join("DisplayDriver"),
    ])
}

/// The Recycle Bin and the event logs are not folders this app resolves, so
/// they have nothing to list. The engine measures and clears them directly.
fn nothing(_: &Context) -> Vec<PathBuf> {
    Vec::new()
}

const SHADER_ABOUT: &str = "Compiled shader binaries for your exact GPU and driver. The driver rebuilds them the next time a game needs them, so the first launch afterwards can be slower.";

pub const PROVIDERS: &[Provider] = &[
    // ---- shader caches ----
    Provider {
        id: "nvidia",
        label: "NVIDIA shader cache",
        blurb: "DirectX, OpenGL and compute caches written by the NVIDIA driver.",
        group: "shaders",
        kind: Kind::Folders,
        vendor: Some("nvidia"),
        default_on: true,
        is_shader_cache: true,
        processes: &[],
        queue_ok: true,
        min_age_hours: None,
        file_filter: None,
        note: Some(
            "A few index files are held by the display driver. Those are queued for your next restart.",
        ),
        caution: None,
        about: "Compiled shader binaries for your exact GPU and driver. The driver rebuilds them the next time a game needs them, so the first launch afterwards can be slower. NVIDIA caps this cache at 16 GB by default. You can change the limit in the NVIDIA App under Graphics, Global Settings, Shader Cache Size.",
        resolve: nvidia,
    },
    Provider {
        id: "amd",
        label: "AMD shader cache",
        blurb: "DirectX 9, 11, 12, OpenGL and Vulkan caches written by the AMD driver.",
        group: "shaders",
        kind: Kind::Folders,
        vendor: Some("amd"),
        default_on: true,
        is_shader_cache: true,
        processes: &[],
        queue_ok: true,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: SHADER_ABOUT,
        resolve: amd,
    },
    Provider {
        id: "intel",
        label: "Intel shader cache",
        blurb: "Shader cache written by the Intel graphics driver.",
        group: "shaders",
        kind: Kind::Folders,
        vendor: Some("intel"),
        default_on: true,
        is_shader_cache: true,
        processes: &[],
        queue_ok: true,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: SHADER_ABOUT,
        resolve: intel,
    },
    Provider {
        id: "windows",
        label: "Windows DirectX cache",
        blurb: "The shader cache Windows keeps for every GPU. Disk Cleanup lists it too.",
        group: "shaders",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: true,
        processes: &[],
        queue_ok: true,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "Shaders Windows compiled for DirectX games and apps on any GPU. Windows versions it by driver and rebuilds it on demand.",
        resolve: windows_d3d,
    },
    Provider {
        id: "steam",
        label: "Steam shader cache",
        blurb: "Pre-compiled shaders Steam stores next to your games, in every library.",
        group: "shaders",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: true,
        processes: &[],
        queue_ok: true,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "Shaders Steam downloaded or compiled for a game. Steam fetches or rebuilds them the next time that game starts. On Windows these folders are usually small.",
        resolve: steam_shaders,
    },
    // ---- game launchers ----
    Provider {
        id: "discord",
        label: "Discord cache",
        blurb: "Saved images, media, scripts and GPU data from Discord, including PTB and Canary.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &[
            "Discord.exe",
            "DiscordPTB.exe",
            "DiscordCanary.exe",
            "DiscordDevelopment.exe",
        ],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "Discord's Cache, Code Cache and GPU cache folders. Clearing them does not log you out or touch messages, servers or friends, which live on Discord's servers. Close Discord first for a full clean.",
        resolve: discord,
    },
    Provider {
        id: "epic",
        label: "Epic Games Launcher cache",
        blurb: "The launcher's saved web pages and images.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &["EpicGamesLauncher.exe", "EpicWebHelper.exe"],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "The webcache folders Epic's own support page tells you to delete to fix launcher display problems and slow downloads. Installed games and progress are not affected. Close the launcher first.",
        resolve: epic,
    },
    Provider {
        id: "battlenet",
        label: "Battle.net cache",
        blurb: "The Battle.net app's saved web content and downloaded data.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &["Battle.net.exe"],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "Only the Cache and BrowserCaches folders. Your account folder, game installs and the Agent that manages updates are left alone. Close Battle.net first.",
        resolve: battlenet,
    },
    Provider {
        id: "steamclient",
        label: "Steam client web cache",
        blurb: "The saved pages and images behind Steam's store, library and overlay.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &["steam.exe", "steamwebhelper.exe"],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "The htmlcache folder, which is what Steam's own Delete web browser cache button in Settings clears. Your games are not affected. Close Steam first.",
        resolve: steam_client,
    },
    Provider {
        id: "ea",
        label: "EA app cache",
        blurb: "Saved content from the EA app.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &["EADesktop.exe", "EABackgroundService.exe"],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "The EA Desktop cache folder. EA's own App recovery option clears the same cache. Close the EA app first.",
        resolve: ea,
    },
    Provider {
        id: "ubisoft",
        label: "Ubisoft Connect cache",
        blurb: "Saved web content from Ubisoft Connect.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &["UbisoftConnect.exe", "upc.exe", "UplayWebCore.exe"],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "Only the cache folders. The games and savegames folders next to them are never touched. Close Ubisoft Connect first.",
        resolve: ubisoft,
    },
    Provider {
        id: "gog",
        label: "GOG Galaxy cache",
        blurb: "Saved web content from GOG Galaxy.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &["GalaxyClient.exe"],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "The webcache folders, which hold Galaxy's browser data. Installed games and saves are not affected. Close Galaxy first.",
        resolve: gog,
    },
    Provider {
        id: "wow",
        label: "World of Warcraft cache",
        blurb: "The Cache folder of each game version, such as retail, Classic and the PTR.",
        group: "launchers",
        kind: Kind::Folders,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[
            "Wow.exe",
            "WowClassic.exe",
            "WowT.exe",
            "WowB.exe",
            "WowClassicT.exe",
        ],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: Some(
            "The first login afterwards re-downloads item and quest data, so it loads slower once.",
        ),
        caution: None,
        about: "Blizzard's troubleshooting steps start with removing the Cache folder, and the game rebuilds it. WTF (your settings) and Interface (your add-ons) are never touched, and neither is the Data folder that holds the game. Close the game first.",
        resolve: wow,
    },
    // ---- Windows housekeeping ----
    Provider {
        id: "temp",
        label: "Temporary files",
        blurb: "Files apps left in your Temp folder and in Windows\\Temp.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: Some(24),
        file_filter: None,
        note: Some(
            "Files changed in the last 24 hours are skipped, so a running installer is not pulled out from under itself.",
        ),
        caution: None,
        about: "Temporary files apps create and forget. Anything an app still has open is skipped.",
        resolve: temp_files,
    },
    Provider {
        id: "thumbnails",
        label: "Thumbnail cache",
        blurb: "The pictures File Explorer saved of your files and folders.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &[],
        queue_ok: true,
        min_age_hours: None,
        file_filter: Some(is_thumbnail_database),
        note: None,
        caution: None,
        about: "Only the thumbcache database files. Explorer rebuilds them as you open folders, so the first visit to a folder of pictures is a little slower.",
        resolve: thumbnails,
    },
    Provider {
        id: "deliveryopt",
        label: "Delivery Optimization files",
        blurb: "Windows Update download pieces Windows keeps to share with other PCs.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: true,
        is_shader_cache: false,
        processes: &[],
        queue_ok: true,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: None,
        about: "Disk Cleanup lists these as Delivery Optimization Files. Windows downloads again only what it needs, and clearing them does not affect Windows Update.",
        resolve: delivery_optimization,
    },
    Provider {
        id: "wucache",
        label: "Windows Update downloads",
        blurb: "Installation files Windows Update already downloaded.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: Some(
            "Skip this while an update is downloading or installing. Windows downloads again whatever it still needs.",
        ),
        caution: None,
        about: "SoftwareDistribution\\Download. Files the update service has open stay, because ShaderSweep does not stop any service.",
        resolve: windows_update,
    },
    Provider {
        id: "prefetch",
        label: "Prefetch files",
        blurb: "Windows' record of which files apps load at start.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: Some(
            "This does not make anything faster. Apps start slightly slower the first time afterwards.",
        ),
        about: "Windows uses Prefetch to speed up launching apps, so removing it has no benefit and Windows rebuilds it as you work. It is here for people who want it gone.",
        resolve: prefetch,
    },
    Provider {
        id: "errors",
        label: "Error reports and crash dumps",
        blurb: "Windows Error Reporting files and your apps' crash dumps.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: Some("Keep these if you are chasing a crash. They are the evidence."),
        about: "The CrashDumps folder and the Windows Error Reporting archive and queue, which Disk Cleanup lists as Windows Error Reports.",
        resolve: error_reports,
    },
    Provider {
        id: "gpudumps",
        label: "GPU crash dumps",
        blurb: "Windows kernel dumps written when the graphics driver stops responding.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: Some("Keep these if a driver vendor or Microsoft support may ask for one."),
        about: "The LiveKernelReports folder. A single dump from a GPU timeout can be several gigabytes. Removing them does not affect the driver or stability.",
        resolve: gpu_dumps,
    },
    Provider {
        id: "recyclebin",
        label: "Recycle Bin",
        blurb: "Everything in the Recycle Bin on every drive.",
        group: "housekeeping",
        kind: Kind::RecycleBin,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: Some("This permanently deletes your files. It cannot be undone."),
        about: "Emptied through Windows itself, exactly as the Empty Recycle Bin command does.",
        resolve: nothing,
    },
    Provider {
        id: "eventlogs",
        label: "Event Viewer logs",
        blurb: "Application, System and the other Windows event logs. The Security log is never cleared.",
        group: "housekeeping",
        kind: Kind::EventLogs,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: None,
        caution: Some(
            "These logs are the record of what went wrong. Clearing them cannot be undone.",
        ),
        about: "Cleared through the Windows Event Log service, so nothing is forced. The Security log is the audit trail and is always kept.",
        resolve: nothing,
    },
    Provider {
        id: "installers",
        label: "Driver installer leftovers",
        blurb: "Extracted installers in C:\\AMD and C:\\NVIDIA\\DisplayDriver. Off by default so you can still roll back.",
        group: "housekeeping",
        kind: Kind::Folders,
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        processes: &[],
        queue_ok: false,
        min_age_hours: None,
        file_filter: None,
        note: Some("Skip this while a driver install is running."),
        caution: None,
        about: "Only needed to reinstall or roll back that driver without downloading it again. Nothing the driver uses while running lives here.",
        resolve: installers,
    },
];

pub fn find(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::is_allowed;

    #[test]
    fn ids_are_unique_and_groups_are_known() {
        let mut ids: Vec<_> = PROVIDERS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROVIDERS.len());
        for p in PROVIDERS {
            assert!(
                ["shaders", "launchers", "housekeeping"].contains(&p.group),
                "{}",
                p.id
            );
        }
    }

    #[test]
    fn anything_destructive_or_diagnostic_is_off_by_default() {
        for must_be_off in [
            "installers",
            "gpudumps",
            "errors",
            "prefetch",
            "wucache",
            "recyclebin",
            "eventlogs",
            "wow",
        ] {
            assert!(
                PROVIDERS
                    .iter()
                    .any(|p| p.id == must_be_off && !p.default_on),
                "{must_be_off} should start off"
            );
        }
        for must_be_on in [
            "nvidia", "amd", "intel", "windows", "steam", "discord", "temp",
        ] {
            assert!(
                PROVIDERS.iter().any(|p| p.id == must_be_on && p.default_on),
                "{must_be_on} should start on"
            );
        }
    }

    #[test]
    fn irreversible_rows_carry_a_warning() {
        for id in ["recyclebin", "eventlogs"] {
            assert!(find(id).unwrap().caution.is_some(), "{id}");
        }
    }

    #[test]
    fn launcher_rows_name_what_to_close_and_never_queue_for_restart() {
        for p in PROVIDERS.iter().filter(|p| p.group == "launchers") {
            assert!(!p.processes.is_empty(), "{} lists no process", p.id);
            assert!(!p.queue_ok, "{} must not queue files for restart", p.id);
        }
    }

    #[test]
    fn a_mixed_folder_is_only_ever_cleared_through_a_file_filter() {
        // The Explorer folder holds settings beside the thumbnail databases.
        let thumbs = find("thumbnails").unwrap();
        let filter = thumbs.file_filter.expect("thumbnails must be filtered");
        assert!(filter("thumbcache_256.db"));
        assert!(filter("ThumbCache_idx.db"));
        assert!(!filter("iconcache_32.db"));
        assert!(!filter("settings.dat"));
    }

    #[test]
    fn temp_files_only_go_when_they_are_a_day_old() {
        assert_eq!(find("temp").unwrap().min_age_hours, Some(24));
    }

    #[test]
    fn every_resolved_target_passes_the_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let profile = root.join("user");
        for rel in [
            r"AppData\Local\NVIDIA\DXCache",
            r"AppData\LocalLow\NVIDIA\PerDriverVersion\GLCache",
            r"AppData\Roaming\NVIDIA\ComputeCache",
            r"AppData\Local\AMD\VkCache",
            r"AppData\LocalLow\Intel\ShaderCache",
            r"AppData\Local\D3DSCache",
            r"AppData\Roaming\discord\Cache",
            r"AppData\Roaming\discord\GPUCache",
            r"AppData\Roaming\discordptb\Code Cache",
            r"AppData\Local\EpicGamesLauncher\Saved\webcache_4430",
            r"AppData\Local\Battle.net\Cache",
            r"AppData\Local\Battle.net\BrowserCaches",
            r"AppData\Local\Steam\htmlcache",
            r"AppData\Local\Electronic Arts\EA Desktop\Cache",
            r"AppData\Local\Ubisoft Game Launcher\cache",
            r"AppData\Local\GOG.com\Galaxy\webcache",
            r"AppData\Local\Temp",
            r"AppData\Local\Microsoft\Windows\Explorer",
            r"AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
            r"AppData\Local\CrashDumps",
            // Look alikes that must never be picked up.
            r"AppData\Roaming\discord\Local Storage",
            r"AppData\Roaming\discord\IndexedDB",
            r"AppData\Local\EpicGamesLauncher\Saved\Config",
            r"AppData\Local\EpicGamesLauncher\Saved\Crashes",
            r"AppData\Local\Battle.net\Account",
        ] {
            fs::create_dir_all(profile.join(rel)).unwrap();
        }

        let sys = root.join("sys");
        let windows = sys.join("Windows");
        for rel in [
            "Temp",
            "Prefetch",
            r"SoftwareDistribution\Download",
            "LiveKernelReports",
        ] {
            fs::create_dir_all(windows.join(rel)).unwrap();
        }
        fs::create_dir_all(sys.join("AMD")).unwrap();
        fs::create_dir_all(sys.join("NVIDIA").join("DisplayDriver")).unwrap();

        let pd = root.join("pd");
        fs::create_dir_all(pd.join("GOG.com").join("Galaxy").join("webcache")).unwrap();
        fs::create_dir_all(
            pd.join("Microsoft")
                .join("Windows")
                .join("WER")
                .join("ReportQueue"),
        )
        .unwrap();

        let lib = root.join("lib");
        fs::create_dir_all(lib.join("steamapps").join("shadercache")).unwrap();

        let wow_root = root.join("World of Warcraft");
        fs::create_dir_all(wow_root.join("_retail_").join("Cache")).unwrap();
        fs::create_dir_all(wow_root.join("_classic_era_").join("Cache")).unwrap();
        fs::create_dir_all(wow_root.join("_retail_").join("WTF")).unwrap();
        fs::create_dir_all(wow_root.join("Data")).unwrap();

        let pf = root.join("pf");
        let ubisoft = pf.join("Ubisoft").join("Ubisoft Game Launcher");
        fs::create_dir_all(ubisoft.join("cache")).unwrap();
        fs::create_dir_all(ubisoft.join("games")).unwrap();

        let ctx = Context {
            profiles: vec![profile],
            system_drive: sys.clone(),
            windows_dir: windows.clone(),
            program_data: pd,
            program_files: vec![pf],
            steam_libraries: vec![lib],
            wow_roots: vec![wow_root],
            gl_override: None,
        };

        let forbidden = [
            "local storage",
            "indexeddb",
            "config",
            "crashes",
            "account",
            "wtf",
            "data",
            "games",
        ];

        for p in PROVIDERS.iter().filter(|p| p.kind == Kind::Folders) {
            let targets = (p.resolve)(&ctx);
            assert!(!targets.is_empty(), "{} found nothing", p.id);
            for t in targets {
                assert!(
                    is_allowed(&t, &sys, &windows),
                    "{} produced {}",
                    p.id,
                    t.display()
                );
                let leaf = t
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                assert!(
                    !forbidden.contains(&leaf.as_str()),
                    "{} picked up {}",
                    p.id,
                    t.display()
                );
            }
        }

        // Both WoW versions were found, and nothing else under the game folder.
        assert_eq!((find("wow").unwrap().resolve)(&ctx).len(), 2);
    }
}
