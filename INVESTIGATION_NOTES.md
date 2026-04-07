# PresenceJam-Desktop - Teams OAuth Investigation Notes

**Date Started:** 2026-04-07
**Last Updated:** 2026-04-07
**Status:** ⚠️ BLOCKED - Network/Proxy Issue

---

## Issue Summary

**Problem:** The "Sign in with Microsoft" button on Step 2 of onboarding does nothing when clicked.

**Symptom Progression:**
1. Initially: Button click had no effect at all (no console logs, no errors)
2. After adding test buttons: Simple state mutation works, `invoke()` calls silently fail
3. After adding visual error handling: Error revealed - "Failed to parse device code response: error decoding response body"

---

## Technical Investigation Timeline

### Day 1: Initial Discovery
- User reports Teams button does nothing on Step 2
- Spotify auth works correctly (opens browser, deep link callback works)
- DevTools don't open in the app (Tauri uses WebView2, F12 doesn't work)

### Initial Hypothesis: Svelte 5 Event Handler Issue
- Test CLICK (simple `teamsUserCode = 'TEST'`) works
- Sign in with MS (calls `connectTeams()` async function) does not work
- TEST INVOKE (direct `invoke()` call) also silently fails
- Conclusion: Not a Svelte issue, but an invoke/Rust issue

### Investigation Steps Taken

#### 1. Shell Plugin vs Opener Plugin Discovery
- Found that Spotify uses `tauri_plugin_opener::open_url()` (Rust backend)
- Teams was using `@tauri-apps/plugin-shell` `open()` (JavaScript frontend)
- Changed Teams to use same opener plugin pattern as Spotify
- **Result:** Issue persisted - not the root cause

#### 2. Svelte 5 Event Handler Investigation
- Found potential issue with async functions passed directly to onclick
- Changed `onclick={connectTeams}` to `onclick={() => connectTeams()}`
- Added test buttons to isolate the issue
- **Result:** Issue persisted - not the root cause

#### 3. Invoke Command Deep Dive
- Verified command registration in `lib.rs` invoke_handler
- Confirmed `start_teams_auth_device_code` was properly registered
- Added detailed Rust logging to trace execution
- **Result:** Rust command WAS being called (logs confirmed this)

#### 4. Visual Error Handling Added
- Modified TEST INVOKE button to show `alert()` popup on error
- Error finally revealed: "Failed to parse device code response: error decoding response body"

#### 5. Network Request Confirmed Working
- Rust logging showed:
  - `start_teams_auth_device_code called`
  - `teams::start_teams_auth_device_code: starting`
  - `teams::start_teams_auth_device_code: client created`
  - `teams::start_teams_auth_device_code: calling devicecode endpoint`
  - `teams::start_teams_auth_device_code: send succeeded`
  - **BUT:** "Failed to parse device code response"

---

## Root Cause Analysis

### Confirmed Working Components
- ✅ Tauri invoke command registration and routing
- ✅ JavaScript frontend to Rust command bridge
- ✅ Network socket creation
- ✅ HTTP request sent to `https://login.microsoftonline.com/common/oauth2/v2.0/devicecode`
- ✅ Spotify OAuth (deep link callback) - works perfectly

### Failing Component
- ❌ Microsoft server returning non-JSON response

### Error Details
```
Error: Failed to parse device code response: error decoding response body
```

**Interpretation:**
1. The HTTP request reached Microsoft's server
2. Microsoft returned a response
3. The response body was NOT valid JSON
4. Likely causes: proxy error page, blocked request, server error, certificate interception, etc.

---

## Possible Root Causes

### 1. Corporate Proxy/Network Interception (Most Likely)
Corporate firewalls often intercept HTTPS traffic and may:
- Inject error pages (HTML, not JSON)
- Present certificate warnings
- Block certain endpoints
- Require authentication via proxy

**Evidence:** Response is non-JSON suggests an HTML error page was returned instead of the expected JSON.

### 2. User-Agent Blocking
- Microsoft might be rejecting requests without proper User-Agent
- `reqwest::blocking::Client` default User-Agent might be flagged
- Some Microsoft endpoints are known to reject non-browser User-Agents

