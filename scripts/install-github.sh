#!/usr/bin/env bash
#
# GitHub Integration Setup for BorgClaw
# =====================================
#
# PURPOSE:
#   Guides you through setting up GitHub integration for BorgClaw,
#   including token generation and validation.
#
# WHAT THIS DOES:
#   - Explains how to create a GitHub Personal Access Token
#   - Validates the token has required permissions
#   - Tests authentication with GitHub API
#   - Stores the token securely in the encrypted vault
#
# EXTERNAL SETUP REQUIRED:
#   You need a GitHub Personal Access Token with these permissions:
#   - repo          (Full control of private repositories)
#   - read:user     (Read user profile data)
#   - read:org      (Read organization membership)
#
#   To create a token:
#   1. Go to https://github.com/settings/tokens
#   2. Click "Generate new token (classic)"
#   3. Select the scopes listed above
#   4. Generate and copy the token
#
# RUN THIS SCRIPT:
#   ./scripts/install-github.sh
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

log() { echo -e "${GREEN}[github-setup]${NC} $*"; }
warn() { echo -e "${YELLOW}[github-setup]${NC} $*"; }
error() { echo -e "${RED}[github-setup]${NC} $*"; }
info() { echo -e "${BLUE}[github-setup]${NC} $*"; }
section() { echo -e "${CYAN}$*${NC}"; }
prompt() { echo -e "${YELLOW}▶${NC} $*"; }

clear
echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                                                                ║"
echo "║              🔧 GitHub Integration Setup 🔧                     ║"
echo "║                                                                ║"
echo "║     Configure GitHub access for repository management          ║"
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

# Check for GitHub CLI (optional but recommended)
if command -v gh &>/dev/null; then
    GH_CLI_AVAILABLE=true
    log "✓ GitHub CLI detected: $(gh --version | head -1)"
else
    GH_CLI_AVAILABLE=false
    warn "○ GitHub CLI not found (optional but recommended)"
    info "  Install from: https://cli.github.com"
fi

# ============================================================================
# STEP 1: EXTERNAL SETUP INSTRUCTIONS
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 1: Create GitHub Personal Access Token"
section "═══════════════════════════════════════════════════════════════"
echo ""

warn "⚠️  IMPORTANT: You need a GitHub Personal Access Token"
echo ""
info "Required Permissions (Scopes):"
info "  ✓ repo       - Full repository access (read files, create PRs)"
info "  ✓ read:user  - Read user profile information"
info "  ✓ read:org   - Read organization membership"
echo ""
info "To create your token:"
info "  1. Go to: https://github.com/settings/tokens"
info "  2. Click 'Generate new token (classic)'"
info "  3. Enter a note: 'BorgClaw AI Assistant'"
info "  4. Select these scopes: repo, read:user, read:org"
info "  5. Click 'Generate token' at the bottom"
info "  6. COPY THE TOKEN IMMEDIATELY (you won't see it again)"
echo ""
info "The token will look like: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
echo ""

prompt "Have you created your GitHub token? [y/N]: "
read -r has_token

if [[ ! "$has_token" =~ ^[Yy]$ ]]; then
    echo ""
    warn "Please create a GitHub token first:"
    warn "  https://github.com/settings/tokens"
    exit 0
fi

# ============================================================================
# STEP 2: COLLECT AND VALIDATE TOKEN
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 2: Enter and Validate Token"
section "═══════════════════════════════════════════════════════════════"
echo ""

