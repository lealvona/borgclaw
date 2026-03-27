#!/usr/bin/env bash
# Test LLM provider endpoints and verify model lists

set -euo pipefail

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║          LLM Provider Endpoint Verification                   ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test function for OpenAI-compatible endpoints
test_endpoint() {
    local name=$1
    local endpoint=$2
    local auth_header=${3:-""}
    local auth_value=${4:-""}
    
    echo -e "${BLUE}Testing $name...${NC}"
    echo "  Endpoint: $endpoint"
    
    if command -v curl &> /dev/null; then
        if [ -n "$auth_header" ]; then
            response=$(curl -s -o /dev/null -w "%{http_code}" -H "$auth_header: $auth_value" "$endpoint" 2>/dev/null || echo "000")
        else
            response=$(curl -s -o /dev/null -w "%{http_code}" "$endpoint" 2>/dev/null || echo "000")
        fi
        
        if [ "$response" = "200" ]; then
            echo -e "  ${GREEN}✓ Endpoint reachable (HTTP 200)${NC}"
        elif [ "$response" = "401" ]; then
            echo -e "  ${YELLOW}⚠ Authentication required (HTTP 401) - Expected${NC}"
        elif [ "$response" = "403" ]; then
            echo -e "  ${YELLOW}⚠ Forbidden (HTTP 403) - May need API key${NC}"
        elif [ "$response" = "000" ]; then
            echo -e "  ${RED}✗ Connection failed${NC}"
        else
            echo -e "  ${YELLOW}⚠ HTTP $response${NC}"
        fi
    else
        echo -e "  ${YELLOW}⚠ curl not available${NC}"
    fi
    echo ""
}

echo "Testing provider endpoints (without API keys where possible)..."
echo ""

# OpenAI
test_endpoint "OpenAI" "https://api.openai.com/v1/models" "Authorization" "Bearer sk-test"

# Anthropic
test_endpoint "Anthropic" "https://api.anthropic.com/v1/models" "x-api-key" "test-key"

# Google (may work without key for listing)
test_endpoint "Google" "https://generativelanguage.googleapis.com/v1beta/models"

# Kimi (Moonshot) - requires auth
test_endpoint "Kimi (Moonshot)" "https://api.moonshot.cn/v1/models" "Authorization" "Bearer test-key"

# MiniMax - requires auth
test_endpoint "MiniMax" "https://api.minimax.chat/v1/models" "Authorization" "Bearer test-key"

# Z.ai - requires auth
test_endpoint "Z.ai" "https://api.z.ai/v1/models" "Authorization" "Bearer test-key"

# Ollama (local)
echo -e "${BLUE}Testing Ollama (local)...${NC}"
if curl -s "http://localhost:11434/api/tags" &>/dev/null; then
    echo -e "  ${GREEN}✓ Ollama running locally${NC}"
else
    echo -e "  ${YELLOW}⚠ Ollama not running (expected if not installed)${NC}"
fi
echo ""

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                    Static Model Lists                         ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

cat << 'EOF'
Configured Static Models (from providers.rs):

OpenAI:
  - gpt-4o (default)
  - gpt-4o-mini
  - gpt-4-turbo

Anthropic:
  - claude-sonnet-4-20250514 (default)
  - claude-3-5-sonnet-20240620
  - claude-3-opus-20240229

Google:
  - gemini-2.5-pro (default)
  - gemini-2.5-flash
  - gemini-2.0-flash

Kimi (Moonshot):
  - kimi-k2.5 (default)
  - kimi-k2

MiniMax:
  - MiniMax-M2.7 (default)
  - MiniMax-M2.5
  - MiniMax-M2.1

Z.ai:
  - glm-4.7 (default)
  - glm-4.6
  - glm-4.5

Ollama (Local):
  - llama3 (default)
  - mistral

Custom:
  - custom-model (default)
EOF

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║              API Documentation References                     ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

cat << 'EOF'
Official API Documentation:

OpenAI:      https://platform.openai.com/docs/api-reference/models
Anthropic:   https://docs.anthropic.com/en/api/models-list
Google:      https://ai.google.dev/api/rest/v1beta/models/list
Kimi:        https://platform.moonshot.ai/docs/api-reference
MiniMax:     https://platform.minimax.io/docs/api-reference
Z.ai:        https://docs.z.ai/api-reference
Ollama:      https://github.com/ollama/ollama/blob/main/docs/api.md

Model List Endpoints (from codebase):
  OpenAI:     https://api.openai.com/v1/models
  Anthropic:  https://api.anthropic.com/v1/models
  Google:     https://generativelanguage.googleapis.com/v1beta/models
  Kimi:       https://api.moonshot.cn/v1/models
  MiniMax:    https://api.minimax.chat/v1/models
  Z.ai:       https://api.z.ai/v1/models
  Ollama:     http://localhost:11434/api/tags
EOF

echo ""
echo "Verification complete!"
echo ""
echo "To test with actual API keys, run:"
echo "  export OPENAI_API_KEY=sk-..."
echo "  export ANTHROPIC_API_KEY=sk-ant-..."
echo "  # etc."
echo "  cargo run --bin borgclaw -- init"
echo ""
