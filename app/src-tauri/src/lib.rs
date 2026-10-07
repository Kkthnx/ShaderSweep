mod clean;
pub mod cli;
mod context;
mod discovery;
mod drivers;
mod engine;
mod fsutil;
mod links;
mod model;
mod native;
mod pending;
mod power;
mod providers;
mod report;
mod safety;
mod state;
mod steam;
mod system;

use tauri::{AppHandle, Emitter};

use crate::model::{CleanResult, ScanResult};

#[tauri::command]
async fn scan(app: AppHandle) -> Result<ScanResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        engine::scan(&|step| {
            let _ = app.emit("scan-step", step);
        })
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn clean(
    app: AppHandle,
    ids: Vec<String>,
    preview: bool,
    queue_locked: bool,
) -> Result<CleanResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        engine::clean(&ids, preview, queue_locked, |p| {
            let _ = app.emit("clean-progress", p);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_clean() {
    engine::request_cancel();
}

#[tauri::command]
async fn reveal(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Only a folder one of the rows resolves to right now may be opened.
        if !engine::is_known_folder(&path) {
            return Err("That folder is not one ShaderSweep manages.".to_string());
        }
        std::process::Command::new("explorer.exe")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens one of the fixed links in the default browser.
#[tauri::command]
fn open_link(name: String) -> Result<(), String> {
    let url = links::url_for(&name).ok_or("That link is not one ShaderSweep offers.")?;
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Starts a restart countdown. Windows asks about unsaved work itself.
#[tauri::command]
fn restart_pc(delay_seconds: u32) -> Result<u32, String> {
    power::request_restart(delay_seconds)?;
    Ok(power::clamp_delay(delay_seconds))
}

#[tauri::command]
fn cancel_restart() -> Result<(), String> {
    power::cancel_restart()
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan,
            clean,
            cancel_clean,
            reveal,
            open_link,
            restart_pc,
            cancel_restart
        ])
        .run(tauri::generate_context!())
        .expect("failed to start ShaderSweep");
}
