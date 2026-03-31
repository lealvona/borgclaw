#!/usr/bin/env bash
#
# BorgClaw Onboarding Wizard
# ==========================
#
# PURPOSE:
#   Collects user-specific configuration and optional component settings.
#   This is INTERACTIVE and should be run AFTER bootstrap.sh.
#
# WHAT THIS DOES:
#   - Collects your preferred AI provider (OpenAI, Anthropic, Google, etc.)
#   - Configures communication channels (WebSocket, Telegram, Webhook, etc.)
#   - Sets up optional integrations (GitHub, Google Workspace, Browser)
#   - Validates all inputs (checks port availability, token validity)
#
# EXTERNAL SETUP REQUIRED:
#   Some features require external accounts/services. This script will:
#   - EXPLAIN what external setup is needed
#   - PROVIDE links/instructions for setup
#   - VALIDATE your inputs work before accepting them
#   - NEVER proceed with invalid configuration
#
# RUN THIS SECOND:
#   ./scripts/onboarding.sh
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
source "$ROOT_DIR/scripts/lib/config.sh"
borgclaw_prepare_build_env
source "$ROOT_DIR/scripts/lib/validation.sh" 2>/dev/null || true

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${GREEN}[onboarding]${NC} $*"; }
warn() { echo -e "${YELLOW}[onboarding]${NC} $*"; }
error() { echo -e "${RED}[onboarding]${NC} $*"; }
info() { echo -e "${BLUE}[onboarding]${NC} $*"; }
section() { echo -e "${CYAN}$*${NC}"; }
prompt() { echo -e "${YELLOW}▶${NC} $*"; }

show_help() {
    cat << EOF
Usage: ./scripts/onboarding.sh [OPTIONS]

BorgClaw Onboarding Wizard - User Configuration

Run this AFTER ./scripts/bootstrap.sh to configure your personal settings.

OPTIONS:
    --quick              Minimal onboarding (skip optional integrations)
    --update             Reconfigure existing setup
    --features          Show available feature flags
    -h, --help          Show this help message

EXAMPLES:
    # Standard onboarding (recommended)
    ./scripts/onboarding.sh

    # Quick setup (minimal configuration)
    ./scripts/onboarding.sh --quick

    # Update existing configuration
    ./scripts/onboarding.sh --update

EXTERNAL SETUP DOCUMENTATION:
    GitHub:        https://github.com/settings/tokens
    Google OAuth:  https://console.cloud.google.com/apis/credentials
    Telegram Bot:  https://t.me/BotFather
    Anthropic:     https://console.anthropic.com/settings/keys
    OpenAI:        https://platform.openai.com/api-keys
EOF
}

show_features() {
    cat << EOF
Available Optional Features:

AI PROVIDERS (at least one required):
  anthropic         Claude models (recommended)
  openai            GPT models
  google            Gemini models
  kimi              Moonshot AI
  minimax           MiniMax AI
  z                 Z.ai models

COMMUNICATION CHANNELS:
  websocket         WebSocket channel (default: enabled)
  telegram          Telegram Bot channel
  webhook           HTTP webhook channel
  signal            Signal messenger channel

INTEGRATIONS (optional):
  github            GitHub repository management
  google            Google Workspace (Gmail, Drive, Calendar)
  browser           Browser automation (Playwright)
  stt               Speech-to-text (whisper.cpp)
  tts               Text-to-speech (ElevenLabs)
  image             Image generation

EXTERNAL SETUP REQUIRED:
  github            → GitHub Personal Access Token
  google            → Google OAuth 2.0 credentials
  telegram          → Telegram Bot Token from @BotFather
  browser           → Node.js 18+ installed

For detailed setup instructions, run: ./scripts/onboarding.sh
EOF
}

QUICK_MODE=false
UPDATE_MODE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --quick)
            QUICK_MODE=true
            shift
            ;;
        --update)
            UPDATE_MODE=true
            shift
            ;;
        --features)
            show_features
            exit 0
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                                                                ║"
echo "║              🤖 BorgClaw Onboarding Wizard 🤖                   ║"
echo "║                                                                ║"
echo "║     Configure your AI assistant with personalized settings     ║"
echo "║                                                                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

if [ "$QUICK_MODE" = true ]; then
    warn "Running in QUICK mode - skipping optional integrations"
    echo ""
fi

# Locate BorgClaw binary (prefer release, fallback to debug)
BORGCLAW_BIN="$(borgclaw_locate_binary)"
if [ -z "$BORGCLAW_BIN" ]; then
    error "BorgClaw binary not found!"
    error "Please run ./scripts/bootstrap.sh first to build the binaries."
    exit 1
