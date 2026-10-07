//! Ties the pieces together: scan what exists, then clean what was chosen.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::clean::{clear_contents, Options, Outcome};
use crate::context::Context;
use crate::drivers::{self, Adapter};
use crate::fsutil::{dir_size, dir_size_where};
use crate::model::{
    CleanResult, DiskInfo, DriverChange, Folder, Progress, ProviderResult, ProviderScan,
    ScanResult, ScanStep,
};
use crate::native;
use crate::pending;
use crate::providers::{self, Kind, Provider, PROVIDERS};
use crate::report;
use crate::safety::is_allowed;
use crate::state::{self, LastClean, QueuedFile, RunSummary};
use crate::system;

/// Set by the window's Cancel button and checked between files.
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn request_cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// How often the window hears about progress, at most. Updating on every file
/// would flood it, and the eye cannot follow more than this anyway.
const PROGRESS_INTERVAL_MS: u128 = 60;

/// What is true of the machine right now, apart from its folders.
#[derive(Debug, Clone, Default)]
pub struct Live {
    /// Lower cased names of the programs running now.
    pub running: HashSet<String>,
    /// When Windows last started, in seconds since the Unix epoch.
    pub boot_secs: u64,
}

impl Live {
    pub fn detect() -> Self {
        Self {
            running: system::running_processes(),
            boot_secs: native::boot_time_secs(),
        }
    }
}

/// Folders for one provider, with anything the allow list refuses dropped.
fn targets(provider: &Provider, ctx: &Context) -> Vec<PathBuf> {
    (provider.resolve)(ctx)
        .into_iter()
        .filter(|p| is_allowed(p, &ctx.system_drive, &ctx.windows_dir))
        .collect()
}

fn event_log_folder(ctx: &Context) -> PathBuf {
    ctx.windows_dir.join("System32").join("winevt").join("Logs")
}

fn scan_provider(provider: &Provider, ctx: &Context, live: &Live) -> ProviderScan {
    let mut folders = Vec::new();
    let mut bytes = 0;
    let mut files = 0;
    let found;

    match provider.kind {
        Kind::Folders => {
            let eligible = provider.eligible();
            for path in targets(provider, ctx) {
                let size = dir_size_where(&path, &eligible);
                bytes += size.bytes;
                files += size.files;
                folders.push(Folder {
                    path: path.to_string_lossy().into_owned(),
                    bytes: size.bytes,
                });
            }
            folders.sort_by_key(|f| std::cmp::Reverse(f.bytes));
            found = !folders.is_empty();
        }
        Kind::RecycleBin => {
            let bin = system::recycle_bin();
            bytes = bin.bytes;
            files = bin.items;
            found = bin.items > 0;
        }
        Kind::EventLogs => {
            let size = dir_size(&event_log_folder(ctx));
            bytes = size.bytes;
            files = size.files;
            found = size.files > 0;
        }
    }

    ProviderScan {
        id: provider.id,
        label: provider.label,
        blurb: provider.blurb,
        note: provider.note,
        caution: provider.caution,
        about: provider.about,
        group: provider.group,
        vendor: provider.vendor,
        default_on: provider.default_on,
        found,
        bytes,
        files,
        folders,
        running: system::running_among(&live.running, provider.processes),
    }
}

pub fn scan(on_step: &(dyn Fn(ScanStep) + Sync)) -> ScanResult {
    let result = scan_with(
        &Context::detect(),
        state::load(),
        drivers::installed(),
        &pending::load(),
        &Live::detect(),
        on_step,
    );

    // The outcome of last time's queue is reported once, then forgotten.
    if result.queue_check.as_ref().is_some_and(|c| c.restarted) {
        let _ = state::clear_queue();
    }
    result
}

