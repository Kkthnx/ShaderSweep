//! Headless mode, for chaining after a driver install or running from a
//! scheduled task. No window opens. The result is written to `last-run.txt`
//! and printed when a console is available.

use std::fs::OpenOptions;
use std::io::Write;

use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};

use crate::engine;
use crate::providers::PROVIDERS;

pub const EXIT_OK: i32 = 0;
/// Some files could not be removed.
pub const EXIT_PARTIAL: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub preview: bool,
    pub ids: Vec<String>,
    pub queue_locked: bool,
}

pub const USAGE: &str = "ShaderSweep headless mode

  ShaderSweep.exe --clean [options]

Options
  --preview             Measure only, delete nothing
  --only a,b            Only these rows, for example nvidia,windows
  --installers          Also clear driver installer leftovers
  --no-restart-queue    Do not queue driver held files for the next restart
  --help                Show this text

Rows: nvidia, amd, intel, windows, steam, installers
Exit codes: 0 done, 1 some files were in use, 2 error
The result is saved to %LOCALAPPDATA%\\ShaderSweep\\last-run.txt";

pub fn wants_headless(args: &[String]) -> bool {
    args.iter().any(|a| a == "--clean" || a == "--help")
}

/// Turns the command line into options, or an error message to show.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut preview = false;
    let mut only: Option<Vec<String>> = None;
    let mut installers = false;
    let mut queue_locked = true;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--clean" => {}
            "--preview" => preview = true,
            "--installers" => installers = true,
            "--no-restart-queue" => queue_locked = false,
            "--only" => {
                let list = iter
                    .next()
                    .ok_or("--only needs a list such as nvidia,windows")?;
                only = Some(
                    list.split(',')
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect(),
                );
            }
            other => return Err(format!("Unknown option: {other}")),
        }
    }

    let ids = match only {
        Some(list) => {
            if let Some(bad) = list
                .iter()
                .find(|id| !PROVIDERS.iter().any(|p| p.id == *id))
            {
                return Err(format!("Unknown row: {bad}"));
            }
            list
        }
        None => PROVIDERS
            .iter()
            .filter(|p| p.default_on || (installers && p.id == "installers"))
            .map(|p| p.id.to_string())
            .collect(),
    };

    Ok(Options {
        preview,
        ids,
        queue_locked,
    })
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

    match engine::clean(&options.ids, options.preview, options.queue_locked, |_| {}) {
        Ok(result) => {
            print(&result.report);
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
        assert!(!o.preview && o.queue_locked);
        assert!(o.ids.contains(&"nvidia".to_string()));
        assert!(!o.ids.contains(&"installers".to_string()));
    }

    #[test]
    fn installers_are_opt_in() {
        let o = parse(&args(&["--clean", "--installers"])).unwrap();
        assert!(o.ids.contains(&"installers".to_string()));
    }

    #[test]
    fn only_picks_exact_rows_and_checks_the_names() {
        let o = parse(&args(&["--clean", "--only", "NVIDIA, windows"])).unwrap();
        assert_eq!(o.ids, vec!["nvidia", "windows"]);

        assert!(parse(&args(&["--clean", "--only", "nvidia,nope"])).is_err());
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
}
