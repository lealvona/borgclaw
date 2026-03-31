# Provider Uniformity Audit

**Date:** 2026-03-30  
**Version:** 0.16.0

## Summary

All LLM providers now have uniform shape and functionality. The following standardizations were implemented:

## Changes Made

### 1. Uniform Think Block Extraction

**Before:**
- `OpenAI`, `Anthropic`, `Google`, `Ollama` used `ProviderResponse::text()` - no think block extraction
- `Kimi`, `MiniMax`, `Z` used `ProviderResponse::from_text_with_think_blocks()` - extracted `<think>` blocks

**After:**
- **ALL** providers now use `ProviderResponse::from_text_with_think_blocks()`
- Think blocks (`<think>...</think>`) are extracted into `TranscriptArtifacts.reasoning`
- Provider metadata is captured in `TranscriptArtifacts.provider_metadata`

### 2. Uniform API Base URL Resolution

**Before:**
- `OpenAI`, `Anthropic`, `Google` had hardcoded URLs
- `Kimi`, `MiniMax`, `Z` supported `{PROVIDER}_API_BASE` env var
- `Ollama` used `OLLAMA_BASE_URL` env var

**After:**
- **ALL** providers support `{PROVIDER}_API_BASE` env var for URL override:
  - `OPENAI_API_BASE` (default: `https://api.openai.com/v1`)
  - `ANTHROPIC_API_BASE` (default: `https://api.anthropic.com/v1`)
  - `GOOGLE_API_BASE` (default: `https://generativelanguage.googleapis.com/v1beta`)
  - `OLLAMA_API_BASE` (default: `http://localhost:11434`)
  - `KIMI_API_BASE` (default: `https://api.moonshot.cn/v1`)
  - `MINIMAX_API_BASE` (default: `https://api.minimax.io/v1`)
  - `Z_API_BASE` (default: `https://api.z.ai/api/paas/v4`)
- `Ollama` also supports legacy `OLLAMA_BASE_URL` for backward compatibility

### 3. System Message Handling

Each provider correctly handles system messages according to their API requirements:

| Provider | System Message Handling |
|----------|------------------------|
| `OpenAI` | Native support in messages array |
| `Anthropic` | Extracted to separate `system` field |
| `Google` | Extracted to `system_instruction` field |
| `Ollama` | Native support in messages array |
| `Kimi` | Native support in messages array |
| `MiniMax` | Converted: prepended to first user message |
| `Z` | Native support in messages array |

### 4. Response Format

**All providers now return:**
- `text`: The visible response text
- `artifacts.reasoning`: Extracted think blocks (if any)
- `artifacts.provider_metadata`: Provider identifier and additional metadata

### 5. Error Handling

**All providers have consistent error handling:**
- `ProviderError::MissingEnv` - Missing API key
- `ProviderError::Request` - HTTP request failed
- `ProviderError::Parse` - Response parsing failed
- `ProviderError::RateLimited` - Rate limit exceeded (429)

## Testing

All 464 tests pass, including:
- 27 provider-specific tests
- 80 tool tests
- 140 agent tests

New test added:
- `all_providers_support_api_base_env_var` - Verifies all providers support API base URL override

## Migration Guide

No migration required. All changes are backward compatible:
- Existing configurations continue to work
- New `*_API_BASE` env vars are optional
- Think block extraction is automatic

## Future Considerations

When adding new providers:
1. Use `ProviderResponse::from_text_with_think_blocks()` for response handling
2. Implement `resolve_base_url()` with `{PROVIDER}_API_BASE` env var support
3. Handle system messages according to the provider's API specification
4. Follow the existing error handling patterns
