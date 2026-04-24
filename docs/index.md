# Documentation Index

Canonical navigation map for this repository's documentation.

This repository is a Rust Zellij plugin with terminal hook scripts. Docs are intentionally lightweight.

---

## Core Docs (`docs/`)

| Path | Purpose |
|---|---|
| [`docs/index.md`](index.md) | This file; documentation navigation map |
| [`docs/runtime-reference.md`](runtime-reference.md) | Runtime paths, install/reload loop, and local customization snapshot |
| [`docs/architecture.md`](architecture.md) | System topology, runtime boundaries, and component responsibilities |
| [`docs/conventions.md`](conventions.md) | Code style and implementation patterns |
| [`docs/testing.md`](testing.md) | Test strategy and commands |
| [`docs/decisions.md`](decisions.md) | Architectural decision log |

---

## Maintenance Rules

- When adding a doc: add its row here in the same commit.
- When removing or renaming a doc: update this file and inbound links in the same commit.
- Keep AGENTS guidance concise; move detailed operational/reference material into `docs/`.
