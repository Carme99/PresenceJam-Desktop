# PresenceJam-Desktop — MS Learn + Spotify Docs Alignment Review

**Date:** 2026-08-09
**Scope:** Full read of `src-tauri/src/*.rs` (23 files), `src/` frontend (30 files), and all docs (README, USAGE, SETUP, TROUBLESHOOTING, ARCHITECTURE, SECURITY, CHANGELOG, CONTRIBUTING, STATE-OF-FEATURES, homebrew formula)
**Method:** 7 category-scoped read-only review agents, each grounding every finding against **current** developer.spotify.com and learn.microsoft.com pages (every cited URL actually read). Severity: High = contradicts current documented behavior; Med = deprecated/missing documented handling; Low = drift/cosmetic.
**Result:** 24 findings → logged as issues **#151–#174** (label `docs-alignment`).

---

## Findings summary

| # | File(s) | Severity | Finding | Issue |
|---|---|---|---|---|
| 1 | teams.rs:117 | **High** | Device-code scope omits `offline_access` → refresh token never issued → Teams auto-refresh dead | #151 |
| 2 | teams.rs:255-267 | Med | Device-code polling ignores server `interval`; `slow_down` mishandled (RFC 8628 §3.5) | #152 |
| 3 | teams.rs:569-571 | Med | 403 → `ExpiredToken` misclassification → re-auth loop | #153 |
| 4 | teams.rs:474-530 | Med | Graph 429/Retry-After never handled on set/clear | #154 |
| 5 | teams.rs:498-530 | Med | "Clear" posts never-expiring placeholder, not a clear | #155 |
| 6 | poll_once.rs:528-530 | Med | `expiryDateTime.dateTime` carries explicit UTC offset (schema violation) | #156 |
| 7 | Settings.svelte:82-91 | Med | Teams re-auth discards DeviceCodeResponse — flow stalls | #157 |
| 8 | teams.rs:286-352 | Low | `complete_teams_auth` unreachable; device_code stored as code_verifier | #158 |
| 9 | spotify.rs:228 | Med | 429 ignores `Retry-After`; fixed 60s backoff | #159 |
| 10 | spotify.rs refresh | Med | `invalid_grant` never parsed; 6-month token expiry → silent failure; SECURITY.md "TTL is long" wrong | #160 |
| 11 | spotify.rs:169-200 | Med | 200 parsed without checking `item.type` / `currently_playing_type` | #161 |
| 12 | spotify_auth.rs | Med | Manual callback fallback skips `state` validation (CSRF) | #162 |
| 13 | config.rs | Low | `config.spotify.scopes` dead config | #163 |
| 14 | spotify_auth.rs:78 | Low | Unencoded space in `scope` query param | #164 |
| 15 | spotify.rs:26,219 | Low | `progress_ms` null → 0, losing documented null semantics | #165 |
| 16 | ARCHITECTURE.md:170,259 | **High** | Diagrams say `PATCH /me/presence/setStatusMessage` → 204; API+code say POST → 200 | #166 |
| 17 | SETUP.md:11, STATE-OF-FEATURES.md:40 | **High** | Personal Microsoft account support claims contradict presence permission matrix | #167 |
| 18 | USAGE.md:66-68, TROUBLESHOOTING.md:116-119 | Med | "Teams may shorten window" / "24-hour cap" — no such documented behavior | #168 |
| 19 | SETUP.md, SECURITY.md | Med | Spotify app-creation walkthrough drift (Redirect URIs step, CREATE/Save, ROTATE/Reset) | #169 |
| 20 | ARCHITECTURE.md, README, STATE-OF-FEATURES | Med | Docs describe secret-free PKCE; code sends client secret via Basic (hybrid) | #170 |
| 21 | STATE-OF-FEATURES.md:40 | Low | Admin-consent claim contradicts permissions reference (AdminConsentRequired: No) | #171 |
| 22 | ARCHITECTURE.md:161,163 | Low | `verification_url` vs documented `verification_uri` | #172 |
| 23 | SECURITY.md:196 | Low | `slow_down` cited from device-code docs; absent from current MS error table | #173 |
| 24 | ARCHITECTURE.md, STATE-OF-FEATURES.md | Low | ~720 calls/day ≠ 30s interval (≈2880/day); internal math slip | #174 |

## High-severity detail