fi

log "Using BorgClaw binary: $BORGCLAW_BIN"

# ============================================================================
# HELPER FUNCTIONS
# ============================================================================

# Check if a port is available
check_port_available() {
    local port="$1"
    if command -v nc &>/dev/null; then
        if nc -z localhost "$port" 2>/dev/null; then
            return 1  # Port is in use
        fi
    elif command -v lsof &>/dev/null; then
        if lsof -i :"$port" &>/dev/null; then
            return 1  # Port is in use
        fi
    elif [ -f /proc/net/tcp ]; then
        # Linux fallback
        local hex_port
        hex_port=$(printf '%04X' "$port")
        if grep -q ":${hex_port} " /proc/net/tcp 2>/dev/null; then
            return 1
        fi
    fi
    return 0  # Port is available
}

# Validate port with user feedback
validate_port() {
    local port="$1"
    local service="$2"
    
    if ! [[ "$port" =~ ^[0-9]+$ ]]; then
        error "Port must be a number"
        return 1
    fi
    
    if [ "$port" -lt 1024 ] || [ "$port" -gt 65535 ]; then
        error "Port must be between 1024 and 65535"
        return 1
    fi
    
    if ! check_port_available "$port"; then
        error "Port $port is already in use"
        info "Please choose a different port or stop the service using port $port"
        return 1
    fi
    
    return 0
}

# Test GitHub token
test_github_token() {
    local token="$1"
    if command -v gh &>/dev/null; then
        if echo "$token" | gh auth login --with-token 2>/dev/null; then
            return 0
        fi
    fi
    # Fallback to curl
    if curl -s -o /dev/null -w "%{http_code}" -H "Authorization: token $token" \
         https://api.github.com/user | grep -q "^20"; then
        return 0
    fi
    return 1
}

# ============================================================================
# SECTION 1: AI PROVIDER SELECTION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "SECTION 1: AI Provider (Required)"
section "═══════════════════════════════════════════════════════════════"
echo ""
info "BorgClaw needs an AI provider to function. This is the LLM that will"
info "power your assistant's responses and tool usage."
echo ""
info "External Setup Required:"
info "  • Anthropic (Claude): https://console.anthropic.com/settings/keys"
info "  • OpenAI (GPT):       https://platform.openai.com/api-keys"
info "  • Google (Gemini):    https://makersuite.google.com/app/apikey"
info "  • Kimi (Moonshot):    https://platform.moonshot.ai/"
info "  • MiniMax:            https://platform.minimax.io/"
info "  • Z.ai:               https://z.ai/model-api"
echo ""

# Provider API endpoints for fetching models
declare -A PROVIDER_API_BASES=(
    ["anthropic"]="https://api.anthropic.com/v1"
    ["openai"]="https://api.openai.com/v1"
    ["google"]="https://generativelanguage.googleapis.com/v1beta"
    ["kimi"]="https://api.moonshot.cn/v1"
    ["minimax"]="https://api.minimax.io/v1"
    ["z"]="https://api.z.ai/api/paas/v4"
)

# Fetch models from provider API
fetch_provider_models() {
    local provider="$1"
    local api_key="$2"
    local base_url="${PROVIDER_API_BASES[$provider]}"
    local models=""
    
    case "$provider" in
        anthropic)
            # Anthropic doesn't have a models endpoint yet, use known models
            models="claude-sonnet-4-20250514
claude-opus-4-20250514
claude-haiku-3.5"
            ;;
        openai)
            models=$(curl -s -H "Authorization: Bearer $api_key" \
                "${base_url}/models" 2>/dev/null | \
                grep -o '"id": "[^"*]*"' | \
                sed 's/"id": "//;s/"$//' | \
                grep -E '^(gpt-|o1-|o3-)' | \
                grep -v 'turbo\|instruct\|audio\|realtime\|vision' | \
                sort)
            ;;
        google)
            models=$(curl -s "${base_url}/models?key=${api_key}" 2>/dev/null | \
                grep -o '"name": "models/[^"]*"' | \
                sed 's/"name": "models\///;s/"$//' | \
                grep -E 'gemini' | \
                grep -v 'embedding\|vision' | \
                sort)
            ;;
        kimi)
            models=$(curl -s -H "Authorization: Bearer $api_key" \
                "${base_url}/models" 2>/dev/null | \
                grep -o '"id": "[^"]*"' | \
                sed 's/"id": "//;s/"$//' | \
                sort)
            ;;
        minimax)
            models=$(curl -s -H "Authorization: Bearer $api_key" \
                "${base_url}/models" 2>/dev/null | \
                grep -o '"id": "[^"]*"' | \
                sed 's/"id": "//;s/"$//' | \
                sort)
            ;;
        z)
            # Z.ai doesn't publish a models list endpoint, use known models
            models="glm-4.7
