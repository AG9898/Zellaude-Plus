# AGENTS.md

## Purpose Of This Repo

This clone is a **feature layer** on top of the original/forked `zellaude` project.
Treat it as a fast-moving customization workspace, not a long-lived product with heavy docs.

- Primary goal: add or tweak behavior quickly for local workflow needs.
- Secondary goal: keep changes clean enough to upstream or compare against the base fork later.

## Overview

This repository builds a Rust Zellij plugin (`zellaude.wasm`) plus terminal hook scripts that send activity payloads through `zellij pipe`.
Most work here is plugin behavior, rendering, settings toggles, and local runtime wiring.
AGENTS is the concise working guide; detailed operational references live under `docs/`.

## Quick Start

```bash
# Build wasm plugin
cargo build --release --target wasm32-wasip1

# Install runtime wasm used by Zellij
cp target/wasm32-wasip1/release/zellaude.wasm ~/.config/zellij/plugins/zellaude.wasm

# Reload plugin in a running session
zellij -s <session> action start-or-reload-plugin file:~/.config/zellij/plugins/zellaude.wasm

# Fast style check
cargo fmt --check
```

## Build & Verification Commands

| Command | What it checks | Speed |
|---|---|---|
| `cargo fmt --check` | Rust formatting compliance | fast |
| `cargo build --release --target wasm32-wasip1` | Compiles the shipped plugin artifact | fast |
| `cargo test` | Rust tests (if present) | slow |

## Repository Structure

```text
src/                    Rust plugin source
  main.rs               Event loop, settings load/save, click + pipe handling
  render.rs             Status bar rendering + settings menu UI
  state.rs              Core state enums/structs + persisted settings
  event_handler.rs      Hook payload -> activity/session state transitions
  installer.rs          Hook installation logic
scripts/                Hook bridge + install helper scripts
layout.kdl              Example local layout
install.sh              Local install helper
docs/                   Project documentation
  index.md              Documentation navigation map
  runtime-reference.md  Runtime paths, reload loop, customization snapshot
  architecture.md       Topology and runtime boundaries
  conventions.md        Coding conventions and patterns
  testing.md            Test guidance and commands
  decisions.md          Open/resolved architectural decisions
```

Docs navigation: [`docs/index.md`](docs/index.md)

## Architecture Constraints

- Plugin UI is event-driven by Zellij events and hook payloads sent via `zellij pipe`.
- Do not introduce new background processes from plugin code; use the hook/pipe model.
- Extend existing activity and tab-selection logic instead of creating parallel logic paths.
- When adding a new setting, wire all required touchpoints (`state.rs`, `main.rs`, `render.rs`).
- Runtime behavior depends on `~/.config/zellij/...` files as much as repo code.

Full details: [`docs/runtime-reference.md`](docs/runtime-reference.md) and [`docs/architecture.md`](docs/architecture.md)

## Code Style & Constraints

### Never

- Never commit secrets or credentials.
- Never use destructive git commands to revert unrelated user changes.
- Never bypass the existing hook/pipe pathway for activity updates.

### Always

- Always keep changes focused and reversible.
- Always prefer small local modifications over broad refactors.
- Always run fast verification commands before marking work done.
- Always update relevant docs in the same change when behavior or constraints move.
- Always append a brief entry to `changed_work` for completed changes.

### Patterns

- Keep persisted settings shape simple in `state.rs` with serde derives.
- Gate new render behavior behind explicit settings where appropriate.
- Keep implementation details in docs; keep AGENTS concise and directive.

Conventions detail: [`docs/conventions.md`](docs/conventions.md)

## Scope Guidance For Future Agents

- Update `README.md` only when user-facing behavior changes.
- Keep AGENTS concise; move detailed runtime/operational reference content into `docs/`.
- If adding one-off local behavior for this clone, document it briefly in `changed_work` and expand details in docs as needed.

## Maintaining Docs

Docs must stay current with code. Update the relevant doc in the same commit as the change.

| What changed | Doc to update |
|---|---|
| Runtime paths, reload loop, local customization details | [`docs/runtime-reference.md`](docs/runtime-reference.md) |
| System topology or component boundaries | [`docs/architecture.md`](docs/architecture.md) |
| Coding pattern, naming rule, or hard constraints | [`docs/conventions.md`](docs/conventions.md) |
| Test commands/patterns/coverage expectations | [`docs/testing.md`](docs/testing.md) |
| Architectural question or decision | [`docs/decisions.md`](docs/decisions.md) |
| Any docs add/remove/rename/move | [`docs/index.md`](docs/index.md) |

## Debugging & Gotchas

- If code changes are not visible, confirm active layout/plugin path with:
  - `zellij -s <session> action dump-layout`
- Verify runtime files when behavior seems stale:
  - `~/.config/zellij/plugins/zellaude.wasm`
  - `~/.config/zellij/plugins/zellaude.json`
  - `~/.config/zellij/plugins/zellaude-hook.sh`
  - `~/.config/zellij/plugins/zellaude-codex-hook.sh`
- `cwd` labels appear only after relevant hook events arrive.
- Zellij pipe names with colons do not work via CLI; use hyphenated names (for example `zellaude-buddy`).

## Deployment

This repo is local-runtime oriented. Do not publish/release/deploy externally unless explicitly requested.

## changed_work

- 2026-04-24: Integrated major sections from `~/projects/ag.dev/AGENTS_EX.md` into this repo's `AGENTS.md` (quick start, verification commands, repo structure, architecture constraints, doc maintenance map, and debugging gotchas), adapted for Rust + terminal/Zellij workflow.
- 2026-04-24: Created `docs/` scaffolding from the `~/projects/ag.dev/docs` template (trimmed for this Rust/Zellij plugin repo), moved runtime/operational reference content out of `AGENTS.md` into `docs/runtime-reference.md`, and added `docs/index.md` as the canonical documentation map.
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
