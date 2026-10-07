//! What the app knows how to clean. Each provider turns the machine context
//! into a list of folders, and `safety` decides whether each one may be
//! touched. Adding a vendor means adding one entry here.

use std::path::PathBuf;

use crate::context::Context;
use crate::discovery::{drop_nested, find_named};

pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    /// Matches `Adapter::vendor`, so a driver change can point at its row.
    pub vendor: Option<&'static str>,
    pub default_on: bool,
    /// True for shader caches, which is what a "last cleaned" record tracks.
    pub is_shader_cache: bool,
    pub note: Option<&'static str>,
    pub about: &'static str,
    pub resolve: fn(&Context) -> Vec<PathBuf>,
}

fn under_profiles(ctx: &Context, relative: &[&str]) -> Vec<PathBuf> {
    ctx.profiles
        .iter()
        .flat_map(|p| relative.iter().map(move |r| p.join(r)))
        .collect()
}

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
    under_profiles(ctx, &[r"AppData\LocalLow\Intel\ShaderCache"])
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}

fn windows(ctx: &Context) -> Vec<PathBuf> {
    under_profiles(ctx, &[r"AppData\Local\D3DSCache"])
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}

fn steam(ctx: &Context) -> Vec<PathBuf> {
    ctx.steam_libraries
        .iter()
        .map(|lib| lib.join("steamapps").join("shadercache"))
        .filter(|p| p.is_dir())
        .collect()
}

fn installers(ctx: &Context) -> Vec<PathBuf> {
    [
        ctx.system_drive.join("AMD"),
        ctx.system_drive.join("NVIDIA").join("DisplayDriver"),
    ]
    .into_iter()
    .filter(|p| p.is_dir())
    .collect()
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "nvidia",
        label: "NVIDIA shader cache",
        blurb: "DirectX, OpenGL and compute caches written by the NVIDIA driver.",
        vendor: Some("nvidia"),
        default_on: true,
        is_shader_cache: true,
        note: Some(
            "A few index files are held by the display driver. Those are queued for your next restart.",
        ),
        about: "Compiled shader binaries for your exact GPU and driver. The driver rebuilds them the next time a game needs them, so the first launch afterwards can be slower. NVIDIA caps this cache at 16 GB by default. You can change the limit in the NVIDIA App under Graphics, Global Settings, Shader Cache Size.",
        resolve: nvidia,
    },
    Provider {
        id: "amd",
        label: "AMD shader cache",
        blurb: "DirectX 9, 11, 12, OpenGL and Vulkan caches written by the AMD driver.",
        vendor: Some("amd"),
        default_on: true,
        is_shader_cache: true,
        note: None,
        about: "Compiled shader binaries for your exact GPU and driver. The driver rebuilds them the next time a game needs them, so the first launch afterwards can be slower.",
        resolve: amd,
    },
    Provider {
        id: "intel",
        label: "Intel shader cache",
        blurb: "Shader cache written by the Intel graphics driver.",
        vendor: Some("intel"),
        default_on: true,
        is_shader_cache: true,
        note: None,
        about: "Compiled shader binaries for your exact GPU and driver. The driver rebuilds them the next time a game needs them, so the first launch afterwards can be slower.",
        resolve: intel,
    },
    Provider {
        id: "windows",
        label: "Windows DirectX cache",
        blurb: "The shader cache Windows keeps for every GPU. Disk Cleanup lists it too.",
        vendor: None,
        default_on: true,
        is_shader_cache: true,
        note: None,
        about: "Shaders Windows compiled for DirectX games and apps on any GPU. Windows versions it by driver and rebuilds it on demand.",
        resolve: windows,
    },
    Provider {
        id: "steam",
        label: "Steam shader cache",
        blurb: "Pre-compiled shaders Steam stores next to your games, in every library.",
        vendor: None,
        default_on: true,
        is_shader_cache: true,
        note: None,
        about: "Shaders Steam downloaded or compiled for a game. Steam fetches or rebuilds them the next time that game starts. On Windows these folders are usually small.",
        resolve: steam,
    },
    Provider {
        id: "installers",
        label: "Driver installer leftovers",
        blurb: "Extracted installers in C:\\AMD and C:\\NVIDIA\\DisplayDriver. Off by default so you can still roll back.",
        vendor: None,
        default_on: false,
        is_shader_cache: false,
        note: Some("Skip this while a driver install is running."),
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
    use std::fs;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = PROVIDERS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROVIDERS.len());
    }

    #[test]
    fn only_installers_are_off_by_default() {
        for p in PROVIDERS {
            assert_eq!(p.default_on, p.id != "installers", "{}", p.id);
        }
    }

    #[test]
    fn every_resolved_target_passes_the_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("user");
        for rel in [
            r"AppData\Local\NVIDIA\DXCache",
            r"AppData\LocalLow\NVIDIA\PerDriverVersion\GLCache",
            r"AppData\Roaming\NVIDIA\ComputeCache",
            r"AppData\Local\AMD\VkCache",
            r"AppData\LocalLow\Intel\ShaderCache",
            r"AppData\Local\D3DSCache",
        ] {
            fs::create_dir_all(profile.join(rel)).unwrap();
        }
        let sys = dir.path().join("sys");
        fs::create_dir_all(sys.join("AMD")).unwrap();
        fs::create_dir_all(sys.join("NVIDIA").join("DisplayDriver")).unwrap();
        let lib = dir.path().join("lib");
        fs::create_dir_all(lib.join("steamapps").join("shadercache")).unwrap();

        let ctx = Context {
            profiles: vec![profile],
            system_drive: sys.clone(),
            program_data: dir.path().join("pd"),
            steam_libraries: vec![lib],
            gl_override: None,
        };

        for p in PROVIDERS {
            let targets = (p.resolve)(&ctx);
            assert!(!targets.is_empty(), "{} found nothing", p.id);
            for t in targets {
                assert!(is_allowed(&t, &sys), "{} produced {}", p.id, t.display());
            }
        }
    }
}
