//! `MprisSource` — Linux MPRIS (issue #862).
//!
//! MPRIS is the freedesktop.org media-session spec. Every desktop audio
//! app that wants the system volume keys / player controls to "just
//! work" registers a D-Bus object under a name like
//! `org.mpris.MediaPlayer2.spotify` (or `…chromium`,
//! `…vlc`, `…mpd`, `…spotifyd`, `…firefox`, etc.) and exposes its
//! `Metadata` + `PlaybackStatus` properties over the session bus.
//!
//! PresenceJam's `MprisSource` walks every name under the well-known
//! MPRIS prefix, picks the one whose `PlaybackStatus == "Playing"`,
//! and reads its `Metadata` map. The map's keys are the spec-defined
//! `xesam:title`, `xesam:artist` (array), `xesam:album`, `mpris:artUrl`,
//! `mpris:length` (microseconds), plus `mpris:trackid`.
//!
//! Implementation notes:
//!
//! - The blocking D-Bus connection is created lazily on the first poll
//!   and reused. `zbus::blocking::Connection::session` is the documented
//!   entry point; the polling thread does not have an async runtime,
//!   so the blocking flavour is required.
//! - If no session is registered or no session reports `Playing`, the
//!   source returns `Ok(None)`. A transport error on the bus becomes
//!   `SourceError::Transient` so the next poll can retry.
//! - `mpris:length` is in microseconds; convert to milliseconds with
//!   integer division. A track without a known duration (a live radio,
//!   a podcast episode that has not advertised one) reports `length == 0`,
//!   which the trait surface represents as `duration_ms = 0` and the
//!   poll loop's duration-derived sleep skips.

#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::OnceLock;

use zbus::blocking::{Connection, Proxy};
use zbus::names::OwnedBusName;

use super::{NowPlaying, PlaybackSource, PlaybackSourceId, SourceCaps, SourceError};

/// `MprisSource` — the MPRIS-backed playback source.
pub struct MprisSource {
    /// `true` after the first successful session-bus connection. Used by
    /// tests to assert the cache is exercised.
    cached: bool,
    /// Last-seen active bus name. Used as a hint when picking the
    /// "active" session — see [`pick_active_bus`]. Sticky on its own
    /// across polls so a momentarily paused Spotify does not flip the
    /// source's pick to a different app on every iteration; the actual
    /// playback-status check below still vetoes a paused session.
    last_active: Option<String>,
}

impl MprisSource {
    pub fn new() -> Self {
        Self {
            cached: false,
            last_active: None,
        }
    }

    pub fn has_cached_connection(&self) -> bool {
        self.cached
    }
}

impl Default for MprisSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Cached session-bus connection. `zbus::blocking::Connection::session()`
/// returns `Err` if there is no session bus (no `dbus-daemon` /
/// `dbus-broker` running), which on a headless build manifests as a
/// `SourceError::Transient` the next poll can retry — the daemon
/// starting later makes the source start working without a restart.
static CONNECTION: OnceLock<std::sync::Mutex<Option<Connection>>> = OnceLock::new();

fn connection_slot() -> &'static std::sync::Mutex<Option<Connection>> {
    CONNECTION.get_or_init(|| std::sync::Mutex::new(None))
}

fn acquire_connection() -> Result<Connection, SourceError> {
    {
        let slot = connection_slot().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(conn) = slot.as_ref() {
            return Ok(conn.clone());
        }
    }
    let conn = Connection::session()
        .map_err(|e| SourceError::Transient(format!("D-Bus session bus unavailable: {e}")))?;
    {
        let mut slot = connection_slot().lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(conn.clone());
    }
    Ok(conn)
}

impl PlaybackSource for MprisSource {
    fn poll(&mut self) -> Result<Option<NowPlaying>, SourceError> {
        let conn = acquire_connection()?;
        self.cached = true;

        let names = list_mpris_bus_names(&conn)?;
        if names.is_empty() {
            log::debug!("[SOURCES] MprisSource: no MPRIS sessions registered");
            return Ok(None);
        }

        let active = match pick_active_bus(&conn, &names, self.last_active.as_deref())? {
            Some(bus_name) => bus_name,
            None => {
                log::debug!("[SOURCES] MprisSource: registered sessions, none Playing");
                return Ok(None);
            }
        };
        self.last_active = Some(active.clone());

        let metadata = read_metadata(&conn, &active)?;
        let playback_status = read_playback_status(&conn, &active)?;
        Ok(Some(metadata_to_now_playing(metadata, playback_status)))
    }

    fn capabilities(&self) -> SourceCaps {
        // Most MPRIS implementations populate the spec-required
        // `Metadata` keys (`xesam:title`, `xesam:artist`, `xesam:album`,
        // `mpris:artUrl`) and `PlaybackStatus`. `mpris:length` is
        // optional — many streaming clients leave it unset.
        SourceCaps {
            has_progress: false,
            has_duration: true,
            has_album_art: true,
        }
    }

