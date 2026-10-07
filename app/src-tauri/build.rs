fn main() {
    // The app clears caches owned by the system and service profiles and
    // queues driver held files in HKLM, so it asks for administrator rights
    // up front instead of failing halfway through a run.
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));

    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run the Tauri build step");
}
