#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if shadersweep_lib::cli::wants_headless(&args) {
        std::process::exit(shadersweep_lib::cli::run(&args));
    }
    shadersweep_lib::run();
}
