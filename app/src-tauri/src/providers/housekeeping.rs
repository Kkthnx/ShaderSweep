//! General Windows cleanup. The risky rows start off in the table.

use std::path::PathBuf;

use super::{existing, under_profiles};
use crate::context::Context;

pub(super) fn temp_files(ctx: &Context) -> Vec<PathBuf> {
    let mut paths = under_profiles(ctx, &[r"AppData\Local\Temp"]);
    paths.push(ctx.windows_dir.join("Temp"));
    existing(paths)
}

pub(super) fn is_thumbnail_database(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("thumbcache_") && name.ends_with(".db")
}

pub(super) fn thumbnails(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[r"AppData\Local\Microsoft\Windows\Explorer"],
    ))
}

pub(super) fn delivery_optimization(ctx: &Context) -> Vec<PathBuf> {
    existing(under_profiles(
        ctx,
        &[r"AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"],
    ))
}

pub(super) fn windows_update(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![ctx
        .windows_dir
        .join("SoftwareDistribution")
        .join("Download")])
}

pub(super) fn prefetch(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![ctx.windows_dir.join("Prefetch")])
}

pub(super) fn error_reports(ctx: &Context) -> Vec<PathBuf> {
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

pub(super) fn gpu_dumps(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![ctx.windows_dir.join("LiveKernelReports")])
}

pub(super) fn installers(ctx: &Context) -> Vec<PathBuf> {
    existing(vec![
        ctx.system_drive.join("AMD"),
        ctx.system_drive.join("NVIDIA").join("DisplayDriver"),
    ])
}
