pub mod clash;
pub mod generate;
pub mod share_link;
pub mod store;
pub mod vless_compat;

/// Protocol label for an entry the parsers had to drop, so the import can say
/// *what* it skipped instead of silently reporting a smaller count.
///
/// The value comes from the subscription, so it is clamped here rather than at
/// each display site: lowercase, `[a-z0-9+._-]` only, and short enough that a
/// crafted feed cannot turn the import log into a wall of text.
pub fn skipped_label(raw: &str) -> String {
    let s: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '_' | '-'))
        .take(16)
        .collect();
    if s.is_empty() {
        "unknown".into()
    } else {
        s
    }
}

/// Scheme of a share URI (`hysteria2://…` → `hysteria2`), for [`skipped_label`].
pub fn scheme_of(line: &str) -> String {
    skipped_label(line.split("://").next().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_clamped_not_echoed() {
        assert_eq!(scheme_of("hysteria2://pw@h:443#x"), "hysteria2");
        assert_eq!(scheme_of("naive+https://a@b:443"), "naive+https");
        // A line with no scheme, and one trying to smuggle markup or bulk text.
        assert_eq!(scheme_of("garbage"), "garbage");
        assert_eq!(skipped_label("<img src=x onerror=1>"), "unknown");
        assert_eq!(skipped_label(&"a".repeat(500)).len(), 16);
        assert_eq!(skipped_label("   "), "unknown");
    }
}
