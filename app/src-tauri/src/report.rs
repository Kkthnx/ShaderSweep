//! Plain text summary of a run. The same text is copied from the window,
//! saved as `last-run.txt` and printed by the headless mode.

use crate::drivers::Adapter;
use crate::model::CleanResult;

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// `2026-10-07 20:15 UTC` from seconds since the Unix epoch.
pub fn utc_stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;

    // Civil date from a day count, after Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3_600,
        (rem % 3_600) / 60
    )
}

pub fn build(
    result: &CleanResult,
    adapters: &[Adapter],
    version: &str,
    now: u64,
    label_of: impl Fn(&str) -> String,
) -> String {
    let mut lines = vec![
        format!("ShaderSweep {version}"),
        format!(
            "{} on {}",
            if result.preview {
                "Preview, nothing was deleted"
            } else {
                "Clean"
            },
            utc_stamp(now)
        ),
    ];

    for a in adapters {
        lines.push(format!(
            "GPU: {}, driver {} ({})",
            a.name, a.version, a.date
        ));
    }

    let verb = if result.preview {
        "Would free"
    } else {
        "Freed"
    };
    lines.push(format!("{verb}: {}", format_bytes(result.freed)));
    if result.cancelled {
        lines.push("Stopped early at the user's request".to_string());
    }

    for item in &result.providers {
        let mut parts = vec![format!(
            "{} {}",
            if result.preview {
                "would free"
            } else {
                "freed"
            },
            format_bytes(item.freed)
        )];
        if !result.preview {
            parts.push(format!("{} files removed", item.removed_files));
        }
        if item.queued_files > 0 {
            parts.push(format!(
                "{} queued for restart ({})",
                item.queued_files,
                format_bytes(item.queued_bytes)
            ));
        }
        if item.failed_files > 0 {
            parts.push(format!(
                "{} in use ({})",
                item.failed_files,
                format_bytes(item.failed_bytes)
            ));
        }
        if item.skipped_recent > 0 {
            parts.push(format!("{} recent files left alone", item.skipped_recent));
        }
        lines.push(format!("- {}: {}", label_of(&item.id), parts.join(", ")));
        if !item.holders.is_empty() {
            lines.push(format!("  held by: {}", item.holders.join(", ")));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProviderResult;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(10_568_519_352), "9.84 GB");
    }

    #[test]
    fn formats_known_dates() {
        assert_eq!(utc_stamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc_stamp(1_791_403_568), "2026-10-07 20:06 UTC");
    }

    #[test]
    fn report_lists_each_row_and_who_holds_files() {
        let result = CleanResult {
            preview: false,
            providers: vec![ProviderResult {
                id: "nvidia".into(),
                freed: 2048,
                removed_files: 3,
                queued_files: 2,
                queued_bytes: 100,
                failed_files: 1,
                failed_bytes: 50,
                skipped_recent: 4,
                holders: vec!["game.exe".into()],
            }],
            freed: 2048,
            queued_files: 2,
            queued_bytes: 100,
            failed_files: 1,
            cancelled: true,
            report: String::new(),
        };
        let adapter = Adapter {
            vendor: "nvidia".into(),
            name: "RTX 5070".into(),
            version: "616.92".into(),
            date: "2026-09-04".into(),
        };

        let text = build(&result, &[adapter], "2.0.0", 0, |_| {
            "NVIDIA shader cache".into()
        });

        assert!(text.contains("ShaderSweep 2.0.0"));
        assert!(text.contains("GPU: RTX 5070, driver 616.92 (2026-09-04)"));
        assert!(text.contains("Freed: 2.00 KB"));
        assert!(text.contains("- NVIDIA shader cache: freed 2.00 KB, 3 files removed, 2 queued for restart (100 B), 1 in use (50 B)"));
        assert!(text.contains("4 recent files left alone"));
        assert!(text.contains("Stopped early"));
        assert!(text.contains("held by: game.exe"));
    }
}