### 3. Rate Limiting
- Too many device code requests
- Microsoft returning error page instead of JSON

### 4. Wrong Endpoint or Scopes
- Device code endpoint might have changed
- Scopes (`Presence.ReadWrite User.Read`) might require additional consent

### 5. Azure AD Conditional Access
- Account might have policies blocking device code flow
- Location-based restrictions
- Managed device requirements

### 6. Network Connectivity Issues
- DNS resolution problems
- SSL/TLS handshake failures
- Packet filtering

---

## Code Changes Made During Investigation

### Files Modified

#### `src-tauri/src/commands.rs`
- Added `open_external_url` command (uses `tauri_plugin_opener`)
- Added `start_teams_auth_device_code` command (device code flow)
- Added `poll_teams_auth` command (polling for auth completion)
- Added comprehensive logging to all Teams auth commands

#### `src-tauri/src/teams.rs`
- Added `MICROSOFT_GRAPH_CLIENT_ID` constant (`14d82eec-204b-4c2f-b7e8-296a70dab67e`)
- Restored device code flow functions (`start_teams_auth_device_code`, `poll_teams_auth`)
- Restored PKCE functions (for future auth code flow support)
- Added comprehensive logging throughout

#### `src-tauri/src/lib.rs`
- Registered new commands in invoke handler
- Added `open_external_url` command
- Deep link handling for Spotify and Teams callbacks

#### `src-tauri/tauri.conf.json`
- Added `microsoft.com` and `*.microsoft.com` to CSP connect-src
- Changed bundle targets to MSI-only (NSIS has known deep link bug)
- Configured deep-link plugin with `presencejam://` scheme

#### `src-tauri/capabilities/default.json`
- Shell plugin permissions
- Opener plugin permissions
- HTTP plugin permissions

#### `src/lib/components/Onboarding.svelte`
- Added test buttons (TEST CLICK, TEST INVOKE)
- Added detailed console logging
- Modified connectTeams function
- Added visual error handling with alert()

### Files Added (Untracked)
- `src/routes/+layout.svelte` (purpose to be verified)

---

## Test Results Log

| Test | Environment | Result | Notes |
|------|-------------|--------|-------|
| Spotify Connect | MSI | ✅ WORKS | Deep link callback works properly |
| Spotify Connect | Dev Mode | ❌ FAILS | `presencejam://` not registered in Windows (dev mode limitation) |
| TEST CLICK | MSI | ✅ WORKS | Simple state mutation |
| TEST INVOKE | MSI | ❌ FAILS | Error: "Failed to parse device code response" |
| Sign in with MS | MSI | ❌ FAILS | Same invoke error |

---

## HTTP Request Details

### Request Being Made
```http
POST https://login.microsoftonline.com/common/oauth2/v2.0/devicecode
Content-Type: application/x-www-form-urlencoded
User-Agent: (reqwest default)
Accept: */*
Content-Length: 85

client_id=14d82eec-204b-4c2f-b7e8-296a70dab67e&scope=Presence.ReadWrite%20User.Read
```

### Client ID Used
`14d82eec-204b-4c2f-b7e8-296a70dab67e`

This is Microsoft's **well-known client ID** for the Graph PowerShell SDK. It's the same client ID that `Connect-MgGraph` uses internally in the original PowerShell script.

### Expected Response Format
```json
{
  "user_code": "CPQBDTJK",
  "device_code": "OAQABAAEAAAAp-iq9HQ-g0t...",
  "verification_url": "https://microsoft.com/devicelogin",
  "interval": 5,
  "expires_in": 900
}
```

### Actual Response
Non-JSON response (presumed HTML error page from proxy)

---

## Key Discoveries

### 1. Tauri 2 Deep Link Registration
- Deep links (`presencejam://`) are only properly registered when installed via MSI
- Dev mode (`npm run tauri dev`) does NOT register protocol handlers
- This is expected behavior - MSI installer handles Windows registry entries

### 2. Microsoft Graph Client ID
- The PowerShell `Connect-MgGraph` uses a well-known client ID
- This same client ID works for device code flow
- User does NOT need to register their own Azure AD app

