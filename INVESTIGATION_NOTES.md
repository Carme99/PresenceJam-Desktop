# PresenceJam-Desktop - Teams OAuth Investigation Notes

**Date Started:** 2026-04-07
**Last Updated:** 2026-04-08
**Status:** 🔍 DEEP INVESTIGATION COMPLETE - Root Cause Identified

---

## Executive Summary

**The investigation has concluded that the issue is NOT a corporate proxy problem as originally suspected.** After testing on an off-network machine (fresh Windows install with no corporate network), the same error occurred. This rules out network-level interception.

**The actual root cause:** The `teams.rs` code lacks proper HTTP response handling. Specifically:
1. It never checks the HTTP status code before attempting JSON parsing
2. It never logs the raw response body when JSON parsing fails
3. The error message "error decoding response body" is the only information available - we cannot see what Microsoft actually returned

---

## Issue Summary

**Problem:** The "Sign in with Microsoft" button on Step 2 of onboarding fails with "Failed to parse device code response: error decoding response body"

**Symptom Progression:**
1. Initially: Button click had no effect at all (no console logs, no errors)
2. After adding test buttons: Simple state mutation works, `invoke()` calls silently fail
3. After adding visual error handling: Error revealed - "Failed to parse device code response: error decoding response body"
4. After investigation: Identified critical code gap - no HTTP response debugging in teams.rs

---

## Technical Investigation Timeline

### Day 1: Initial Discovery
- User reports Teams button does nothing on Step 2
- Spotify auth works correctly (opens browser, deep link callback works)
- DevTools don't open in the app (Tauri uses WebView2, F12 doesn't work)

### Day 1: Investigation Steps Taken

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

### Day 2: Deep Code Analysis (2026-04-08)

#### Critical Discovery: Missing HTTP Response Handling

**Comparison of Spotify vs Teams HTTP handling:**

| Aspect | Spotify (WORKS) | Teams (FAILS) |
|--------|-----------------|---------------|
| HTTP Status Check | Line 95: `if !response.status().is_success()` | ❌ NO STATUS CHECK |
| Raw Body on Error | Line 97: `response.text()` logs body | ❌ NO RAW BODY LOGGING |
| Error Message | Line 98: `status + body` in error | ❌ Only "Failed to parse" |
| JSON Parse After Error Check | Yes - only parses if success | ❌ Directly parses without check |

**Spotify code (spotify.rs:88-98):**
```rust
let response = client
    .post("https://accounts.spotify.com/api/token")
    .form(&params)
    .basic_auth(client_id, Some(client_secret))
    .send()
    .map_err(|e| format!("Failed to send token request: {}", e))?;

if !response.status().is_success() {  // ✅ CHECKS STATUS
    let status = response.status();
    let body = response.text().unwrap_or_default();  // ✅ GETS RAW BODY
    return Err(format!("Token request failed: {} - {}", status, body));  // ✅ INCLUDES BODY IN ERROR
}

let token_resp: TokenResponse = response  // ✅ ONLY PARSES AFTER SUCCESS CHECK
    .json()
    .map_err(|e| format!("Failed to parse token response: {}", e))?;
```

**Teams code (teams.rs:74-86):**
```rust
let response = client
    .post("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode")
    .form(&params)
    .send()
    .map_err(|e| {
        log::error!("teams::start_teams_auth_device_code: send failed: {}", e);
        format!("Failed to send device code request: {}", e)
    })?;
log::info!("teams::start_teams_auth_device_code: send succeeded");

let raw: DeviceCodeResponseRaw = response  // ❌ NO STATUS CHECK
    .json()  // ❌ DIRECT PARSE WITHOUT CHECKING STATUS
    .map_err(|e| format!("Failed to parse device code response: {}", e))?;  // ❌ NO RAW BODY IN ERROR
```

#### Discovery: tauri-plugin-http Initialized But Never Used

**Finding in `src-tauri/src/lib.rs:177`:**
```rust
.plugin(tauri_plugin_http::init())
```

This plugin is initialized in the Tauri app but is **never actually used anywhere** in the codebase. Both Spotify and Teams use `reqwest::blocking::Client` instead.

**Investigation showed:**
- No usage of `tauri_plugin_http` in any Rust code
- No usage in any JavaScript/TypeScript code
- The plugin is present but provides no benefit to the current implementation

**Note:** Using `tauri-plugin-http` would NOT solve the issue because:
1. It still makes HTTP requests to the same Microsoft endpoints
2. The same non-JSON response would be returned
3. The same parsing error would occur

The issue isn't the HTTP library - it's the **lack of response debugging**.

#### Discovery: Off-Network Machine Also Fails

Testing on a separate off-network machine (fresh Windows install) confirmed:
- **Same error occurs** - "Failed to parse device code response"
- **This rules out corporate proxy** as the root cause
- The issue is in the code itself, not the network

---

## Root Cause Analysis

### The Actual Problem

**The `teams.rs` code skips critical HTTP response validation:**

