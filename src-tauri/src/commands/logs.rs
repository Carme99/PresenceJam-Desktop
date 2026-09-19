//! On-disk log-tail command backing the LogViewer pane (issue #595).
//!
//! The LogViewer renders `log://log` events streamed by `tauri-plugin-log`,
//! so a pane opened after the app had been running stayed empty until the
//! next live event even though `PresenceJam.log` already held the session's
//! history — and the "Copy snapshot" button in the same toolbar pasted that
//! very history, so the two surfaces contradicted each other. This module
//! exposes the tail so the pane can seed its buffer on mount and then hand
//! authority back to the live stream.
//!
//! # Redaction policy (issue #595 decision: raw, not redacted)
//!
//! [`crate::diagnostics::tail_log_file`] redacts its tail because its
//! consumer is the Copy-snapshot button and that artifact is pasted into
//! public bug reports (issues #434/#487). This command's consumer is the
//! local Logs pane showing the same file the user can already open with
//! `open_logs_folder`, on the same machine, with no egress path — redacting
//! here would only make the viewer disagree with the file it claims to
//! display, and would hide from the user the very lines they opened the
//! pane to read. The redacted snapshot therefore stays the single
//! paste-able surface; this one is never pasted.
//!
//! # Bounds
//!
//! `limit` is clamped into `1..=`[`MAX_LOG_LINES`] and the read itself
//! never touches more than the last [`LOG_TAIL_MAX_BYTES`] bytes, so one
//! invoke cannot materialise an unbounded tail. The read runs on the
//! blocking pool (#215 convention) — the UI thread never waits on the file.

use std::path::Path;

use tauri::{AppHandle, Manager};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.LOGS]";

/// Log file written by `tauri-plugin-log`'s `LogDir` target
/// (`file_name: Some("PresenceJam")` — see `lib.rs::run`). Same file
/// `diagnostics::LOG_FILE_NAME` names; `test_log_file_name_matches_diagnostics`
/// keeps the two in step.
const LOG_FILE_NAME: &str = "PresenceJam.log";

/// Hard ceiling on returned lines. Matches the LogViewer's 500-entry buffer
/// (#399) — lines beyond it would be discarded by the pane immediately.
const MAX_LOG_LINES: usize = 500;

/// Cap on how many trailing bytes are read before splitting lines. The
/// plugin rotates this file at `logging.max_file_size_mb` (default `10`,
/// i.e. 10 MB — `config::default_max_file_size_mb`), so this is a safety
/// net for a hand-grown file rather than the normal case; the line clamp
/// above is what bounds the payload.
const LOG_TAIL_MAX_BYTES: u64 = 256 * 1024;

/// Read up to `limit` trailing lines of `path`, oldest first.
///
/// A missing file is `Ok(empty)` — that is the normal first run, not an
/// error. Any other failure to stat or read is `Err`, carrying only the bare
/// file name: the absolute path embeds the OS username (issue #409 hygiene)
/// and the message can surface in the webview console.
fn read_log_tail(path: &Path, limit: usize, max_bytes: u64) -> Result<Vec<String>, String> {
    // `Path::exists()` collapses *every* stat error to `false`, which
    // reported a permission or I/O failure as "no log file yet" — an empty
    // pane with nothing logged anywhere (issue #825). Only `NotFound` is the
    // first-run case; the one stat call also supplies the length.
    let len = match std::fs::metadata(path) {
        Ok(md) => md.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("error reading {LOG_FILE_NAME}: {e}")),
    };
    let start = len.saturating_sub(max_bytes);
    let (bytes, on_record_boundary) =
        read_from_offset(path, start).map_err(|e| format!("error reading {LOG_FILE_NAME}: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    // Seeking mid-file lands inside a line; that fragment is not a log
    // record and would render as a truncated one — drop it. A seek that
    // landed exactly on a newline is at a record boundary though, and the
    // first split line is then whole: dropping it would lose a complete
    // record, so the drop is conditional on the byte before the window
    // (issue #824).
    if start > 0 && !lines.is_empty() && !on_record_boundary {
        lines.remove(0);
    }
    let first = lines.len().saturating_sub(limit);
    Ok(lines[first..].iter().map(|l| (*l).to_string()).collect())
}

/// Offset read, so the byte cap above is actually honoured — `fs::read`
/// would pull the whole file in before any bound could apply.
///
/// The same open also probes the byte at `offset - 1`: a newline there means
/// the window starts on a record boundary and its first line is whole
/// (issue #824). Returns the window plus that flag.
fn read_from_offset(path: &Path, offset: u64) -> Result<(Vec<u8>, bool), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let on_record_boundary = if offset == 0 {
        true
    } else {
        f.seek(SeekFrom::Start(offset - 1))
            .map_err(|e| e.to_string())?;
        let mut prev = [0u8; 1];
        // A short read (a rotation truncating the file under us) is not a
        // newline: fall back to dropping the first fragment, as before.
        let n = f.read(&mut prev).map_err(|e| e.to_string())?;
        n == 1 && prev[0] == b'\n'
    };
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok((buf, on_record_boundary))
}

