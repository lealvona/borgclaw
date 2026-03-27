# LLM Provider Endpoint Test Report

**Date:** 2026-03-26  
**Tester:** Automated endpoint verification  
**Status:** ⚠️ Issues Found

---

## Summary

| Provider | Endpoint Status | Model List API | Static Models | Issues |
|----------|----------------|----------------|---------------|---------|
| OpenAI | ✅ 401 (Auth required) | ✅ Supported | 3 models | None |
| Anthropic | ✅ 401 (Auth required) | ✅ Supported | 3 models | None |
| Google | ⚠️ 403 (Key required) | ✅ Supported | 3 models | None |
| Kimi | ✅ 401 (Auth required) | ✅ Supported | 2 models | None |
| **MiniMax** | ❌ **404** | ❌ **Not supported** | 3 models | **Wrong endpoint** |
| **Z.ai** | ❌ **404** | ❌ **Not supported** | 3 models | **Wrong endpoint** |
| Ollama | ✅ Running locally | ✅ Supported | 2 models | None |

---

## Detailed Findings

### ✅ Working Providers

#### OpenAI
- **Base URL:** `https://api.openai.com/v1`
- **Models Endpoint:** `https://api.openai.com/v1/models`
- **Status:** HTTP 401 (Expected - requires API key)
- **Static Models:** gpt-4o, gpt-4o-mini, gpt-4-turbo
- **Default:** gpt-4o
- **Issue:** None

#### Anthropic
- **Base URL:** `https://api.anthropic.com/v1`
- **Models Endpoint:** `https://api.anthropic.com/v1/models`
- **Status:** HTTP 401 (Expected - requires API key)
- **Static Models:** claude-sonnet-4-20250514, claude-3-5-sonnet-20240620, claude-3-opus-20240229
- **Default:** claude-sonnet-4-20250514
- **Issue:** None

#### Google
- **Base URL:** `https://generativelanguage.googleapis.com/v1`
- **Models Endpoint:** `https://generativelanguage.googleapis.com/v1beta/models`
- **Status:** HTTP 403 (Expected - requires API key)
- **Static Models:** gemini-2.5-pro, gemini-2.5-flash, gemini-2.0-flash
- **Default:** gemini-2.5-pro
- **Issue:** None

#### Kimi (Moonshot)
- **Base URL:** `https://api.moonshot.cn/v1`
- **Models Endpoint:** `https://api.moonshot.cn/v1/models`
- **Status:** HTTP 401 (Expected - requires API key)
- **Static Models:** kimi-k2.5, kimi-k2
- **Default:** kimi-k2.5
- **Issue:** None

#### Ollama (Local)
- **Base URL:** `http://localhost:11434/api`
- **Models Endpoint:** `http://localhost:11434/api/tags`
- **Status:** ✅ Running locally
- **Static Models:** llama3, mistral
- **Default:** llama3
- **Issue:** None

---

### ❌ Broken Providers

#### MiniMax
- **Current Base URL (Wrong):** `https://api.minimax.chat/v1`
- **Correct Base URL:** `https://api.minimax.io/v1`
- **Models Endpoint:** ❌ **NOT SUPPORTED**
- **Status:** HTTP 404
- **Static Models:** MiniMax-M2.7, MiniMax-M2.5, MiniMax-M2.1
- **Default:** MiniMax-M2.7
- **Issue:** 
  - Wrong base URL (`api.minimax.chat` → should be `api.minimax.io`)
  - No `/v1/models` endpoint exists
  - Model listing not supported by MiniMax API

**Evidence:**
```bash
curl https://api.minimax.chat/v1/models
# Returns: 404 page not found

curl https://api.minimax.io/v1/models  
# Also returns: 404 page not found
```

**Documentation:** https://platform.minimax.io/docs/api-reference/text-chat

**Note:** MiniMax uses a different API structure. They don't expose a model listing endpoint.

---

#### Z.ai
- **Current Base URL (Wrong):** `https://api.z.ai/v1`
- **Correct Base URL:** `https://api.z.ai/api/paas/v4`
- **Models Endpoint:** ❌ **NOT SUPPORTED**
- **Status:** HTTP 404
- **Static Models:** glm-4.7, glm-4.6, glm-4.5
- **Default:** glm-4.7
- **Issue:**
  - Wrong base URL (`/v1` → should be `/api/paas/v4`)
  - No `/v1/models` endpoint exists
  - Model listing not supported by Z.ai API

**Evidence:**
```bash
curl https://api.z.ai/v1/models
# Returns: 404 Not Found (nginx)
```

**Documentation:** https://docs.z.ai/guides/develop/http/introduction

**Note:** Z.ai uses a different API structure. Their endpoint is `/api/paas/v4/chat/completions`, not `/v1/models`.

---

## Code Issues

### File: `borgclaw-cli/src/onboarding/providers.rs`

#### MiniMax Configuration (Lines 184-196)
```rust
// CURRENT (WRONG)
api_base: "https://api.minimax.chat/v1".to_string(),
models_endpoint: "https://api.minimax.chat/v1/models".to_string(),

// SHOULD BE
api_base: "https://api.minimax.io/v1".to_string(),
// models_endpoint: NOT SUPPORTED - remove or use static models only
```

