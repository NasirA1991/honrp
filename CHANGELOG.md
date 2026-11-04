# Changelog

## Unreleased

- Rust port MVP (`rust/honrp`): `/proc` + prefix detection, UTF-16LE log tail
  (starts at EOF), mod-state parsing, match-log map/mode, activity builder,
  Discord IPC with clear-on-exit, `status`/`doctor`/`install`/`service`,
  unit tests and a mock-Discord end-to-end run. 814 KB release binary, ~6 MiB
  RSS idle.
- Rust CI job; musl release workflow on tags.

## 0.1.0 — 2026-11-04

Initial prototype.

- Lua bridge (`startup.lua`) driven by the engine's UI triggers: match/queue/
  lobby/hero-select/panels, hero level, KDA, spectator state.
- Daemon: juvio.exe discovery, Proton prefix/user-dir detection, UTF-16LE
  `console.log` tailing, match-log parsing, Discord IPC with reconnect and
  throttling.
- Presence: ranked/unranked, short map labels (`FoW`, `Mid Wars`, `Field Test`),
  `hero • Lv N • K/D/A`, panel cards (Plinko, Store, Learn, ...), no timer by
  default.
- 10 s heartbeat + event-driven emits; log flush per emit.
- Discord connection only while a game runs; clear-on-exit with the game's pid.
- Tests: Python (22) + Lua harness (mocked K2 environment).
- Packaging: static-binary release plan and Rust port tracked in docs/PLAN.md.
