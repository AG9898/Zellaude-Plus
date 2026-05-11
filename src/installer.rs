use std::collections::BTreeMap;
use zellij_tile::prelude::run_command;

const HOOK_VERSION_TAG: &str = concat!("# zellaude v", env!("CARGO_PKG_VERSION"));

fn hook_script_content() -> String {
    let original = include_str!("../scripts/zellaude-hook.sh");
    if let Some(pos) = original.find('\n') {
        let (shebang, rest) = original.split_at(pos);
        format!("{shebang}\n{HOOK_VERSION_TAG}{rest}")
    } else {
        original.to_string()
    }
}

fn codex_hook_script_content() -> String {
    let original = include_str!("../scripts/zellaude-codex-hook.sh");
    if let Some(pos) = original.find('\n') {
        let (shebang, rest) = original.split_at(pos);
        format!("{shebang}\n{HOOK_VERSION_TAG}{rest}")
    } else {
        original.to_string()
    }
}

const INSTALL_TEMPLATE: &str = r##"set -e
HOOK_PATH="$HOME/.config/zellij/plugins/zellaude-hook.sh"
SETTINGS="$HOME/.claude/settings.json"

# Check if already current
if grep -qF '__VERSION_TAG__' "$HOOK_PATH" 2>/dev/null; then
  if [ -f "$SETTINGS" ] && grep -qF "$HOOK_PATH" "$SETTINGS" 2>/dev/null; then
    echo "current"
    exit 0
  fi
fi

# Write hook script
mkdir -p "$(dirname "$HOOK_PATH")"
cat > "$HOOK_PATH" << 'ZELLAUDE_HOOK_EOF'
__HOOK_SCRIPT__
ZELLAUDE_HOOK_EOF
chmod +x "$HOOK_PATH"

# Register hooks (requires jq)
if ! command -v jq >/dev/null 2>&1; then
  echo "no_jq"
  exit 0
fi

if [ ! -f "$SETTINGS" ]; then
  mkdir -p "$HOME/.claude"
  echo '{}' > "$SETTINGS"
fi

# Back up settings before modifying
cp "$SETTINGS" "$SETTINGS.bak"

# Remove ALL existing zellaude hook entries (any path ending in zellaude-hook.sh)
tmp=$(mktemp)
jq '
  if .hooks and (.hooks | type == "object") then
    .hooks |= with_entries(
      .value |= [
        .[] | . as $group |
        ($group.hooks // []) | map(select((.command // "") | endswith("zellaude-hook.sh") | not)) |
        . as $filtered |
        if length > 0 then ($group | .hooks = $filtered) else empty end
      ]
    ) | .hooks |= with_entries(select(.value | length > 0)) |
    if .hooks == {} then del(.hooks) else . end
  else . end
' "$SETTINGS" > "$tmp" && mv "$tmp" "$SETTINGS"

# Add new hook entries
EVENTS='["PreToolUse","PostToolUse","PostToolUseFailure","UserPromptSubmit","PermissionRequest","Notification","Stop","SubagentStop","SessionStart","SessionEnd"]'
ENTRY=$(jq -nc --arg cmd "$HOOK_PATH" '[{"hooks": [{"type": "command", "command": $cmd, "timeout": 5, "async": true}]}]')
tmp=$(mktemp)
jq --argjson events "$EVENTS" --argjson entry "$ENTRY" '
  .hooks //= {} |
  reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
' "$SETTINGS" > "$tmp" && mv "$tmp" "$SETTINGS"

echo "installed"
"##;

pub fn run_install() {
    let cmd = INSTALL_TEMPLATE
        .replace("__VERSION_TAG__", HOOK_VERSION_TAG)
        .replace("__HOOK_SCRIPT__", &hook_script_content());

    let mut ctx = BTreeMap::new();
    ctx.insert("type".into(), "install_hooks".into());
    run_command(&["sh", "-c", &cmd], ctx);
}

const CODEX_INSTALL_TEMPLATE: &str = r##"set -e
HOOK_PATH="$HOME/.config/zellij/plugins/zellaude-codex-hook.sh"
CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
HOOKS_FILE="$CODEX_HOME/hooks.json"
CONFIG_FILE="$CODEX_HOME/config.toml"

# Skip silently if Codex has never been set up
if ! command -v codex >/dev/null 2>&1 && [ ! -d "$CODEX_HOME" ]; then
  echo "skipped"
  exit 0
fi

# Check if already current
if grep -qF '__VERSION_TAG__' "$HOOK_PATH" 2>/dev/null; then
  if [ -f "$HOOKS_FILE" ] && grep -qF "$HOOK_PATH" "$HOOKS_FILE" 2>/dev/null; then
    echo "current"
    exit 0
  fi
fi

# Write hook script
mkdir -p "$(dirname "$HOOK_PATH")"
cat > "$HOOK_PATH" << 'ZELLAUDE_CODEX_HOOK_EOF'
__CODEX_HOOK_SCRIPT__
ZELLAUDE_CODEX_HOOK_EOF
chmod +x "$HOOK_PATH"

if ! command -v jq >/dev/null 2>&1; then
  echo "no_jq"
  exit 0
fi

mkdir -p "$CODEX_HOME"
if [ ! -f "$HOOKS_FILE" ]; then
  echo '{}' > "$HOOKS_FILE"
fi

cp "$HOOKS_FILE" "$HOOKS_FILE.bak"

# Remove any existing zellaude-codex-hook.sh entries
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
' "$HOOKS_FILE" > "$tmp" && mv "$tmp" "$HOOKS_FILE"

# Add new entries — Codex hooks.json expects PascalCase event keys.
EVENTS='["PreToolUse","PostToolUse","PermissionRequest","SessionStart","UserPromptSubmit","Stop"]'
ENTRY=$(jq -nc --arg cmd "$HOOK_PATH" '[{"hooks": [{"type": "command", "command": $cmd, "timeout": 5}]}]')
tmp=$(mktemp)
jq --argjson events "$EVENTS" --argjson entry "$ENTRY" '
  .hooks //= {} |
  reduce ($events[]) as $event (.; .hooks[$event] = (.hooks[$event] // []) + $entry)
' "$HOOKS_FILE" > "$tmp" && mv "$tmp" "$HOOKS_FILE"

# Ensure hooks feature is enabled
if [ -f "$CONFIG_FILE" ]; then
  if ! grep -q '^\[features\]' "$CONFIG_FILE" 2>/dev/null; then
    printf '\n[features]\n' >> "$CONFIG_FILE"
  fi
  if ! grep -q '^hooks[[:space:]]*=' "$CONFIG_FILE" 2>/dev/null; then
    sed -i '/^\[features\]/a hooks = true' "$CONFIG_FILE"
  fi
else
  printf '[features]\nhooks = true\n' > "$CONFIG_FILE"
fi

echo "installed"
"##;

pub fn run_codex_install() {
    let cmd = CODEX_INSTALL_TEMPLATE
        .replace("__VERSION_TAG__", HOOK_VERSION_TAG)
        .replace("__CODEX_HOOK_SCRIPT__", &codex_hook_script_content());

    let mut ctx = BTreeMap::new();
    ctx.insert("type".into(), "install_codex_hooks".into());
    run_command(&["sh", "-c", &cmd], ctx);
}