glm-4.6
glm-4.5"
            ;;
    esac
    
    echo "$models"
}

# Select model from fetched list or fallback to default
select_model() {
    local provider="$1"
    local api_key="$2"
    
    echo ""
    info "Fetching available models from $provider..."
    
    local models
    models=$(fetch_provider_models "$provider" "$api_key")
    
    if [ -z "$models" ] || [ "$(echo "$models" | wc -l)" -lt 1 ]; then
        warn "Could not fetch models from $provider API."
        warn "Using default model."
        set_default_model
        return
    fi
    
    echo ""
    log "Available models:"
    local i=1
    local model_array=()
    while IFS= read -r model; do
        [ -z "$model" ] && continue
        model_array+=("$model")
        local marker=""
        # Mark recommended models
        case "$provider" in
            anthropic)
                [[ "$model" == *"sonnet-4"* ]] && marker=" (recommended)"
                ;;
            openai)
                [[ "$model" == *"gpt-4o"* ]] && [[ "$model" != *"mini"* ]] && marker=" (recommended)"
                ;;
            google)
                [[ "$model" == *"pro"* ]] && [[ "$model" != *"vision"* ]] && marker=" (recommended)"
                ;;
            kimi)
                [[ "$model" == *"k2.5"* ]] && marker=" (recommended)"
                ;;
            minimax)
                [[ "$model" == *"M2"* ]] && marker=" (recommended)"
                ;;
        esac
        echo "  $i) $model$marker"
        ((i++))
    done <<< "$models"
    
    echo ""
    prompt "Select model (1-$((i-1)), or press Enter for recommended): "
    read -r model_choice
    
    if [ -z "$model_choice" ]; then
        # Use recommended default
        set_default_model
    elif [[ "$model_choice" =~ ^[0-9]+$ ]] && [ "$model_choice" -ge 1 ] && [ "$model_choice" -lt "$i" ]; then
        MODEL="${model_array[$((model_choice-1))]}"
    else
        warn "Invalid selection. Using default model."
        set_default_model
    fi
    
    log "Selected model: $MODEL"
}

# Default model for each provider
set_default_model() {
    case "$PROVIDER" in
        anthropic) MODEL="claude-sonnet-4-20250514" ;;
        openai) MODEL="gpt-4o" ;;
        google) MODEL="gemini-2.5-pro" ;;
        kimi) MODEL="kimi-k2.5" ;;
        minimax) MODEL="MiniMax-M2.7" ;;
        z) MODEL="glm-4.7" ;;
        *) MODEL="gpt-4o" ;;
    esac
}

select_provider() {
    echo "Select your AI provider:"
    echo "  1) Anthropic Claude (recommended)"
    echo "  2) OpenAI GPT"
    echo "  3) Google Gemini"
    echo "  4) Kimi (Moonshot AI)"
    echo "  5) MiniMax"
    echo "  6) Z.ai"
    echo ""
    
    while true; do
        prompt "Enter choice (1-6): "
        read -r provider_choice
        
        case "$provider_choice" in
            1) PROVIDER="anthropic"; API_KEY_NAME="ANTHROPIC_API_KEY"; break ;;
            2) PROVIDER="openai"; API_KEY_NAME="OPENAI_API_KEY"; break ;;
            3) PROVIDER="google"; API_KEY_NAME="GOOGLE_API_KEY"; break ;;
            4) PROVIDER="kimi"; API_KEY_NAME="KIMI_API_KEY"; break ;;
            5) PROVIDER="minimax"; API_KEY_NAME="MINIMAX_API_KEY"; break ;;
            6) PROVIDER="z"; API_KEY_NAME="Z_API_KEY"; break ;;
            *) error "Invalid choice. Please enter 1-6." ;;
        esac
    done
    
    log "Selected provider: $PROVIDER"
}

