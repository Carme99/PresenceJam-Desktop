//! Accepted-risk currency guard (issue #1197).
//!
//! `SECURITY.md` records `glib` 0.18.5 (RUSTSEC-2024-0429) as an accepted
//! transitive risk on the grounds that it is **unfixable on Tauri 2.x**: the
//! whole gtk-rs stack is pinned at the 0.18 generation because `tauri`,
//! `tauri-runtime-wry`, `wry`, `tao`, `webkit2gtk` and friends all take a
//! `gtk = "^0.18"` dependency, so `glib` cannot move alone.
//!
//! That reasoning is correct today, but it is *conditional* — it expires the
//! moment the ecosystem moves. A note in a markdown file cannot notice that;
//! this test can. It fails as soon as the locked tree resolves a `glib` outside
//! the vulnerable range, which is the signal to delete the accepted-risk entry
//! from `SECURITY.md` rather than carry a stale exception forever.
//!
//! The assertion is deliberately one-directional. It does NOT fail while the
//! vulnerable version is still present — that is the documented, accepted
//! state. It fails only when the acceptance has become obsolete, i.e. when a
//! future `cargo update` (or a Tauri 3 migration) finally lifts `glib` past
//! 0.20.0.
//!
//! Re-derive the range from the advisory rather than trusting this comment:
//! `https://rustsec.org/packages/glib.json` → `RUSTSEC-2024-0429` →
//! `affected.ranges` = `introduced 0.15.0`, `fixed 0.20.0`.

use std::path::PathBuf;

/// The first `glib` version that carries the RUSTSEC-2024-0429 fix.
const GLIB_FIXED_IN: (u64, u64, u64) = (0, 20, 0);

/// Parses `name = "glib"` + the following `version = "x.y.z"` out of a
/// `Cargo.lock`. Returns `None` when the package is absent (a target that does
/// not build the Linux GTK stack, e.g. Windows or macOS).
///
/// A hand-rolled scan rather than a toml dev-dependency on purpose: this test
/// must not add a crate to the build graph it is auditing.
fn locked_glib_versions(lockfile: &str) -> Vec<(u64, u64, u64)> {
    let mut versions = Vec::new();
    let mut in_glib_block = false;
    for line in lockfile.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            in_glib_block = false;
            continue;
        }
        if trimmed == "name = \"glib\"" {
            in_glib_block = true;
            continue;
        }
        if in_glib_block {
            if let Some(rest) = trimmed.strip_prefix("version = \"") {
                let raw = rest.trim_end_matches('"');
                let mut parts = raw.split('.');
                let Some(major) = parts.next().and_then(|p| p.parse().ok()) else {
                    continue;
                };
                let Some(minor) = parts.next().and_then(|p| p.parse().ok()) else {
                    continue;
                };
                let patch_raw = parts.next().unwrap_or("0");
                let patch_digits: String = patch_raw
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                let patch = patch_digits.parse().unwrap_or(0);
                versions.push((major, minor, patch));
                in_glib_block = false;
            }
        }
    }
    versions
}

/// The version the guard must act on: the NEWEST `glib` in the tree.
///
/// Returning the first block was a real bug. A Cargo.lock can legitimately hold
/// two `glib` versions at once, and `cargo` writes duplicate package names in
/// ASCENDING version order — so during a partial gtk-rs migration (some crates
/// moved to 0.20+, others not yet) the first block is the OLD one and the guard
/// would read `0.18.5` while `0.22.x` sat below it, passing exactly when the
/// acceptance had become stale. Taking the maximum is the only reading that
/// matches the question the guard asks: "is any of this tree past the fix?"
fn newest_glib(lockfile: &str) -> Option<(u64, u64, u64)> {
    locked_glib_versions(lockfile).into_iter().max()
}

/// The decision half, kept pure so both branches are directly testable.
///
/// `Some(reason)` means the accepted risk in `SECURITY.md` no longer describes
/// the tree and must be removed. `None` means the acceptance is still in force.
fn stale_acceptance_reason(version: Option<(u64, u64, u64)>) -> Option<String> {
    match version {
        None => Some(
            "no `glib` package in Cargo.lock: the RUSTSEC-2024-0429 accepted risk in \
             SECURITY.md no longer describes this tree — remove it (issue #1197)"
                .to_string(),
        ),
        Some(v) if v >= GLIB_FIXED_IN => Some(format!(
            "`glib` resolves to {}.{}.{}, at or above the {}.{}.{} fix for \
             RUSTSEC-2024-0429. The accepted risk in SECURITY.md (\"glib 0.18.5 — \
             accepted, unfixable on Tauri 2.x\") is now obsolete: delete that entry \
             and the matching commentary in the `dep-audit` job of ci.yml \
             (issue #1197).",
            v.0, v.1, v.2, GLIB_FIXED_IN.0, GLIB_FIXED_IN.1, GLIB_FIXED_IN.2
        )),
        Some(_) => None,
    }
}

