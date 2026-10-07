//! Ties the pieces together: scan what exists, then clean what was chosen.

use std::collections::HashSet;
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::clean::{clear_contents, Outcome};
use crate::context::Context;
use crate::drivers::{self, Adapter};
use crate::fsutil::dir_size;
use crate::model::{
    CleanResult, DiskInfo, DriverChange, Folder, Progress, ProviderResult, ProviderScan, ScanResult,
};
use crate::native;
use crate::pending;
use crate::providers::{self, Provider, PROVIDERS};
use crate::report;
use crate::safety::is_allowed;
use crate::state::{self, LastClean};

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
    scan_with(
        &Context::detect(),
        state::load(),
        drivers::installed(),
        &pending::load(),
    )
}

/// The scan itself, with every machine specific input handed in so tests can
/// run it against a made up machine.
pub fn scan_with(
    ctx: &Context,
    last: Option<LastClean>,
    adapters: Vec<Adapter>,
    queue: &HashSet<String>,
) -> ScanResult {
    // Providers are independent, so measure them side by side.
    let providers = thread::scope(|scope| {
        let handles: Vec<_> = PROVIDERS
            .iter()
            .map(|p| scope.spawn(move || scan_provider(p, ctx)))
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect::<Vec<_>>()
    });

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
    let pending_restart = pending::count_inside(queue, &folders);

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
        last_clean_at: last.as_ref().map(|l| l.at),
        total_freed: last.map_or(0, |l| l.total_freed),
        driver_changes,
    }
}

/// True only for a folder a provider resolves to right now. This is what lets
/// the window offer "open folder" without ever being handed a free form path.
pub fn is_known_folder(path: &str) -> bool {
    let ctx = Context::detect();
    let wanted = path.to_ascii_lowercase();
    PROVIDERS.iter().any(|p| {
        targets(p, &ctx)
            .iter()
            .any(|t| t.to_string_lossy().to_ascii_lowercase() == wanted)
    })
}

pub fn clean(
    ids: &[String],
    preview: bool,
    queue_locked: bool,
    progress: impl FnMut(Progress),
) -> Result<CleanResult, String> {
    let ctx = Context::detect();
    let queue = if queue_locked {
        pending::load()
    } else {
        HashSet::new()
    };
    let (mut result, shader_cleaned) =
        clean_with(&ctx, &queue, ids, preview, queue_locked, progress)?;

    let adapters = drivers::installed();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    result.report = report::build(&result, &adapters, env!("CARGO_PKG_VERSION"), now, |id| {
        providers::find(id).map_or_else(|| id.to_string(), |p| p.label.to_string())
    });

    if !preview {
        // A failed write only means the driver banner stays up next time.
        if shader_cleaned {
            let _ = state::save(&adapters, result.freed);
        }
        let _ = state::write_log(&result.report);
    }

    Ok(result)
}