#### Z.ai Configuration (Lines 198-211)
```rust
// CURRENT (WRONG)
api_base: "https://api.z.ai/v1".to_string(),
models_endpoint: "https://api.z.ai/v1/models".to_string(),

// SHOULD BE
api_base: "https://api.z.ai/api/paas/v4".to_string(),
// models_endpoint: NOT SUPPORTED - remove or use static models only
```

### File: `borgclaw-cli/src/onboarding/mod.rs`

#### fetch_models Function (Lines 2236-2256)

The `fetch_models` function tries to query model endpoints for MiniMax and Z.ai, but these providers don't support model listing:

```rust
"kimi" | "minimax" | "z" => {
    // OpenAI-compatible model listing
    // ...
}
```

**Problem:** MiniMax and Z.ai are NOT OpenAI-compatible for model listing. They only support chat completions.

---

## Recommended Fixes

### 1. Fix MiniMax Base URL
**File:** `borgclaw-cli/src/onboarding/providers.rs`

```rust
providers.insert(
    "minimax".to_string(),
    ProviderDef {
        id: "minimax".to_string(),
        display: "MiniMax".to_string(),
        api_base: "https://api.minimax.io/v1".to_string(),  // FIXED
        models_endpoint: "".to_string(),  // NOT SUPPORTED
        // ... rest unchanged
    },
);
```

### 2. Fix Z.ai Base URL
**File:** `borgclaw-cli/src/onboarding/providers.rs`

```rust
providers.insert(
    "z".to_string(),
    ProviderDef {
        id: "z".to_string(),
        display: "Z.ai".to_string(),
        api_base: "https://api.z.ai/api/paas/v4".to_string(),  // FIXED
        models_endpoint: "".to_string(),  // NOT SUPPORTED
        // ... rest unchanged
    },
);
```

### 3. Update fetch_models to Handle Missing Model Endpoints
**File:** `borgclaw-cli/src/onboarding/mod.rs`

Remove MiniMax and Z.ai from the OpenAI-compatible model listing match arm:

```rust
"kimi" => {
    // OpenAI-compatible model listing
    // ...
}
"minimax" | "z" => {
    // These providers don't support model listing
    // Return static models only
    Ok(provider.static_models.clone())
}
```

### 4. Fix default_models_endpoint Helper
**File:** `borgclaw-cli/src/onboarding/providers.rs`

Update to handle providers without model endpoints:

```rust
fn default_models_endpoint(id: &str, api_base: &str) -> String {
    match id {
        "openai" => format!("{api_base}/models"),
        "anthropic" => format!("{api_base}/models"),
        "minimax" | "z" => "".to_string(),  // NOT SUPPORTED
        _ => format!("{}/models", api_base.trim_end_matches('/')),
    }
}
```

---

## Model Accuracy Verification

### Current Static Models

| Provider | Models Listed | Verified Against Docs |
|----------|---------------|----------------------|
| OpenAI | gpt-4o, gpt-4o-mini, gpt-4-turbo | ✅ Yes |
| Anthropic | claude-sonnet-4-20250514, claude-3-5-sonnet-20240620, claude-3-opus-20240229 | ✅ Yes |
| Google | gemini-2.5-pro, gemini-2.5-flash, gemini-2.0-flash | ✅ Yes |
| Kimi | kimi-k2.5, kimi-k2 | ✅ Yes |
| MiniMax | MiniMax-M2.7, MiniMax-M2.5, MiniMax-M2.1 | ✅ Yes |
| Z.ai | glm-4.7, glm-4.6, glm-4.5 | ✅ Yes |
| Ollama | llama3, mistral | ✅ Yes |

All static models are accurate according to provider documentation.

---

## Testing Checklist

- [x] OpenAI endpoint reachable
- [x] Anthropic endpoint reachable  
- [x] Google endpoint reachable
- [x] Kimi endpoint reachable
- [ ] MiniMax endpoint reachable (WRONG URL)
- [ ] Z.ai endpoint reachable (WRONG URL)
- [x] Ollama endpoint reachable

- [x] OpenAI model list API supported
- [x] Anthropic model list API supported
- [x] Google model list API supported
- [x] Kimi model list API supported
- [ ] MiniMax model list API supported (NOT SUPPORTED)
- [ ] Z.ai model list API supported (NOT SUPPORTED)
- [x] Ollama model list API supported

---

## Conclusion

**Critical Issues:**
1. MiniMax base URL is wrong (`api.minimax.chat` → `api.minimax.io`)
2. Z.ai base URL is wrong (`/v1` → `/api/paas/v4`)
3. Both providers don't support model listing - code tries to fetch from non-existent endpoints

**Impact:**
- Users cannot use MiniMax or Z.ai providers
- Model fetching fails silently, falls back to static models
- API calls will fail with 404 errors

**Priority:** HIGH - Fix before next release

---

## References

- MiniMax Docs: https://platform.minimax.io/docs/api-reference/text-chat
- Z.ai Docs: https://docs.z.ai/guides/develop/http/introduction
- Kimi Docs: https://platform.moonshot.ai/docs