### #151 — Teams refresh path is dead (code)
`teams.rs:117` requests `scope = "Presence.ReadWrite User.Read"`. MS identity platform docs: `refresh_token` is "Issued if the original `scope` parameter included `offline_access`" ([v2-oauth2-device-code](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)). `offline_access` appears nowhere in `src-tauri/src`, so `refresh_teams_token` always fails with "No refresh token available" after the ~60-90 min access-token lifetime. Contradicts USAGE.md ("refresh automatically") and CHANGELOG v2.5.0 ("Teams refresh token rotation"). **Spot-checked against the live doc — confirmed.**

### #166 — ARCHITECTURE.md documents wrong HTTP method/response (docs)
Both mermaid diagrams: `PATCH /me/presence/setStatusMessage` → `204 No Content`. Graph v1.0: `POST /users/{id}/presence/setStatusMessage` → `200 OK` ([presence-setstatusmessage](https://learn.microsoft.com/en-us/graph/api/presence-setstatusmessage?view=graph-rest-1.0)). The code (`teams.rs:471,512`) correctly uses POST. **Spot-checked against the live doc — confirmed.**

### #167 — Personal MSA support claims (docs)
SETUP.md "personal or work — both work"; STATE-OF-FEATURES "work because `Presence.ReadWrite` is granted by default". Graph docs: presence APIs are "Not supported" for delegated personal Microsoft accounts ([setStatusMessage permissions table](https://learn.microsoft.com/en-us/graph/api/presence-setstatusmessage?view=graph-rest-1.0)). Compounded by the `/common` authority admitting MSA users through onboarding. **Spot-checked against the live doc — confirmed.**

## Verified-clean highlights

- **Spotify endpoint**: `GET https://api.spotify.com/v1/me/player/currently-playing` — exact, current, with `market` omission correct for user tokens; 204-vs-200/`item:null` semantics handled correctly; all parsed fields exist in the current schema.
- **Spotify scopes**: `user-read-currently-playing user-read-playback-state` — exact current scope names.
- **Spotify PKCE**: 86-char verifier / 43-char S256 challenge per RFC 7636; verifier held in memory only; `state` validated on the deep-link path.
- **Teams endpoints**: devicecode/token against `login.microsoftonline.com/common/oauth2/v2.0` — documented tenant value + paths; poll request shape byte-for-byte the doc example; 15-minute timeout == documented `expires_in` default.
- **Teams payload**: `POST /me/presence/setStatusMessage` with `statusMessage.message.contentType = "text"` — matches docs; `/me` alias is documented for the signed-in user.
- **Permissions**: `Presence.ReadWrite` is the documented least-privileged delegated permission for setStatusMessage.
- **Cadence**: Spotify default 30s / max 300s pause backoff and Teams set-on-change + 500ms debounce are far below documented rate limits (presence: 10,000 req/30s; GET presence: 1,500 req/30s).
- **Doc links**: no broken links. All developer.spotify.com URLs (dashboard, terms, privacy) and Microsoft URLs (servicesagreement, privacystatement, Graph Explorer) resolve; no learn.microsoft.com URLs exist in the docs at all.

## Unverifiable / [INFERENCE]

- Whether Graph rejects or tolerates the offset-bearing `expiryDateTime.dateTime` (runtime behavior; only the "shouldn't include time zone" constraint is documented).
- Whether GET /me/presence accepts a `Presence.ReadWrite`-scoped token at runtime (presence-get delegated table lists `Presence.Read` only).
- Numeric Spotify rate-limit budgets (docs publish no numbers); "player endpoints have stricter limits" is **not** in current docs (rate-limits page mentions only app-wide limits + playlist image upload).
- The 600s authorization-code expiry (code comment; Spotify does not document a code TTL).
- Whether the token endpoint accepts a PKCE request that also carries a Basic auth header (undocumented either way; the app demonstrably works in production).
- "Apps created after Nov 2022 have no client secret" — **contradicted** by the current Apps page (apps are issued both Client ID and Client Secret); onboarding's hard secret requirement is consistent with current docs.
- Current MS Learn refresh-token doc: default lifetime is **90 days** for non-SPA flows (not "don't expire"); rotate-on-refresh design is compatible, moot only because #151 prevents any Teams refresh token from existing.

## Method note

Seven agents reviewed disjoint slices (Spotify OAuth, Spotify API, Teams OAuth, Teams/Graph, frontend, Spotify docs, MS Learn docs) in parallel; all findings doc-grounded with actually-read URLs. Cross-slice confirmation: #166 (docs) contradicted by both Graph docs and the code; #151 confirmed by two independent agents.
