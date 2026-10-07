//! Ties the pieces together: scan what exists, then clean what was chosen.

use std::collections::HashSet;
use std::path::PathBuf;
use std::thread;

use crate::clean::{clear_contents, Outcome};
use crate::context::Context;
use crate::drivers;
use crate::fsutil::dir_size;
use crate::model::{
    CleanResult, DiskInfo, DriverChange, Folder, Progress, ProviderResult, ProviderScan, ScanResult,
};
use crate::native;
use crate::pending;
use crate::providers::{self, Provider, PROVIDERS};
use crate::safety::is_allowed;
use crate::state;

/// Folders for one provider, with anything the allow list refuses dropped.
fn targets(provider: &Provider, ctx: &Context) -> Vec<PathBuf> {
    (provider.resolve)(ctx)
        .into_iter()
        .filter(|p| is_allowed(p, &ctx.system_drive))
        .collect()
}

fn scan_provider(provider: &Provider, ctx: &Context) -> ProviderScan {
    let mut folders = Vec::new();
    let mut bytes = 0;
    let mut files = 0;

    for path in targets(provider, ctx) {
        let size = dir_size(&path);
        bytes += size.bytes;
        files += size.files;
        folders.push(Folder {
            path: path.to_string_lossy().into_owned(),
            bytes: size.bytes,
        });
    }

    folders.sort_by_key(|f| std::cmp::Reverse(f.bytes));

    ProviderScan {
        id: provider.id,
        label: provider.label,
        blurb: provider.blurb,
        note: provider.note,
        about: provider.about,
        vendor: provider.vendor,
        default_on: provider.default_on,
        found: !folders.is_empty(),
        bytes,
        files,
        folders,
    }
}

pub fn scan() -> ScanResult {
    let ctx = Context::detect();

    // Providers are independent, so measure them side by side.
    let providers = thread::scope(|scope| {
        let handles: Vec<_> = PROVIDERS
            .iter()
            .map(|p| {
                let ctx = &ctx;
                scope.spawn(move || scan_provider(p, ctx))
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect::<Vec<_>>()
    });

    let adapters = drivers::installed();
    let last = state::load();
    let driver_changes = state::changed_since(last.as_ref(), &adapters)
        .into_iter()
        .map(|a| DriverChange {
            vendor: a.vendor.clone(),
            name: a.name.clone(),
            version: a.version.clone(),
        })
        .collect();

    let folders: Vec<PathBuf> = providers
        .iter()
        .flat_map(|p| p.folders.iter().map(|f| PathBuf::from(&f.path)))
        .collect();
    let pending_restart = pending::count_inside(&pending::load(), &folders);

    let disk = native::disk_space(&ctx.system_drive).map(|d| DiskInfo {
        drive: ctx
            .system_drive
            .to_string_lossy()
            .trim_end_matches('\\')
            .to_string(),
        free: d.free,
        total: d.total,
    });

    ScanResult {
        is_admin: native::is_admin(),
        disk,
        pending_restart,
        adapters,
        providers,
        last_clean_at: last.map(|l| l.at),
        driver_changes,
    }
}

pub fn clean(
    ids: &[String],
    preview: bool,
    queue_locked: bool,
    mut progress: impl FnMut(Progress),
) -> Result<CleanResult, String> {
    let ctx = Context::detect();

    // Resolve every id before touching anything, so one typo cannot leave a
    // run half finished.
    let chosen: Vec<&Provider> = ids
        .iter()
        .map(|id| providers::find(id).ok_or_else(|| format!("Unknown cache type: {id}")))
        .collect::<Result<_, _>>()?;

    let queue = if queue_locked {
        pending::load()
    } else {
        HashSet::new()
    };
    let mut results = Vec::new();
    let mut total = Outcome::default();
    let mut shader_cleaned = false;

    for provider in chosen {
        progress(Progress {
            id: provider.id.to_string(),
            done: false,
            freed: 0,
        });

        let mut outcome = Outcome::default();
        for path in targets(provider, &ctx) {
            if preview {
                outcome.freed += dir_size(&path).bytes;
            } else {
                outcome.add(clear_contents(&path, queue_locked, &queue));
            }
        }

        if !preview && provider.is_shader_cache {
            shader_cleaned = true;
        }

        progress(Progress {
            id: provider.id.to_string(),
            done: true,
            freed: outcome.freed,
        });

        let holders = native::holders(&outcome.failed_samples);
        results.push(ProviderResult {
            id: provider.id.to_string(),
            freed: outcome.freed,
            removed_files: outcome.removed_files,
            queued_files: outcome.queued_files,
            queued_bytes: outcome.queued_bytes,
            failed_files: outcome.failed_files,
            failed_bytes: outcome.failed_bytes,
            holders,
        });
        total.add(outcome);
    }

    if shader_cleaned {
        // A failed write only means the driver banner stays up next time.
        let _ = state::save(&drivers::installed());
    }

    Ok(CleanResult {
        preview,
        providers: results,
        freed: total.freed,
        queued_files: total.queued_files,
        queued_bytes: total.queued_bytes,
        failed_files: total.failed_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_runs_on_this_machine_without_panicking() {
        let result = scan();
        assert_eq!(result.providers.len(), PROVIDERS.len());
        for p in &result.providers {
            assert_eq!(p.found, !p.folders.is_empty());
            assert_eq!(p.bytes, p.folders.iter().map(|f| f.bytes).sum::<u64>());
        }
    }

    /// Prints what the app would show on this machine.
    /// Run with: cargo test print_scan -- --ignored --nocapture
    #[test]
    #[ignore]
    fn print_scan() {
        println!("{}", serde_json::to_string_pretty(&scan()).unwrap());
    }

    #[test]
    fn preview_deletes_nothing_and_unknown_ids_fail_early() {
        assert!(clean(&["nope".to_string()], true, false, |_| {}).is_err());

        let ids: Vec<String> = PROVIDERS.iter().map(|p| p.id.to_string()).collect();
        let before = scan();
        let result = clean(&ids, true, false, |_| {}).unwrap();
        let after = scan();

        assert!(result.preview);
        for (b, a) in before.providers.iter().zip(after.providers.iter()) {
            assert_eq!(b.files, a.files, "{} changed during a preview", b.id);
        }
    }
}