/// The scan itself, with every machine specific input handed in so tests can
/// run it against a made up machine.
pub fn scan_with(
    ctx: &Context,
    last: Option<LastClean>,
    adapters: Vec<Adapter>,
    queue: &HashSet<String>,
    live: &Live,
    on_step: &(dyn Fn(ScanStep) + Sync),
) -> ScanResult {
    let total = PROVIDERS.len() as u32;
    let checked = AtomicU32::new(0);

    // Providers are independent, so measure them side by side.
    let providers = thread::scope(|scope| {
        let handles: Vec<_> = PROVIDERS
            .iter()
            .map(|p| {
                let checked = &checked;
                scope.spawn(move || {
                    let scanned = scan_provider(p, ctx, live);
                    on_step(ScanStep {
                        label: p.label.to_string(),
                        checked: checked.fetch_add(1, Ordering::Relaxed) + 1,
                        total,
                    });
                    scanned
                })
            })
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

    let queue_check = last
        .as_ref()
        .and_then(|l| state::check_queue(l, live.boot_secs, |p| Path::new(p).exists()));

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
        queue_check,
        adapters,
        providers,
        last_clean_at: last.as_ref().filter(|l| l.at > 0).map(|l| l.at),
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

/// Everything a finished clean produced, before it is turned into a result.
pub struct Run {
    pub result: CleanResult,
    /// A shader cache was really cleared all the way through.
    pub shader_cleaned: bool,
    /// Files queued for the next restart, to check on after it.
    pub queued: Vec<QueuedFile>,
}

pub fn clean(
    ids: &[String],
    preview: bool,
    queue_locked: bool,
    progress: impl FnMut(Progress),
) -> Result<CleanResult, String> {
    CANCEL.store(false, Ordering::Relaxed);

    let ctx = Context::detect();
    let queue = if queue_locked {
        pending::load()
    } else {
        HashSet::new()
    };
    let mut run = clean_with(&ctx, &queue, ids, preview, queue_locked, &CANCEL, progress)?;

    let adapters = drivers::installed();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    run.result.report = report::build(
        &run.result,
        &adapters,
        env!("CARGO_PKG_VERSION"),
        now,
        |id| providers::find(id).map_or_else(|| id.to_string(), |p| p.label.to_string()),
    );

    if !preview {
        // A failed write only means the notices stay as they were.
        let _ = state::save(&RunSummary {
            // A stopped run did not finish the cache, so the driver record
            // does not move.
            shader_adapters: (run.shader_cleaned && !run.result.cancelled).then_some(&adapters[..]),
            freed: run.result.freed,
            queued: run.queued.clone(),
        });
        let _ = state::write_log(&run.result.report);
    }

    Ok(run.result)
}

/// The clean itself, against whatever machine `ctx` describes.
pub fn clean_with(
    ctx: &Context,
    queue: &HashSet<String>,
    ids: &[String],
    preview: bool,
    queue_locked: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Run, String> {
    // Resolve every id before touching anything, so one typo cannot leave a
    // run half finished.
    let chosen: Vec<&Provider> = ids
        .iter()
        .map(|id| providers::find(id).ok_or_else(|| format!("Unknown cache type: {id}")))
        .collect::<Result<_, _>>()?;

    let mut results = Vec::new();
    let mut total = Outcome::default();
    let mut shader_cleaned = false;
    let mut cancelled = false;

    for provider in chosen {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let id = provider.id.to_string();
        progress(Progress {
            id: id.clone(),
            done: false,
            freed: 0,
            files: 0,
            current: None,
        });

        let outcome = match provider.kind {
            Kind::Folders => clean_folders(
                provider,
                ctx,
                queue,
                preview,
                queue_locked,
                cancel,
                &mut progress,
            ),
            Kind::RecycleBin => clean_recycle_bin(preview),
            Kind::EventLogs => clean_event_logs(ctx, preview),
        };

        if outcome.cancelled {
            cancelled = true;
        }
        if !preview && provider.is_shader_cache && !outcome.cancelled {
            shader_cleaned = true;
        }

        progress(Progress {
            id: id.clone(),
            done: true,
            freed: outcome.freed,
            files: outcome.removed_files,
            current: None,
        });

        results.push(ProviderResult {
            id,
            freed: outcome.freed,
            removed_files: outcome.removed_files,
            queued_files: outcome.queued_files,
            queued_bytes: outcome.queued_bytes,
            failed_files: outcome.failed_files,
            failed_bytes: outcome.failed_bytes,
            skipped_recent: outcome.skipped_recent,
            holders: native::holders(&outcome.failed_samples),
        });
        total.add(outcome);
    }

    let queued = total
        .queued_paths
        .iter()
        .map(|(path, bytes)| QueuedFile {
            path: path.to_string_lossy().into_owned(),
            bytes: *bytes,
        })
        .collect();

    Ok(Run {
        result: CleanResult {
            preview,
            providers: results,
            freed: total.freed,
            queued_files: total.queued_files,
            queued_bytes: total.queued_bytes,
            failed_files: total.failed_files,
            cancelled,
            report: String::new(),
        },
        shader_cleaned,
        queued,
    })
}

fn clean_folders(
    provider: &Provider,
    ctx: &Context,
    queue: &HashSet<String>,
    preview: bool,
    queue_locked: bool,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Progress),
) -> Outcome {
    let id = provider.id;
    let mut outcome = Outcome::default();
    let mut last_emit = Instant::now();
    let eligible = provider.eligible();

    for path in targets(provider, ctx) {
        if preview {
            outcome.freed += dir_size_where(&path, &eligible).bytes;
            progress(Progress {
                id: id.to_string(),
                done: false,
                freed: outcome.freed,
                files: 0,
                current: Some(path.to_string_lossy().into_owned()),
            });
            continue;
        }

        // Totals for the folders finished before this one, so the running
        // count carries on instead of restarting for each.
        let (freed_before, files_before) = (outcome.freed, outcome.removed_files);

        let mut options = Options::new(queue_locked && provider.queue_ok, queue, cancel);
        options.eligible = eligible;

        let part = clear_contents(&path, &options, &mut |tick| {
            if last_emit.elapsed().as_millis() < PROGRESS_INTERVAL_MS {
                return;
            }
            last_emit = Instant::now();
            progress(Progress {
                id: id.to_string(),
                done: false,
                freed: freed_before + tick.freed,
                files: files_before + tick.files,
                current: Some(tick.current.to_string_lossy().into_owned()),
            });
        });
        outcome.add(part);

        if outcome.cancelled {
            break;
        }
    }

    outcome
}

fn clean_recycle_bin(preview: bool) -> Outcome {
    let before = system::recycle_bin();
    let mut outcome = Outcome::default();

    if preview {
        outcome.freed = before.bytes;
        return outcome;
    }

    system::empty_recycle_bin();
    let after = system::recycle_bin();
    outcome.freed = before.bytes.saturating_sub(after.bytes);
    outcome.removed_files = before.items.saturating_sub(after.items) as u32;
    outcome
}

fn clean_event_logs(ctx: &Context, preview: bool) -> Outcome {
    let folder = event_log_folder(ctx);
    let before = dir_size(&folder);
    let mut outcome = Outcome::default();

    if preview {
        outcome.freed = before.bytes;
        return outcome;
    }

    outcome.removed_files = system::clear_event_logs();
    let after = dir_size(&folder);
    outcome.freed = before.bytes.saturating_sub(after.bytes);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::time::Duration;

    fn no_steps() -> impl Fn(ScanStep) + Sync {
        |_| {}
    }

    #[test]
    fn scan_runs_on_this_machine_without_panicking() {
        let result = scan(&no_steps());
        assert_eq!(result.providers.len(), PROVIDERS.len());
        for p in &result.providers {
            if p.folders.is_empty() {
                continue;
            }
            assert_eq!(p.bytes, p.folders.iter().map(|f| f.bytes).sum::<u64>());
        }
    }

    /// Prints what the app would show on this machine.
    /// Run with: cargo test print_scan -- --ignored --nocapture
    #[test]
    #[ignore]
    fn print_scan() {
        println!(
            "{}",
            serde_json::to_string_pretty(&scan(&no_steps())).unwrap()
        );
    }

    #[test]
    fn a_preview_of_everything_on_this_machine_changes_nothing() {
        // Includes the Recycle Bin and the event logs, which a preview only
        // measures.
        let ids: Vec<String> = PROVIDERS.iter().map(|p| p.id.to_string()).collect();
        let before = scan(&no_steps());
        let run = clean_with(
            &Context::detect(),
            &HashSet::new(),
            &ids,
            true,
            false,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        let after = scan(&no_steps());

        assert!(run.result.preview && !run.shader_cleaned);
        for (b, a) in before.providers.iter().zip(after.providers.iter()) {
            // Other programs add files while the test runs, so only the rows
            // that stay still are compared.
            if [
                "recyclebin",
                "eventlogs",
                "temp",
                "thumbnails",
                "discord",
                "steamclient",
                "battlenet",
                "epic",
                "ubisoft",
                "deliveryopt",
                "errors",
            ]
            .contains(&b.id)
            {
                continue;
            }
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

    fn make_old(path: &Path) {
        let when = SystemTime::now() - Duration::from_secs(3 * 86_400);
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
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
        discord_cache: PathBuf,
        discord_login: PathBuf,
        wow_cache: PathBuf,
        wow_settings: PathBuf,
        temp_old: PathBuf,
        temp_fresh: PathBuf,
        explorer_thumb: PathBuf,
        explorer_other: PathBuf,
    }

    fn fake_machine() -> Fake {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let profile = root.join("user");
        let sys = root.join("sys");
        let windows = sys.join("Windows");
        fs::create_dir_all(&windows).unwrap();

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

        let discord = profile.join(r"AppData\Roaming\discord");
        let discord_cache = discord.join("Cache");
        put(&discord_cache.join("Cache_Data").join("data_1"), 300);
        let discord_login = discord
            .join("Local Storage")
            .join("leveldb")
            .join("token.ldb");
        put(&discord_login, 77);

        let wow_root = root.join("World of Warcraft");
        let wow_cache = wow_root.join("_retail_").join("Cache");
        put(&wow_cache.join("ADB").join("DBCache.bin"), 50);
        let wow_settings = wow_root.join("_retail_").join("WTF").join("Config.wtf");
        put(&wow_settings, 40);

        let temp = profile.join(r"AppData\Local\Temp");
        let temp_old = temp.join("old.tmp");
        let temp_fresh = temp.join("running-installer.tmp");
        put(&temp_old, 60);
        put(&temp_fresh, 30);
        make_old(&temp_old);

        let explorer = profile.join(r"AppData\Local\Microsoft\Windows\Explorer");
        let explorer_thumb = explorer.join("thumbcache_256.db");
        let explorer_other = explorer.join("customsettings.dat");
        put(&explorer_thumb, 25);
        put(&explorer_other, 9);

        let ctx = Context {
            profiles: vec![profile],
            system_drive: sys,
            windows_dir: windows,
            program_data: root.join("pd"),
            program_files: vec![],
            steam_libraries: vec![],
            wow_roots: vec![wow_root],
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
            discord_cache,
            discord_login,
            wow_cache,
            wow_settings,
            temp_old,
            temp_fresh,
            explorer_thumb,
            explorer_other,
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn live(running: &[&str], boot_secs: u64) -> Live {
        Live {
            running: running.iter().map(|s| s.to_ascii_lowercase()).collect(),
            boot_secs,
        }
    }

    fn do_clean(m: &Fake, list: &[&str], preview: bool) -> Run {
        clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(list),
            preview,
            false,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap()
    }

    fn do_scan(m: &Fake, running: &[&str]) -> ScanResult {
        scan_with(
            &m.ctx,
            None,
            vec![],
            &HashSet::new(),
            &live(running, 0),
            &no_steps(),
        )
    }

    #[test]
    fn scan_finds_exactly_what_is_there() {
        let m = fake_machine();
        let result = do_scan(&m, &[]);
        let by = |id: &str| result.providers.iter().find(|p| p.id == id).unwrap();

        assert_eq!(by("nvidia").bytes, 1500);
        assert_eq!(by("nvidia").files, 2);
        assert_eq!(by("windows").bytes, 200);
        assert_eq!(by("installers").bytes, 4000);
        assert_eq!(by("discord").bytes, 300);
        assert_eq!(by("wow").bytes, 50);
        assert!(!by("amd").found);
        assert!(!by("intel").found);
        assert!(!by("steam").found);
        assert!(!by("epic").found);
    }

    #[test]
    fn the_size_shown_is_only_what_can_really_be_removed() {
        let m = fake_machine();
        let result = do_scan(&m, &[]);
        let by = |id: &str| result.providers.iter().find(|p| p.id == id).unwrap();

        // The fresh temp file is too recent, so it is not promised.
        assert_eq!(by("temp").bytes, 60);
        // Only the thumbnail database counts, not its neighbour.
        assert_eq!(by("thumbnails").bytes, 25);
    }

    #[test]
    fn rows_for_running_apps_say_so() {
        let m = fake_machine();
        let result = do_scan(&m, &["discord.exe", "WOW.EXE"]);
        let by = |id: &str| result.providers.iter().find(|p| p.id == id).unwrap();

        assert_eq!(by("discord").running, vec!["Discord.exe"]);
        assert_eq!(by("wow").running, vec!["Wow.exe"]);
        assert!(by("nvidia").running.is_empty());
    }

    #[test]
    fn scan_reports_each_provider_as_it_is_checked() {
        let m = fake_machine();
        let steps = std::sync::Mutex::new(Vec::new());
        scan_with(
            &m.ctx,
            None,
            vec![],
            &HashSet::new(),
            &live(&[], 0),
            &|s: ScanStep| steps.lock().unwrap().push((s.checked, s.total)),
        );

        let mut seen = steps.into_inner().unwrap();
        seen.sort_unstable();
        let total = PROVIDERS.len() as u32;
        assert_eq!(seen.len() as u32, total);
        assert_eq!(seen.first(), Some(&(1, total)));
        assert_eq!(seen.last(), Some(&(total, total)));
    }

    #[test]
    fn preview_measures_and_deletes_nothing() {
        let m = fake_machine();
        let run = do_clean(&m, &["nvidia", "windows", "discord", "temp"], true);

        assert_eq!(run.result.freed, 1500 + 200 + 300 + 60);
        assert!(!run.shader_cleaned, "a preview must not count as a clean");
        assert!(m.nvidia.join("a.nvph").exists());
        assert!(m.d3d.join("shader.bin").exists());
        assert!(m.temp_old.exists());
    }

    #[test]
    fn a_real_clean_frees_exactly_the_chosen_rows_and_nothing_else() {
        let m = fake_machine();
        let mut events = Vec::new();
        let run = clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(&["nvidia", "windows"]),
            false,
            false,
            &AtomicBool::new(false),
            |p| events.push((p.id, p.done)),
        )
        .unwrap();

        assert_eq!(run.result.freed, 1700);
        assert_eq!(run.result.failed_files, 0);
        assert!(run.shader_cleaned);
        assert!(m.nvidia.is_dir() && fs::read_dir(&m.nvidia).unwrap().count() == 0);
        assert!(m.d3d.is_dir() && fs::read_dir(&m.d3d).unwrap().count() == 0);

        // Unchosen and unrelated data is untouched.
        assert!(m.installer.join("setup").join("big.cab").exists());
        assert!(m.settings.exists());
        assert!(m.save.exists());
        assert!(m.discord_cache.exists());

        // A start then a finish for each row. Updates in between are
        // throttled, so only the bookends are guaranteed.
        assert_eq!(events.first(), Some(&("nvidia".to_string(), false)));
        assert_eq!(events.last(), Some(&("windows".to_string(), true)));
        assert!(events.contains(&("nvidia".to_string(), true)));
        assert!(events.contains(&("windows".to_string(), false)));
    }

    #[test]
    fn launcher_clean_keeps_the_login_and_the_game_settings() {
        let m = fake_machine();
        let run = do_clean(&m, &["discord", "wow"], false);

        assert_eq!(run.result.freed, 350);
        assert!(!m.discord_cache.join("Cache_Data").join("data_1").exists());
        assert!(
            m.discord_login.exists(),
            "Discord's login must survive a cache clean"
        );
        assert!(!m.wow_cache.join("ADB").join("DBCache.bin").exists());
        assert!(
            m.wow_settings.exists(),
            "WoW settings must survive a cache clean"
        );
    }

    #[test]
    fn temp_files_in_use_right_now_survive_and_are_counted() {
        let m = fake_machine();
        let run = do_clean(&m, &["temp"], false);

        assert_eq!(run.result.freed, 60);
        assert_eq!(run.result.providers[0].skipped_recent, 1);
        assert!(!m.temp_old.exists());
        assert!(
            m.temp_fresh.exists(),
            "a file changed minutes ago may belong to a running installer"
        );
    }

    #[test]
    fn the_thumbnail_clean_leaves_the_rest_of_the_explorer_folder_alone() {
        let m = fake_machine();
        let run = do_clean(&m, &["thumbnails"], false);

        assert_eq!(run.result.freed, 25);
        assert!(!m.explorer_thumb.exists());
        assert!(
            m.explorer_other.exists(),
            "Explorer's other files are not a cache"
        );
    }

    #[test]
    fn installers_are_cleaned_when_asked_but_are_not_a_shader_clean() {
        let m = fake_machine();
        let run = do_clean(&m, &["installers"], false);

        assert_eq!(run.result.freed, 4000);
        assert!(
            !run.shader_cleaned,
            "installer leftovers are not a shader clean"
        );
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
            &AtomicBool::new(false),
            |_| {},
        )
        .err()
        .unwrap();

        assert!(err.contains("everything"));
        assert!(m.nvidia.join("a.nvph").exists());
    }

    #[test]
    fn cancelling_stops_the_run_and_does_not_count_it_as_a_finished_clean() {
        let m = fake_machine();
        let run = clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(&["nvidia", "windows"]),
            false,
            false,
            &AtomicBool::new(true),
            |_| {},
        )
        .unwrap();

        assert!(run.result.cancelled);
        assert!(!run.shader_cleaned);
        assert!(m.nvidia.join("a.nvph").exists(), "nothing ran after cancel");
    }

    #[test]
    fn a_cancel_part_way_through_keeps_what_is_left() {
        let m = fake_machine();
        let cancel = AtomicBool::new(false);
        let run = clean_with(
            &m.ctx,
            &HashSet::new(),
            &ids(&["nvidia", "windows"]),
            false,
            false,
            &cancel,
            |p| {
                // Stop as soon as the first row reports it has begun.
                if p.id == "nvidia" && !p.done {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();

        assert!(run.result.cancelled);
        assert!(!run.shader_cleaned);
        assert!(m.d3d.join("shader.bin").exists());
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
            ..LastClean::default()
        };

        let result = scan_with(
            &m.ctx,
            Some(last),
            vec![adapter],
            &HashSet::new(),
            &live(&[], 0),
            &no_steps(),
        );

        assert_eq!(result.driver_changes.len(), 1);
        assert_eq!(result.last_clean_at, Some(42));
        assert_eq!(result.total_freed, 777);
    }

    #[test]
    fn the_scan_checks_whether_the_restart_removed_the_queued_files() {
        let m = fake_machine();
        let gone = m.nvidia.join("gone-after-restart.nvph");
        let stuck = m.nvidia.join("a.nvph"); // still exists
        let last = LastClean {
            queued: vec![
                QueuedFile {
                    path: gone.to_string_lossy().into_owned(),
                    bytes: 100,
                },
                QueuedFile {
                    path: stuck.to_string_lossy().into_owned(),
                    bytes: 1000,
                },
            ],
            queued_at: 1_000,
            ..LastClean::default()
        };

        let after = scan_with(
            &m.ctx,
            Some(last.clone()),
            vec![],
            &HashSet::new(),
            &live(&[], 2_000),
            &no_steps(),
        );
        let check = after.queue_check.expect("a queue was recorded");
        assert!(check.restarted);
        assert_eq!((check.removed, check.remaining), (1, 1));

        let before = scan_with(
            &m.ctx,
            Some(last),
            vec![],
            &HashSet::new(),
            &live(&[], 500),
            &no_steps(),
        );
        assert!(!before.queue_check.unwrap().restarted);
    }
}