# Check if provider already configured
CONFIG_FILE="${HOME}/.config/borgclaw/config.toml"
EXISTING_PROVIDER=""
EXISTING_MODEL=""
if [ -f "$CONFIG_FILE" ]; then
    EXISTING_PROVIDER=$(grep -E '^provider\s*=' "$CONFIG_FILE" 2>/dev/null | head -1 | sed 's/.*=\s*"\([^"]*\)".*/\1/' || echo "")
    EXISTING_MODEL=$(grep -E '^model\s*=' "$CONFIG_FILE" 2>/dev/null | head -1 | sed 's/.*=\s*"\([^"]*\)".*/\1/' || echo "")
fi

if [ "$UPDATE_MODE" = true ] || [ -z "$EXISTING_PROVIDER" ]; then
    select_provider
else
    log "Current provider: $EXISTING_PROVIDER"
    prompt "Change provider? [y/N]: "
    read -r change_provider
    if [[ "$change_provider" =~ ^[Yy]$ ]]; then
        select_provider
    else
        PROVIDER="$EXISTING_PROVIDER"
        case "$PROVIDER" in
            anthropic) API_KEY_NAME="ANTHROPIC_API_KEY" ;;
            openai) API_KEY_NAME="OPENAI_API_KEY" ;;
            google) API_KEY_NAME="GOOGLE_API_KEY" ;;
            kimi) API_KEY_NAME="KIMI_API_KEY" ;;
            minimax) API_KEY_NAME="MINIMAX_API_KEY" ;;
            z) API_KEY_NAME="Z_API_KEY" ;;
        esac
        # Use existing model if valid, otherwise use default
        if [ -n "$EXISTING_MODEL" ] && [ "$EXISTING_MODEL" != "default" ]; then
            MODEL="$EXISTING_MODEL"
        else
            set_default_model
        fi
    fi
fi

# ============================================================================
# SECTION 2: API KEY CONFIGURATION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "SECTION 2: API Key Configuration"
section "═══════════════════════════════════════════════════════════════"
echo ""

info "Your API key will be stored in an encrypted vault for security."
info "The key is NEVER written to disk in plain text."
echo ""

# Check if key already exists
if "$BORGCLAW_BIN" secrets check "$API_KEY_NAME" &>/dev/null; then
    log "✓ API key $API_KEY_NAME is already configured"
    prompt "Update API key? [y/N]: "
    read -r update_key
    if [[ ! "$update_key" =~ ^[Yy]$ ]]; then
        log "Keeping existing API key"
    else
        NEED_API_KEY=true
    fi
else
    NEED_API_KEY=true
fi

if [ "${NEED_API_KEY:-false}" = true ]; then
    warn ""
    warn "External Setup Required:"
    case "$PROVIDER" in
        anthropic)
            warn "  1. Visit: https://console.anthropic.com/settings/keys"
            warn "  2. Click 'Create Key'"
            warn "  3. Copy the key (starts with 'sk-ant-')"
            ;;
        openai)
            warn "  1. Visit: https://platform.openai.com/api-keys"
            warn "  2. Click 'Create new secret key'"
            warn "  3. Copy the key (starts with 'sk-')"
            ;;
        google)
            warn "  1. Visit: https://makersuite.google.com/app/apikey"
            warn "  2. Click 'Create API Key'"
            warn "  3. Copy the key"
            ;;
        *)
            warn "  Visit your provider's website to generate an API key"
            ;;
    esac
    warn ""
    
    while true; do
        prompt "Enter your $API_KEY_NAME: "
        read -rs API_KEY
        echo ""
        
        if [ -z "$API_KEY" ]; then
            error "API key cannot be empty"
            continue
        fi
        
        # Basic format validation
        case "$PROVIDER" in
            anthropic)
                if [[ ! "$API_KEY" =~ ^sk-ant- ]]; then
                    warn "Warning: Anthropic keys usually start with 'sk-ant-'"
                    prompt "Continue anyway? [y/N]: "
                    read -r continue_anyway
                    [[ "$continue_anyway" =~ ^[Yy]$ ]] || continue
                fi
                ;;
            openai)
                if [[ ! "$API_KEY" =~ ^sk- ]]; then
                    warn "Warning: OpenAI keys usually start with 'sk-'"
                    prompt "Continue anyway? [y/N]: "
                    read -r continue_anyway
                    [[ "$continue_anyway" =~ ^[Yy]$ ]] || continue
                fi
                ;;
        esac
        
        # Store the key
        log "Storing API key in encrypted vault..."
        if "$BORGCLAW_BIN" secrets set "$API_KEY_NAME" --value "$API_KEY"; then
            log "✓ API key stored successfully"
            unset API_KEY  # Clear from memory
            break
        else
            error "✗ Failed to store API key"
            prompt "Try again? [Y/n]: "
            read -r try_again
            [[ "$try_again" =~ ^[Nn]$ ]] && break
        fi
    done
