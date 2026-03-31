#!/usr/bin/env bash
#
# BorgClaw Validation Library
# ===========================
#
# PURPOSE:
#   Provides reusable validation functions for BorgClaw setup scripts.
#   All functions are read-only (they don't modify system state).
#
# USAGE:
#   source "$ROOT_DIR/scripts/lib/validation.sh"
#

if [[ -n "${BORGCLAW_VALIDATION_SH_LOADED:-}" ]]; then
    return 0 2>/dev/null || exit 0
fi
readonly BORGCLAW_VALIDATION_SH_LOADED=1

# ============================================================================
# PORT VALIDATION
# ============================================================================

# Check if a port is available (not in use)
# Usage: borgclaw_port_available <port>
# Returns: 0 if available, 1 if in use
borgclaw_port_available() {
    local port="$1"
    
    # Validate port is a number
    if ! [[ "$port" =~ ^[0-9]+$ ]]; then
        return 1
    fi
    
    # Check range
    if [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
        return 1
    fi
    
    # Try different methods to check port availability
    if command -v nc &>/dev/null; then
        # netcat method
        if nc -z localhost "$port" 2>/dev/null; then
            return 1  # Port is in use
        fi
    elif command -v lsof &>/dev/null; then
        # lsof method
        if lsof -i :"$port" &>/dev/null; then
            return 1  # Port is in use
        fi
    elif command -v ss &>/dev/null; then
        # ss method (Linux)
        if ss -tuln | grep -q ":${port} "; then
            return 1
        fi
    elif [ -f /proc/net/tcp ]; then
        # Linux /proc fallback
        local hex_port
        hex_port=$(printf '%04X' "$port")
        if grep -q ":${hex_port} " /proc/net/tcp 2>/dev/null || \
           grep -q ":${hex_port} " /proc/net/tcp6 2>/dev/null; then
            return 1
        fi
    fi
    
    return 0  # Port appears available
}

# Validate port with detailed error message
# Usage: borgclaw_validate_port <port> <service_name>
# Returns: 0 if valid, 1 if invalid (prints error to stderr)
borgclaw_validate_port() {
    local port="$1"
    local service="${2:-service}"
    
    if ! [[ "$port" =~ ^[0-9]+$ ]]; then
        echo "Error: Port must be a number" >&2
        return 1
    fi
    
    if [ "$port" -lt 1024 ]; then
        echo "Error: Port $port is in the privileged range (1-1023)" >&2
        echo "       Choose a port between 1024 and 65535" >&2
        return 1
    fi
    
    if [ "$port" -gt 65535 ]; then
        echo "Error: Port must be 65535 or less" >&2
        return 1
    fi
    
    if ! borgclaw_port_available "$port"; then
        echo "Error: Port $port is already in use" >&2
        echo "       Choose a different port or stop the $service using this port" >&2
        return 1
    fi
    
    return 0
}

# Find an available port starting from a base port
# Usage: borgclaw_find_available_port <base_port> [max_attempts]
# Returns: Available port number (prints to stdout)
borgclaw_find_available_port() {
    local base_port="$1"
    local max_attempts="${2:-10}"
    local port="$base_port"
    local attempts=0
    
    while [ "$attempts" -lt "$max_attempts" ]; do
        if borgclaw_port_available "$port"; then
            echo "$port"
            return 0
        fi
        port=$((port + 1))
        attempts=$((attempts + 1))
    done
    
    return 1
}

# ============================================================================
# API KEY VALIDATION
# ============================================================================

# Validate GitHub token format
# Usage: borgclaw_validate_github_token_format <token>
# Returns: 0 if valid format, 1 otherwise
borgclaw_validate_github_token_format() {
    local token="$1"
    
    # Classic PAT format: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
    if [[ "$token" =~ ^ghp_[a-zA-Z0-9]{36}$ ]]; then
        return 0
    fi
    
    # Fine-grained PAT format: github_pat_xxx_xxx_xxx
    if [[ "$token" =~ ^github_pat_[a-zA-Z0-9_]+$ ]]; then
        return 0
    fi
    
    # Old token format (40 hex chars)
    if [[ "$token" =~ ^[a-f0-9]{40}$ ]]; then
        return 0
    fi
    
    return 1
}

# Test GitHub token via API (read-only)
# Usage: borgclaw_test_github_token <token>
# Returns: 0 if valid, 1 otherwise
borgclaw_test_github_token() {
    local token="$1"
    local response
    local http_code
    
    response=$(curl -s -w "\n%{http_code}" \
        -H "Authorization: token ${token}" \
        -H "Accept: application/vnd.github.v3+json" \
        https://api.github.com/user 2>/dev/null)
    
    http_code=$(echo "$response" | tail -n1)
    
    if [ "$http_code" = "200" ]; then
        return 0
    else
        return 1
    fi
}

# Get GitHub username from token
# Usage: borgclaw_get_github_user <token>
# Returns: Username (prints to stdout) or empty if invalid
borgclaw_get_github_user() {
    local token="$1"
    local response
    
    response=$(curl -s -H "Authorization: token ${token}" \
        -H "Accept: application/vnd.github.v3+json" \
        https://api.github.com/user 2>/dev/null)
    
    echo "$response" | grep -o '"login":"[^"]*"' | head -1 | cut -d'"' -f4
}

# Validate Telegram bot token format
# Usage: borgclaw_validate_telegram_token_format <token>
borgclaw_validate_telegram_token_format() {
    local token="$1"
    
    # Format: 123456789:ABCdefGHIjklMNOpqrsTUVwxyz
    if [[ "$token" =~ ^[0-9]+:[A-Za-z0-9_-]+$ ]]; then
        return 0
    fi
    
    return 1
}

# Test Telegram token via API (read-only)
# Usage: borgclaw_test_telegram_token <token>
# Returns: 0 if valid, 1 otherwise
borgclaw_test_telegram_token() {
    local token="$1"
    local response
    
    response=$(curl -s "https://api.telegram.org/bot${token}/getMe" 2>/dev/null)
    
    if echo "$response" | grep -q '"ok":true'; then
        return 0
    else
        return 1
    fi
}

# Get Telegram bot info from token
# Usage: borgclaw_get_telegram_bot_info <token>
# Returns: JSON with bot info
borgclaw_get_telegram_bot_info() {
    local token="$1"
    
    curl -s "https://api.telegram.org/bot${token}/getMe" 2>/dev/null
}

# ============================================================================
# PROVIDER API KEY VALIDATION
# ============================================================================

# Validate Anthropic API key format
# Usage: borgclaw_validate_anthropic_key_format <key>
borgclaw_validate_anthropic_key_format() {
    local key="$1"
    
    if [[ "$key" =~ ^sk-ant-[a-zA-Z0-9_-]+$ ]]; then
        return 0
    fi
    
    return 1
}

# Validate OpenAI API key format
# Usage: borgclaw_validate_openai_key_format <key>
borgclaw_validate_openai_key_format() {
    local key="$1"
    
    if [[ "$key" =~ ^sk-[a-zA-Z0-9]{48}$ ]]; then
        return 0
    fi
    
    return 1
}

# ============================================================================
# OAUTH VALIDATION
# ============================================================================

# Validate Google OAuth Client ID format
# Usage: borgclaw_validate_google_client_id <client_id>
borgclaw_validate_google_client_id() {
    local client_id="$1"
    
    # Format: xxx.apps.googleusercontent.com
    if [[ "$client_id" =~ \.apps\.googleusercontent\.com$ ]]; then
        return 0
    fi
    
    return 1
}

# Build Google OAuth authorization URL
# Usage: borgclaw_build_google_auth_url <client_id> <redirect_uri> [scopes]
borgclaw_build_google_auth_url() {
    local client_id="$1"
    local redirect_uri="$2"
    local scopes="${3:-https://www.googleapis.com/auth/gmail.readonly https://www.googleapis.com/auth/gmail.send https://www.googleapis.com/auth/drive.readonly}"
    local encoded_redirect
    local encoded_scopes
    
    # URL encode using jq if available, otherwise basic encoding
    if command -v jq &>/dev/null; then
        encoded_redirect=$(printf '%s' "$redirect_uri" | jq -sRr @uri)
        encoded_scopes=$(printf '%s' "$scopes" | jq -sRr @uri)
    else
        # Basic encoding (not perfect but works for most cases)
        encoded_redirect=$(printf '%s' "$redirect_uri" | sed 's/ /%20/g; s/:/%3A/g; s/\//%2F/g')
        encoded_scopes=$(printf '%s' "$scopes" | sed 's/ /%20/g; s/:/%3A/g; s/\//%2F/g')
    fi
    
    echo "https://accounts.google.com/o/oauth2/v2/auth?client_id=${client_id}&redirect_uri=${encoded_redirect}&response_type=code&scope=${encoded_scopes}&access_type=offline&prompt=consent"
}

# ============================================================================
# NETWORK VALIDATION
# ============================================================================

# Check if a host is reachable
# Usage: borgclaw_host_reachable <host> [port] [timeout]
# Returns: 0 if reachable, 1 otherwise
borgclaw_host_reachable() {
    local host="$1"
    local port="${2:-443}"
    local timeout="${3:-5}"
    
    if command -v nc &>/dev/null; then
        nc -z -w "$timeout" "$host" "$port" 2>/dev/null
        return $?
    elif command -v curl &>/dev/null; then
        curl -s --max-time "$timeout" "https://${host}:${port}" &>/dev/null
        return $?
    fi
    
    # Fallback: assume reachable if we can't test
    return 0
}

# Check if Ollama is running
# Usage: borgclaw_ollama_running [endpoint]
# Returns: 0 if running, 1 otherwise
borgclaw_ollama_running() {
    local endpoint="${1:-http://127.0.0.1:11434}"
    
    curl -s --max-time 2 "${endpoint}/api/tags" &>/dev/null
    return $?
}

# Check if provider API endpoint is reachable
# Usage: borgclaw_provider_reachable <provider>
# Returns: 0 if reachable, 1 otherwise
borgclaw_provider_reachable() {
    local provider="$1"
    local endpoint
    
    case "$provider" in
        anthropic)
            endpoint="https://api.anthropic.com"
            ;;
        openai)
            endpoint="https://api.openai.com"
            ;;
        google)
            endpoint="https://generativelanguage.googleapis.com"
            ;;
        *)
            return 0  # Unknown provider, assume reachable
            ;;
    esac
    
    borgclaw_host_reachable "$endpoint" 443 3
}
