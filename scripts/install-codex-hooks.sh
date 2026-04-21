#!/usr/bin/env bash
# install-codex-hooks.sh — Add zellaude hook entries to ~/.codex/hooks.json
#
# Usage: ./scripts/install-codex-hooks.sh [--uninstall]
set -euo pipefail

CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
HOOKS_FILE="$CODEX_HOME/hooks.json"
CONFIG_FILE="$CODEX_HOME/config.toml"
HOOK_SCRIPT="$(cd "$(dirname "$0")" && pwd)/zellaude-codex-hook.sh"

if ! command -v jq &>/dev/null; then
  echo "Error: jq is required. Install with: sudo apt install jq" >&2
  exit 1
fi

if [ ! -f "$HOOK_SCRIPT" ]; then
  echo "Error: Hook script not found at $HOOK_SCRIPT" >&2
  exit 1
fi

# Codex hooks.json expects PascalCase event keys.
EVENTS='["PreToolUse","PostToolUse","PermissionRequest","SessionStart","UserPromptSubmit","Stop"]'

HOOK_ENTRY=$(jq -nc --arg cmd "$HOOK_SCRIPT" '[{
  "hooks": [{
    "type": "command",
    "command": $cmd,
    "timeout": 5
  }]
}]')

backup_file() {
  if [ -f "$1" ]; then
    cp "$1" "$1.bak"
    echo "Backed up $1 to $1.bak"
  fi
}

uninstall() {
  if [ ! -f "$HOOKS_FILE" ]; then
    echo "No hooks file found at $HOOKS_FILE"
    exit 0
  fi

  backup_file "$HOOKS_FILE"

  local tmp
  tmp=$(mktemp)
  jq '
    if .hooks and (.hooks | type == "object") then
      .hooks |= with_entries(
        .value |= [
          .[] | . as $group |
          ($group.hooks // []) | map(select((.command // "") | endswith("zellaude-codex-hook.sh") | not)) |
          . as $filtered |
          if length > 0 then ($group | .hooks = $filtered) else empty end
        ]
      ) | .hooks |= with_entries(select(.value | length > 0)) |
      if .hooks == {} then del(.hooks) else . end
    else . end
  ' "$HOOKS_FILE" > "$tmp"
  mv "$tmp" "$HOOKS_FILE"
  echo "Uninstalled zellaude Codex hooks from $HOOKS_FILE"
}

install() {
  mkdir -p "$CODEX_HOME"

  if [ ! -f "$HOOKS_FILE" ]; then
    echo '{}' > "$HOOKS_FILE"
  fi

  backup_file "$HOOKS_FILE"
  uninstall 2>/dev/null || true

  local tmp
  tmp=$(mktemp)
  jq --argjson events "$EVENTS" --argjson entry "$HOOK_ENTRY" '
    .hooks //= {} |
    reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
  ' "$HOOKS_FILE" > "$tmp"
  mv "$tmp" "$HOOKS_FILE"
  echo "Installed zellaude Codex hooks into $HOOKS_FILE"
  echo "Hook script: $HOOK_SCRIPT"
  echo "Events: PreToolUse, PostToolUse, PermissionRequest, SessionStart, UserPromptSubmit, Stop"

  # Ensure codex_hooks feature is enabled in config.toml
  if [ -f "$CONFIG_FILE" ]; then
    if ! grep -q 'codex_hooks' "$CONFIG_FILE" 2>/dev/null; then
      printf '\n[features]\ncodex_hooks = true\n' >> "$CONFIG_FILE"
      echo "Enabled codex_hooks feature in $CONFIG_FILE"
    fi
  else
    printf '[features]\ncodex_hooks = true\n' > "$CONFIG_FILE"
    echo "Created $CONFIG_FILE with codex_hooks feature enabled"
  fi
}

case "${1:-}" in
  --uninstall)
    uninstall
    ;;
  *)
    install
    ;;
esac