fi

# ============================================================================
# SECTION 3: MODEL SELECTION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "SECTION 3: Model Selection"
section "═══════════════════════════════════════════════════════════════"
echo ""

# Retrieve API key from vault for model fetching
API_KEY_FOR_MODELS=$("$BORGCLAW_BIN" secrets get "$API_KEY_NAME" 2>/dev/null || echo "")

if [ -n "$API_KEY_FOR_MODELS" ]; then
    select_model "$PROVIDER" "$API_KEY_FOR_MODELS"
    unset API_KEY_FOR_MODELS
else
    warn "Could not retrieve API key for model fetching."
    set_default_model
    log "Using default model: $MODEL"
fi

# ============================================================================
# SECTION 4: WEBSOCKET GATEWAY CONFIGURATION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "SECTION 4: WebSocket Gateway Configuration"
section "═══════════════════════════════════════════════════════════════"
echo ""

info "The WebSocket Gateway provides a web dashboard for interacting with"
info "BorgClaw through your browser. It includes a chat interface and"
info "configuration editor."
echo ""

# Get current or default port
CURRENT_WS_PORT=$(borgclaw_ws_port)
log "Current WebSocket port: $CURRENT_WS_PORT"

prompt "Change WebSocket port? [y/N]: "
read -r change_ws_port

if [[ "$change_ws_port" =~ ^[Yy]$ ]]; then
    while true; do
        prompt "Enter WebSocket port (1024-65535, default 3000): "
        read -r WS_PORT
        WS_PORT=${WS_PORT:-3000}
        
        if validate_port "$WS_PORT" "WebSocket"; then
            log "✓ Port $WS_PORT is available"
            break
        fi
    done
else
    WS_PORT="$CURRENT_WS_PORT"
fi

# ============================================================================
# SECTION 5: OPTIONAL INTEGRATIONS (Skip in quick mode)
# ============================================================================

