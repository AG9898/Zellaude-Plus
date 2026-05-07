#!/usr/bin/env bash
# zellaude-codex-hook.sh — Codex hook → zellij pipe bridge
# Forwards hook events to the zellaude Zellij plugin via pipe.
#
# Usage in ~/.codex/hooks.json:
#   "command": "/path/to/zellaude-codex-hook.sh"

[ -z "$ZELLIJ_SESSION_NAME" ] && exit 0
[ -z "$ZELLIJ_PANE_ID" ] && exit 0

TS_MS=$(jq -nc 'now * 1000 | floor')
INPUT=$(cat)

# Codex currently sends snake_case hook input fields; keep camelCase and a few
# nested fallbacks so older/newer builds do not silently disappear.
HOOK_EVENT=$(echo "$INPUT" | jq -r '.hook_event_name // .hookEventName // .event // empty')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // .sessionId // .thread_id // empty')
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // .toolName // .tool // .name // .tool_call.name // empty')
CWD=$(echo "$INPUT" | jq -r '.cwd // empty')

[ -z "$HOOK_EVENT" ] && exit 0

# Normalize snake_case Codex event names to PascalCase to match internal event names
case "$HOOK_EVENT" in
  pre_tool_use)       HOOK_EVENT="PreToolUse" ;;
  post_tool_use)      HOOK_EVENT="PostToolUse" ;;
  permission_request) HOOK_EVENT="PermissionRequest" ;;
  session_start)      HOOK_EVENT="SessionStart" ;;
  user_prompt_submit) HOOK_EVENT="UserPromptSubmit" ;;
  stop)               HOOK_EVENT="Stop" ;;
  subagent_stop)      HOOK_EVENT="SubagentStop" ;;
  session_end)        HOOK_EVENT="SessionEnd" ;;
  after_tool_use)     HOOK_EVENT="PostToolUse" ;;
  before_tool_use)    HOOK_EVENT="PreToolUse" ;;
esac

PAYLOAD=$(jq -nc \
  --arg pane_id "$ZELLIJ_PANE_ID" \
  --arg session_id "$SESSION_ID" \
  --arg hook_event "$HOOK_EVENT" \
  --arg tool_name "$TOOL_NAME" \
  --arg cwd "$CWD" \
  --arg zellij_session "$ZELLIJ_SESSION_NAME" \
  --arg term_program "${TERM_PROGRAM:-}" \
  --arg ts_ms "$TS_MS" \
  --arg source "codex" \
  '{
    pane_id: ($pane_id | tonumber),
    session_id: $session_id,
    hook_event: $hook_event,
    tool_name: (if $tool_name == "" then null else $tool_name end),
    cwd: (if $cwd == "" then null else $cwd end),
    zellij_session: $zellij_session,
    term_program: (if $term_program == "" then null else $term_program end),
    ts_ms: ($ts_ms | tonumber),
    source: $source
  }')

if [ "$HOOK_EVENT" = "PermissionRequest" ]; then
  if [ -e /dev/tty ]; then
    (printf '\a' >/dev/tty) 2>/dev/null || true
  fi

  SETTINGS_FILE="$HOME/.config/zellij/plugins/zellaude.json"
  NOTIFY_MODE="Always"
  if [ -f "$SETTINGS_FILE" ]; then
    NOTIFY_MODE=$(jq -r '.notifications // "Always"' "$SETTINGS_FILE" 2>/dev/null)
  fi

  SHOULD_NOTIFY=false
  case "$NOTIFY_MODE" in
    Always) SHOULD_NOTIFY=true ;;
    Unfocused)
      TERM_FOCUSED=false
      case "$(uname)" in
        Darwin)
          EXPECTED="${TERM_PROGRAM:-}"
          case "$EXPECTED" in
            Apple_Terminal) EXPECTED="Terminal" ;;
            iTerm.app)     EXPECTED="iTerm2" ;;
          esac
          FRONT_APP=$(osascript -e 'tell application "System Events" to get name of first application process whose frontmost is true' 2>/dev/null)
          [ "$FRONT_APP" = "$EXPECTED" ] && TERM_FOCUSED=true
          ;;
        Linux)
          if command -v xdotool >/dev/null 2>&1; then
            ACTIVE_PID=$(xdotool getactivewindow getwindowpid 2>/dev/null)
            if [ -n "$ACTIVE_PID" ]; then
              PID=$$
              while [ "$PID" -gt 1 ] 2>/dev/null; do
                [ "$PID" = "$ACTIVE_PID" ] && { TERM_FOCUSED=true; break; }
                PID=$(ps -o ppid= -p "$PID" 2>/dev/null | tr -d ' ')
              done
            fi
          fi
          ;;
      esac
      [ "$TERM_FOCUSED" = false ] && SHOULD_NOTIFY=true
      ;;
  esac

  if [ "$SHOULD_NOTIFY" = true ]; then
    TOOL_SUFFIX=""
    [ -n "$TOOL_NAME" ] && TOOL_SUFFIX=" - $TOOL_NAME"
    TITLE="Codex"
    MESSAGE="Permission requested${TOOL_SUFFIX}"

    LOCK="/tmp/zellaude-notify-codex-${ZELLIJ_PANE_ID}"
    NOW=$(date +%s)
    LAST=0
    [ -f "$LOCK" ] && LAST=$(cat "$LOCK" 2>/dev/null)
    if [ $((NOW - LAST)) -ge 10 ]; then
      echo "$NOW" > "$LOCK"

      ZELLIJ_BIN=$(command -v zellij)
      FOCUS_CMD="${ZELLIJ_BIN} -s '${ZELLIJ_SESSION_NAME}' pipe --name zellaude:focus -- ${ZELLIJ_PANE_ID}"

      case "$(uname)" in
        Darwin)
          [ -n "${TERM_PROGRAM:-}" ] && FOCUS_CMD="open -a '${TERM_PROGRAM}' && ${FOCUS_CMD}"
          if command -v terminal-notifier >/dev/null 2>&1; then
            terminal-notifier -title "$TITLE" -message "$MESSAGE" -execute "$FOCUS_CMD" &
          else
            osascript -e "display notification \"$MESSAGE\" with title \"$TITLE\"" &
          fi
          ;;
        Linux)
          if command -v notify-send >/dev/null 2>&1; then
            notify-send "$TITLE" "$MESSAGE" &
          fi
          ;;
      esac
    fi
  fi
fi

zellij pipe --name "zellaude" -- "$PAYLOAD" >/dev/null 2>&1 || true
