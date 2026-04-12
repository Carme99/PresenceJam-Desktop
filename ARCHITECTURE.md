# Architecture

A deep-dive into how PresenceJam works under the hood.

## Overview

PresenceJam is a Tauri 2 desktop application with:

- **Frontend:** Svelte 5 + TypeScript (SPA mode via `@sveltejs/adapter-static`)
- **Backend:** Rust (Tauri 2 command handlers + polling thread)
- **Storage:** `tauri-plugin-store` for tokens, JSON files for config
- **Auth:** Spotify PKCE OAuth 2.0 + Microsoft Teams Device Code flow
- **Platform:** Windows + macOS (single-instance enforcement, system tray, DPAPI token encryption on Windows, Keychain on macOS)

## System Diagram

```mermaid
graph TD
    subgraph Frontend ["Frontend (Svelte)"]
        UI["+page.svelte<br/>Dashboard / Onboarding<br/>Settings / LogViewer"]
        Stores["Stores<br/>app.ts, config.ts<br/>spotify.ts, teams.ts"]
    end

    subgraph Backend ["Backend (Rust / Tauri)"]
        Commands["commands.rs<br/>invoke handlers"]
        Polling["polling.rs<br/>sync thread"]
        SpotifyAPI["spotify.rs<br/>Spotify Web API"]
        TeamsAPI["teams.rs<br/>Microsoft Graph API"]
        Tray["tray.rs<br/>system tray"]
    end

    subgraph Storage ["Storage"]
        TokenStore["tauri-plugin-store<br/>tokens.json"]
        Config["config.json<br/>%APPDATA%\\PresenceJam"]
    end

    UI <-->|"invoke / events"| Commands
    Commands -->|"HTTP"| SpotifyAPI
    Commands -->|"HTTP"| TeamsAPI
    Polling -->|"HTTP"| SpotifyAPI
    Polling -->|"HTTP"| TeamsAPI
    Commands --> Polling
    SpotifyAPI -->|"tokens"| TokenStore
    TeamsAPI -->|"tokens"| TokenStore
```

## CI/CD Pipeline

Releases are automated via GitHub Actions. When a version tag is pushed, the pipeline builds and releases for both platforms:

```mermaid
flowchart TD
    subgraph Trigger["🔔 Trigger: git tag v* pushed"]
        tag["git tag v2.2.0 && git push --tags"]
    end

    subgraph Build["🔨 Build Matrix"]
        direction LR
        macos_build["macOS Build<br/>aarch64-apple-darwin"]
        windows_build["Windows Build<br/>x86_64-pc-windows-msvc"]
    end

    subgraph Package["📦 Package"]
        direction LR
        macos_zip["macOS: .app → .zip"]
        msi["Windows: .msi"]
    end

    subgraph Release["🚀 GitHub Release"]
        release["Auto-created Release<br/>Drafted from tag"]
        assets["Artifacts attached:<br/>PresenceJam-v2.2.0-macos.dmg<br/>PresenceJam-v2.2.0.msi"]
    end

    tag --> Build
    Build --> Package
    Package --> Release
```

### Release Process

1. **Tag Push:** Developer runs `git tag v2.2.0 && git push --tags`
2. **Workflow Trigger:** GitHub Actions detects the `v*` tag pattern
3. **Parallel Build:** macOS and Windows builds run concurrently on GitHub's hosted runners
4. **Artifact Upload:** Build outputs are uploaded as workflow artifacts
5. **Release Creation:** `release-action` creates a GitHub Release and attaches all artifacts

### Workflow File

The CI/CD workflow is defined in [`.github/workflows/release.yml`](.github/workflows/release.yml).

## Authentication Flows

### Spotify PKCE OAuth

```mermaid
sequenceDiagram
    actor User
    participant App
    participant Spotify as Spotify<br/>Developer Portal
    participant Browser

    User->>App: Enter Client ID + Secret
    App->>Spotify: POST /api/token (code_verifier)
    Spotify-->>Browser: Open auth URL with code_challenge
    User->>Browser: Login + Authorize
    Browser-->>App: Redirect to presencejam://callback?code=XXX
    App->>Spotify: POST /api/token (code, code_verifier)
    Spotify-->>App: access_token + refresh_token
    App->>App: Store tokens via tauri-plugin-store
```

