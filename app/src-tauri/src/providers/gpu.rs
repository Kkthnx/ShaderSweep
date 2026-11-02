//! Where the GPU driver and Windows keep compiled shaders.

use std::path::PathBuf;

use super::{existing, under_profiles};
use crate::context::Context;
use crate::discovery::{drop_nested, find_named};

pub(super) fn nvidia(ctx: &Context) -> Vec<PathBuf> {
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

pub(super) fn amd(ctx: &Context) -> Vec<PathBuf> {
    let roots = under_profiles(ctx, &[r"AppData\Local\AMD"]);
    find_named(
        &roots,
        &[
            "DX9Cache", "DxCache", "DxcCache", "OglCache", "GLCache", "VkCache",
        ],
        1,
    )
}

/// Intel's driver has written this under `LocalLow` for years. Some newer
/// driver builds and guides put it under `Local` instead, so both are checked.
pub(super) fn intel(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[
            r"AppData\LocalLow\Intel\ShaderCache",
            r"AppData\Local\Intel\ShaderCache",
        ],
    ))
}

pub(super) fn windows_d3d(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(ctx, &[r"AppData\Local\D3DSCache"]))
}

pub(super) fn steam_shaders(ctx: &Context) -> Vec<PathBuf> {
    existing(
        ctx.steam_libraries
            .iter()
            .map(|lib| lib.join("steamapps").join("shadercache"))
            .collect(),
    )
}
