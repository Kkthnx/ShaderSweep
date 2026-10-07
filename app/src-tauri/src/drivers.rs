//! Reads the installed display drivers from the registry, so the app can tell
//! when a driver changed since the last clean. No WMI and no helper process.

use serde::Serialize;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Adapter {
    pub vendor: String,
    pub name: String,
    /// The version people recognise, for example `616.92` for NVIDIA.
    pub version: String,
    pub date: String,
}

impl Adapter {
    /// Stable key for the "last cleaned" record.
    pub fn key(&self) -> String {
        format!("{}:{}", self.vendor, self.name)
    }
}

/// Windows reports an NVIDIA driver as `32.0.16.1692`. The number on the box
/// is the last digit of the third part followed by the fourth, with a dot
/// before the final two digits, so that one is `616.92`.
pub fn nvidia_version(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let tail = parts[2].chars().last()?;
    let digits = format!("{tail}{:0>4}", parts[3]);
    if digits.len() < 3 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let split = digits.len() - 2;
    Some(format!("{}.{}", &digits[..split], &digits[split..]))
}

/// Registry dates look like `9-4-2026` (month, day, year).
pub fn iso_date(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('-').collect();
    if let [m, d, y] = parts[..] {
        if let (Ok(m), Ok(d), Ok(y)) = (m.parse::<u32>(), d.parse::<u32>(), y.parse::<u32>()) {
            return format!("{y:04}-{m:02}-{d:02}");
        }
    }
    raw.to_string()
}

fn vendor_of(provider: &str, name: &str) -> Option<&'static str> {
    let haystack = format!("{provider} {name}").to_ascii_lowercase();
    if haystack.contains("nvidia") {
        Some("nvidia")
    } else if haystack.contains("advanced micro devices") || haystack.contains("radeon") {
        Some("amd")
    } else if haystack.contains("intel") {
        Some("intel")
    } else {
        None
    }
}

pub fn installed() -> Vec<Adapter> {
    let Ok(class) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(DISPLAY_CLASS) else {
        return Vec::new();
    };

    let mut adapters = Vec::new();
    for sub in class.enum_keys().flatten() {
        // Instances are numbered `0000`, `0001` and so on. `Properties` and
        // `Configuration` sit beside them and are not adapters.
        if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(key) = class.open_subkey(&sub) else {
            continue;
        };
        let name: String = key.get_value("DriverDesc").unwrap_or_default();
        let provider: String = key.get_value("ProviderName").unwrap_or_default();
        let raw: String = key.get_value("DriverVersion").unwrap_or_default();
        let date: String = key.get_value("DriverDate").unwrap_or_default();

        let Some(vendor) = vendor_of(&provider, &name) else {
            continue;
        };
        let version = if vendor == "nvidia" {
            nvidia_version(&raw).unwrap_or_else(|| raw.clone())
        } else {
            raw.clone()
        };

        adapters.push(Adapter {
            vendor: vendor.to_string(),
            name,
            version,
            date: iso_date(&date),
        });
    }
    adapters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_nvidia_versions() {
        assert_eq!(nvidia_version("32.0.16.1692").as_deref(), Some("616.92"));
        assert_eq!(nvidia_version("32.0.15.6094").as_deref(), Some("560.94"));
        assert_eq!(nvidia_version("31.0.15.3623").as_deref(), Some("536.23"));
        assert_eq!(nvidia_version("30.0.14.7168").as_deref(), Some("471.68"));
    }

    #[test]
    fn rejects_malformed_nvidia_versions() {
        assert_eq!(nvidia_version(""), None);
        assert_eq!(nvidia_version("1.2.3"), None);
        assert_eq!(nvidia_version("32.0.16.abcd"), None);
    }

    #[test]
    fn reorders_registry_dates() {
        assert_eq!(iso_date("9-4-2026"), "2026-09-04");
        assert_eq!(iso_date("12-31-2025"), "2025-12-31");
        assert_eq!(iso_date("garbage"), "garbage");
    }

    #[test]
    fn recognises_vendors_and_skips_the_rest() {
        assert_eq!(
            vendor_of("NVIDIA", "NVIDIA GeForce RTX 5070"),
            Some("nvidia")
        );
        assert_eq!(
            vendor_of("Advanced Micro Devices, Inc.", "AMD Radeon RX 9070"),
            Some("amd")
        );
        assert_eq!(
            vendor_of("Intel Corporation", "Intel(R) Arc(TM) B580"),
            Some("intel")
        );
        assert_eq!(
            vendor_of("Microsoft", "Microsoft Basic Display Adapter"),
            None
        );
    }
}