1. App generates a PKCE `code_verifier` (random 64-byte string)
2. App sends `code_challenge` (SHA256 hash of verifier) to Spotify
3. Spotify returns an auth URL → App opens browser
4. User authorizes → Spotify redirects to `presencejam://callback?code=XXX`
5. App extracts the `code`, sends it + `code_verifier` to Spotify
6. Spotify returns `access_token` + `refresh_token` → stored via `tauri-plugin-store`

### Microsoft Teams Device Code Flow

```mermaid
sequenceDiagram
    actor User
    participant App
    participant Microsoft as Microsoft<br/>login.microsoftonline.com
    participant Teams as Microsoft Graph API

    User->>App: Click "Sign in with Microsoft"
    App->>Microsoft: POST /devicecode
    Microsoft-->>App: user_code + verification_url
    App->>User: Display code + URL
    User->>Browser: Visit verification_url, enter code
    User->>Microsoft: Enter code in browser
    loop Poll every 5s
        App->>Microsoft: POST /token (device_code)
        Note over Microsoft: authorization_pending
    end
    Microsoft-->>App: access_token + refresh_token
    App->>Teams: PATCH /me/presence/setStatusMessage
    Teams-->>App: 204 No Content
```

The app polls Microsoft's token endpoint every 5 seconds while the user completes the browser auth. Once authorized, tokens are stored and the status message is set via Graph API.

## Startup Loading

On app launch, PresenceJam loads saved config and tokens from persistent storage into `AppState`:

```mermaid
sequenceDiagram
    participant App as Tauri App
    participant Config as Config Module
    participant Polling as Polling Module
    participant Store as tauri-plugin-store
    
    App->>App: app.manage(state)
    App->>Config: load_config()
    Config-->>App: AppConfig
    App->>App: state.config = Some(cfg)
    App->>Polling: load_spotify_tokens()
    Polling->>Store: get("spotify_tokens")
    Store-->>Polling: SpotifyTokens | None
    Polling-->>App: Some(tokens) | None
    App->>App: state.spotify_tokens = Some(tokens)
    App->>Polling: load_teams_tokens()
    Polling->>Store: get("teams_tokens")
    Store-->>Polling: TeamsTokens | None
    Polling-->>App: Some(tokens) | None
    App->>App: state.teams_tokens = Some(tokens)
```

This means:
- **On first launch**, the app starts fresh and requires onboarding
- **On subsequent launches**, the app auto-connects if valid tokens exist
- **Reconnect** clears tokens from both memory and store, forcing re-auth

## Reconnect Flow

When a user clicks "Reconnect" in Settings, the app clears auth state and triggers re-authentication:

```mermaid
sequenceDiagram
    actor User
    participant UI as Settings UI
    participant Commands as Tauri Commands
    participant Polling as Polling Module
    participant Store as tauri-plugin-store
    
    User->>UI: Click Spotify reconnect
    UI->>Commands: invoke("reconnect_spotify")
    Commands->>Commands: state.spotify_tokens = None
    Commands->>Commands: state.pending_spotify_auth = None
    Commands->>Polling: clear_spotify_tokens()
    Polling->>Store: delete("spotify_tokens")
    Polling->>Store: delete("spotify_client_id, client_secret, etc.")
    Commands->>UI: emit("spotify-reconnect-required")
    UI->>User: Show re-authentication flow
```

### Commands

| Command | Action |
|---------|--------|
| `reconnect_spotify` | Clears Spotify tokens from state and store, emits `spotify-reconnect-required` event |
| `reconnect_teams` | Clears Teams tokens from state and store, emits `teams-reconnect-required` event |

## Polling Loop

