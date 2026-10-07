//! Headless mode, for chaining after a driver install or running from a
//! scheduled task. No window opens. The result is written to `last-run.txt`
//! and printed when a console is available.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;

use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};

use crate::engine;
use crate::providers::{self, PROVIDERS};
use crate::system;

pub const EXIT_OK: i32 = 0;
/// Some files could not be removed.
pub const EXIT_PARTIAL: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub preview: bool,
    pub ids: Vec<String>,
    /// The rows were not named, so rows for running apps are left out.
    pub chosen_by_default: bool,
    pub queue_locked: bool,
}

pub const USAGE: &str = "ShaderSweep headless mode

  ShaderSweep.exe --clean [options]

Options
  --preview             Measure only, delete nothing
  --only a,b            Only these rows, for example nvidia,windows
  --include a,b         Add rows that are off by default, for example installers
  --installers          Same as --include installers
  --no-restart-queue    Do not queue driver held files for the next restart
  --help                Show this text

Rows are listed in the window. A row for a program that is running right now,
such as Discord or Steam, is skipped unless you name it with --only.
Exit codes: 0 done, 1 some files were in use, 2 error
The result is saved to %LOCALAPPDATA%\\ShaderSweep\\last-run.txt";

pub fn wants_headless(args: &[String]) -> bool {
    args.iter().any(|a| a == "--clean" || a == "--help")
}

fn names(list: &str) -> Vec<String> {
    list.split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn known(ids: &[String]) -> Result<(), String> {
    match ids.iter().find(|id| providers::find(id).is_none()) {
        Some(bad) => Err(format!("Unknown row: {bad}")),
        None => Ok(()),
    }
}

/// Turns the command line into options, or an error message to show.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut preview = false;
    let mut only: Option<Vec<String>> = None;
    let mut include: Vec<String> = Vec::new();
    let mut queue_locked = true;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--clean" => {}
            "--preview" => preview = true,
            "--installers" => include.push("installers".to_string()),
            "--no-restart-queue" => queue_locked = false,
            "--only" => {
                let list = iter
                    .next()
                    .ok_or("--only needs a list such as nvidia,windows")?;
                only = Some(names(list));
            }
            "--include" => {
                let list = iter
                    .next()
                    .ok_or("--include needs a list such as installers,gpudumps")?;
                include.extend(names(list));
            }
            other => return Err(format!("Unknown option: {other}")),
        }
    }

    known(&include)?;
    let chosen_by_default = only.is_none();
    let ids = match only {
        Some(list) => {
            known(&list)?;
            list
        }
        None => {
            let mut ids: Vec<String> = PROVIDERS
                .iter()
                .filter(|p| p.default_on || include.iter().any(|i| i == p.id))
                .map(|p| p.id.to_string())
                .collect();
            ids.dedup();
            ids
        }
    };

    Ok(Options {
        preview,
        ids,
        chosen_by_default,
        queue_locked,
    })
}

/// Splits the rows into those to run and those to skip because their app is
/// running. A row named with `--only` always runs.
pub fn split_running(options: &Options, running: &HashSet<String>) -> (Vec<String>, Vec<String>) {
    if !options.chosen_by_default {
        return (options.ids.clone(), Vec::new());
    }

    let mut keep = Vec::new();
    let mut skipped = Vec::new();
    for id in &options.ids {
        let busy = providers::find(id)
            .map(|p| !system::running_among(running, p.processes).is_empty())
            .unwrap_or(false);
        if busy {
            skipped.push(id.clone());
        } else {
            keep.push(id.clone());
        }
    }
    (keep, skipped)
}

/// Prints to the console that launched us, if there was one. A GUI process
/// starts without a console, so it has to attach to its parent's first.
fn print(text: &str) {
    // SAFETY: attaching to a parent console has no preconditions. It fails
    // harmlessly when there is no parent console.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;
    if !attached {
        return;
    }
    if let Ok(mut out) = OpenOptions::new().write(true).open("CONOUT$") {
        let _ = writeln!(out, "{text}");
    }
}

pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help") {
        print(USAGE);
        return EXIT_OK;
    }

    let options = match parse(args) {
        Ok(options) => options,
        Err(message) => {
            print(&format!("{message}\n\n{USAGE}"));
            return EXIT_ERROR;
        }
    };

    let (ids, skipped) = split_running(&options, &system::running_processes());

    match engine::clean(&ids, options.preview, options.queue_locked, |_| {}) {
        Ok(result) => {
            let mut text = result.report.clone();
            for id in &skipped {
                let label = providers::find(id).map_or(id.as_str(), |p| p.label);
                text.push_str(&format!("\n- {label}: skipped, the app is running"));
            }
            print(&text);
            if result.failed_files > 0 {
                EXIT_PARTIAL
            } else {
                EXIT_OK
            }
        }
        Err(message) => {
            print(&message);
            EXIT_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plain_clean_uses_the_default_rows() {
        let o = parse(&args(&["--clean"])).unwrap();
        assert!(!o.preview && o.queue_locked && o.chosen_by_default);
        assert!(o.ids.contains(&"nvidia".to_string()));
        assert!(o.ids.contains(&"discord".to_string()));
        for off in ["installers", "recyclebin", "eventlogs", "gpudumps", "wow"] {
            assert!(!o.ids.contains(&off.to_string()), "{off}");
        }
    }

    #[test]
    fn rows_that_start_off_are_opt_in() {
        let o = parse(&args(&["--clean", "--installers"])).unwrap();
        assert!(o.ids.contains(&"installers".to_string()));

        let o = parse(&args(&["--clean", "--include", "gpudumps, WOW"])).unwrap();
        assert!(o.ids.contains(&"gpudumps".to_string()));
        assert!(o.ids.contains(&"wow".to_string()));
        assert!(!o.ids.contains(&"installers".to_string()));
    }

    #[test]
    fn only_picks_exact_rows_and_checks_the_names() {
        let o = parse(&args(&["--clean", "--only", "NVIDIA, windows"])).unwrap();
        assert_eq!(o.ids, vec!["nvidia", "windows"]);
        assert!(!o.chosen_by_default);

        assert!(parse(&args(&["--clean", "--only", "nvidia,nope"])).is_err());
        assert!(parse(&args(&["--clean", "--include", "nope"])).is_err());
        assert!(parse(&args(&["--clean", "--only"])).is_err());
    }

    #[test]
    fn flags_and_unknown_options() {
        let o = parse(&args(&["--clean", "--preview", "--no-restart-queue"])).unwrap();
        assert!(o.preview && !o.queue_locked);
        assert!(parse(&args(&["--clean", "--wat"])).is_err());
    }

    #[test]
    fn only_clean_and_help_start_headless_mode() {
        assert!(wants_headless(&args(&["--clean"])));
        assert!(wants_headless(&args(&["--help"])));
        assert!(!wants_headless(&args(&[])));
        assert!(!wants_headless(&args(&["--preview"])));
    }

    #[test]
    fn default_runs_skip_rows_whose_app_is_running() {
        let o = parse(&args(&["--clean"])).unwrap();
        let running: HashSet<String> = ["discord.exe"].into_iter().map(String::from).collect();

        let (keep, skipped) = split_running(&o, &running);
        assert_eq!(skipped, vec!["discord"]);
        assert!(!keep.contains(&"discord".to_string()));
        assert!(keep.contains(&"nvidia".to_string()));
    }

    #[test]
    fn naming_a_row_runs_it_even_if_its_app_is_running() {
        let o = parse(&args(&["--clean", "--only", "discord"])).unwrap();
        let running: HashSet<String> = ["discord.exe"].into_iter().map(String::from).collect();

        let (keep, skipped) = split_running(&o, &running);
        assert_eq!(keep, vec!["discord"]);
        assert!(skipped.is_empty());
    }
}
