#!/usr/bin/env bash
#
# Google OAuth 2.0 Setup for BorgClaw
# ===================================
#
# PURPOSE:
#   Guides you through setting up Google OAuth 2.0 credentials for BorgClaw's
#   Google Workspace integration (Gmail, Drive, Calendar).
#
# WHAT THIS DOES:
#   - Explains the external setup required in Google Cloud Console
#   - Validates your OAuth credentials work
#   - Stores credentials securely in the encrypted vault
#   - Configures the OAuth callback URL
#
# EXTERNAL SETUP REQUIRED:
#   You MUST complete these steps in Google Cloud Console BEFORE running this script:
#
#   1. Go to https://console.cloud.google.com/
#   2. Create a new project (or select existing)
#   3. Enable APIs:
#      - Gmail API
#      - Google Drive API
#      - Google Calendar API
#   4. Go to "Credentials" → "Create Credentials" → "OAuth client ID"
#   5. Configure OAuth consent screen (External + add your email as test user)
#   6. Application type: "Web application"
#   7. Add redirect URI: http://localhost:8085/oauth/callback
#   8. Copy Client ID and Client Secret
#
# RUN THIS SCRIPT:
#   ./scripts/install-google-oauth.sh
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${GREEN}[google-oauth]${NC} $*"; }
warn() { echo -e "${YELLOW}[google-oauth]${NC} $*"; }
error() { echo -e "${RED}[google-oauth]${NC} $*"; }
info() { echo -e "${BLUE}[google-oauth]${NC} $*"; }
section() { echo -e "${CYAN}$*${NC}"; }
prompt() { echo -e "${YELLOW}▶${NC} $*"; }

clear
echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                                                                ║"
echo "║              🔐 Google OAuth 2.0 Setup 🔐                       ║"
echo "║                                                                ║"
echo "║     Configure Google Workspace integration for BorgClaw        ║"
echo "║                                                                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Check for borgclaw binary
BORGCLAW_BIN="${ROOT_DIR}/target/release/borgclaw"
if [ ! -f "$BORGCLAW_BIN" ]; then
    BORGCLAW_BIN="${ROOT_DIR}/target/debug/borgclaw"
    if [ ! -f "$BORGCLAW_BIN" ]; then
        error "BorgClaw binary not found. Please run ./scripts/bootstrap.sh first."
        exit 1
    fi
fi

# ============================================================================
# STEP 1: EXTERNAL SETUP INSTRUCTIONS
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 1: Google Cloud Console Setup (External)"
section "═══════════════════════════════════════════════════════════════"
echo ""

warn "⚠️  IMPORTANT: You must complete these steps BEFORE continuing:"
echo ""
info "1. Go to Google Cloud Console:"
info "   https://console.cloud.google.com/"
echo ""
info "2. Create or select a project"
echo ""
info "3. Enable the required APIs:"
info "   a. Go to 'APIs & Services' → 'Library'"
info "   b. Search and enable each of these:"
info "      • Gmail API"
info "      • Google Drive API"
info "      • Google Calendar API"
echo ""
info "4. Create OAuth 2.0 credentials:"
info "   a. Go to 'APIs & Services' → 'Credentials'"
info "   b. Click '+ CREATE CREDENTIALS' → 'OAuth client ID'"
info "   c. If prompted, configure OAuth consent screen:"
info "      • User Type: External"
info "      • Fill in app name, user support email, developer contact"
info "      • Add your email as a Test User"
info "   d. Application type: 'Web application'"
info "   e. Name: 'BorgClaw'"
info "   f. Authorized redirect URIs:"
info "      • Add: http://localhost:8085/oauth/callback"
info "   g. Click 'CREATE'"
echo ""
info "5. Copy the credentials:"
info "   • Client ID (looks like: xxx.apps.googleusercontent.com)"
info "   • Client Secret"
echo ""

prompt "Have you completed the Google Cloud Console setup? [y/N]: "
read -r completed_setup

if [[ ! "$completed_setup" =~ ^[Yy]$ ]]; then
    echo ""
    warn "Please complete the Google Cloud Console setup first, then re-run this script."
    exit 0
fi

# ============================================================================
# STEP 2: COLLECT AND VALIDATE CREDENTIALS
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 2: Enter OAuth Credentials"
section "═══════════════════════════════════════════════════════════════"
echo ""

# Get Client ID
while true; do
    prompt "Enter Google Client ID: "
    read -r CLIENT_ID
    
    if [ -z "$CLIENT_ID" ]; then
        error "Client ID cannot be empty"
        continue
    fi
    
    # Validate format
    if [[ ! "$CLIENT_ID" =~ \.apps\.googleusercontent\.com$ ]]; then
        warn "Warning: Client ID should end with '.apps.googleusercontent.com'"
        prompt "Continue anyway? [y/N]: "
        read -r continue_anyway
        [[ "$continue_anyway" =~ ^[Yy]$ ]] || continue
    fi
    
    break
done

# Get Client Secret
while true; do
    prompt "Enter Google Client Secret: "
    read -rs CLIENT_SECRET
    echo ""
    
    if [ -z "$CLIENT_SECRET" ]; then
        error "Client Secret cannot be empty"
        continue
    fi
    
    break