```mermaid
flowchart TD
    START[Start Syncing] --> TOKEN_CHECK{Spotify Tokens<br/>Available?}
    TOKEN_CHECK -->|No| WAIT_30[Sleep 30s] --> TOKEN_CHECK
    TOKEN_CHECK -->|Yes| EXPIRED{Token<br/>Expired?}
    EXPIRED -->|Yes| REFRESH[Refresh Spotify Token] --> TRACK_POLL
    EXPIRED -->|No| TRACK_POLL[Poll Spotify<br/>/me/player/currently-playing]
    TRACK_POLL --> TRACK_CHANGED{Track<br/>Changed?}
    TRACK_CHANGED -->|No| SLEEP_SMART[Smart Sleep<br/>remaining - 5s buffer]
    TRACK_CHANGED -->|Yes| FORMAT_STATUS[Format Status<br/>from template]
    FORMAT_STATUS --> PROFANITY_CHECK{Profanity<br/>Filter<br/>Enabled?}
    PROFANITY_CHECK -->|Yes| FILTER_PROFANITY[Filter Status<br/>replace profane<br/>with placeholder]
    PROFANITY_CHECK -->|No| UPDATE_TEAMS
    FILTER_PROFANITY --> UPDATE_TEAMS[Set Teams Status<br/>with final message]
    UPDATE_TEAMS --> SLEEP_SMART
    SLEEP_SMART --> TOKEN_CHECK
    TRACK_POLL --> NO_TRACK{No Track<br/>Playing?}
    NO_TRACK -->|Yes| CLEAR_STATUS[Clear Teams Status]
    NO_TRACK -->|No| TRACK_CHANGED
    CLEAR_STATUS --> SLEEP_30[Sleep 30s] --> TOKEN_CHECK
```

### Profanity Filter

The `profanity.rs` module screens the formatted status before it is sent to Teams. If profanity is detected, the status is replaced with a safe placeholder rather than exposing the profane content.

**How it works:**

1. `format_status()` produces the status string from the track template
2. If `config.teams.profanity_filter` is enabled, `filter_status()` is called
3. `contains_profanity()` normalizes the text and checks against a 25-word curated list
4. If matched, the placeholder (with `{emoji}` resolved to 🎵 or ⏸️) replaces the status
5. The replacement is logged, but the original profane content is **never written to logs**

**Detection features:**
- Leetspeak normalization (1→i, 3→e, $→s, @→a, 0→o, 5→s, 7→t, !→i, |→i)
- Repeated-character collapse (shiiit → shiit, but not excessive repeats)
- Word-boundary checks prevent false positives (class, assassin, cocktail, vacuum)
- Special handling: "fucking", "fucked", "fucker" are detected as profane variants
- Safe suffix words (tail, head, hand, etc.) allow compound words without false positives

**TODO:** Currently filters the formatted status string. A future refactor should filter raw Spotify fields (track, artist, album) before formatting to prevent placeholder injection via custom templates with `{emoji}`.

### Smart Sleep Logic

When a track is playing, the app calculates the exact time remaining until the track ends:

```rust
let remaining_ms = track.duration_ms - track.progress_ms;
let buffer_ms = 5000u64; // 5 second buffer
let sleep_secs = (remaining_ms / 1000).saturating_sub(buffer_ms / 1000);
sleep_secs.max(MINIMUM_INTERVAL_SECONDS).min(MAX_INTERVAL_SECONDS)
```

This means:
- **No API calls** while you're listening to a 4-minute track (~240 seconds of silence)
- **Polling resumes immediately** when the track changes
- **Minimum 10 seconds** between polls (configurable)

## Event Bus

The Rust backend communicates with the Svelte frontend via Tauri events:

```mermaid
sequenceDiagram
    participant Polling
    participant App as Rust AppHandle
    participant Frontend as Svelte

    Polling->>App: emit("spotify-track-changed", trackInfo)
    App->>Frontend: listen("spotify-track-changed")
    Polling->>App: emit("presence-updated", status)
    App->>Frontend: listen("presence-updated")
    Polling->>App: emit("presence-cleared", {})
    App->>Frontend: listen("presence-cleared")
    Polling->>App: emit("error", errorInfo)
    App->>Frontend: listen("error")
```

