# AGENTS.md

## Purpose Of This Repo

This clone is a **feature layer** on top of the original/forked `zellaude` project.
Treat it as a fast-moving customization workspace, not a long-lived product with heavy docs.

- Primary goal: add or tweak behavior quickly for local workflow needs.
- Secondary goal: keep changes clean enough to upstream or compare against the base fork later.

## Project Shape

- Rust Zellij plugin source:
  - `src/main.rs` - plugin event loop, settings load/save, click handling
  - `src/render.rs` - status bar rendering and settings menu UI
  - `src/state.rs` - core state structs, enums, persisted settings
  - `src/event_handler.rs` - hook payload -> activity/session state transitions
  - `src/installer.rs` - hook installation logic
- Hook bridge scripts:
  - `scripts/zellaude-hook.sh` (Claude hooks -> `zellij pipe`)
  - `scripts/zellaude-codex-hook.sh` (Codex hooks -> `zellij pipe`)
- Install helpers:
  - `install.sh`
  - `scripts/install-hooks.sh`
  - `scripts/install-codex-hooks.sh`
- Example layout for repo-local testing:
  - `layout.kdl`

## Runtime Paths You Must Remember

Most behavior issues are in the runtime files under `~/.config`, not only this repo.

- Active plugin binary:
  - `~/.config/zellij/plugins/zellaude.wasm`
- Persisted plugin settings:
  - `~/.config/zellij/plugins/zellaude.json`
- Installed hook scripts used at runtime:
  - `~/.config/zellij/plugins/zellaude-hook.sh`
  - `~/.config/zellij/plugins/zellaude-codex-hook.sh`
- Zellij config/layouts:
  - `~/.config/zellij/config.kdl`
  - `~/.config/zellij/layouts/default.kdl`

If code changes are not visible, verify the loaded layout/plugin path with:
- `zellij -s <session> action dump-layout`

## Build / Deploy / Reload Loop

From repo root:

1. Build:
   - `cargo build --release --target wasm32-wasip1`
2. Install wasm:
   - `cp target/wasm32-wasip1/release/zellaude.wasm ~/.config/zellij/plugins/zellaude.wasm`
3. Reload running session plugin:
   - `zellij -s <session> action start-or-reload-plugin file:~/.config/zellij/plugins/zellaude.wasm`
4. Optional format/check:
   - `cargo fmt --check`

## Code Patterns To Follow

- Keep state shape simple in `state.rs` and derive serde for persisted settings.
- When adding a new setting, wire all of these together:
  1. `Settings` struct + default in `state.rs`
  2. `SettingKey` enum in `state.rs`
  3. Toggle handling in `main.rs` click handler
  4. UI control in `render_settings_menu()` in `render.rs`
  5. Any render logic gated by the setting in `render.rs`
- Avoid introducing new background processes from plugin code; use existing hook/pipe model.
- Prefer extending existing activity and tab-selection logic instead of parallel logic paths.

## Behavior Notes That Commonly Matter

- Plugin UI is event-driven by Zellij events + hook payloads sent via `zellij pipe`.
- Some fields (like `cwd`) only appear after relevant hook events arrive.
- If behavior seems stale, validate:
  - correct plugin path loaded in active session
  - runtime settings JSON content
  - hook registration files in `~/.claude/settings.json` and `~/.codex/hooks.json`

## Scope Guidance For Future Agents

- Keep changes focused and reversible.
- Prefer small, local modifications over broad refactors.
- Update `README.md` only when user-facing behavior changes.
- If adding one-off local behavior for this clone, document it briefly here.
- For every completed change, add a brief entry to `changed_work`.

## Current Local Customizations

Snapshot of local behavior currently layered on this clone:

- Zellij runtime is configured to use the local layout file path:
  - `~/.config/zellij/config.kdl` -> `default_layout "/home/ag9898/.config/zellij/layouts/default.kdl"`
- Custom layout at `~/.config/zellij/layouts/default.kdl` includes:
  - top `zellaude` plugin pane
  - bottom `zellij:status-bar` pane
  - custom `swap_tiled_layout` entries for `vertical` and `horizontal`
- Plugin settings/features added in this clone:
  - `mode_indicator` setting is present and enabled by default
  - `cwd` setting is present and enabled by default
  - tracked tabs can render CWD leaf text (eg `zellaude`) next to tab activity/name
- Runtime persisted settings file used by plugin:
  - `~/.config/zellij/plugins/zellaude.json`
  - expected keys include: `notifications`, `flash`, `elapsed_time`, `mode_indicator`, `cwd`
