//! Game launchers and games, matched by the tail of their cache path so logins,
//! settings and the games themselves are never in scope.

use std::path::PathBuf;

use super::{child_dirs, existing, name_of, under_profiles};
use crate::context::Context;
use crate::discovery::find_named;

const DISCORD_APPS: &[&str] = &[
    "discord",
    "discordptb",
    "discordcanary",
    "discorddevelopment",
];

pub(super) fn discord(ctx: &Context) -> Vec<PathBuf> {
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
pub(super) fn epic(ctx: &Context) -> Vec<PathBuf> {
    under_profiles(ctx, &[r"AppData\Local\EpicGamesLauncher\Saved"])
        .iter()
        .flat_map(|saved| child_dirs(saved))
        .filter(|dir| name_of(dir).to_ascii_lowercase().starts_with("webcache"))
        .collect()
}

pub(super) fn battlenet(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[
            r"AppData\Local\Battle.net\Cache",
            r"AppData\Local\Battle.net\BrowserCaches",
        ],
    ))
}

pub(super) fn steam_client(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(ctx, &[r"AppData\Local\Steam\htmlcache"]))
}

pub(super) fn ea(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[r"AppData\Local\Electronic Arts\EA Desktop\Cache"],
    ))
}

pub(super) fn ubisoft(ctx: &Context) -> Vec<PathBuf> {
    let mut paths = under_profiles(ctx, &[r"AppData\Local\Ubisoft Game Launcher\cache"]);
    paths.extend(ctx.program_files.iter().map(|pf| {
        pf.join("Ubisoft")
            .join("Ubisoft Game Launcher")
            .join("cache")
    }));
    existing(paths)
}

pub(super) fn gog(ctx: &Context) -> Vec<PathBuf> {
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
pub(super) fn wow(ctx: &Context) -> Vec<PathBuf> {
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
