# dd-run

[English](./README.md) | [简体中文](./README.zh-CN.md)

> **状态**：生效中 ｜ **版本**：v0.1.1 ｜ **最后更新**：2026-09-13

**A command palette / launcher written in Rust, built on a platform-agnostic core.** Currently shipping on **Windows** only (macOS / Linux are planned from v0.2). Design inspired by the [PowerToys Command Palette (CmdPal)](https://github.com/microsoft/PowerToys/tree/main/src/modules/cmdpal) module.

> **Status**: MVP milestones **M0–M9 are all closed** and verified on real hardware (M0–M6 core panel & settings; M7 release engineering; M8 extension ecosystem validation; M9 in-process built-ins). **v0.1.1** published 2026-09-10. See [`docs/implementation.md`](./docs/implementation.md) §5 for milestone progress and §6.1 for the follow-ups ledger.

---

## What is it

`dd-run` is a **command palette / launcher built from scratch in Rust** with a **platform-agnostic core**: summoned by a global hotkey, type-to-search, keyboard-first, extensible through an extension ecosystem. **Current builds run on Windows only** — the macOS / Linux platform layer is stubbed out and is planned for v0.2.

Its architecture and extension contracts are **distilled from Microsoft PowerToys' CmdPal module** — but it is not a port. CmdPal is deeply bound to Windows (WinRT / COM / XAML); `dd-run` abstracts the platform-independent parts (UI model, extension contracts, host lifecycle) and replaces them with Rust-ecosystem equivalents:

| CmdPal (Windows) | dd-run (platform-agnostic design) |
|---|---|
| Out-of-process COM / WinRT | **Sub-process + NDJSON JSON-RPC over NDJSON** (see [`docs/protocol.md`](./docs/protocol.md)) |
| AppExtensionCatalog / registry discovery | **Manifest file scanning** (`extensions.d/*.json`, see [`docs/manifest-schema.md`](./docs/manifest-schema.md)) |
| WinUI 3 / XAML | egui (see [`docs/implementation.md`](./docs/implementation.md) ADR-2) |
| C# extensions | Any language that can read/write stdin/stdout |

## Highlights

- **Keyboard-first**: the entire core loop (summon → search → select → execute → back → close) is 100% keyboard-driven.
- **5 built-in extensions + file-search sidecar**: Apps, Calculator, System, Web Search, Shell run in-process; File Search (Everything-backed, Windows; press `Ctrl+F` in the panel) ships as a separate sidecar process.
- **Pinyin matching**: CJK app names match by full Pinyin and initials (e.g. `jsq` → 计算器).
- **Settings pages** (open with `Ctrl+,`): appearance (light/dark theme, Mica/Acrylic material), general (autostart, customizable global hotkey — default `Win+Alt+Space`), search-engine management (preset + custom engines with `{q}` templates), extension management (enable/disable per extension).
- **Localized UI**: the panel follows the system display language.
- **Fast & isolated**: built-in extensions are always ready (in-process, no launch cost); third-party and the file-search sidecar launch lazily via frozen/stub/LRU; the file-search sidecar runs in its own process, so its crash can't take the host down.
- **Portable distribution**: `dist/dd-run-0.1.1.exe` (~8 MB, 8,601,088 bytes; `strip = "symbols"`) is a single file bundling the host and the five built-in extensions in-process (no embedded exes since M9). The file-search extension ships as a sidecar in `dist/extensions.d/` and is discovered automatically next to the executable. No installer: unzip the release zip and run.

## Goals and non-goals

### Goals

1. **Cross-platform** (planned for v0.2+; **Windows-only today**): one contract and one codebase for all three platforms; platform differences are confined to adapter layers.
2. **Keyboard-first**: the core paths are 100% keyboard-completable (acceptance A11).
3. **Fast**: cold start reads on-disk command stubs; extension processes load lazily.
4. **Extensible**: third-party extensions are independent processes that talk a plain text protocol; their crashes never affect the host.
5. **Simple to build**: the MVP implements the minimal useful set; no complexity is paid upfront for imaginary requirements.

### Non-goals (explicitly out of scope for MVP)

- **No cross-platform ports of Windows-only extensions**: 9 🪟 items in the design doc §7 (registry, Windows settings, WinGet, …) have no cross-platform equivalents.
- **No extension store (Gallery)**: optional module, not MVP.
- **No WASM sandbox / process-registration discovery**: advanced options; MVP takes the simplest path (ADR-1 / ADR-3).
- **No mobile / web clients.**
- **No cloud sync, telemetry, or accounts.**

## Build and packaging

> **Hard rule: the host is a single file — the five built-in extensions run in-process (no embedded exes).**
> Release layout = `dist/dd-run-<version>.exe` + `dist/extensions.d/` (file-search sidecar), zipped as a whole and attached to the GitHub Release. Unzip and run; no installer.

| Item | Detail |
|---|---|
| Entry artifact | `dist/dd-run-0.1.1.exe` (Windows, ~8 MB / 8,601,088 bytes; version tracks `crates/dd-gui/Cargo.toml`) |
| Bundling (M9) | The five built-in extensions are compiled in-process: the host drives `dd_ext::serve_line` directly, so no extension exes are embedded. `dd-gui/build.rs` embeds an empty table; `assets/embed/` is a leftover scratch dir and is gitignored |
| Sidecar | The file-search extension (`dd-ext-search.exe` + `com.ddrun.filesearch.json`) ships in `dist/extensions.d/`; the host scans an `extensions.d/` next to the executable (batch 7.5), so the zip is unzip & run — precedence: user data dir > sidecar > built-ins |
| Naming | `dd-run.exe` = GUI host (crate stays `dd-gui`); the M0 CLI is renamed `dd-run-cli.exe` (keeps self-check ability, yields the artifact name) |
| One-command package | `bash tools/package.sh` (uses the `+stable-x86_64-pc-windows-gnu` toolchain: build `dd-ext` → build `dd-gui --bin dd-run` → copy single-file artifact + collect sidecar into `dist/`; a bare `cargo` on MSVC fails to find `std`) |
| Release automation | tag push → `.github/workflows/release.yml`: verify tag = crate version → `package.sh` → zip `dd-run-<ver>.exe + extensions.d/` → attach to GitHub Release |
| Repo hygiene | No binaries in the source tree (`/dist/` and `/crates/dd-gui/assets/embed/*.exe` are gitignored) |
| Verification bar | `cargo fmt --check` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` all green, plus an out-of-tree smoke test (built-ins ready in-process + sidecar discovered → warm handshakes) |

**Why no installer / why file-search stays a sidecar**: the project explicitly chooses "simple to build" — a portable zip delivers "zero install ceremony" without paying registry/installer/uninstaller complexity (an Inno Setup installer was explored in M7 batch 7.3 and then dropped by decision — no installer code is kept). The file-search extension is kept out of the bundled set because it depends on a user-environment dependency (Everything): shipping it as a sidecar decouples "with or without file search" from the host binary (M7 batch 7.4 decision record, `docs/implementation.md` §5).

## Documentation

| Document | Contents | Audience |
|---|---|---|
| [`cmdpal-platform-agnostic-design.md`](./cmdpal-platform-agnostic-design.md) | **Design reference (upstream source)**: CmdPal's UI model, extension contracts, host model, built-in extension inventory, Rust reference implementation, acceptance criteria A1–A12 | Understand *why* it is designed this way |
| [`cmdpal-ui-mockups.html`](./cmdpal-ui-mockups.html) | **Interactive UI spec** (v4.18): dark/light theme component gallery — root view, search, list/detail pages, settings cards, context menus, dialogs, toasts, loading skeletons, keyboard walkthrough. v4.18 back-ports the file-search `Ctrl+F` direct entry, the panel key table (§6.5), real Shell icons and the placeholder copy (decisions D39–D41, acceptance group I). Behaviour-only batches v4.14–v4.17a are **not** in the HTML (no visual spec change; see [`docs/implementation.md`](./docs/implementation.md) §7) | See what it looks like |
| [`docs/implementation.md`](./docs/implementation.md) | **Implementation plan**: milestones M0–M9, ADR decision records, A1–A12 acceptance mapping, current progress, follow-ups ledger | Write code |
| [`docs/protocol.md`](./docs/protocol.md) | **dd-run Extension Protocol v1.0**: NDJSON framing, JSON-RPC envelope, methods, error codes, lifecycle state machine, timeouts and crash recovery | Host or extension authors |
| [`docs/manifest-schema.md`](./docs/manifest-schema.md) | **Extension manifest schema**: field tables, per-platform config directories, minimal copyable example | Extension authors |
| [`docs/extensions.md`](./docs/extensions.md) | **Writing an extension**: 10-minute path, the three hard rules, three Windows traps, `host/*` round-trips, self-check (`dd-run-cli --conformance`), debugging cookbook | Extension authors — **start here** |
| [`docs/search-file.md`](./docs/search-file.md) | **File-search spec** (v3.7): dual-channel transport (IPC primary + `es.exe` fallback), `Ctrl+F` direct entry, real Shell icons, real-machine acceptance & thresholds (two open gaps) | Extension authors |
| [`docs/search.md`](./docs/search.md) | **File-search user guide** (v0.1.0+): Everything setup (`es.exe` is an optional fallback), search syntax cheat sheet, troubleshooting | End users |

Suggested reading order: design doc §1–§7 (the model) → [`docs/implementation.md`](./docs/implementation.md) (what to build first) → [`docs/protocol.md`](./docs/protocol.md) + [`docs/manifest-schema.md`](./docs/manifest-schema.md) (build against these).

## Design decisions (summary)

Full records and rationale live in the ADR section of [`docs/implementation.md`](./docs/implementation.md).

| # | Decision | Outcome |
|---|---|---|
| ADR-1 | Extension isolation | **Sub-process + NDJSON JSON-RPC** (WASM sandbox demoted to advanced/optional) |
| ADR-2 | GUI framework | **egui** (Slint / iced as runners-up) |
| ADR-3 | Extension discovery | **Manifest file scanning** (process registration / WASM embedding demoted to optional) |
| ADR-4 | Protocol framing | **NDJSON** (one JSON per line, `\n` terminated; handshake reserves a `transport` field) |

> **Note (M9)**: built-in extensions now run in-process (the host calls `dd_ext::serve_line` directly) but still speak the NDJSON JSON-RPC protocol; only the file-search sidecar keeps sub-process isolation (ADR-1).

## Upstream attribution and license

- The design docs **distill and adapt** material from the CmdPal module of the [`microsoft/PowerToys`](https://github.com/microsoft/PowerToys) repository (README / SDK spec / UI anatomy / design principles / Gallery notes); per-item attributions are listed in the design doc §11.
- **Verification baseline**: factual claims about upstream are verified against `microsoft/PowerToys` tag **v0.101.2362.0** (checked 2026-09-01); upstream is still in preview and may evolve (design doc §11 and implementation.md R6).
- PowerToys is **MIT licensed**; `dd-run` is likewise **MIT licensed** (root `LICENSE` + `license = "MIT"` in every crate). Re-checking the attribution boundary is follow-up L6 (implementation.md §6.1).
- `dd-run` contains no PowerToys source code; all Rust code is an independent implementation.

## Roadmap

- **M0–M4 closed**: protocol freeze → minimal panel → command execution & state machine → cache/lazy loading → 5 built-in extensions & robustness (commit `757f3b4`).
- **M5 complete**: ueli-style UI redesign, batches 1–4.2 + six rounds of real-hardware feedback fixes (commit `5cf32b7`) + design-spec group C (loading skeletons, dialog overlays, toast intents).
- **M6 complete** (verified on real hardware via the consolidated regression A–F on 2026-09-08): settings pages (appearance / general / search / extensions), Mica/Acrylic window material, tray icon, drag & resize, custom global hotkey, autostart, Pinyin search, i18n, cold-start CJK font background loading, and the file-search extension (now entered with `Ctrl+F`) (commits `a007656`…`2e5b3da`; UI spec batches v4.17/v4.17a — the mockup HTML file itself stops at v4.13).
- **M7 closed** (release engineering, 2026-09-10): GitHub Actions CI (build / test / clippy `-D warnings` / fmt on windows-gnu), 256px icon tier in `assets/app.ico`. Distribution: **portable single-file only** (an Inno Setup installer was explored and then dropped — no installer code is kept); the file-search extension ships as a sidecar in `dist/extensions.d/`; the host scans an `extensions.d/` next to the executable, so the green zip is **unzip & run**. **v0.1.1** published 2026-09-10 via tag → GitHub Release.
- **M8 closed** (extension ecosystem validation, 2026-09-10): a non-Rust (Python) extension walks the full protocol chain (`--conformance` 10/10 ✓); extension authoring guide + conformance self-check shipped.
- **M9 closed** (in-process built-ins, 2026-09-11): the five built-in extensions now run **inside the host process** (no extension exes spawned or embedded) — single-file distribution; process isolation kept for third-party / sidecar extensions. Protocol v1.0 unchanged.
- **Candidate next steps**: A2 cold-start GUI profiling (wgpu + 22 MB font ≈ 2.8 s before the async fix), cross-platform work. Full progress and the follow-ups ledger: [`docs/implementation.md`](./docs/implementation.md) §5 / §6.1.