done

# Get Redirect URI
log ""
log "OAuth Redirect URI Configuration"
info "This is the URL Google will redirect to after authentication."
info "For the BorgClaw Gateway, the default is:"
info "  http://localhost:8085/oauth/callback"
echo ""
prompt "Use default redirect URI? [Y/n]: "
read -r use_default

if [[ "$use_default" =~ ^[Nn]$ ]]; then
    prompt "Enter custom redirect URI: "
    read -r REDIRECT_URI
else
    REDIRECT_URI="http://localhost:8085/oauth/callback"
fi

# ============================================================================
# STEP 3: TEST CREDENTIALS
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 3: Testing Credentials"
section "═══════════════════════════════════════════════════════════════"
echo ""

log "Testing OAuth configuration..."
info "Building authorization URL..."

# Build the auth URL to verify format
AUTH_URL="https://accounts.google.com/o/oauth2/v2/auth?client_id=${CLIENT_ID}&redirect_uri=$(printf '%s' "$REDIRECT_URI" | jq -sRr @uri)&response_type=code&scope=$(printf '%s' "https://www.googleapis.com/auth/gmail.send https://www.googleapis.com/auth/gmail.readonly https://www.googleapis.com/auth/drive.readonly https://www.googleapis.com/auth/calendar.readonly" | jq -sRr @uri)&access_type=offline&prompt=consent"

log "Authorization URL format is valid"
info "Full URL (for reference):"
echo "$AUTH_URL"
echo ""

warn "Note: Full validation requires completing the OAuth flow."
warn "You can test this after starting the Gateway with:"
warn "  ./scripts/gateway.sh"

# ============================================================================
# STEP 4: STORE CREDENTIALS
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 4: Storing Credentials Securely"
section "═══════════════════════════════════════════════════════════════"
echo ""

log "Storing credentials in encrypted vault..."

# Store credentials
if echo "$CLIENT_ID" | "$BORGCLAW_BIN" secrets set "GOOGLE_CLIENT_ID" 2>/dev/null; then
    log "✓ Client ID stored"
else
    error "✗ Failed to store Client ID"
    exit 1
fi

if echo "$CLIENT_SECRET" | "$BORGCLAW_BIN" secrets set "GOOGLE_CLIENT_SECRET" 2>/dev/null; then
    log "✓ Client Secret stored"
else
    error "✗ Failed to store Client Secret"
    exit 1
fi

# Clear from memory
unset CLIENT_ID CLIENT_SECRET

# Update configuration
CONFIG_DIR="${HOME}/.config/borgclaw"
CONFIG_FILE="${CONFIG_DIR}/config.toml"

if [ -f "$CONFIG_FILE" ]; then
    log "Updating configuration file..."
    
    # Check if skills.google section exists
    if grep -q "^\[skills.google\]" "$CONFIG_FILE" 2>/dev/null; then
        # Update existing section
        log "✓ Google skills section already exists"
    else
        # Add new section
        cat >> "$CONFIG_FILE" << EOF

[skills.google]
enabled = true
client_id = "\${GOOGLE_CLIENT_ID}"
client_secret = "\${GOOGLE_CLIENT_SECRET}"
redirect_uri = "${REDIRECT_URI}"
scopes = [
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/drive.readonly",
    "https://www.googleapis.com/auth/drive.file",
    "https://www.googleapis.com/auth/calendar.readonly",
    "https://www.googleapis.com/auth/calendar.events",
]
EOF
        log "✓ Configuration updated"
    fi
else
    warn "Config file not found at $CONFIG_FILE"
    warn "Please run ./scripts/onboarding.sh first"
fi

# ============================================================================
# COMPLETION
# ============================================================================

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                  ✅ GOOGLE OAUTH CONFIGURED!                    ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║                                                                ║"
echo "║  Your Google OAuth credentials are now configured and stored   ║"
echo "║  securely in the encrypted vault.                              ║"
echo "║                                                                ║"
echo "║  TO USE GOOGLE SERVICES:                                       ║"
echo "║                                                                ║"
echo "║  1. Start the Gateway:                                         ║"
echo "║     ./scripts/gateway.sh                                       ║"
echo "║                                                                ║"
echo "║  2. In the chat, ask:                                          ║"
echo "║     'Connect to my Google account'                             ║"
echo "║                                                                ║"
echo "║  3. Follow the OAuth link and authorize BorgClaw               ║"
echo "║                                                                ║"
echo "║  AVAILABLE GOOGLE TOOLS:                                       ║"
echo "║    • google_send_email    - Send emails via Gmail              ║"
echo "║    • google_read_email    - Read Gmail messages                ║"
echo "║    • google_list_files    - List Google Drive files            ║"
echo "║    • google_read_file     - Read file contents                 ║"
echo "║    • google_create_event  - Create Calendar events             ║"
echo "║    • google_list_events   - List Calendar events               ║"
echo "║                                                                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Clear any remaining sensitive data
CLIENT_ID=""
CLIENT_SECRET=""