| Event | Payload | Triggered When |
|-------|---------|---------------|
| `spotify-track-changed` | `TrackInfo` | New track detected or track state changed |
| `presence-updated` | `{status, timestamp}` | Teams status successfully updated |
| `presence-cleared` | `{timestamp}` | Teams status cleared |
| `error` | `{source, message}` | Any API error (Spotify, Teams, or auth) |
| `spotify-reconnect-required` | — | User clicked reconnect for Spotify |
| `teams-reconnect-required` | — | User clicked reconnect for Teams |
| `tray-click` | — | User clicks tray icon |
| `toggle-pause` | — | User clicks Pause in tray menu |

## Deep Link Routing

PresenceJam registers a custom URL scheme to handle OAuth callbacks:

| Scheme | Used For |
|--------|----------|
| `presencejam://callback` | Spotify PKCE OAuth redirect |
| `presencejam://teams-callback` | Teams auth (reserved for future use) |

The app's `lib.rs` intercepts these URLs via Tauri's `deep-link` plugin and routes them to the appropriate handler.

## Directory Structure

```
PresenceJam-Desktop/
├── src/                          # Svelte frontend
│   ├── lib/
│   │   ├── components/
│   │   │   ├── Dashboard.svelte      # Main sync view
│   │   │   ├── Onboarding.svelte     # 3-step auth wizard
│   │   │   ├── Settings.svelte        # Config editor
│   │   │   └── LogViewer.svelte       # In-app log viewer
│   │   └── stores/
│   │       ├── app.ts                 # currentView, isSyncing, appError
│   │       ├── config.ts               # configStore, saveConfig
│   │       ├── spotify.ts              # Spotify token state
│   │       └── teams.ts                # Teams token state
│   └── routes/
│       └── +page.svelte               # SPA entry, routes to views
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs                     # Tauri entry, command registration
│   │   ├── commands.rs               # All invoke() command handlers
│   │   ├── config.rs                 # JSON config load/save, AppConfig struct
│   │   ├── polling.rs                # Polling loop (thread::spawn)
│   │   ├── profanity.rs             # Profanity filter module
│   │   ├── spotify.rs                # Spotify Web API client
│   │   ├── teams.rs                  # Microsoft Graph API client
│   │   └── tray.rs                   # System tray setup
│   ├── Cargo.toml                    # Rust dependencies
│   ├── tauri.conf.json               # Tauri 2 config (window, deep-link, plugins)
│   └── capabilities/
│       └── default.json              # Permission grants (store, http, deep-link, etc.)
├── package.json                     # Node dependencies
├── svelte.config.js                # SvelteKit SPA config (adapter-static)
├── vite.config.js                  # Vite/Tauri dev server config
└── tsconfig.json                   # TypeScript config
```

## State Management

### Rust State (`AppState`)

```rust
pub struct AppState {
    pub spotify_tokens: RwLock<Option<SpotifyTokens>>,
    pub teams_tokens: RwLock<Option<TeamsTokens>>,
    pub config: RwLock<Option<AppConfig>>,
    pub current_track: RwLock<Option<TrackInfo>>,
    pub is_syncing: RwLock<bool>,
}
```

Multiple threads access this shared state via `RwLock`:
- `polling.rs` thread writes to `spotify_tokens`, `teams_tokens`, `current_track`, `is_syncing`
- `commands.rs` handlers read/write via `tauri::State<AppState>`

### Frontend Stores

| Store | Type | Purpose |
|-------|------|---------|
| `currentView` | `'onboarding' \| 'dashboard' \| 'settings' \| 'logs'` | Active view |
| `isSyncing` | `boolean` | Sync running/paused |
| `appError` | `string \| null` | Current error message |
| `currentTrack` | `TrackInfo \| null` | Currently playing track |
| `configStore` | `AppConfig` | Full app config |