#[test]
fn glib_accepted_risk_is_still_needed_or_the_acceptance_must_go() {
    let lock_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let lockfile = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", lock_path.display()));

    let version = newest_glib(&lockfile);
    if let Some(reason) = stale_acceptance_reason(version) {
        panic!("{reason}");
    }

    // The acceptance is still live, so pin the fact it rests on: the tree must
    // carry a glib inside the vulnerable range for the SECURITY.md wording to
    // be accurate.
    let (major, minor, patch) = version.expect("checked Some above");
    assert!(
        (major, minor, patch) >= (0, 15, 0),
        "glib {major}.{minor}.{patch} predates the 0.15.0 introduction point claimed in \
         SECURITY.md — the advisory wording needs re-checking"
    );
}

#[test]
fn stale_acceptance_fires_when_glib_moves_past_the_fix() {
    // The guard's whole value is that it CAN fail. Prove it on the exact
    // version a future Cargo.lock would carry once the gtk-rs stack moves.
    let reason = stale_acceptance_reason(Some((0, 22, 10)))
        .expect("glib 0.22.10 must be reported as making the acceptance stale");
    assert!(
        reason.contains("RUSTSEC-2024-0429"),
        "the message must name the advisory so the follow-up is obvious: {reason}"
    );

    // And the exact fixed version, not just a later one.
    assert!(
        stale_acceptance_reason(Some(GLIB_FIXED_IN)).is_some(),
        "the fix version itself must count as fixed (range is `fixed 0.20.0`)"
    );

    // One patch below the fix stays accepted — the boundary is exclusive.
    assert_eq!(
        stale_acceptance_reason(Some((0, 19, 99))),
        None,
        "0.19.99 is still vulnerable, so the acceptance is still in force"
    );

    // A tree without glib at all also invalidates the entry.
    assert!(stale_acceptance_reason(None).is_some());
}

#[test]
fn locked_glib_version_reads_a_real_lockfile_fragment() {
    // Pins the parser against the exact shape `cargo` writes, so a future
    // reformat of Cargo.lock cannot make the guard above pass vacuously by
    // returning `None` for a present package.
    let fragment = r#"
[[package]]
name = "gio"
version = "0.18.4"

[[package]]
name = "glib"
version = "0.18.5"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "0000"

[[package]]
name = "glib-macros"
version = "0.18.5"
"#;
    assert_eq!(newest_glib(fragment), Some((0, 18, 5)));

    // Must not confuse `glib-macros` for `glib`, and must not pick up the
    // version of a neighbouring package.
    assert_eq!(
        newest_glib("[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n"),
        None
    );

    let modern = "[[package]]\nname = \"glib\"\nversion = \"0.22.10\"\n";
    assert_eq!(newest_glib(modern), Some((0, 22, 10)));
}

/// A Cargo.lock can hold two `glib` versions at once, and `cargo` writes
/// duplicate package names in ASCENDING order. During the gtk-rs migration
/// the first block is therefore the OLD 0.18.x and the fixed one sits below
/// it — the shape in which the original "return the first block" parser
/// read 0.18.5 and passed while 0.22.x was present.
#[test]
fn newest_glib_wins_when_the_lockfile_holds_two_versions() {
    let ascending = "[[package]]\nname = \"glib\"\nversion = \"0.18.5\"\n\n[[package]]\nname = \"glib\"\nversion = \"0.22.10\"\n";
    assert_eq!(locked_glib_versions(ascending).len(), 2);
    assert_eq!(
        newest_glib(ascending),
        Some((0, 22, 10)),
        "the newest block must win, regardless of file order"
    );
    assert!(
        stale_acceptance_reason(newest_glib(ascending)).is_some(),
        "a tree holding a fixed glib alongside the old one makes the \
         SECURITY.md acceptance stale — the guard must fire"
    );

    // Descending order must give the same answer: file order is irrelevant.
    let descending = "[[package]]\nname = \"glib\"\nversion = \"0.22.10\"\n\n[[package]]\nname = \"glib\"\nversion = \"0.18.5\"\n";
    assert_eq!(newest_glib(descending), Some((0, 22, 10)));

    // And two vulnerable versions still read as the highest of them.
    let both_old = "[[package]]\nname = \"glib\"\nversion = \"0.18.5\"\n\n[[package]]\nname = \"glib\"\nversion = \"0.19.2\"\n";
    assert_eq!(newest_glib(both_old), Some((0, 19, 2)));
    assert!(
        stale_acceptance_reason(newest_glib(both_old)).is_none(),
        "0.19.2 is still vulnerable, so the acceptance stands"
    );
}