if [ "$QUICK_MODE" = false ]; then
    echo ""
    section "═══════════════════════════════════════════════════════════════"
    section "SECTION 5: Optional Integrations"
    section "═══════════════════════════════════════════════════════════════"
    echo ""
    info "These integrations enhance BorgClaw's capabilities but require"
    info "external accounts and setup. You can skip any and add them later."
    echo ""
    
    # GitHub Integration
    echo ""
    section "─── GitHub Integration ───"
    echo ""
    info "Allows BorgClaw to:"
    info "  • Read repository files and issues"
    info "  • Create pull requests and comments"
    info "  • Search code across repositories"
    echo ""
    
    # Check if already configured
    if "$BORGCLAW_BIN" secrets check "GITHUB_TOKEN" &>/dev/null; then
        log "✓ GitHub integration is already configured"
        prompt "Reconfigure GitHub integration? [y/N]: "
        read -r setup_github
        if [[ ! "$setup_github" =~ ^[Yy]$ ]]; then
            log "Keeping existing GitHub configuration"
            ENABLE_GITHUB=true
        fi
    else
        warn "External Setup Required:"
        warn "  1. Visit: https://github.com/settings/tokens"
        warn "  2. Click 'Generate new token (classic)'"
        warn "  3. Select scopes: repo, read:user, read:org"
        warn "  4. Copy the token (starts with 'ghp_')"
        echo ""
        prompt "Configure GitHub integration? [y/N]: "
        read -r setup_github
    fi
    
    if [[ "$setup_github" =~ ^[Yy]$ ]]; then
        while true; do
            prompt "Enter GitHub Personal Access Token: "
            read -rs GITHUB_TOKEN
            echo ""
            
            if [ -z "$GITHUB_TOKEN" ]; then
                error "Token cannot be empty"
                continue
            fi
            
            # Validate token format
            if [[ ! "$GITHUB_TOKEN" =~ ^ghp_[a-zA-Z0-9]{36}$ ]]; then
                warn "Warning: Token format looks unusual (should start with 'ghp_')"
                prompt "Continue anyway? [y/N]: "
                read -r continue_anyway
                [[ "$continue_anyway" =~ ^[Yy]$ ]] || continue
            fi
            
            # Test token
            log "Testing GitHub token..."
            if test_github_token "$GITHUB_TOKEN"; then
                log "✓ GitHub token is valid"
                if "$BORGCLAW_BIN" secrets set "GITHUB_TOKEN" --value "$GITHUB_TOKEN"; then
                    log "✓ GitHub token stored"
                    ENABLE_GITHUB=true
                fi
                unset GITHUB_TOKEN
                break
            else
                error "✗ GitHub token validation failed"
                info "Possible causes:"
                info "  • Token is expired or revoked"
                info "  • Token doesn't have required scopes"
                info "  • Network connectivity issue"
                prompt "Try again? [Y/n]: "
                read -r try_again
                [[ "$try_again" =~ ^[Nn]$ ]] && break
            fi
        done
    fi
    
    # Google Workspace Integration
    echo ""
    section "─── Google Workspace Integration ───"
    echo ""
    info "Allows BorgClaw to:"
    info "  • Send and read Gmail emails"
    info "  • Access Google Drive files"
    info "  • Read and create Calendar events"
    info "  • Use OAuth for secure authentication"
    echo ""
    
    # Check if already configured (has both client ID and secret)
    if "$BORGCLAW_BIN" secrets check "GOOGLE_CLIENT_ID" &>/dev/null && \
       "$BORGCLAW_BIN" secrets check "GOOGLE_CLIENT_SECRET" &>/dev/null; then
        log "✓ Google Workspace integration is already configured"
        prompt "Reconfigure Google Workspace integration? [y/N]: "
        read -r setup_google
        if [[ ! "$setup_google" =~ ^[Yy]$ ]]; then
            log "Keeping existing Google Workspace configuration"
        fi
    else
        warn "External Setup Required:"
        warn "  This requires Google OAuth 2.0 credentials. Run after onboarding:"
        warn "    ./scripts/install-google-oauth.sh"
        warn ""
        warn "  The setup script will guide you through:"
        warn "    1. Creating a Google Cloud project"
        warn "    2. Enabling Gmail, Drive, Calendar APIs"
        warn "    3. Creating OAuth 2.0 credentials"
        warn "    4. Configuring the OAuth consent screen"
        echo ""
        prompt "Configure Google Workspace integration? [y/N]: "
        read -r setup_google
    fi
    
    if [[ "$setup_google" =~ ^[Yy]$ ]]; then
        log "Running Google OAuth setup..."
        if [ -f "$ROOT_DIR/scripts/install-google-oauth.sh" ]; then
            "$ROOT_DIR/scripts/install-google-oauth.sh"
        else
            warn "install-google-oauth.sh not found. Please run it manually."
        fi
    fi
    
    # Telegram Integration
    echo ""
    section "─── Telegram Bot Integration ───"
    echo ""
    info "Allows BorgClaw to:"
    info "  • Receive messages from Telegram users"
    info "  • Send responses via Telegram"
    echo ""
    
    # Check if already configured
    if "$BORGCLAW_BIN" secrets check "TELEGRAM_BOT_TOKEN" &>/dev/null; then
        log "✓ Telegram integration is already configured"
        prompt "Reconfigure Telegram integration? [y/N]: "
        read -r setup_telegram
        if [[ ! "$setup_telegram" =~ ^[Yy]$ ]]; then
            log "Keeping existing Telegram configuration"
            ENABLE_TELEGRAM=true
        fi
    else
        warn "External Setup Required:"
        warn "  1. Message @BotFather on Telegram"
        warn "  2. Send /newbot command"
        warn "  3. Follow instructions to create bot"
        warn "  4. Copy the bot token (format: 123456789:ABCdefGHI..."
        echo ""
        prompt "Configure Telegram integration? [y/N]: "
        read -r setup_telegram
    fi
    
    if [[ "$setup_telegram" =~ ^[Yy]$ ]]; then
        while true; do
            prompt "Enter Telegram Bot Token: "
            read -r TELEGRAM_TOKEN
            
            if [ -z "$TELEGRAM_TOKEN" ]; then
                error "Token cannot be empty"
                continue
            fi
            
            # Basic format validation (123456789:ABC...)
            if [[ ! "$TELEGRAM_TOKEN" =~ ^[0-9]+:[A-Za-z0-9_-]+$ ]]; then
                warn "Warning: Token format looks unusual"
                warn "  Expected format: 123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
                prompt "Continue anyway? [y/N]: "
                read -r continue_anyway
                [[ "$continue_anyway" =~ ^[Yy]$ ]] || continue
            fi
            
            # Test token via Telegram API
            log "Testing Telegram token..."
            BOT_INFO=$(curl -s "https://api.telegram.org/bot${TELEGRAM_TOKEN}/getMe")
            if echo "$BOT_INFO" | grep -q '"ok":true'; then
                BOT_NAME=$(echo "$BOT_INFO" | grep -o '"username":"[^"]*"' | cut -d'"' -f4)
                log "✓ Token valid! Bot: @$BOT_NAME"
                if "$BORGCLAW_BIN" secrets set "TELEGRAM_BOT_TOKEN" --value "$TELEGRAM_TOKEN"; then
                    log "✓ Telegram token stored"
                    ENABLE_TELEGRAM=true
                fi
                break
            else
                error "✗ Telegram token validation failed"
                ERROR_DESC=$(echo "$BOT_INFO" | grep -o '"description":"[^"]*"' | cut -d'"' -f4)
                if [ -n "$ERROR_DESC" ]; then
                    error "  Error: $ERROR_DESC"
                fi
                prompt "Try again? [Y/n]: "
                read -r try_again
                [[ "$try_again" =~ ^[Nn]$ ]] && break
            fi
        done
    fi
    
    # Browser/Playwright
    echo ""
    section "─── Browser Automation (Playwright) ───"
    echo ""
    info "Allows BorgClaw to:"
    info "  • Navigate websites and take screenshots"
    info "  • Fill forms and click buttons"
    info "  • Extract data from web pages"
    echo ""
    
    # Check if already configured (playwright command available)
    PLAYWRIGHT_ALREADY_CONFIGURED=false
    if command -v npx &>/dev/null && npx playwright --version &>/dev/null 2>&1; then
        PLAYWRIGHT_ALREADY_CONFIGURED=true
        log "✓ Playwright is already installed: $(npx playwright --version 2>/dev/null | head -1)"
        prompt "Reinstall or update Playwright? [y/N]: "
        read -r install_playwright
        if [[ ! "$install_playwright" =~ ^[Yy]$ ]]; then
            log "Keeping existing Playwright installation"
        fi
    elif [ -f "$ROOT_DIR/.local/playwright/node_modules/.bin/playwright" ]; then
        PLAYWRIGHT_ALREADY_CONFIGURED=true
        log "✓ Playwright is already installed in workspace"
        prompt "Reinstall or update Playwright? [y/N]: "
        read -r install_playwright
        if [[ ! "$install_playwright" =~ ^[Yy]$ ]]; then
            log "Keeping existing Playwright installation"
        fi
    else
        warn "External Setup Required:"
        warn "  • Node.js 18+ must be installed"
        warn "  • Run: ./scripts/install-playwright.sh"
        echo ""
        
        if command -v node &>/dev/null; then
            log "✓ Node.js found: $(node --version)"
            prompt "Install Playwright now? [Y/n]: "
            read -r install_playwright
        else
            warn "✗ Node.js not found"
            info "Install from: https://nodejs.org"
            info "Then run: ./scripts/install-playwright.sh"
        fi
    fi
    
    if [[ "${install_playwright:-n}" =~ ^[Yy]$ ]]; then
        if [ -f "$ROOT_DIR/scripts/install-playwright.sh" ]; then
            "$ROOT_DIR/scripts/install-playwright.sh"
        else
            warn "install-playwright.sh not found. Please run it manually."
        fi
    fi
    
    # Webhook Channel
    echo ""
    section "─── Webhook Channel ───"
    echo ""
    info "Allows external services to send messages to BorgClaw via HTTP POST."
    info "Useful for CI/CD integrations, monitoring alerts, etc."
    echo ""
    
    # Check if already configured in existing config
    EXISTING_WEBHOOK_ENABLED=false
    EXISTING_WEBHOOK_PORT=""
    if [ -f "$CONFIG_FILE" ]; then
        if grep -q '^\[channels.webhook\]' "$CONFIG_FILE" 2>/dev/null; then
            if grep -A1 '^\[channels.webhook\]' "$CONFIG_FILE" | grep -q 'enabled = true'; then
                EXISTING_WEBHOOK_ENABLED=true
                EXISTING_WEBHOOK_PORT=$(grep -A2 '^\[channels.webhook\]' "$CONFIG_FILE" | grep 'port' | sed 's/.*=\s*//' | tr -d ' ')
            fi
        fi
    fi
    
    if [ "$EXISTING_WEBHOOK_ENABLED" = true ]; then
        log "✓ Webhook channel is already enabled (port: ${EXISTING_WEBHOOK_PORT:-8080})"
        prompt "Reconfigure webhook channel? [y/N]: "
        read -r setup_webhook
        if [[ ! "$setup_webhook" =~ ^[Yy]$ ]]; then
            log "Keeping existing webhook configuration"
            ENABLE_WEBHOOK=true
            WEBHOOK_PORT="${EXISTING_WEBHOOK_PORT:-8080}"
        fi
    else
        prompt "Enable webhook channel? [y/N]: "
        read -r setup_webhook
    fi
    
    if [[ "$setup_webhook" =~ ^[Yy]$ ]]; then
        while true; do
            prompt "Enter webhook port (1024-65535, default 8080): "
            read -r WEBHOOK_PORT
            WEBHOOK_PORT=${WEBHOOK_PORT:-8080}
            
            if validate_port "$WEBHOOK_PORT" "Webhook"; then
                log "✓ Port $WEBHOOK_PORT is available"
                ENABLE_WEBHOOK=true
                break
            fi
        done
    fi