    fn id(&self) -> PlaybackSourceId {
        PlaybackSourceId::Mpris
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Walk the session bus for every name under `org.mpris.MediaPlayer2.*`
/// and return them as owned names. The bus daemon exposes a name list
/// via the `org.freedesktop.DBus` interface; the call uses the standard
/// `ListNames` method.
fn list_mpris_bus_names(conn: &Connection) -> Result<Vec<OwnedBusName>, SourceError> {
    let dbus_proxy: Proxy<'_> = Proxy::new(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .map_err(|e| SourceError::Transient(format!("MPRIS ListNames proxy build failed: {e}")))?;
    let raw: Vec<String> = dbus_proxy
        .call_method("ListNames", &())
        .map_err(|e| SourceError::Transient(format!("MPRIS ListNames failed: {e}")))?
        .body()
        .deserialize()
        .map_err(|e| SourceError::Transient(format!("MPRIS ListNames deserialize failed: {e}")))?;
    let prefix = "org.mpris.MediaPlayer2.";
    Ok(raw
        .into_iter()
        .filter_map(|n| {
            if n.starts_with(prefix) {
                OwnedBusName::try_from(n).ok()
            } else {
                None
            }
        })
        .collect())
}

/// Pick the active MPRIS bus: prefer the previous winner if it is still
/// `Playing`, otherwise the first name whose `PlaybackStatus == "Playing"`.
fn pick_active_bus(
    conn: &Connection,
    names: &[OwnedBusName],
    last_active: Option<&str>,
) -> Result<Option<String>, SourceError> {
    if let Some(prev) = last_active {
        if let Some(prev_bus) = names.iter().find(|n| n.as_str() == prev) {
            if read_playback_status(conn, prev_bus.as_str())?.as_deref() == Some("Playing") {
                return Ok(Some(prev.to_string()));
            }
        }
    }
    for name in names {
        if let Some(status) = read_playback_status(conn, name.as_str())? {
            if status == "Playing" {
                return Ok(Some(name.to_string()));
            }
        }
    }
    Ok(None)
}

/// Read `PlaybackStatus` from the `org.mpris.MediaPlayer2.Player` interface
/// of a given bus name. Returns `Ok(None)` on the "no such property" /
/// "no reply" path so the picker can ignore a missing peer.
fn read_playback_status(conn: &Connection, bus_name: &str) -> Result<Option<String>, SourceError> {
    let path = bus_name.to_string();
    let proxy: Proxy<'_> = match Proxy::new(
        conn,
        path.clone(),
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
    ) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let reply = proxy.call_method("Get", &("org.mpris.MediaPlayer2.Player", "PlaybackStatus"));
    let msg = match reply {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let value: zbus::zvariant::OwnedValue = match msg.body().deserialize() {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    Ok(Some(value.to_string()))
}

/// Read the `Metadata` property of the player and return it as a flat
/// `HashMap<String, String>`. The values are still typed (`Value::Str`,
/// `Value::Array`, etc.); the conversion to `NowPlaying` happens in
/// [`metadata_to_now_playing`]. Returning `String` keeps the trait
/// free of `zvariant`'s lifetime parameters — zvariant values are not
/// `'static`, and a `zvariant::Value<'a>` cannot outlive the D-Bus
/// reply it came from. Stringifying is lossy for arrays; the helper
/// readers (`read_first_string_array`) re-parse the comma-joined form.
fn read_metadata(
    conn: &Connection,
    bus_name: &str,
) -> Result<HashMap<String, String>, SourceError> {
    let path = bus_name.to_string();
    let proxy: Proxy<'_> = Proxy::new(
        conn,
        path,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
    )
    .map_err(|e| SourceError::Transient(format!("MPRIS Properties proxy build failed: {e}")))?;
    let msg = proxy
        .call_method("Get", &("org.mpris.MediaPlayer2.Player", "Metadata"))
        .map_err(|e| SourceError::Transient(format!("MPRIS Metadata Get failed: {e}")))?;
    // Bind the body to a `let` so the deserialized `Value<'_>` lifetimes
    // are tied to a local that outlives the `let map = …` expression —
    // chaining `msg.body().deserialize()` would create a temporary
    // `MessageBody<'_>` that is dropped at the end of the statement,
    // dangling the borrowed `Value<'_>` entries inside the map.
    let body = msg.body();
    let map: HashMap<String, zbus::zvariant::Value<'_>> = body
        .deserialize()
        .map_err(|e| SourceError::Transient(format!("MPRIS Metadata deserialize failed: {e}")))?;
    Ok(map
        .into_iter()
        .map(|(k, v)| (k, value_to_string(&v)))
        .collect())
}

/// Convert a zvariant value to its string form. Arrays render as
/// comma-joined elements; numbers as their base-10 representation;
/// everything else falls back to `format!("{v:?}")` so an unrecognised
/// type is still observable in the diagnostic snapshot.
fn value_to_string(v: &zbus::zvariant::Value<'_>) -> String {
    use zbus::zvariant::Value;
    match v {
        Value::Str(s) => s.to_string(),
        Value::U8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(arr) => arr
            .iter()
            .map(value_to_string)
            .collect::<Vec<_>>()
            .join(", "),
        _ => format!("{v:?}"),
    }
}

/// Flatten the MPRIS metadata map + playback status into the trait
/// shape. Each field has a default so a sparse metadata map (a player
/// that did not advertise album art, for example) still produces a
/// `NowPlaying` — the formatter pipeline treats empty strings as
/// "substitute the placeholder".
fn metadata_to_now_playing(
    map: HashMap<String, String>,
    playback_status: Option<String>,
) -> NowPlaying {
    let title = read_string(&map, "xesam:title");
    let album = read_string(&map, "xesam:album");
    let artist = read_first_string_array(&map, "xesam:artist").unwrap_or_default();
    let album_art_url = read_string(&map, "mpris:artUrl");
    let is_playing = playback_status.as_deref() == Some("Playing");
    let duration_ms = read_u64(&map, "mpris:length") / 1_000;
    // `Position` is not in the Metadata map; it is exposed via the
    // `Position` property of `org.mpris.MediaPlayer2.Player`, which we
    // intentionally skip here to keep the poll cycle to one D-Bus call
    // per session. A future enhancement can add a second read.
    let progress_ms: Option<u64> = None;

    NowPlaying {
        title,
        artist,
        album,
        album_art_url,
        is_playing,
        progress_ms,
        duration_ms,
    }
}

fn read_string(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

fn read_u64(map: &HashMap<String, String>, key: &str) -> u64 {
    map.get(key)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

/// MPRIS exposes multi-valued strings (`xesam:artist` is an array of
/// names). After the stringifying pass in `read_metadata` an array
/// renders as a comma-joined string — split on the first comma and
/// take the head. The formatter has no concept of "artist 1 / artist
/// 2" today.
fn read_first_string_array(map: &HashMap<String, String>, key: &str) -> Option<String> {
    let raw = map.get(key)?;
    let head = raw.split(',').next()?.trim();
    if head.is_empty() {
        None
    } else {
        Some(head.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn metadata_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        let mut m: HashMap<String, String> = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    /// The conversion from a populated metadata map must produce a
    /// `NowPlaying` whose every field matches the map. The MPRIS spec
    /// stores `mpris:length` in microseconds — the conversion divides by
    /// 1_000 to produce milliseconds.
    #[test]
    fn metadata_to_now_playing_full_map() {
        let map = metadata_with(&[
            ("xesam:title", "Bohemian Rhapsody"),
            ("xesam:artist", "Queen"),
            ("xesam:album", "A Night at the Opera"),
            ("mpris:artUrl", "https://example.com/art.jpg"),
            ("mpris:length", "355000000"),
        ]);
        let np = metadata_to_now_playing(map, Some("Playing".into()));
        assert_eq!(np.title, "Bohemian Rhapsody");
        assert_eq!(np.artist, "Queen");
        assert_eq!(np.album, "A Night at the Opera");
        assert_eq!(np.album_art_url, "https://example.com/art.jpg");
        assert!(np.is_playing);
        assert_eq!(np.duration_ms, 355_000);
        assert!(np.progress_ms.is_none());
    }

    /// `PlaybackStatus == "Paused"` must produce `is_playing == false`
    /// even with a populated metadata map — the user-facing status
    /// reflects a paused track.
    #[test]
    fn metadata_to_now_playing_paused_is_not_playing() {
        let map = metadata_with(&[("xesam:title", "Bohemian Rhapsody")]);
        let np = metadata_to_now_playing(map, Some("Paused".into()));
        assert!(!np.is_playing);
    }

    /// A sparse map (no album art, no length) still produces a
    /// `NowPlaying` — the formatter treats the empty string as "use the
    /// placeholder". This is the common case for many browser-based
    /// players that do not advertise album art.
    #[test]
    fn metadata_to_now_playing_sparse_map() {
        let map: HashMap<String, String> = HashMap::new();
        let np = metadata_to_now_playing(map, Some("Playing".into()));
        assert!(np.title.is_empty());
        assert!(np.artist.is_empty());
        assert!(np.album.is_empty());
        assert!(np.album_art_url.is_empty());
        assert!(np.is_playing);
        assert_eq!(np.duration_ms, 0);
    }

    /// `read_first_string_array` returns the first entry of a
    /// comma-joined artist list — the formatter does not understand
    /// "artist 1 / artist 2" today.
    #[test]
    fn read_first_string_array_picks_first_artist() {
        let mut m: HashMap<String, String> = HashMap::new();
        m.insert("xesam:artist".into(), "First, Second".into());
        let first = read_first_string_array(&m, "xesam:artist");
        assert_eq!(first.as_deref(), Some("First"));
    }

    #[test]
    fn source_id_is_mpris() {
        let s = MprisSource::new();
        assert_eq!(s.id(), PlaybackSourceId::Mpris);
    }

    #[test]
    fn capabilities_advertises_full_set() {
        let s = MprisSource::new();
        let caps = s.capabilities();
        assert!(caps.has_duration);
        assert!(caps.has_album_art);
        // MPRIS does not surface `Position` cheaply — the poll loop
        // skips it for now and the gate treats `progress_ms = None`
        // like Spotify's live-track case.
        assert!(!caps.has_progress);
    }
}