1. **No HTTP status code check** - We don't know if Microsoft returned 200, 400, 401, 403, 500, etc.
2. **No raw response body logging** - We don't know what Microsoft actually returned
3. **Direct JSON parsing without validation** - If the response is an error page or different JSON structure, `.json()` fails silently
4. **Error message is useless** - "error decoding response body" tells us nothing about what the response actually was

### What Microsoft Could Be Returning

Without proper debugging, we can only guess:

| Possibility | What Microsoft Returns | Why It Would Fail |
|-------------|------------------------|-------------------|
| **AADSTS Error** | JSON with `error` and `error_description` fields | Different structure than `DeviceCodeResponseRaw` expects |
| **Browser redirect** | HTML redirect page | Not JSON at all |
| **Rate limit page** | HTML with "too many requests" | Not JSON |
| **Cert warning** | Browser block page | Not JSON |
| **Empty response** | No body | JSON parse fails immediately |

### Why Spotify Works and Teams Doesn't

**Spotify properly handles all responses:**
```rust
if !response.status().is_success() {
    let body = response.text().unwrap_or_default();
    return Err(format!("Token request failed: {} - {}", status, body));
}
```

**Teams assumes success and blindly parses:**
```rust
let raw: DeviceCodeResponseRaw = response.json()...  // Fails if not success or wrong structure
```

---

## Files Analyzed

### Source Files
| File | Purpose | Key Finding |
|------|---------|------------|
| `src-tauri/src/teams.rs` | Teams OAuth implementation | **CRITICAL: No HTTP response debugging** |
| `src-tauri/src/spotify.rs` | Spotify OAuth (reference) | ✅ Proper error handling with status + body |
| `src-tauri/src/commands.rs` | Tauri command handlers | Properly registered, issue is in teams.rs |
| `src-tauri/src/lib.rs` | App initialization | tauri-plugin-http initialized but unused |
| `src/lib/components/Onboarding.svelte` | Frontend UI | Test buttons added, invokes correctly |
| `src-tauri/Cargo.toml` | Rust dependencies | reqwest with "blocking" feature |
| `src-tauri/tauri.conf.json` | Tauri config | CSP includes Microsoft domains |
| `src-tauri/capabilities/default.json` | Permissions | HTTP permissions granted |

### HTTP Request Being Made

```http
POST https://login.microsoftonline.com/common/oauth2/v2.0/devicecode
Content-Type: application/x-www-form-urlencoded
User-Agent: (reqwest default - typically "reqwest/0.x.x")
Accept: */*
Content-Length: 85

client_id=14d82eec-204b-4c2f-b7e8-296a70dab67e&scope=Presence.ReadWrite%20User.Read
```

### Client ID Used
`14d82eec-204b-4c2f-b7e8-296a70dab67e`

This is Microsoft's **well-known client ID** for the Graph PowerShell SDK.

---

## Code Changes Required

### Phase 1: Add HTTP Response Debugging (CRITICAL)

**Modify `src-tauri/src/teams.rs` - `start_teams_auth_device_code()`:**

| Step | Current Code | Required Change |
|------|-------------|----------------|
| 1 | No status logging | Log `response.status()` BEFORE json() |
| 2 | No raw body | Extract `response.text()` and log it |
| 3 | No success check | Check `if !response.status().is_success()` before parse |
| 4 | No error body | On error, return `status + body` in error message |
| 5 | No parse error details | On json() fail, include raw body in error |

**Same changes required for:**
- `poll_teams_auth()` - has identical pattern
- `complete_teams_auth()` - has identical pattern
- `refresh_teams_token()` - has identical pattern

### Phase 2: Add HTTP Headers

Add to device code request:
```rust
.header("Accept", "application/json")
.header("User-Agent", "PresenceJam/2.0")
```

### Phase 3: Handle Microsoft Error Format

Microsoft Azure AD errors return:
```json
{
  "error": "invalid_client",
  "error_description": "AADSTS70000: ..." 
}
```

Need to handle this separately from success format.

---

## Test Results Log

| Test | Environment | Result | Notes |
|------|-------------|--------|-------|
| Spotify Connect | MSI | ✅ WORKS | Deep link callback works properly |
| Spotify Connect | Dev Mode | ❌ FAILS | `presencejam://` not registered in Windows (dev mode limitation) |
| TEST CLICK | MSI | ✅ WORKS | Simple state mutation |
| TEST INVOKE | MSI | ❌ FAILS | Error: "Failed to parse device code response" |
| Sign in with MS | MSI | ❌ FAILS | Same invoke error |
| Sign in with MS | Off-Network | ❌ FAILS | **Same error - NOT corporate proxy issue** |

---

## Proposed Fix Plan

### Priority 1: Add Comprehensive Debug Logging (MUST DO FIRST)

Modify `teams.rs::start_teams_auth_device_code()`:

```rust
pub fn start_teams_auth_device_code() -> Result<DeviceCodeResponse, String> {
    log::info!("teams::start_teams_auth_device_code: starting");
    
    let client = reqwest::blocking::Client::builder()
        .user_agent("PresenceJam/2.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    let params = [
        ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
        ("scope", "Presence.ReadWrite User.Read"),
    ];
    
    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .map_err(|e| {
            log::error!("teams::start_teams_auth_device_code: send failed: {}", e);
            format!("Failed to send device code request: {}", e)
        })?;
    
    // CRITICAL: Log status code
    let status = response.status();
    log::info!("teams::start_teams_auth_device_code: response status: {}", status);
    
    // CRITICAL: Get raw body BEFORE attempting JSON parse
    let raw_body = response.text().map_err(|e| {
        log::error!("teams::start_teams_auth_device_code: failed to read body: {}", e);
        format!("Failed to read response body: {}", e)
    })?;
    log::info!("teams::start_teams_auth_device_code: raw response body: {}", raw_body);
    
    // CRITICAL: Check if the request actually succeeded
    if !status.is_success() {
        return Err(format!(
            "Device code request failed with status {}: {}",
            status, raw_body
        ));
    }
    
    // Now try to parse as success format
    let raw: DeviceCodeResponseRaw = serde_json::from_str(&raw_body)
        .map_err(|e| format!("Failed to parse device code response: {} (body was: {})", e, raw_body))?;
    
    // ... rest of function
}
```

### Priority 2: Handle Microsoft Error Response Format

Add error response handling:

```rust
#[derive(Debug, Deserialize)]
struct MicrosoftErrorResponse {
    error: String,
    error_description: Option<String>,
    timestamp: Option<String>,
    trace_id: Option<String>,
}

// Try success format first, then error format
let raw: Result<DeviceCodeResponseRaw, _> = serde_json::from_str(&raw_body);
match raw {
    Ok(resp) => { /* success */ }
    Err(_) => {
        // Try Microsoft error format
        let error_resp: MicrosoftErrorResponse = serde_json::from_str(&raw_body)
            .map_err(|e| format!("Unknown response format: {}", raw_body))?;
        return Err(format!(
            "Microsoft error: {} - {}",
            error_resp.error,
            error_resp.error_description.unwrap_or_default()
        ));
    }
}
```

### Priority 3: Apply Same Fixes to Other Auth Functions

Same debug logging needs to be added to:
- `poll_teams_auth()`
- `complete_teams_auth()`
- `refresh_teams_token()`
- All other HTTP functions in teams.rs

---

## Key Discoveries

### 1. tauri-plugin-http Is Unused
- Initialized in `lib.rs:177` but never called anywhere
- Both Spotify and Teams use `reqwest::blocking::Client`
- Using it would NOT solve the issue - same HTTP response would fail same way

### 2. The Real Issue Is Code, Not Network
- Off-network machine fails with same error
- Corporate proxy ruled out as root cause
- Problem is in `teams.rs` - lacks HTTP response debugging

### 3. Spotify vs Teams Pattern Difference
- Spotify: Checks status → logs body → returns error with body → parses on success
- Teams: Blindly parses → fails silently → error message is useless

### 4. HTTP Response Debugging Is Critical
- Need to log status code before json()
- Need to log raw body before json()
- Need to return error with status + body when request fails

### 5. Microsoft Error Format Is Different
- Azure AD errors have `error` and `error_description` fields
- Device code success has `user_code`, `device_code`, etc.
- Current code doesn't handle the error format

---

## Why This Was Hard to Debug

1. **Silent failures** - The error "error decoding response body" provides no information about what the response actually was
2. **No HTTP-level visibility** - We couldn't see status codes or response bodies
3. **Assumption of success** - The code assumes every response is JSON success and fails silently when it's not
4. **Limited logging** - Even with Rust `log::info!` statements, we never logged the raw response
5. **DevTools unavailable** - Tauri/WebView2 doesn't support F12 debugging in the same way

---

## Next Steps

1. **Implement Phase 1 fix** - Add HTTP response debugging to `start_teams_auth_device_code()`
2. **Rebuild and test** - Get actual error details from Microsoft
3. **Implement Phase 2** - Handle Microsoft error response format
4. **Apply to all auth functions** - Same fixes needed in poll, complete, refresh
5. **Verify fix** - Confirm Teams auth works on both networks

---

## Related Files

### Source Files
- `src-tauri/src/teams.rs` - Teams OAuth implementation (NEEDS FIX)
- `src-tauri/src/spotify.rs` - Spotify OAuth (REFERENCE - works correctly)
- `src-tauri/src/commands.rs` - Tauri command handlers
- `src-tauri/src/lib.rs` - App initialization

### Configuration Files
- `src-tauri/tauri.conf.json` - Tauri configuration
- `src-tauri/capabilities/default.json` - Permissions
- `src-tauri/Cargo.toml` - Rust dependencies

### Documentation
- [Microsoft OAuth 2.0 Device Authorization Grant](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)
- [reqwest HTTP Client](https://docs.rs/reqwest/)
- [Microsoft AADSTS Error Codes](https://learn.microsoft.com/en-us/entra/identity-platform/reference-aadsts-error-codes)

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
- It uses the .NET HTTP stack which may handle responses differently
- The .NET stack may have different default behaviors
- It may include proper response error handling internally
