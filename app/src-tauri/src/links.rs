//! The only web addresses the window may open. The page sends a link name,
//! never an address, so it cannot be turned into a way to launch anything.

pub const LINKS: &[(&str, &str)] = &[
    ("github", "https://github.com/Kkthnx"),
    ("website", "https://kkthnx.com/"),
    ("source", "https://github.com/Kkthnx/ShaderSweep"),
    ("issues", "https://github.com/Kkthnx/ShaderSweep/issues"),
    (
        "releases",
        "https://github.com/Kkthnx/ShaderSweep/releases/latest",
    ),
];

pub fn url_for(name: &str) -> Option<&'static str> {
    LINKS.iter().find(|(n, _)| *n == name).map(|(_, url)| *url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_resolve_and_unknown_ones_do_not() {
        assert_eq!(url_for("website"), Some("https://kkthnx.com/"));
        assert_eq!(url_for("github"), Some("https://github.com/Kkthnx"));
        assert_eq!(url_for("calc.exe"), None);
        assert_eq!(url_for("https://example.com"), None);
        assert_eq!(url_for(""), None);
    }

    #[test]
    fn every_link_is_plain_https_with_no_spaces_or_shell_characters() {
        for (name, url) in LINKS {
            assert!(url.starts_with("https://"), "{name}");
            assert!(
                !url.contains([' ', '&', '|', '^', '"', '<', '>', '%']),
                "{name} has a character cmd would act on"
            );
        }
    }
}
