#!/usr/bin/env bash
# BorgClaw configuration helper library
# Provides functions to read configuration values from config.toml

if [[ -n "${BORGCLAW_CONFIG_SH_LOADED:-}" ]]; then
    return 0 2>/dev/null || exit 0
fi
readonly BORGCLAW_CONFIG_SH_LOADED=1

# Default configuration values
readonly BORGCLAW_DEFAULT_WS_PORT=3000
readonly BORGCLAW_DEFAULT_WEBHOOK_PORT=8080

# Get the config file path
borgclaw_config_path() {
    if [[ -n "${BORGCLAW_CONFIG:-}" ]]; then
        echo "$BORGCLAW_CONFIG"
        return 0
    fi
    
    # Check XDG config location first
    if [[ -n "${XDG_CONFIG_HOME:-}" && -f "${XDG_CONFIG_HOME}/borgclaw/config.toml" ]]; then
        echo "${XDG_CONFIG_HOME}/borgclaw/config.toml"
        return 0
    fi
    
    # Check standard locations
    local locations=(
        "$HOME/.config/borgclaw/config.toml"
        ".borgclaw/config.toml"
        "config.toml"
    )
    
    for location in "${locations[@]}"; do
        if [[ -f "$location" ]]; then
            echo "$location"
            return 0
        fi
    done
    
    # Return default location even if it doesn't exist
    echo "$HOME/.config/borgclaw/config.toml"
}

# Extract a TOML value using basic parsing (handles simple key = value patterns)
# Usage: borgclaw_get_toml_value <config_file> <section> <key> [default_value]
borgclaw_get_toml_value() {
    local config_file="$1"
    local section="$2"
    local key="$3"
    local default_value="${4:-}"
    
    if [[ ! -f "$config_file" ]]; then
        echo "$default_value"
        return 0
    fi
    
    # Read file and extract value from section
    local in_section=0
    local value=""
    
    while IFS= read -r line || [[ -n "$line" ]]; do
        # Check for section header
        if [[ "$line" =~ ^\[${section}\]$ ]]; then
            in_section=1
            continue
        fi
        
        # Check for section end (new section starts)
        if [[ "$line" =~ ^\[.*\]$ && "$in_section" -eq 1 ]]; then
            break
        fi
        
        # Extract key value if in correct section
        if [[ "$in_section" -eq 1 && "$line" =~ ^${key}[[:space:]]*=[[:space:]]*(.+)$ ]]; then
            value="${BASH_REMATCH[1]}"
            # Remove quotes if present
            value="${value#\"}"
            value="${value%\"}"
            value="${value#'"'}"
            value="${value%'"'}"
            echo "$value"
            return 0
        fi
    done < "$config_file"
    
    echo "$default_value"
}

# Get WebSocket port from configuration
# Usage: borgclaw_ws_port
borgclaw_ws_port() {
    local config_file
    config_file=$(borgclaw_config_path)
    
    # First try channels.websocket.port
    local port
    port=$(borgclaw_get_toml_value "$config_file" "channels.websocket" "port" "")
    
    # If not found, try the old config path or default
    if [[ -z "$port" ]]; then
        port=$(borgclaw_get_toml_value "$config_file" "gateway" "port" "")
    fi
    
    # Fall back to default
    if [[ -z "$port" ]]; then
        port="$BORGCLAW_DEFAULT_WS_PORT"
    fi
    
    echo "$port"
}

# Get Webhook port from configuration
# Usage: borgclaw_webhook_port
borgclaw_webhook_port() {
    local config_file
    config_file=$(borgclaw_config_path)
    
    local port
    port=$(borgclaw_get_toml_value "$config_file" "channels.webhook" "port" "")
    
    # Fall back to default
    if [[ -z "$port" ]]; then
        port="$BORGCLAW_DEFAULT_WEBHOOK_PORT"
    fi
    
    echo "$port"
}

# Print configuration summary
# Usage: borgclaw_print_config
borgclaw_print_config() {
    local config_file
    config_file=$(borgclaw_config_path)
    
    local ws_port
    ws_port=$(borgclaw_ws_port)
    
    local webhook_port
    webhook_port=$(borgclaw_webhook_port)
    
    echo "Configuration:"
    echo "  Config file:  $config_file"
    echo "  WebSocket:    port $ws_port"
    if [[ -f "$config_file" ]]; then
        local webhook_enabled
        webhook_enabled=$(borgclaw_get_toml_value "$config_file" "channels.webhook" "enabled" "false")
        if [[ "$webhook_enabled" == "true" ]]; then
            echo "  Webhook:      port $webhook_port"
        fi
    fi
}