### 3. Spotify vs Teams Auth Pattern
- Spotify uses Authorization Code + PKCE flow (browser redirect)
- Teams uses Device Code flow (user visits microsoft.com/devicelogin)
- Both require deep link callback handling

### 4. Svelte 5 Event Handlers
- Async functions passed to onclick should use arrow function wrapper
- `onclick={() => asyncFunction()}` is more reliable than `onclick={asyncFunction}`

---

## Proposed Next Steps

### Priority 1: Debug HTTP Response
- [ ] Log HTTP status code and raw response body before JSON parsing
- [ ] Check if response is HTML (proxy error page)
- [ ] Verify request headers being sent
- [ ] Add explicit User-Agent header

### Priority 2: Try Different Network
- [ ] Test from mobile hotspot
- [ ] Test from personal network (non-corporate)
- [ ] Rule out corporate proxy as root cause

### Priority 3: Add Request Headers
- [ ] Add explicit User-Agent header (e.g., "PresenceJam/2.0")
- [ ] Add Accept: application/json header
- [ ] Configure reqwest client with proper defaults

### Priority 4: Use tauri-plugin-http Instead
- [ ] The app has `tauri-plugin-http` installed
- [ ] It may handle proxies differently than reqwest
- [ ] Worth testing as alternative HTTP client

### Priority 5: Manual Fallback Implementation
- [ ] If network is truly blocked, implement manual flow
- [ ] Show device code and URL to user
- [ ] User visits microsoft.com/devicelogin manually
- [ ] User pastes redirect URL back to app
- [ ] App completes auth via manual token exchange

### Priority 6: Check Azure AD App Registration (Alternative)
- [ ] Register an Azure AD app for full control
- [ ] Use Authorization Code + PKCE flow (like Spotify)
- [ ] Requires user to create app in Azure Portal
- [ ] More complex but more reliable

---

## Related Files

### Source Files
- `src-tauri/src/teams.rs` - Teams OAuth implementation
- `src-tauri/src/commands.rs` - Tauri command handlers
- `src-tauri/src/lib.rs` - App initialization and deep link handling
- `src-tauri/src/spotify.rs` - Spotify OAuth (reference for working implementation)
- `src/lib/components/Onboarding.svelte` - Frontend UI with test buttons

### Configuration Files
- `src-tauri/tauri.conf.json` - Tauri configuration
- `src-tauri/capabilities/default.json` - Permissions and capabilities
- `src-tauri/Cargo.toml` - Rust dependencies

### Documentation
- [Microsoft OAuth 2.0 Device Authorization Grant](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)
- [Tauri 2 Deep Link Plugin](https://v2.tauri.app/plugin/deep-linking/)
- [Tauri 2 Shell Plugin](https://v2.tauri.app/plugin/shell/)
- [reqwest HTTP Client](https://docs.rs/reqwest/)

---

## Conclusion

The Teams device code OAuth flow is **correctly implemented at the application layer**. The issue is at the **network level**:

1. The Rust command is invoked successfully
2. The HTTP request is sent to Microsoft
3. Microsoft returns a response
4. The response is NOT valid JSON

**Most likely root cause:** Corporate proxy/network interception returning an HTML error page instead of the expected JSON device code response.

**Recommended actions:**
1. Test from mobile hotspot to confirm network is the issue
2. Add HTTP response debugging to see actual status code and body
3. Try adding explicit request headers
4. If truly blocked, implement manual fallback flow

---

## Appendix: Original PowerShell Reference

The original `PresenceJam.ps1` script uses:
```powershell
Connect-MgGraph -Scopes "Presence.ReadWrite"
```

This uses the Microsoft Graph PowerShell SDK which internally:
1. Uses the same well-known client ID (`14d82eec-204b-4c2f-b7e8-296a70dab67e`)
2. Performs device code flow
3. Opens browser to `microsoft.com/devicelogin`

The Tauri implementation attempts to replicate this flow but with a native HTTP client (`reqwest`) instead of the PowerShell SDK's built-in authentication.

**Note:** The PowerShell script may work on the same machine because:
- It uses the .NET HTTP stack which may handle proxies differently
- It may have different User-Agent characteristics
- The user's PowerShell environment may have different network settings
