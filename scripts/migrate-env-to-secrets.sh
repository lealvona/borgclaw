#!/usr/bin/env bash
# Migrate secrets from .env file to BorgClaw encrypted secret store

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
borgclaw_prepare_build_env

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║      🔐 Migrate .env Secrets to Encrypted Store               ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

# Check if .env exists
if [ ! -f ".env" ]; then
    echo -e "${YELLOW}⚠ No .env file found in $ROOT_DIR${NC}"
    echo "Nothing to migrate."
    exit 0
fi

echo -e "${BLUE}Found .env file. Scanning for secrets...${NC}"
echo ""

# Parse .env and extract potential secrets
declare -a found_secrets=()
while IFS= read -r line; do
    # Skip comments and empty lines
    [[ "$line" =~ ^#.*$ ]] && continue
    [[ -z "$line" ]] && continue
    
    # Check if line looks like a secret (contains _API_KEY, _TOKEN, _SECRET, etc.)
    if [[ "$line" =~ (_API_KEY|_TOKEN|_SECRET|_PASSWORD|CLIENT_ID|CLIENT_SECRET)= ]]; then
        key="${line%%=*}"
        value="${line#*=}"
        
        # Skip empty values
        [[ -z "$value" ]] && continue
        
        found_secrets+=("$key")
        echo -e "${CYAN}Found:${NC} $key"
    fi
done < ".env"

if [ ${#found_secrets[@]} -eq 0 ]; then
    echo -e "${YELLOW}No secrets found in .env file.${NC}"
    exit 0
fi

echo ""
echo -e "${GREEN}Found ${#found_secrets[@]} potential secret(s)${NC}"
echo ""

# Check if BorgClaw binary exists
BORGCLAW_BIN=""
TARGET_DIR="$(borgclaw_target_dir)"
if [ -f "$TARGET_DIR/release/borgclaw" ]; then
    BORGCLAW_BIN="$TARGET_DIR/release/borgclaw"
elif [ -f "$TARGET_DIR/debug/borgclaw" ]; then
    BORGCLAW_BIN="$TARGET_DIR/debug/borgclaw"
elif command -v borgclaw &> /dev/null; then
    BORGCLAW_BIN="borgclaw"
else
    echo -e "${RED}✗ BorgClaw binary not found${NC}"
    echo "Please build first: ./scripts/with-build-env.sh cargo build --release"
    exit 1
fi

echo -e "${BLUE}Using BorgClaw binary:${NC} $BORGCLAW_BIN"
echo ""

# Verify secret store is working
echo "Checking encrypted secret store..."
if ! $BORGCLAW_BIN secrets list &>/dev/null 2>&1; then
    echo -e "${YELLOW}⚠ Secret store may not be initialized yet.${NC}"
    echo "It will be initialized during migration."
    echo ""
fi

# Confirm migration
echo -e "${YELLOW}⚠ Warning: This will:${NC}"
echo "  1. Read secrets from .env file"
echo "  2. Store them in BorgClaw's encrypted secret store"
echo "  3. Show you what was migrated"
echo ""
echo -e "${CYAN}The .env file will NOT be deleted automatically.${NC}"
echo ""
read -p "Continue with migration? [y/N]: " confirm

if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
    echo "Migration cancelled."
    exit 0
fi

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                   Migrating Secrets...                        ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

# Migrate each secret
success_count=0
failed_count=0

for secret_key in "${found_secrets[@]}"; do
    # Extract value from .env
    secret_value=$(grep "^${secret_key}=" ".env" | cut -d'=' -f2-)
    
    # Skip if empty
    [[ -z "$secret_value" ]] && continue
    
    echo -n "Migrating $secret_key... "
    
    # Use borgclaw secrets set command
    if echo "$secret_value" | $BORGCLAW_BIN secrets set "$secret_key" &>/dev/null; then
        echo -e "${GREEN}✓${NC}"
        ((success_count++))
    else
        echo -e "${RED}✗${NC}"
        ((failed_count++))
    fi
done

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                   Migration Complete                          ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""
echo -e "${GREEN}Successfully migrated: $success_count secret(s)${NC}"
if [ $failed_count -gt 0 ]; then
    echo -e "${RED}Failed to migrate: $failed_count secret(s)${NC}"
fi

echo ""
echo -e "${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║${NC}  ${YELLOW}IMPORTANT: ACTION REQUIRED${NC}                               ${CYAN}║${NC}"
echo -e "${CYAN}╠═══════════════════════════════════════════════════════════╣${NC}"
echo -e "${CYAN}║${NC}                                                           ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  Secrets are now stored in the encrypted secret store.    ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  The .env file still exists with PLAINTEXT secrets.       ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}                                                           ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  ${RED}RECOMMENDED:${NC}                                            ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  Run the following command to remove the .env file:       ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}                                                           ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    ${GREEN}rm $ROOT_DIR/.env${NC}                                     ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}                                                           ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  Or backup first:                                         ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    ${GREEN}mv $ROOT_DIR/.env $ROOT_DIR/.env.backup.$(date +%Y%m%d)${NC} ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}                                                           ${CYAN}║${NC}"
echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Show verification command
echo "To verify secrets were stored correctly:"
echo -e "  ${BLUE}$BORGCLAW_BIN secrets list${NC}"
echo ""

# Show current .env status
echo -e "${YELLOW}.env file status:${NC}"
ls -lh ".env"
echo ""

echo "Migration script completed!"