while true; do
    prompt "Enter your GitHub Personal Access Token: "
    read -rs GITHUB_TOKEN
    echo ""
    
    if [ -z "$GITHUB_TOKEN" ]; then
        error "Token cannot be empty"
        continue
    fi
    
    # Basic format validation
    if [[ ! "$GITHUB_TOKEN" =~ ^ghp_[a-zA-Z0-9]{36}$ ]]; then
        warn "Warning: Token format looks unusual"
        warn "  Expected format: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
        prompt "Continue anyway? [y/N]: "
        read -r continue_anyway
        [[ "$continue_anyway" =~ ^[Yy]$ ]] || continue
    fi
    
    # Test the token
    log "Testing token with GitHub API..."
    
    API_RESPONSE=$(curl -s -H "Authorization: token ${GITHUB_TOKEN}" \
        -H "Accept: application/vnd.github.v3+json" \
        https://api.github.com/user 2>&1)
    
    if echo "$API_RESPONSE" | grep -q '"login"'; then
        USERNAME=$(echo "$API_RESPONSE" | grep -o '"login":"[^"]*"' | head -1 | cut -d'"' -f4)
        log "✓ Token is valid!"
        log "✓ Authenticated as: $USERNAME"
        
        # Check scopes
        SCOPES=$(curl -s -I -H "Authorization: token ${GITHUB_TOKEN}" \
            https://api.github.com/user 2>&1 | grep -i "x-oauth-scopes:" | cut -d':' -f2-)
        
        if [ -n "$SCOPES" ]; then
            info "Granted scopes:$SCOPES"
        fi
        
        break
    else
        error "✗ Token validation failed"
        
        if echo "$API_RESPONSE" | grep -q '"message":"Bad credentials"'; then
            error "  The token is invalid or has been revoked"
        elif echo "$API_RESPONSE" | grep -q 'rate limit'; then
            error "  GitHub API rate limit exceeded. Please try again later"
        else
            ERROR_MSG=$(echo "$API_RESPONSE" | grep -o '"message":"[^"]*"' | head -1 | cut -d'"' -f4)
            if [ -n "$ERROR_MSG" ]; then
                error "  Error: $ERROR_MSG"
            fi
        fi
        
        prompt "Try again? [Y/n]: "
        read -r try_again
        if [[ "$try_again" =~ ^[Nn]$ ]]; then
            unset GITHUB_TOKEN
            exit 1
        fi
    fi
done

# ============================================================================
# STEP 3: STORE TOKEN
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 3: Storing Token Securely"
section "═══════════════════════════════════════════════════════════════"
echo ""

log "Storing GitHub token in encrypted vault..."

if echo "$GITHUB_TOKEN" | "$BORGCLAW_BIN" secrets set "GITHUB_TOKEN" 2>/dev/null; then
    log "✓ Token stored successfully"
    
    # Also authenticate with gh CLI if available
    if [ "$GH_CLI_AVAILABLE" = true ]; then
        log "Authenticating GitHub CLI..."
        if echo "$GITHUB_TOKEN" | gh auth login --with-token 2>/dev/null; then
            log "✓ GitHub CLI authenticated"
        else
            warn "Could not authenticate GitHub CLI (non-critical)"
        fi
    fi
    
    unset GITHUB_TOKEN  # Clear from memory
else
    error "✗ Failed to store token"
    unset GITHUB_TOKEN
    exit 1
fi

# ============================================================================
# STEP 4: UPDATE CONFIGURATION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "STEP 4: Updating Configuration"
section "═══════════════════════════════════════════════════════════════"
echo ""

CONFIG_DIR="${HOME}/.config/borgclaw"
CONFIG_FILE="${CONFIG_DIR}/config.toml"

if [ -f "$CONFIG_FILE" ]; then
    log "Updating configuration..."
    
    # Check if skills.github section exists
    if grep -q "^\[skills.github\]" "$CONFIG_FILE" 2>/dev/null; then
        log "✓ GitHub skills section already exists"
    else
        # Add new section
        cat >> "$CONFIG_FILE" << EOF

[skills.github]
enabled = true
token = "\${GITHUB_TOKEN}"
EOF
        log "✓ Configuration updated"
    fi
else
    warn "Config file not found at $CONFIG_FILE"
    warn "Please run ./scripts/onboarding.sh to create initial configuration"
fi

# ============================================================================
# COMPLETION
# ============================================================================

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                  ✅ GITHUB INTEGRATION READY!                   ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║                                                                ║"
echo "║  Your GitHub token is configured and ready to use!             ║"
echo "║                                                                ║"
echo "║  AVAILABLE GITHUB TOOLS:                                       ║"
echo "║    • github_read_file      - Read repository files             ║"
echo "║    • github_search_code    - Search code across repos          ║"
echo "║    • github_create_pr      - Create pull requests              ║"
echo "║    • github_list_issues    - List and filter issues            ║"
echo "║    • github_create_issue   - Create new issues                 ║"
echo "║    • github_list_repos     - List your repositories            ║"
echo "║    • github_create_branch  - Create branches                   ║"
echo "║                                                                ║"
echo "║  SAFETY FEATURES:                                              ║"
echo "║    • Destructive operations require confirmation               ║"
echo "║    • Operations limited to owned/allowlisted repos             ║"
echo "║    • All actions logged for audit                              ║"
echo "║                                                                ║"
echo "║  EXAMPLE USAGE:                                                ║"
echo "║    'List my GitHub repositories'                               ║"
echo "║    'Read the README from my-repo'                              ║"
echo "║    'Search for TODO comments in my code'                       ║"
echo "║                                                                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

log "GitHub integration is now active! 🎉"
echo ""