fi  # End of non-quick mode sections

# ============================================================================
# SECTION 6: GENERATE CONFIGURATION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "SECTION 6: Generating Configuration"
section "═══════════════════════════════════════════════════════════════"
echo ""

log "Creating configuration file..."

# Build the config
CONFIG_DIR="${HOME}/.config/borgclaw"
mkdir -p "$CONFIG_DIR"

CONFIG_FILE="${CONFIG_DIR}/config.toml"

# Start with base configuration
cat > "$CONFIG_FILE" << EOF
# BorgClaw Configuration
# Generated by onboarding wizard on $(date)

[agent]
provider = "${PROVIDER}"
model = "${MODEL}"

[channels.websocket]
enabled = true
port = ${WS_PORT}
EOF

# Add optional channels
if [ "${ENABLE_WEBHOOK:-false}" = true ]; then
    cat >> "$CONFIG_FILE" << EOF

[channels.webhook]
enabled = true
port = ${WEBHOOK_PORT}
secret = "\${WEBHOOK_SECRET:-change-me-in-production}"
EOF
fi

if [ "${ENABLE_TELEGRAM:-false}" = true ]; then
    cat >> "$CONFIG_FILE" << EOF

[channels.telegram]
enabled = true
token = "\${TELEGRAM_BOT_TOKEN}"
EOF
fi

