mod clean;
mod context;
mod discovery;
mod drivers;
mod engine;
mod fsutil;
mod model;
mod native;
mod pending;
mod providers;
mod safety;
mod state;
mod steam;

use tauri::{AppHandle, Emitter};

use crate::model::{CleanResult, ScanResult};

#[tauri::command]
async fn scan() -> Result<ScanResult, String> {
    tauri::async_runtime::spawn_blocking(engine::scan)
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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan, clean])
        .run(tauri::generate_context!())
        .expect("failed to start ShaderSweep");
}