/// Clamp a caller-supplied line count into `1..=`[`MAX_LOG_LINES`].
/// `None` (no argument) asks for the full ceiling.
fn clamp_limit(requested: Option<usize>) -> usize {
    requested.unwrap_or(MAX_LOG_LINES).clamp(1, MAX_LOG_LINES)
}

/// Tail of the on-disk log, oldest first, for the LogViewer's mount-time
/// backfill (issue #595). Read-only, local-only, no network, no redaction —
/// see the module docs for why raw is the right policy here.
///
/// The live `log://log` stream stays authoritative for everything logged
/// after this call; this command only answers "what already happened".
#[tauri::command]
pub async fn get_recent_logs(app: AppHandle, limit: Option<usize>) -> Result<Vec<String>, String> {
    let limit = clamp_limit(limit);
    log::info!("{CMD} get_recent_logs: ENTRY - limit={limit}");
    // #215 convention: filesystem IO goes to the blocking pool, so a pane
    // opening against a large log file never stalls the UI thread.
    let result = tauri::async_runtime::spawn_blocking(move || match app.path().app_log_dir() {
        Ok(dir) => read_log_tail(&dir.join(LOG_FILE_NAME), limit, LOG_TAIL_MAX_BYTES),
        Err(e) => Err(format!("could not resolve app log dir: {e}")),
    })
    .await
    .map_err(|e| format!("get_recent_logs spawn_blocking panicked: {e:?}"))?;
    match result {
        Ok(lines) => {
            log::info!("{CMD} get_recent_logs: SUCCESS - {} lines", lines.len());
            Ok(lines)
        }
        Err(e) => {
            log::error!("{CMD} get_recent_logs: FAILED - {e}");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scratch dir per test, keyed by name so parallel tests never share a
    /// file (same shape as the diagnostics tail tests).
    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pj-logs-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[test]
    fn test_read_log_tail_missing_file_is_empty_not_error() {
        let dir = scratch_dir("missing");
        assert_eq!(
            read_log_tail(&dir.join(LOG_FILE_NAME), 10, LOG_TAIL_MAX_BYTES),
            Ok(Vec::new()),
            "first run has no log file yet — that must not be an error"
        );
    }

    /// A stat failure other than `NotFound` must surface, not pose as a
    /// first run: an empty pane with no error is the one failure the log
    /// viewer must not have (issue #825). A self-referential symlink gives a
    /// deterministic `ELOOP` here, where a mode-based `EACCES` would not be
    /// observable for a root-run test process.
    #[cfg(unix)]
    #[test]
    fn test_read_log_tail_stat_failure_is_an_error_not_an_empty_history() {
        let dir = scratch_dir("statfail");
        let path = dir.join(LOG_FILE_NAME);
        std::os::unix::fs::symlink(&path, &path).expect("create self-referential symlink");

        let result = read_log_tail(&path, 10, LOG_TAIL_MAX_BYTES);
        let err = result.expect_err("a stat failure must not look like a missing log");
        assert!(
            err.starts_with(&format!("error reading {LOG_FILE_NAME}:")),
            "the message names the file and nothing else: {err}"
        );
        assert!(
            !err.contains(dir.to_string_lossy().as_ref()),
            "no absolute path in the message (#409 hygiene): {err}"
        );
    }

    #[test]
    fn test_read_log_tail_returns_only_the_last_limit_lines_oldest_first() {
        let dir = scratch_dir("bounded");
        let path = dir.join(LOG_FILE_NAME);
        let all: Vec<String> = (0..1200).map(|i| format!("line-{i:04}")).collect();
        std::fs::write(&path, all.join("\n") + "\n").expect("write log");

        let tail = read_log_tail(&path, 500, LOG_TAIL_MAX_BYTES).expect("read tail");
        assert_eq!(tail.len(), 500, "payload is clamped to the requested limit");
        assert_eq!(
            tail[0], "line-0700",
            "oldest line of the window comes first"
        );
        assert_eq!(tail[499], "line-1199", "newest line is last");

        // A limit the file cannot fill returns everything, still ordered.
        let short = read_log_tail(&path, 2000, LOG_TAIL_MAX_BYTES).expect("read tail");
        assert_eq!(short.len(), 1200);
        assert_eq!(short[0], "line-0000");
    }

    #[test]
    fn test_read_log_tail_byte_cap_drops_the_partial_first_line() {
        let dir = scratch_dir("bytecap");
        let path = dir.join(LOG_FILE_NAME);
        // Each line is 60 chars + '\n' = 61 bytes; 10 lines = 610 bytes.
        let all: Vec<String> = (0..10)
            .map(|i| format!("line-{i:04}-{}", "x".repeat(50)))
            .collect();
        std::fs::write(&path, all.join("\n") + "\n").expect("write log");

        // Start at 485: 3 bytes inside line 7 (which spans 427..488).
        let tail = read_log_tail(&path, 500, 125).expect("read tail");
        assert_eq!(tail.len(), 2, "the mid-line fragment is dropped");
        assert_eq!(tail[0], all[8], "first whole line after the seek point");
        assert_eq!(tail[1], all[9]);
        assert!(
            tail.iter()
                .all(|l| l.starts_with("line-0") && l.len() == 60),
            "no truncated record may be returned: {tail:?}"
        );
    }

    /// A seek that lands exactly on a newline must return the record that
    /// starts there: the byte check distinguishes a record boundary from a
    /// mid-line seek (issue #824).
    #[test]
    fn test_read_log_tail_seek_on_a_newline_keeps_the_whole_first_line() {
        let dir = scratch_dir("boundary");
        let path = dir.join(LOG_FILE_NAME);
        // Each line is 60 chars + '\n' = 61 bytes; 10 lines = 610 bytes.
        let all: Vec<String> = (0..10)
            .map(|i| format!("line-{i:04}-{}", "x".repeat(50)))
            .collect();
        std::fs::write(&path, all.join("\n") + "\n").expect("write log");

        // 610 - 244 = 366, the exact start of line 6 — the byte before the
        // window is line 5's '\n', so line 6 is complete and must survive.
        let tail = read_log_tail(&path, 500, 244).expect("read tail");
        assert_eq!(
            tail,
            vec![
                all[6].clone(),
                all[7].clone(),
                all[8].clone(),
                all[9].clone()
            ],
            "a boundary seek keeps the line at the offset"
        );
    }

    #[test]
    fn test_clamp_limit_keeps_the_request_inside_the_buffer_size() {
        assert_eq!(clamp_limit(None), MAX_LOG_LINES);
        assert_eq!(clamp_limit(Some(37)), 37);
        assert_eq!(clamp_limit(Some(10_000)), MAX_LOG_LINES);
        assert_eq!(
            clamp_limit(Some(0)),
            1,
            "0 lines is raised to a usable window"
        );
    }

    /// Both readers open the same file; a rename in one module must fail
    /// here rather than silently pointing the pane at a file nobody writes.
    #[test]
    fn test_log_file_name_matches_diagnostics() {
        let expected = format!("const LOG_FILE_NAME: &str = \"{LOG_FILE_NAME}\";");
        assert!(
            include_str!("../diagnostics.rs").contains(&expected),
            "commands/logs.rs and diagnostics.rs must name the same log file: {expected}"
        );
    }
}