# Add skills configuration
if [ "${ENABLE_GITHUB:-false}" = true ]; then
    cat >> "$CONFIG_FILE" << EOF

[skills.github]
enabled = true
token = "\${GITHUB_TOKEN}"
EOF
fi

log "✓ Configuration written to: $CONFIG_FILE"

# ============================================================================
# COMPLETION
# ============================================================================

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                     ✅ SETUP COMPLETE!                          ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║                                                                ║"
echo "║  Your BorgClaw is now configured and ready to use!             ║"
echo "║                                                                ║"
echo "║  CONFIGURATION SUMMARY:                                        ║"
echo "║    • AI Provider:    ${PROVIDER}"
echo "║    • Model:          ${MODEL}"
echo "║    • WebSocket Port: ${WS_PORT}"

if [ "${ENABLE_GITHUB:-false}" = true ]; then
    echo "║    • GitHub:        Enabled"
fi
if [ "${ENABLE_TELEGRAM:-false}" = true ]; then
    echo "║    • Telegram:      Enabled"
fi
if [ "${ENABLE_WEBHOOK:-false}" = true ]; then
    echo "║    • Webhook:       Port ${WEBHOOK_PORT}"
fi

echo "║                                                                ║"
echo "║  NEXT STEPS:                                                   ║"
echo "║                                                                ║"
echo "║  1. Start the Gateway (web dashboard):                         ║"
echo "║     ./scripts/gateway.sh                                       ║"
echo "║                                                                ║"
echo "║  2. Open in browser:                                           ║"
echo "║     http://localhost:${WS_PORT}                                      ║"
echo "║                                                                ║"
echo "║  3. Or use the REPL (command line):                            ║"
echo "║     ./scripts/repl.sh                                          ║"
echo "║                                                                ║"
echo "║  4. Check system health:                                       ║"
echo "║     ./scripts/doctor.sh                                        ║"
echo "║                                                                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

if [ -f "$ROOT_DIR/scripts/install-google-oauth.sh" ] && [ "${setup_google:-n}" != "y" ]; then
    info "Optional: Add Google Workspace integration later:"
    info "  ./scripts/install-google-oauth.sh"
    echo ""
fi

log "Welcome to BorgClaw! 🎉"
echo ""
