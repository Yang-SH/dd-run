# dd-run

[English](./README.md) | [简体中文](./README.zh-CN.md)

**A cross-platform command palette / launcher, written in Rust.** Design inspired by the [PowerToys Command Palette (CmdPal)](https://github.com/microsoft/PowerToys/tree/main/src/modules/cmdpal) module.

> **Status**: MVP milestones M0–M4 are all closed and verified on real hardware. M5 (ueli-style UI redesign) is complete. M6 (settings & system integration) has shipped: settings pages, Mica/Acrylic window material, tray icon, customizable global hotkey, Pinyin search, and a file-search extension. See [`docs/implementation.md`](./docs/implementation.md) §5 for milestone progress and §6.1 for the follow-ups ledger.

---

## What is it

`dd-run` is a **cross-platform (Windows / macOS / Linux) command palette / launcher built from scratch in Rust**: summoned by a global hotkey, type-to-search, keyboard-first, extensible through an extension ecosystem.

Its architecture and extension contracts are **distilled from Microsoft PowerToys' CmdPal module** — but it is not a port. CmdPal is deeply bound to Windows (WinRT / COM / XAML); `dd-run` abstracts the platform-independent parts (UI model, extension contracts, host lifecycle) and replaces them with Rust-ecosystem equivalents:

| CmdPal (Windows) | dd-run (cross-platform) |
|---|---|
| Out-of-process COM / WinRT | **Sub-process + JSON-RPC over NDJSON** (see [`docs/protocol.md`](./docs/protocol.md)) |
| AppExtensionCatalog / registry discovery | **Manifest file scanning** (`extensions.d/*.json`, see [`docs/manifest-schema.md`](./docs/manifest-schema.md)) |
| WinUI 3 / XAML | egui (see [`docs/implementation.md`](./docs/implementation.md) ADR-2) |
| C# extensions | Any language that can read/write stdin/stdout |

## Highlights

- **Keyboard-first**: the entire core loop (summon → search → select → execute → back → close) is 100% keyboard-driven.
- **6 built-in extensions**: Apps, Calculator, System, Web Search, Shell, and File Search (Everything-backed, Windows; direct entry via the `f ` query prefix).
- **Pinyin matching**: CJK app names match by full Pinyin and initials (e.g. `jsq` → 计算器).
- **Settings pages** (open with `Ctrl+,`): appearance (light/dark theme, Mica/Acrylic material), general (autostart, customizable global hotkey — default `Win+Alt+Space`), search-engine management (preset + custom engines with `{q}` templates), extension management (enable/disable per extension).
- **Localized UI**: the panel follows the system display language.
- **Fast & isolated**: cold start reads on-disk "command stubs" so extension processes launch lazily (frozen/stub/LRU mechanism); every extension runs in its own process — a crash never takes the host down.
- **Single-file distribution**: `dist/dd-run-0.1.0.exe` (~10 MB, `strip = "symbols"` + fat LTO). The built-in extensions are embedded into the host binary at build time and materialized to a per-user cache directory on first launch — **process isolation (ADR-1) is fully preserved**. No installer, no sidecar files: double-click and run.

## Goals and non-goals

### Goals

1. **Cross-platform**: one contract and one codebase for all three platforms; platform differences are confined to adapter layers.
2. **Keyboard-first**: the core paths are 100% keyboard-completable (acceptance A11).
3. **Fast**: cold start reads on-disk command stubs; extension processes load lazily.
4. **Extensible**: third-party extensions are independent processes that talk a plain text protocol; their crashes never affect the host.
5. **Simple to build**: the MVP implements the minimal useful set; no complexity is paid upfront for imaginary requirements.

### Non-goals (explicitly out of scope for MVP)

- **No cross-platform ports of Windows-only extensions**: 9 `🪟` items in the design doc §7 (registry, Windows settings, WinGet, …) have no cross-platform equivalents.
- **No extension store (Gallery)**: optional module, not MVP.
- **No WASM sandbox / process-registration discovery**: advanced options; MVP takes the simplest path (ADR-1 / ADR-3).
- **No mobile / web clients.**
- **No cloud sync, telemetry, or accounts.**

## Build and packaging

> **Hard rule: every package is a single file that is the whole program.**
> The only distribution artifact is `dist/dd-run-<version>.exe` — no sidecar extension exes, resource directories, installers, or extra configuration. Double-click and use; process isolation (ADR-1) is unchanged.

| Item | Detail |
|---|---|
| Entry artifact | `dist/dd-run-0.1.0.exe` (Windows, ~10 MB; version tracks `crates/dd-gui/Cargo.toml`) |
| Embedding | Built-in extension exes (`dd-ext-{apps,calc,system,websearch,shell}`) are embedded into the host bytes at compile time by `dd-gui/build.rs` (`assets/embed/` is a scratch input for the packaging script and is gitignored) |
| Runtime | On first launch, `dd-gui::embedded::materialize` materializes them into `%APPDATA%/dd-run/cache/embedded/` (an `.host-version` marker keeps it idempotent), then the host spawns them via the usual `ensure_builtins` + `ExtensionProcess::spawn` — **process isolation fully preserved** |
| Naming | `dd-run.exe` = GUI host (crate stays `dd-gui`); the M0 CLI is renamed `dd-run-cli.exe` (keeps self-check ability, yields the artifact name) |
| One-command package | `bash tools/package.sh` (build `dd-ext` → copy exes → build `dd-gui --bin dd-run` → copy artifact into `dist/`) |
| Repo hygiene | No binaries in the source tree (`/dist/` and `/crates/dd-gui/assets/embed/*.exe` are gitignored) |
| Verification bar | `cargo fmt --check` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` all green, plus an out-of-tree smoke test (isolated directory containing only `dd-run.exe` → materialization → warm handshakes) |

**Why no installer / no multi-file zip**: during the MVP stage the project explicitly chooses "simple to build" — a single-file artifact delivers "zero install ceremony" and "double-click on any machine" without paying for registry/installer/uninstaller/upgrade-script complexity. See [`docs/implementation.md`](./docs/implementation.md) §5 and the ADR section.

## Documentation

| Document | Contents | Audience |
|---|---|---|
| [`cmdpal-platform-agnostic-design.md`](./cmdpal-platform-agnostic-design.md) | **Design reference (upstream source)**: CmdPal's UI model, extension contracts, host model, built-in extension inventory, Rust reference implementation, acceptance criteria A1–A12 | Understand *why* it is designed this way |
| [`cmdpal-ui-mockups.html`](./cmdpal-ui-mockups.html) | **Interactive UI spec** (v4.17): dark/light theme component gallery — root view, search, list/detail pages, settings cards, context menus, dialogs, toasts, loading skeletons | See what it looks like |
| [`docs/implementation.md`](./docs/implementation.md) | **Implementation plan**: milestones M0–M6, ADR decision records, A1–A12 acceptance mapping, current progress, follow-ups ledger | Write code |
| [`docs/protocol.md`](./docs/protocol.md) | **dd-run Extension Protocol v1.0**: NDJSON framing, JSON-RPC envelope, methods, error codes, lifecycle state machine, timeouts and crash recovery | Host or extension authors |
| [`docs/manifest-schema.md`](./docs/manifest-schema.md) | **Extension manifest schema**: field tables, per-platform config directories, minimal copyable example | Extension authors |
| [`docs/search-file.md`](./docs/search-file.md) | **File-search extension plan** (v3.2): Everything-first provider, `f ` prefix direct entry, performance budget | Extension authors |

Suggested reading order: design doc §1–§7 (the model) → [`docs/implementation.md`](./docs/implementation.md) (what to build first) → [`docs/protocol.md`](./docs/protocol.md) + [`docs/manifest-schema.md`](./docs/manifest-schema.md) (build against these).

## Design decisions (summary)

Full records and rationale live in the ADR section of [`docs/implementation.md`](./docs/implementation.md).

| # | Decision | Outcome |
|---|---|---|
| ADR-1 | Extension isolation | **Sub-process + NDJSON JSON-RPC** (WASM sandbox demoted to advanced/optional) |
| ADR-2 | GUI framework | **egui** (Slint / iced as runners-up) |
| ADR-3 | Extension discovery | **Manifest file scanning** (process registration / WASM embedding demoted to optional) |
| ADR-4 | Protocol framing | **NDJSON** (one JSON per line, `\n` terminated; handshake reserves a `transport` field) |

## Upstream attribution and license

- The design docs **distill and adapt** material from the CmdPal module of the [`microsoft/PowerToys`](https://github.com/microsoft/PowerToys) repository (README / SDK spec / UI anatomy / design principles / Gallery notes); per-item attributions are listed in the design doc §11.
- **Verification baseline**: factual claims about upstream are verified against `microsoft/PowerToys` tag **v0.101.2362.0** (checked 2026-09-01); upstream is still in preview and may evolve (design doc §11 and implementation.md R6).
- PowerToys is **MIT licensed**; `dd-run` is likewise **MIT licensed** (root `LICENSE` + `license = "MIT"` in every crate). Re-checking the attribution boundary is follow-up L6 (implementation.md §6.1).
- `dd-run` contains no PowerToys source code; all Rust code is an independent implementation.

## Roadmap

- **M0–M4 closed**: protocol freeze → minimal panel → command execution & state machine → cache/lazy loading → 5 built-in extensions & robustness (commit `757f3b4`).
- **M5 complete**: ueli-style UI redesign, batches 1–4.2 + six rounds of real-hardware feedback fixes (commit `5cf32b7`) + design-spec group C (loading skeletons, dialog overlays, toast intents).
- **M6 shipped**: settings pages (appearance / general / search / extensions), Mica/Acrylic window material, tray icon, drag & resize, custom global hotkey, autostart, Pinyin search, i18n, cold-start CJK font background loading, and the file-search extension with `f ` direct entry (commits `a007656`…`2e5b3da`, UI spec v4.17/v4.17a).
- **Candidate next steps**: embedding the file-search extension into the single-file package, third-party extension end-to-end validation, A2 cold-start GUI profiling (wgpu + 22 MB font ≈ 2.8 s before the async fix), cross-platform work. Full progress and the follow-ups ledger: [`docs/implementation.md`](./docs/implementation.md) §5 / §6.1.

