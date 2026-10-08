//! tray/testkit.rs — shared helpers for source-scan tests (#756).
#![allow(dead_code)]
use std::sync::OnceLock;
static TRAY_PROD: OnceLock<String> = OnceLock::new();
/// Production half of `src` — everything before the inline test module,
/// so a scan can never match the assertions themselves.
pub fn prod_source(src: &str) -> &str {
    src.split("#[cfg(test)]\nmod tests").next().unwrap_or(src)
}
/// Drops `//` line comments so a source-scan assertion is not fooled by
/// prose that quotes the very construct it forbids.
pub fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// Comment-stripped, brace-counted body isolation for `sig`'s fn.
/// Order-independent: never anchor on the next fn.
pub fn body_of(prod: &str, sig: &str) -> String {
    let stripped = strip_line_comments(prod);
    let after_sig = stripped
        .split(sig)
        .nth(1)
        .unwrap_or_else(|| panic!("tray source has no `{}`", sig));
    let open = after_sig
        .find('{')
        .unwrap_or_else(|| panic!("{} has no opening brace", sig));
    let mut depth = 0usize;
    let mut end = None;
    for (i, ch) in after_sig[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))].to_string()
}
/// Concatenated production halves of all six tray modules, so source-scan
/// guards keep resolving after the #756 split no matter which file owns
/// the scanned fn.
pub fn tray_prod_source() -> &'static str {
    TRAY_PROD.get_or_init(|| {
        [
            include_str!("mod.rs"),
            include_str!("cache.rs"),
            include_str!("dedup.rs"),
            include_str!("snooze.rs"),
            include_str!("devices.rs"),
            include_str!("actions.rs"),
        ]
        .iter()
        .map(|s| prod_source(s))
        .collect::<Vec<_>>()
        .join("\n")
    })
}