/// The clean itself. Also says whether any shader cache was really cleared.
pub fn clean_with(
    ctx: &Context,
    queue: &HashSet<String>,
    ids: &[String],
    preview: bool,
    queue_locked: bool,
    mut progress: impl FnMut(Progress),
) -> Result<(CleanResult, bool), String> {
    // Resolve every id before touching anything, so one typo cannot leave a
    // run half finished.
    let chosen: Vec<&Provider> = ids
        .iter()
        .map(|id| providers::find(id).ok_or_else(|| format!("Unknown cache type: {id}")))
        .collect::<Result<_, _>>()?;

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
        for path in targets(provider, ctx) {
            if preview {
                outcome.freed += dir_size(&path).bytes;
            } else {
                outcome.add(clear_contents(&path, queue_locked, queue));
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

    Ok((
        CleanResult {
            preview,
            providers: results,
            freed: total.freed,
            queued_files: total.queued_files,
            queued_bytes: total.queued_bytes,
            failed_files: total.failed_files,
            report: String::new(),
        },
        shader_cleaned,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

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
    fn a_preview_on_this_machine_changes_nothing() {
        let ids: Vec<String> = PROVIDERS.iter().map(|p| p.id.to_string()).collect();
        let before = scan();
        let (result, shader) = clean_with(
            &Context::detect(),
            &HashSet::new(),
            &ids,
            true,
            false,
            |_| {},
        )
        .unwrap();
        let after = scan();

        assert!(result.preview && !shader);
        for (b, a) in before.providers.iter().zip(after.providers.iter()) {
            assert_eq!(b.files, a.files, "{} changed during a preview", b.id);
        }
    }

    // ---- a made up machine -------------------------------------------------

    fn put(path: &Path, len: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(path)
            .unwrap()
            .write_all(&vec![7u8; len])
            .unwrap();
    }

    struct Fake {
        _dir: tempfile::TempDir,
        ctx: Context,
        nvidia: PathBuf,
        d3d: PathBuf,
        installer: PathBuf,
        settings: PathBuf,
        save: PathBuf,
    }

    fn fake_machine() -> Fake {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let profile = root.join("user");
        let sys = root.join("sys");

        let nvidia = profile.join(r"AppData\Local\NVIDIA\DXCache");
        put(&nvidia.join("a.nvph"), 1000);
        put(&nvidia.join("sub").join("b.nvph"), 500);
        let d3d = profile.join(r"AppData\Local\D3DSCache");
        put(&d3d.join("shader.bin"), 200);
        let installer = sys.join("AMD");
        put(&installer.join("setup").join("big.cab"), 4000);

        // Beside the caches, and must survive every run.
        let settings = profile.join(r"AppData\Local\NVIDIA\settings.json");
        put(&settings, 10);
        let save = profile.join("Documents").join("save.dat");
        put(&save, 99);

        let ctx = Context {
            profiles: vec![profile],
            system_drive: sys,
            program_data: root.join("pd"),
            steam_libraries: vec![],
            gl_override: None,
        };
        Fake {
            _dir: dir,
            ctx,
            nvidia,
            d3d,
            installer,
            settings,
            save,
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn scan_finds_exactly_what_is_there() {
        let m = fake_machine();
        let result = scan_with(&m.ctx, None, vec![], &HashSet::new());
        let by = |id: &str| result.providers.iter().find(|p| p.id == id).unwrap();

        assert_eq!(by("nvidia").bytes, 1500);
        assert_eq!(by("nvidia").files, 2);
        assert_eq!(by("windows").bytes, 200);
        assert_eq!(by("installers").bytes, 4000);
        assert!(!by("amd").found);
        assert!(!by("intel").found);
        assert!(!by("steam").found);
    }

    #[test]
    fn preview_measures_and_deletes_nothing() {
        let m = fake_machine();
        let (result, shader) = clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(&["nvidia", "windows"]),
            true,
            false,
            |_| {},
        )
        .unwrap();

        assert_eq!(result.freed, 1700);
        assert!(!shader, "a preview must not count as a clean");
        assert!(m.nvidia.join("a.nvph").exists());
        assert!(m.d3d.join("shader.bin").exists());
    }

    #[test]
    fn a_real_clean_frees_exactly_the_chosen_rows_and_nothing_else() {
        let m = fake_machine();
        let mut events = Vec::new();
        let (result, shader) = clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(&["nvidia", "windows"]),
            false,
            false,
            |p| events.push((p.id, p.done)),
        )
        .unwrap();

        assert_eq!(result.freed, 1700);
        assert_eq!(result.failed_files, 0);
        assert!(shader);
        assert!(m.nvidia.is_dir() && fs::read_dir(&m.nvidia).unwrap().count() == 0);
        assert!(m.d3d.is_dir() && fs::read_dir(&m.d3d).unwrap().count() == 0);

        // Unchosen and unrelated data is untouched.
        assert!(m.installer.join("setup").join("big.cab").exists());
        assert!(m.settings.exists());
        assert!(m.save.exists());

        // Progress reports a start then a finish for each row, in order.
        assert_eq!(
            events,
            vec![
                ("nvidia".to_string(), false),
                ("nvidia".to_string(), true),
                ("windows".to_string(), false),
                ("windows".to_string(), true),
            ]
        );
    }

    #[test]
    fn installers_are_cleaned_when_asked_but_are_not_a_shader_clean() {
        let m = fake_machine();
        let (result, shader) = clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(&["installers"]),
            false,
            false,
            |_| {},
        )
        .unwrap();

        assert_eq!(result.freed, 4000);
        assert!(!shader, "installer leftovers are not a shader clean");
        assert!(m.installer.is_dir());
        assert_eq!(fs::read_dir(&m.installer).unwrap().count(), 0);
        assert!(m.nvidia.join("a.nvph").exists());
    }

    #[test]
    fn an_unknown_row_fails_before_anything_is_deleted() {
        let m = fake_machine();
        let err = clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(&["nvidia", "everything"]),
            false,
            false,
            |_| {},
        )
        .unwrap_err();

        assert!(err.contains("everything"));
        assert!(m.nvidia.join("a.nvph").exists());
    }

    #[test]
    fn driver_changes_and_totals_come_through_the_scan() {
        let m = fake_machine();
        let adapter = Adapter {
            vendor: "nvidia".into(),
            name: "RTX 5070".into(),
            version: "616.92".into(),
            date: "2026-09-04".into(),
        };
        let last = LastClean {
            at: 42,
            drivers: [("nvidia:RTX 5070".to_string(), "610.10".to_string())].into(),
            total_freed: 777,
        };

        let result = scan_with(&m.ctx, Some(last), vec![adapter], &HashSet::new());

        assert_eq!(result.driver_changes.len(), 1);
        assert_eq!(result.last_clean_at, Some(42));
        assert_eq!(result.total_freed, 777);
    }
}
