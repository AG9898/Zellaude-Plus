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

## changed_work

- 2026-04-24: Updated `src/render.rs` to switch the bar from sharp powerline ribbons to rounded pill segments (``/``) and remapped status/prefix colors to match the active `dark-plus-burgundy` Zellij theme. Add-on: adjusted the top mode/status pill text to use a high-contrast foreground (instead of gray-leaning text) so it reads clearly against red modes. Add-on: increased contrast for tracked tab CWD/elapsed labels and brightened Codex symbol/name colors so repo labels (eg `zellaude-plus-...`) no longer render as low-contrast gray on burgundy pills.
- 2026-04-24: Restored the built-in bottom Zellij status bar by adding a `zellij:status-bar` pane to `layout.kdl` (while keeping the top Zellaude plugin pane).
- 2026-04-24: Added ASCII buddy character to the far right of the status bar (inspired by Claude Code's /buddy feature). The buddy occupies 9 columns (1 space + 8-char expression: 5-char face + 3-char accessory zone). Two styles available — kaomoji `(*_*)`, `(o_o)`, `(>_<)`, `(TwT)`, etc. and cat `=^*^=`, `=^o^=`, `=ToT=`, etc. — each with animated accessories: thinking dots grow left-to-right and waiting exclamations escalate at 250ms/frame. Idle states blink once every 4 seconds. Controlled by a tri-state `BuddyStyle` enum (`Off/Kaomoji/Cat`, default `Off`). Toggle via settings menu click (`○ Buddy: off` / `● Buddy: kaomoji` / `◐ Buddy: cat`) or from any terminal with:
  ```
  zellij pipe --name zellaude-buddy -- "off"      # disable
  zellij pipe --name zellaude-buddy -- "kaomoji"  # kaomoji style
  zellij pipe --name zellaude-buddy -- "cat"      # cat style
  zellij pipe --name zellaude-buddy -- ""         # cycle to next
  ```
  Note: pipe names with colons do not work from the CLI in Zellij — hyphens required. Recommended shell aliases: `alias buddy-cat='zellij pipe --name zellaude-buddy -- "cat"'` etc. Files changed: `src/state.rs` (`BuddyStyle` enum + cycle + `buddy_style` replacing `buddy: bool`), `src/main.rs` (payload-aware pipe handler + `needs_buddy_animation()` + timer extension), `src/render.rs` (`cat_faces()` + updated `render_buddy()` + 3-state settings menu + BuddyStyle import).
